use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateError {
    Network(String),
    Http(u32),
    BadRelease(String),
    BadZip(String),
    Io(String),
    Cancelled,
    Apply(String),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpdateError::Network(why) => write!(f, "network error: {why}"),
            UpdateError::Http(code) => write!(f, "the server answered HTTP {code}"),
            UpdateError::BadRelease(why) => write!(f, "unusable release: {why}"),
            UpdateError::BadZip(why) => write!(f, "unusable archive: {why}"),
            UpdateError::Io(why) => write!(f, "file error: {why}"),
            UpdateError::Cancelled => write!(f, "cancelled"),
            UpdateError::Apply(why) => write!(f, "update failed: {why}"),
        }
    }
}

impl std::error::Error for UpdateError {}

impl From<std::io::Error> for UpdateError {
    fn from(e: std::io::Error) -> Self {
        UpdateError::Io(e.to_string())
    }
}
