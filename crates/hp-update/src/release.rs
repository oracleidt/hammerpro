use serde_json::Value;

use crate::error::UpdateError;
use crate::version::Version;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub version: Version,
    pub title: String,
    pub notes: String,
    pub page_url: String,
    pub assets: Vec<Asset>,
}

const MAX_NOTES: usize = 20_000;

pub fn parse_release(json: &str) -> Result<Release, UpdateError> {
    let v: Value = serde_json::from_str(json).map_err(|e| UpdateError::BadRelease(format!("not JSON ({e})")))?;
    let obj = v.as_object().ok_or_else(|| UpdateError::BadRelease("not an object".into()))?;
    let text = |key: &str| obj.get(key).and_then(Value::as_str).unwrap_or("").to_owned();
    if obj.get("draft").and_then(Value::as_bool) == Some(true) {
        return Err(UpdateError::BadRelease("draft".into()));
    }
    let tag = text("tag_name");
    if tag.is_empty() {
        return Err(UpdateError::BadRelease("no tag_name".into()));
    }
    let version =
        Version::parse(&tag).ok_or_else(|| UpdateError::BadRelease(format!("tag \"{tag}\" is not a version")))?;
    let mut notes = text("body").replace("\r\n", "\n");
    if notes.len() > MAX_NOTES {
        let mut cut = MAX_NOTES;
        while !notes.is_char_boundary(cut) {
            cut -= 1;
        }
        notes.truncate(cut);
    }
    let assets = obj
        .get("assets")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    let name = a.get("name")?.as_str()?.to_owned();
                    let url = a.get("browser_download_url")?.as_str()?.to_owned();
                    let size = a.get("size").and_then(Value::as_u64).unwrap_or(0);
                    Some(Asset { name, url, size })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Release { tag, version, title: text("name"), notes, page_url: text("html_url"), assets })
}

impl Release {
    pub fn is_newer_than(&self, current: &Version) -> bool {
        &self.version > current
    }

    pub fn pick_asset(&self) -> Option<&Asset> {
        let lower = |a: &Asset| a.name.to_ascii_lowercase();
        let exact = format!("hammerpro-{}-windows.zip", self.version).to_ascii_lowercase();
        self.assets
            .iter()
            .find(|a| lower(a) == exact)
            .or_else(|| self.assets.iter().find(|a| lower(a).ends_with(".zip") && lower(a).contains("windows")))
            .or_else(|| self.assets.iter().find(|a| lower(a).ends_with(".zip")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r###"{
        "url": "https://api.github.com/repos/oracleidt/hammerpro/releases/1",
        "html_url": "https://github.com/oracleidt/hammerpro/releases/tag/v1.0.3",
        "tag_name": "v1.0.3",
        "name": "Hammer Pro 1.0.3",
        "draft": false,
        "prerelease": false,
        "body": "## New\r\n- auto update\r\n- fixes",
        "assets": [
            {"name": "notes.txt", "browser_download_url": "https://github.com/x/notes.txt", "size": 10},
            {"name": "HammerPro-1.0.3-linux.zip", "browser_download_url": "https://github.com/x/linux.zip", "size": 20},
            {"name": "HammerPro-1.0.3-windows.zip", "browser_download_url": "https://github.com/x/win.zip", "size": 30},
            {"name": "broken"}
        ]
    }"###;

    #[test]
    fn parses_a_github_release() {
        let r = parse_release(SAMPLE).unwrap();
        assert_eq!(r.tag, "v1.0.3");
        assert_eq!(r.version.to_string(), "1.0.3");
        assert_eq!(r.title, "Hammer Pro 1.0.3");
        assert_eq!(r.notes, "## New\n- auto update\n- fixes");
        assert_eq!(r.assets.len(), 3);
        assert_eq!(r.page_url, "https://github.com/oracleidt/hammerpro/releases/tag/v1.0.3");
        let a = r.pick_asset().unwrap();
        assert_eq!(
            (a.name.as_str(), a.size, a.url.as_str()),
            ("HammerPro-1.0.3-windows.zip", 30, "https://github.com/x/win.zip")
        );
    }

    #[test]
    fn newer_means_strictly_greater() {
        let r = parse_release(SAMPLE).unwrap();
        assert!(r.is_newer_than(&Version::parse("1.0.2").unwrap()));
        assert!(r.is_newer_than(&Version::parse("0.9.9").unwrap()));
        assert!(!r.is_newer_than(&Version::parse("1.0.3").unwrap()));
        assert!(!r.is_newer_than(&Version::parse("1.0.10").unwrap()));
        assert!(!r.is_newer_than(&Version::parse("2.0.0").unwrap()));
    }

    #[test]
    fn asset_choice_falls_back_to_any_zip() {
        let r = parse_release(
            r#"{"tag_name":"1.2.0","assets":[{"name":"app.ZIP","browser_download_url":"https://github.com/a","size":1}]}"#,
        )
        .unwrap();
        assert_eq!(r.pick_asset().unwrap().name, "app.ZIP");
        let none = parse_release(
            r#"{"tag_name":"1.2.0","assets":[{"name":"a.exe","browser_download_url":"https://github.com/a"}]}"#,
        )
        .unwrap();
        assert!(none.pick_asset().is_none());
        assert!(parse_release(r#"{"tag_name":"1.2.0"}"#).unwrap().assets.is_empty());
    }

    #[test]
    fn rejects_unusable_answers() {
        for bad in [
            "",
            "[]",
            "not json",
            r#"{"message":"Not Found"}"#,
            r#"{"tag_name":"nightly"}"#,
            r#"{"tag_name":"v1.0.3","draft":true}"#,
        ] {
            assert!(matches!(parse_release(bad), Err(UpdateError::BadRelease(_))), "{bad}");
        }
    }

    #[test]
    fn long_notes_are_cut_on_a_character_boundary() {
        let body = "\u{e9}".repeat(MAX_NOTES);
        let json = serde_json::json!({ "tag_name": "v2.0.0", "body": body }).to_string();
        let r = parse_release(&json).unwrap();
        assert!(r.notes.len() <= MAX_NOTES && !r.notes.is_empty());
    }
}
