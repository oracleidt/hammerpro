use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};

use crate::apply::Helper;
use crate::config::{PROGRAM_NAME, allowed_download_url, api_url, current_version};
use crate::error::UpdateError;
use crate::http;
use crate::release::{Release, parse_release};
use crate::version::Version;
use crate::zip::Archive;

const MAX_API_ANSWER: u64 = 2 * 1024 * 1024;
const MAX_DOWNLOAD: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckResult {
    UpToDate { latest: Version },
    Available(Release),
    Failed(UpdateError),
}

pub fn check(api_url: &str, current: &Version) -> CheckResult {
    match http::get_text(api_url, MAX_API_ANSWER).and_then(|text| parse_release(&text)) {
        Ok(release) if release.is_newer_than(current) => CheckResult::Available(release),
        Ok(release) => CheckResult::UpToDate { latest: release.version },
        Err(e) => CheckResult::Failed(e),
    }
}

pub fn spawn_check() -> Receiver<CheckResult> {
    let (tx, rx) = channel();
    let spawned = std::thread::Builder::new().name("hp-update-check".into()).spawn({
        let tx = tx.clone();
        move || {
            crate::apply::cleanup_stale();
            let _ = tx.send(check(&api_url(), &current_version()));
        }
    });
    if let Err(e) = spawned {
        let _ = tx.send(CheckResult::Failed(UpdateError::Network(format!("cannot start the check: {e}"))));
    }
    rx
}

#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
}

impl Progress {
    pub fn fraction(&self) -> Option<f32> {
        let total = self.total.load(Ordering::Relaxed);
        (total > 0).then(|| (self.done.load(Ordering::Relaxed) as f64 / total as f64).clamp(0.0, 1.0) as f32)
    }
}

#[derive(Debug)]
pub struct Downloaded {
    pub zip: PathBuf,
    pub helper: Helper,
}

pub struct Download {
    pub progress: Arc<Progress>,
    pub rx: Receiver<Result<Downloaded, UpdateError>>,
}

fn safe_file_name(name: &str) -> String {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let clean: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).collect();
    if clean.is_empty() || clean.starts_with('.') { "update.zip".to_owned() } else { clean }
}

pub fn spawn_download(release: &Release) -> Result<Download, UpdateError> {
    let asset =
        release.pick_asset().ok_or_else(|| UpdateError::BadRelease("the release has no zip archive".into()))?.clone();
    let api = api_url();
    if !allowed_download_url(&asset.url, &api) {
        return Err(UpdateError::BadRelease(format!("the download address is not trusted: {}", asset.url)));
    }
    let dir =
        std::env::temp_dir().join(format!("HammerPro-update-{}-{}", std::process::id(), safe_file_name(&release.tag)));
    let zip = dir.join(safe_file_name(&asset.name));
    let progress = Arc::new(Progress::default());
    progress.total.store(asset.size, Ordering::Relaxed);
    let (tx, rx) = channel();
    let shared = progress.clone();
    std::thread::Builder::new()
        .name("hp-update-download".into())
        .spawn(move || {
            let _ = tx.send(download_and_verify(&asset.url, asset.size, &zip, &shared));
        })
        .map_err(|e| UpdateError::Network(format!("cannot start the download: {e}")))?;
    Ok(Download { progress, rx })
}

pub fn download_and_verify(
    url: &str,
    expected_size: u64,
    zip: &std::path::Path,
    progress: &Progress,
) -> Result<Downloaded, UpdateError> {
    let size = http::download_to(url, zip, &progress.cancel, &mut |done, total| {
        progress.done.store(done, Ordering::Relaxed);
        if let Some(total) = total {
            progress.total.store(total, Ordering::Relaxed);
        }
        if done > MAX_DOWNLOAD {
            progress.cancel.store(true, Ordering::Relaxed);
        }
    })?;
    if expected_size > 0 && size != expected_size {
        return Err(UpdateError::Network(format!("the file has {size} bytes, {expected_size} expected")));
    }
    let file = std::fs::File::open(zip)?;
    let archive = Archive::open(std::io::BufReader::new(file))?;
    if archive.find_file(PROGRAM_NAME).is_none() {
        return Err(UpdateError::BadZip(format!("{PROGRAM_NAME} is not in the archive")));
    }
    let helper = Helper::prepare()?;
    Ok(Downloaded { zip: zip.to_path_buf(), helper })
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use super::*;
    use crate::zip::fake::{Item, build};

    struct Fake {
        base: String,
        stop: Arc<AtomicBool>,
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            let _ = std::net::TcpStream::connect(self.base.trim_start_matches("http://"));
        }
    }

    fn serve(routes: Vec<(&'static str, &'static str, Vec<u8>)>) -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let served = base.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                if flag.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(mut conn) = conn else { continue };
                let mut request = [0u8; 4096];
                let n = conn.read(&mut request).unwrap_or(0);
                let head = String::from_utf8_lossy(&request[..n]).into_owned();
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let (status, kind, body) = match routes.iter().find(|(p, _, _)| *p == path) {
                    Some((_, kind, body)) if kind.contains("json") => {
                        ("200 OK", *kind, String::from_utf8_lossy(body).replace("{BASE}", &served).into_bytes())
                    }
                    Some((_, kind, body)) => ("200 OK", *kind, body.clone()),
                    None => ("404 Not Found", "text/plain", b"nope".to_vec()),
                };
                let header = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = conn.write_all(header.as_bytes());
                let _ = conn.write_all(&body);
            }
        });
        Fake { base, stop }
    }

    fn release_json(base: &str, tag: &str, zip_len: usize) -> Vec<u8> {
        serde_json::json!({
            "tag_name": tag,
            "name": format!("Hammer Pro {tag}"),
            "body": "- new things",
            "html_url": format!("{base}/release"),
            "assets": [{
                "name": format!("HammerPro-{}-windows.zip", tag.trim_start_matches('v')),
                "browser_download_url": format!("{base}/asset.zip"),
                "size": zip_len,
            }],
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn checks_a_local_fake_release_api() {
        let zip = build(&[Item { name: "hammerpro.exe", data: b"MZ-new", deflate: false }]);
        let fake = serve(vec![("/latest", "application/json", release_json("{BASE}", "v1.0.3", zip.len()))]);
        let api = format!("{}/latest", fake.base);
        let old = Version::parse("1.0.2").unwrap();
        match check(&api, &old) {
            CheckResult::Available(r) => {
                assert_eq!(r.tag, "v1.0.3");
                assert_eq!(r.notes, "- new things");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(check(&api, &Version::parse("1.0.3").unwrap()), CheckResult::UpToDate { .. }));
        assert!(matches!(check(&api, &Version::parse("9.0.0").unwrap()), CheckResult::UpToDate { .. }));
        assert!(matches!(check(&format!("{}/other", fake.base), &old), CheckResult::Failed(UpdateError::Http(404))));
    }

    #[test]
    fn offline_and_garbage_never_panic() {
        let closed = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}/latest", l.local_addr().unwrap())
        };
        let old = Version::parse("1.0.2").unwrap();
        assert!(matches!(check(&closed, &old), CheckResult::Failed(UpdateError::Network(_))));
        assert!(matches!(check("not a url", &old), CheckResult::Failed(UpdateError::Network(_))));
        let fake = serve(vec![("/junk", "text/html", b"<html>rate limited</html>".to_vec())]);
        assert!(matches!(check(&format!("{}/junk", fake.base), &old), CheckResult::Failed(UpdateError::BadRelease(_))));
    }

    #[test]
    fn downloads_verifies_and_applies_end_to_end() {
        let big: Vec<u8> = std::iter::once(b'M')
            .chain(std::iter::once(b'Z'))
            .chain((0..400_000u32).map(|i| (i % 253) as u8))
            .collect();
        let zip = build(&[
            Item { name: "HammerPro-1.0.3-windows/readme.txt", data: b"hi", deflate: false },
            Item { name: "HammerPro-1.0.3-windows/hammerpro.exe", data: &big, deflate: true },
        ]);
        let fake = serve(vec![
            ("/latest", "application/json", release_json("{BASE}", "v1.0.3", zip.len())),
            ("/asset.zip", "application/zip", zip.clone()),
        ]);
        let release = match check(&format!("{}/latest", fake.base), &Version::parse("1.0.2").unwrap()) {
            CheckResult::Available(r) => r,
            other => panic!("{other:?}"),
        };
        let asset = release.pick_asset().unwrap();
        let dir = std::env::var_os("HP_UPDATE_TEST_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("hp_update_e2e_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let zip_path = dir.join("dl").join("update.zip");
        let progress = Progress::default();
        let done = download_and_verify(&asset.url, asset.size, &zip_path, &progress).unwrap();
        assert_eq!(std::fs::read(&done.zip).unwrap(), zip);
        assert_eq!(progress.done.load(Ordering::Relaxed), zip.len() as u64);
        assert_eq!(progress.fraction(), Some(1.0));
        assert!(done.helper.exe.exists());
        let _ = std::fs::remove_dir_all(done.helper.exe.parent().unwrap());

        let target = dir.join("install").join("hammerpro.exe");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"MZ-old").unwrap();
        let mut job = crate::apply::ApplyJob::new(zip_path, target.clone(), 0);
        job.kill_by_name = false;
        crate::apply::apply(&job, &mut |_| {}).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), big);
        assert_eq!(std::fs::read(job.backup_path()).unwrap(), b"MZ-old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wrong_size_or_archive_is_refused() {
        let zip = build(&[Item { name: "readme.txt", data: b"no program here", deflate: false }]);
        let fake = serve(vec![
            ("/asset.zip", "application/zip", zip.clone()),
            ("/junk.zip", "application/zip", b"not a zip".to_vec()),
        ]);
        let dir = std::env::temp_dir().join(format!("hp_update_refuse_{}", std::process::id()));
        let progress = Progress::default();
        let url = format!("{}/asset.zip", fake.base);
        let wrong_size = download_and_verify(&url, zip.len() as u64 + 5, &dir.join("a.zip"), &progress);
        assert!(matches!(wrong_size, Err(UpdateError::Network(m)) if m.contains("expected")));
        let no_exe = download_and_verify(&url, zip.len() as u64, &dir.join("b.zip"), &progress);
        assert!(matches!(no_exe, Err(UpdateError::BadZip(_))));
        let junk = download_and_verify(&format!("{}/junk.zip", fake.base), 0, &dir.join("c.zip"), &progress);
        assert!(matches!(junk, Err(UpdateError::BadZip(_))));
        let missing = download_and_verify(&format!("{}/none.zip", fake.base), 0, &dir.join("d.zip"), &progress);
        assert!(matches!(missing, Err(UpdateError::Http(404))));
        assert!(!dir.join("d.part").exists() && !dir.join("d.zip").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancel_stops_the_download_and_leaves_nothing() {
        let body = vec![7u8; 300_000];
        let fake = serve(vec![("/big", "application/octet-stream", body)]);
        let dir = std::env::temp_dir().join(format!("hp_update_cancel_{}", std::process::id()));
        let progress = Progress::default();
        progress.cancel.store(true, Ordering::Relaxed);
        let err = download_and_verify(&format!("{}/big", fake.base), 0, &dir.join("big.zip"), &progress).unwrap_err();
        assert_eq!(err, UpdateError::Cancelled);
        assert!(!dir.join("big.zip").exists() && !dir.join("big.part").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn untrusted_download_addresses_are_refused_and_names_are_cleaned() {
        let release = Release {
            tag: "v1.0.3".into(),
            version: Version::parse("1.0.3").unwrap(),
            title: String::new(),
            notes: String::new(),
            page_url: String::new(),
            assets: vec![crate::release::Asset {
                name: "HammerPro-1.0.3-windows.zip".into(),
                url: "https://evil.example/HammerPro.zip".into(),
                size: 1,
            }],
        };
        assert!(matches!(spawn_download(&release), Err(UpdateError::BadRelease(m)) if m.contains("not trusted")));
        let no_asset = Release { assets: Vec::new(), ..release };
        assert!(matches!(spawn_download(&no_asset), Err(UpdateError::BadRelease(_))));
        assert_eq!(safe_file_name("..\\..\\evil.zip"), "evil.zip");
        assert_eq!(safe_file_name("../x"), "x");
        assert_eq!(safe_file_name(".."), "update.zip");
        assert_eq!(safe_file_name(""), "update.zip");
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::var_os("HP_UPDATE_TEST_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("hp_update_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_network_cut_mid_download_leaves_nothing() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/cut.zip", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Ok((mut conn, _)) = listener.accept() {
                let mut request = [0u8; 2048];
                let _ = conn.read(&mut request);
                let _ = conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 500000\r\nConnection: close\r\n\r\n");
                let _ = conn.write_all(&[1u8; 70_000]);
            }
        });
        let dir = scratch("cut");
        let progress = Progress::default();
        let err = download_and_verify(&url, 500_000, &dir.join("cut.zip"), &progress).unwrap_err();
        assert!(matches!(err, UpdateError::Network(_)), "{err:?}");
        assert!(!dir.join("cut.zip").exists() && !dir.join("cut.part").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancelling_while_bytes_flow_stops_and_cleans_up() {
        let fake = serve(vec![("/big", "application/octet-stream", vec![9u8; 3_000_000])]);
        let dir = scratch("cancel_mid");
        let cancel = AtomicBool::new(false);
        let mut seen = 0u64;
        let err = http::download_to(&format!("{}/big", fake.base), &dir.join("big.zip"), &cancel, &mut |done, _| {
            seen = done;
            if done > 100_000 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap_err();
        assert_eq!(err, UpdateError::Cancelled);
        assert!(seen > 0 && seen < 3_000_000);
        assert!(!dir.join("big.zip").exists() && !dir.join("big.part").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_truncated_archive_is_refused_before_anything_is_closed() {
        let zip = build(&[Item { name: "hammerpro.exe", data: &[b'M'; 5000], deflate: false }]);
        let cut = zip[..zip.len() - 30].to_vec();
        let fake = serve(vec![("/t.zip", "application/zip", cut.clone())]);
        let dir = scratch("trunc");
        let progress = Progress::default();
        let err = download_and_verify(&format!("{}/t.zip", fake.base), cut.len() as u64, &dir.join("t.zip"), &progress)
            .unwrap_err();
        assert!(matches!(err, UpdateError::BadZip(_)), "{err:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wrong_checksum_or_truncated_entry_never_replaces_the_program() {
        let dir = scratch("crc");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("hammerpro.exe");
        let data = b"MZ-new-program-bytes-0123456789".to_vec();
        let good = build(&[Item { name: "hammerpro.exe", data: &data, deflate: false }]);
        let at = good.windows(8).position(|w| w == b"MZ-new-p").unwrap();
        let mut bad_crc = good.clone();
        bad_crc[at + 10] ^= 0x01;
        let mut short = good.clone();
        let size_at = short.windows(4).rposition(|w| w == (data.len() as u32).to_le_bytes()).unwrap();
        short[size_at..size_at + 4].copy_from_slice(&((data.len() as u32) + 50).to_le_bytes());
        for (name, zip) in [("crc", bad_crc), ("short", short)] {
            std::fs::write(&target, b"MZ-old").unwrap();
            let zip_path = dir.join(format!("{name}.zip"));
            std::fs::write(&zip_path, zip).unwrap();
            let mut job = crate::apply::ApplyJob::new(zip_path, target.clone(), 0);
            job.kill_by_name = false;
            assert!(crate::apply::apply(&job, &mut |_| {}).is_err(), "{name}");
            assert_eq!(std::fs::read(&target).unwrap(), b"MZ-old", "{name}");
            assert!(!job.staged_path().exists() && !job.backup_path().exists(), "{name}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_asset_or_missing_program_is_refused() {
        let zip = build(&[Item { name: "readme.txt", data: b"x", deflate: false }]);
        let fake = serve(vec![("/a.zip", "application/zip", zip.clone())]);
        let dir = scratch("absent");
        let progress = Progress::default();
        let gone = download_and_verify(&format!("{}/nothing.zip", fake.base), 0, &dir.join("g.zip"), &progress);
        assert_eq!(gone.unwrap_err(), UpdateError::Http(404));
        let no_exe = download_and_verify(&format!("{}/a.zip", fake.base), 0, &dir.join("n.zip"), &progress);
        assert!(matches!(no_exe, Err(UpdateError::BadZip(m)) if m.contains("hammerpro.exe")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
