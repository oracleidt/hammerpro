use crate::version::Version;

pub const DEFAULT_API_URL: &str = "https://api.github.com/repos/oracleidt/hammerpro/releases/latest";
pub const API_URL_ENV: &str = "HP_UPDATE_API_URL";
pub const VERSION_ENV: &str = "HP_UPDATE_CURRENT_VERSION";
pub const PROGRAM_NAME: &str = "hammerpro.exe";

pub fn api_url() -> String {
    std::env::var(API_URL_ENV)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_API_URL.to_owned())
}

pub fn api_url_overridden() -> bool {
    api_url() != DEFAULT_API_URL
}

pub fn current_version() -> Version {
    std::env::var(VERSION_ENV)
        .ok()
        .and_then(|v| Version::parse(&v))
        .or_else(|| Version::parse(env!("CARGO_PKG_VERSION")))
        .unwrap_or(Version { major: 0, minor: 0, patch: 0, pre: String::new() })
}

pub fn allowed_download_url(url: &str, api_url: &str) -> bool {
    let Some(target) = crate::http::Url::parse(url) else { return false };
    if target.secure {
        let host = target.host.to_ascii_lowercase();
        let github = host == "github.com"
            || host == "api.github.com"
            || host == "githubusercontent.com"
            || host.ends_with(".githubusercontent.com");
        if github {
            return true;
        }
    }
    api_url != DEFAULT_API_URL
        && crate::http::Url::parse(api_url)
            .is_some_and(|api| api.host.eq_ignore_ascii_case(&target.host) && api.port == target.port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_github_hosts_unless_the_api_is_overridden() {
        let api = DEFAULT_API_URL;
        assert!(allowed_download_url("https://github.com/oracleidt/hammerpro/releases/download/v1.0.3/a.zip", api));
        assert!(allowed_download_url("https://objects.githubusercontent.com/x", api));
        assert!(!allowed_download_url("http://github.com/x", api));
        assert!(!allowed_download_url("https://evilgithub.com/x", api));
        assert!(!allowed_download_url("https://github.com.evil.example/x", api));
        assert!(!allowed_download_url("http://127.0.0.1:9/x", api));
        assert!(allowed_download_url("http://127.0.0.1:9/x", "http://127.0.0.1:9/api"));
        assert!(!allowed_download_url("http://127.0.0.1:10/x", "http://127.0.0.1:9/api"));
        assert!(!allowed_download_url("not a url", api));
    }
}
