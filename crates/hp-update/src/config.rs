use crate::http::Url;
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

pub const RELEASES_PAGE: &str = "https://github.com/oracleidt/hammerpro/releases";

pub fn is_github_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "github.com"
        || host == "api.github.com"
        || host == "githubusercontent.com"
        || host.ends_with(".githubusercontent.com")
}

pub fn is_loopback_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "localhost" || host.parse::<std::net::Ipv4Addr>().is_ok_and(|ip| ip.is_loopback())
}

pub fn trusted_url(target: &Url, api_url: &str) -> bool {
    if target.secure && is_github_host(&target.host) {
        return true;
    }
    if api_url == DEFAULT_API_URL {
        return false;
    }
    let Some(api) = Url::parse(api_url) else { return false };
    let same = api.host.eq_ignore_ascii_case(&target.host) && api.port == target.port;
    same && (target.secure || is_loopback_host(&target.host))
}

pub fn allowed_download_url(url: &str, api_url: &str) -> bool {
    Url::parse(url).is_some_and(|target| trusted_url(&target, api_url))
}

pub fn safe_page_url(page: &str, api_url: &str) -> String {
    match Url::parse(page) {
        Some(url) if trusted_url(&url, api_url) => page.trim().to_owned(),
        _ => RELEASES_PAGE.to_owned(),
    }
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

    #[test]
    fn plain_http_only_for_a_local_test_server() {
        assert!(!allowed_download_url("http://example.com:8080/x", "http://example.com:8080/api"));
        assert!(allowed_download_url("https://example.com:8443/x", "https://example.com:8443/api"));
        assert!(allowed_download_url("http://localhost:7000/x", "http://localhost:7000/api"));
        assert!(allowed_download_url("https://release-assets.githubusercontent.com/a", "http://127.0.0.1:9/api"));
        assert!(!allowed_download_url("http://release-assets.githubusercontent.com/a", DEFAULT_API_URL));
    }

    #[test]
    fn release_pages_open_only_on_trusted_hosts() {
        let api = DEFAULT_API_URL;
        let page = "https://github.com/oracleidt/hammerpro/releases/tag/v2.0.0";
        assert_eq!(safe_page_url(page, api), page);
        for bad in ["javascript:alert(1)", "file:///C:/Windows", "https://evil.example/x", "http://github.com/x", ""] {
            assert_eq!(safe_page_url(bad, api), RELEASES_PAGE, "{bad}");
        }
        assert_eq!(safe_page_url("http://127.0.0.1:9/release", "http://127.0.0.1:9/api"), "http://127.0.0.1:9/release");
    }
}
