pub mod activity;
pub mod frame;
pub mod session;
pub mod transport;

pub use activity::{Activity, Button, Privacy, Status, Texts, build_activity, game_image_key};
pub use frame::{Frame, FrameError, Opcode};
pub use session::{Link, Options, Presence, Session, Wanted};
pub use transport::{Conn, Connector, SystemConnector};

pub const DEFAULT_APPLICATION_ID: &str = "1565701717675483247";

pub fn is_application_id(id: &str) -> bool {
    id.len() >= 17 && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())
}

pub fn application_id(setting: &str) -> Option<&str> {
    let id = setting.trim();
    let id = if id.is_empty() { DEFAULT_APPLICATION_ID } else { id };
    is_application_id(id).then_some(id)
}

#[cfg(test)]
mod tests {
    #[test]
    fn application_ids() {
        assert_eq!(super::application_id(" 1234567890123456789 "), Some("1234567890123456789"));
        assert_eq!(super::application_id("hammerpro"), None);
        assert_eq!(super::application_id("12345"), None);
        if super::DEFAULT_APPLICATION_ID.is_empty() {
            assert_eq!(super::application_id(""), None);
            assert_eq!(super::application_id("  "), None);
        } else {
            assert_eq!(super::application_id(""), Some(super::DEFAULT_APPLICATION_ID));
        }
    }

    #[test]
    fn default_application_id_is_empty_or_valid() {
        let id = super::DEFAULT_APPLICATION_ID;
        assert!(id.is_empty() || super::is_application_id(id), "{id}");
    }
}
