use std::fmt;

use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::document::{Change, Snapshot};

pub const VERSION: u32 = 2;
pub const INVITATION_FORMAT: u32 = 1;
pub const LEGACY_VERSION: u32 = 1;
pub const ALPN: &[u8] = b"hammerpro/collaboration/2";
pub const LEGACY_ALPN: &[u8] = b"hammerpro/collaboration/1";
pub const MAX_PARTICIPANTS: usize = 16;
pub const MAX_REASON_CHARS: usize = 256;
pub const MAX_SELECTED: usize = 2048;
pub const MAX_COORDINATE: f64 = 1_000_000.0;
pub const MAX_OBJECT_ID: u32 = 2_000_000_000;
pub const MAX_LATENCY_MS: u64 = 600_000;
pub const MAX_DEVICE_CHARS: usize = 64;
pub const INVITATION_PREFIX: &str = "hpjoin1:";
pub const BINDING_LABEL: &[u8] = b"EXPORTER-hammerpro-team-mapping-2";
const INVITATION_CONTEXT: &str = "hammerpro 2026-11-09 team mapping invitation proof";
const PASSWORD_CONTEXT: &str = "hammerpro 2026-11-09 team mapping password credential";
pub(crate) const LEGACY_REFUSAL: &str = "This session uses Hammer Pro 2.0: update Hammer Pro to join. / Cette session utilise Hammer Pro 2.0 : mets à jour Hammer Pro pour la rejoindre. / Этот сеанс использует Hammer Pro 2.0: обновите Hammer Pro, чтобы подключиться.";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionMode {
    #[default]
    Relay,
    Direct,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Room {
    pub name: String,
}

impl Room {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.trim().is_empty()
                && self.name.len() <= 48
                && self.name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'_' | b'-')),
            "Session name must contain 1 to 48 ASCII letters, digits, spaces, _ or -"
        );
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invitation {
    pub version: u32,
    pub endpoint: String,
    pub relay: Option<String>,
    pub token: [u8; 32],
    pub room: Room,
    pub mode: ConnectionMode,
}

impl fmt::Debug for Invitation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Invitation")
            .field("version", &self.version)
            .field("endpoint", &self.endpoint)
            .field("relay", &self.relay)
            .field("room", &self.room)
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

pub fn random_secret() -> Result<[u8; 32]> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| anyhow::anyhow!("Operating system randomness unavailable"))?;
    Ok(bytes)
}

pub struct Keys {
    invitation: Zeroizing<[u8; 32]>,
    password: Zeroizing<[u8; 32]>,
}

pub struct Transcript<'a> {
    pub binding: &'a [u8; 32],
    pub nonce: &'a [u8; 32],
    pub host: &'a str,
    pub client: &'a str,
    pub name: &'a str,
    pub device: &'a str,
}

impl Transcript<'_> {
    fn digest(&self, key: &[u8; 32], purpose: &[u8]) -> [u8; 32] {
        let mut message = blake3::Hasher::new_keyed(key);
        message.update(ALPN);
        message.update(&(purpose.len() as u64).to_le_bytes());
        message.update(purpose);
        message.update(self.binding);
        message.update(self.nonce);
        for text in [self.host, self.client, self.name, self.device] {
            message.update(&(text.len() as u64).to_le_bytes());
            message.update(text.as_bytes());
        }
        *message.finalize().as_bytes()
    }
}

impl Keys {
    pub fn invitation_proof(&self, transcript: &Transcript<'_>) -> [u8; 32] {
        transcript.digest(&self.invitation, b"invitation")
    }

    pub fn password_proof(&self, transcript: &Transcript<'_>) -> [u8; 32] {
        transcript.digest(&self.password, b"password")
    }
}

fn version_of(bytes: &[u8]) -> Option<u64> {
    serde_json::from_slice::<serde_json::Value>(bytes).ok()?.get("version")?.as_u64()
}

impl Invitation {
    pub fn encode(&self) -> Result<String> {
        Ok(format!("{INVITATION_PREFIX}{}", URL_SAFE_NO_PAD.encode(serde_json::to_vec(self)?)))
    }

    pub fn decode(text: &str) -> Result<Self> {
        ensure!(text.len() <= 4096, "Invalid invitation");
        let text = text.trim();
        ensure!(text.len() <= 2048, "Invalid invitation");
        let encoded = text.strip_prefix(INVITATION_PREFIX).ok_or_else(|| anyhow::anyhow!("Invalid invitation"))?;
        let bytes = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| anyhow::anyhow!("Invalid invitation"))?;
        match version_of(&bytes) {
            Some(version) if version > u64::from(INVITATION_FORMAT) => {
                return Err(Refused::new(Refusal::InvitationNewer).into());
            }
            Some(version) if version == u64::from(INVITATION_FORMAT) => {}
            _ => anyhow::bail!("Invalid invitation"),
        }
        let value: Self = serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("Invalid invitation"))?;
        ensure!(
            value.endpoint.len() <= 128 && value.endpoint.parse::<iroh::EndpointId>().is_ok(),
            "Unsupported invitation"
        );
        value.room.validate()?;
        ensure!(
            (value.mode == ConnectionMode::Relay) == value.relay.is_some(),
            "Invitation connection mode and relay do not match"
        );
        if let Some(relay) = &value.relay {
            ensure!(relay.starts_with("https://") && relay.len() <= 512, "Invalid relay");
            let relay: iroh::RelayUrl = relay.parse()?;
            ensure!(
                iroh::RelayMode::Default.relay_map().contains(&relay),
                "Invitation references an unsupported relay"
            );
        }
        Ok(value)
    }

    pub fn keys(&self, password: &str) -> Keys {
        let invitation = Zeroizing::new(blake3::derive_key(INVITATION_CONTEXT, &self.token));
        let base = Zeroizing::new(blake3::derive_key(PASSWORD_CONTEXT, &self.token));
        let password = Zeroizing::new(*blake3::keyed_hash(&base, password.as_bytes()).as_bytes());
        Keys { invitation, password }
    }
}

pub fn verify_proof(expected: &[u8; 32], provided: &[u8; 32]) -> bool {
    bool::from(expected.ct_eq(provided))
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 12
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub const MAX_NOTES: usize = 4;
pub const MAX_NOTE_BYTES: usize = 240;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub serial: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<[f64; 3]>,
}

impl Note {
    pub fn validate(&self) -> bool {
        self.serial > 0
            && self.text.len() <= MAX_NOTE_BYTES
            && !self.text.chars().any(char::is_control)
            && (!self.text.trim().is_empty() || self.marker.is_some())
            && self.marker.is_none_or(|point| point.iter().all(|n| n.is_finite() && n.abs() <= MAX_COORDINATE))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    pub upload: u32,
    pub download: u32,
    pub round_trip_ms: u32,
}

impl Activity {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub fn valid_device(device: &str) -> bool {
    !device.is_empty() && device.len() <= MAX_DEVICE_CHARS && device.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

pub fn object_number(key: &str) -> Option<u32> {
    let digits = key.strip_prefix("solid:").or_else(|| key.strip_prefix("entity:"))?;
    if digits.is_empty() || digits.len() > 10 || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    digits.parse::<u32>().ok().filter(|id| *id < MAX_OBJECT_ID)
}

pub fn valid_selection_key(key: &str) -> bool {
    object_number(key).is_some()
}

pub fn valid_change_key(key: &str) -> bool {
    key == "world" || key == "metadata" || valid_selection_key(key)
}

pub fn sanitize(text: &str) -> String {
    sanitize_limit(text, MAX_REASON_CHARS)
}

pub fn sanitize_limit(text: &str, limit: usize) -> String {
    text.chars().take(limit).map(|character| if character.is_control() { ' ' } else { character }).collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Role {
    Host,
    #[default]
    Editor,
    Viewer,
}

impl Role {
    pub fn can_edit(self) -> bool {
        self != Role::Viewer
    }

    pub fn id(self) -> &'static str {
        match self {
            Role::Host => "host",
            Role::Editor => "editor",
            Role::Viewer => "viewer",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Presence {
    pub position: [f64; 3],
    pub direction: [f64; 3],
    pub selected: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,
    #[serde(default, skip_serializing_if = "Activity::is_empty")]
    pub activity: Activity,
}

impl Presence {
    pub fn validate(&self) -> bool {
        self.position.iter().chain(&self.direction).all(|n| n.is_finite() && n.abs() <= MAX_COORDINATE)
            && self.selected.len() <= MAX_SELECTED
            && self.selected.iter().all(|key| valid_selection_key(key))
            && self.notes.len() <= MAX_NOTES
            && self.notes.iter().all(Note::validate)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Participant {
    pub id: u32,
    pub name: String,
    pub role: Role,
    pub presence: Presence,
    pub latency_ms: u64,
}

impl Participant {
    pub fn validate(&self) -> bool {
        self.id > 0
            && (self.id == 1) == (self.role == Role::Host)
            && valid_name(&self.name)
            && self.presence.validate()
            && self.latency_ms <= MAX_LATENCY_MS
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    AuthenticationFailed,
    TooManyAttempts,
    Banned,
    Removed,
    Full,
    Revoked,
    NicknameTaken,
    Locked,
    Declined,
    ApprovalTimeout,
    RateLimited,
    Protocol,
    Ended,
    HostOutdated,
    HostNewer,
    InvitationNewer,
    Slow,
    Replaced,
    Busy,
}

pub const CLOSE_REFUSAL_BASE: u32 = 16;

impl Refusal {
    pub const ALL: [Refusal; 19] = [
        Refusal::AuthenticationFailed,
        Refusal::TooManyAttempts,
        Refusal::Banned,
        Refusal::Removed,
        Refusal::Full,
        Refusal::Revoked,
        Refusal::NicknameTaken,
        Refusal::Locked,
        Refusal::Declined,
        Refusal::ApprovalTimeout,
        Refusal::RateLimited,
        Refusal::Protocol,
        Refusal::Ended,
        Refusal::HostOutdated,
        Refusal::HostNewer,
        Refusal::InvitationNewer,
        Refusal::Slow,
        Refusal::Replaced,
        Refusal::Busy,
    ];

    pub fn close_code(self) -> u32 {
        CLOSE_REFUSAL_BASE + Self::ALL.iter().position(|refusal| *refusal == self).unwrap_or(0) as u32
    }

    pub fn from_close_code(code: u64) -> Option<Self> {
        let index = code.checked_sub(u64::from(CLOSE_REFUSAL_BASE))?;
        Self::ALL.get(usize::try_from(index).ok()?).copied()
    }

    pub fn id(self) -> &'static str {
        match self {
            Refusal::AuthenticationFailed => "authentication_failed",
            Refusal::TooManyAttempts => "too_many_attempts",
            Refusal::Banned => "banned",
            Refusal::Removed => "removed",
            Refusal::Full => "full",
            Refusal::Revoked => "revoked",
            Refusal::NicknameTaken => "nickname_taken",
            Refusal::Locked => "locked",
            Refusal::Declined => "declined",
            Refusal::ApprovalTimeout => "approval_timeout",
            Refusal::RateLimited => "rate_limited",
            Refusal::Protocol => "protocol",
            Refusal::Ended => "ended",
            Refusal::HostOutdated => "host_outdated",
            Refusal::HostNewer => "host_newer",
            Refusal::InvitationNewer => "invitation_newer",
            Refusal::Slow => "slow",
            Refusal::Replaced => "replaced",
            Refusal::Busy => "busy",
        }
    }

    fn english(self) -> &'static str {
        match self {
            Refusal::AuthenticationFailed => "Invitation or password rejected",
            Refusal::TooManyAttempts => "Too many wrong passwords for this session. Try again in {0} seconds",
            Refusal::Banned => "Banned from this session ({0}): {1}",
            Refusal::Removed => "Removed by the host",
            Refusal::Full => "Session full",
            Refusal::Revoked => "Access revoked: ask the host for the new invitation",
            Refusal::NicknameTaken => "Nickname already in use",
            Refusal::Locked => "The host closed this session to new participants",
            Refusal::Declined => "The host declined your request to join",
            Refusal::ApprovalTimeout => "The host did not answer your request to join",
            Refusal::RateLimited => "Disconnected: too many messages",
            Refusal::Protocol => "Invalid session message",
            Refusal::Ended => "The host ended the session",
            Refusal::HostOutdated => {
                "The host uses an older Hammer Pro (1.x). Both computers need Hammer Pro 2.0 or newer"
            }
            Refusal::HostNewer => "The host uses a newer Hammer Pro. Update Hammer Pro to join",
            Refusal::InvitationNewer => "This invitation needs a newer Hammer Pro. Update Hammer Pro to join",
            Refusal::Slow => "Disconnected: the connection could not keep up with the session",
            Refusal::Replaced => "Disconnected: this installation joined the session again from another connection",
            Refusal::Busy => "The host is busy with other connections from your network. Try again in a moment",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditRefusal {
    ReadOnly,
    InvalidEdit,
    InvalidKey,
    Selected,
    OutsideRange,
    SideOutsideRange,
    TooLarge,
    Document,
}

impl EditRefusal {
    pub const ALL: [EditRefusal; 8] = [
        EditRefusal::ReadOnly,
        EditRefusal::InvalidEdit,
        EditRefusal::InvalidKey,
        EditRefusal::Selected,
        EditRefusal::OutsideRange,
        EditRefusal::SideOutsideRange,
        EditRefusal::TooLarge,
        EditRefusal::Document,
    ];

    pub fn id(self) -> &'static str {
        match self {
            EditRefusal::ReadOnly => "read_only",
            EditRefusal::InvalidEdit => "invalid_edit",
            EditRefusal::InvalidKey => "invalid_key",
            EditRefusal::Selected => "selected",
            EditRefusal::OutsideRange => "outside_range",
            EditRefusal::SideOutsideRange => "side_outside_range",
            EditRefusal::TooLarge => "too_large",
            EditRefusal::Document => "document",
        }
    }

    pub fn english(self) -> &'static str {
        match self {
            EditRefusal::ReadOnly => "You are read only in this session",
            EditRefusal::InvalidEdit => "Invalid edit",
            EditRefusal::InvalidKey => "Invalid object key",
            EditRefusal::Selected => "Object selected by another participant",
            EditRefusal::OutsideRange => "Object identifier is outside the participant range",
            EditRefusal::SideOutsideRange => "Side identifier is outside the participant range",
            EditRefusal::TooLarge => "Edit too large to share",
            EditRefusal::Document => "Map change refused",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rejection {
    pub code: EditRefusal,
    pub detail: String,
}

impl Rejection {
    pub fn new(code: EditRefusal, detail: impl AsRef<str>) -> Self {
        Self { code, detail: sanitize(detail.as_ref()) }
    }

    pub fn sanitized(self) -> Self {
        Self::new(self.code, &self.detail)
    }

    pub fn too_large(bytes: usize, limit: usize) -> Self {
        let mib = |value: usize| value.div_ceil(1024 * 1024);
        Self::new(EditRefusal::TooLarge, format!("{} MiB, limit {} MiB", mib(bytes), mib(limit)))
    }
}

impl fmt::Display for Rejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.detail.is_empty() {
            formatter.write_str(self.code.english())
        } else {
            write!(formatter, "{}: {}", self.code.english(), self.detail)
        }
    }
}

impl std::error::Error for Rejection {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub sequence: u64,
    pub rejection: Option<Rejection>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refused {
    pub refusal: Refusal,
    pub detail: String,
    pub seconds: Option<u64>,
}

impl Refused {
    pub fn new(refusal: Refusal) -> Self {
        Self { refusal, detail: String::new(), seconds: None }
    }

    pub fn with(refusal: Refusal, detail: &str, seconds: Option<u64>) -> Self {
        Self { refusal, detail: sanitize(detail), seconds: seconds.map(|seconds| seconds.min(31_622_400)) }
    }

    pub fn sanitized(self) -> Self {
        Self::with(self.refusal, &self.detail, self.seconds)
    }

    pub fn format(&self, template: &str, until_session_end: &str) -> String {
        let duration = self.seconds.map_or_else(|| until_session_end.to_owned(), |seconds| format!("{seconds} s"));
        let seconds = self.seconds.unwrap_or(0).to_string();
        let first = if self.refusal == Refusal::Banned { duration } else { seconds };
        template.replace("{0}", &first).replace("{1}", &self.detail)
    }
}

impl fmt::Display for Refused {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.format(self.refusal.english(), "until the session ends"))
    }
}

impl std::error::Error for Refused {}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum Wire {
    Hello {
        version: u32,
    },
    Challenge {
        nonce: [u8; 32],
    },
    Authenticate {
        version: u32,
        name: String,
        session_name: String,
        device: String,
        signature: Vec<u8>,
        invitation: [u8; 32],
        proof: [u8; 32],
    },
    Waiting,
    MapStart {
        participant: u32,
        id_start: u32,
        revision: u64,
        units: u64,
        bytes: u64,
    },
    MapPart(Snapshot),
    MapEnd,
    Edit {
        sequence: u64,
        changes: Vec<Change>,
    },
    Applied {
        author: u32,
        sequence: u64,
        revision: u64,
        changes: Vec<Change>,
    },
    Acked {
        sequence: u64,
        revision: u64,
    },
    Rejected {
        sequence: u64,
        rejection: Rejection,
        revision: u64,
    },
    Resync {
        revision: u64,
        outcomes: Vec<Outcome>,
        units: u64,
        bytes: u64,
    },
    Presence(Presence),
    Participants(Vec<Participant>),
    Reset {
        revision: u64,
        units: u64,
        bytes: u64,
    },
    Ping(u64),
    Pong(u64),
    Closed(Refused),
}

#[derive(Serialize)]
pub(crate) enum LegacyWire {
    Challenge { nonce: [u8; 32] },
    Closed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invitation() -> Invitation {
        Invitation {
            version: INVITATION_FORMAT,
            endpoint: iroh::SecretKey::from_bytes(&[7; 32]).public().to_string(),
            relay: None,
            token: random_secret().unwrap(),
            room: Room { name: "Test".into() },
            mode: ConnectionMode::Direct,
        }
    }

    #[test]
    fn invitation_roundtrip_and_proofs_bind_all_context() {
        let invitation = invitation();
        let decoded = Invitation::decode(&invitation.encode().unwrap()).unwrap();
        let keys = decoded.keys("masked password");
        let (binding, nonce) = (random_secret().unwrap(), random_secret().unwrap());
        let host = invitation.endpoint.clone();
        let transcript =
            Transcript { binding: &binding, nonce: &nonce, host: &host, client: "guest", name: "Alice", device: "d" };
        let expected = keys.password_proof(&transcript);
        assert!(verify_proof(&expected, &invitation.keys("masked password").password_proof(&transcript)));
        assert_ne!(keys.invitation_proof(&transcript), expected);
        let other = [9; 32];
        for changed in [
            Transcript { binding: &other, ..transcript },
            Transcript { nonce: &other, ..transcript },
            Transcript { host: "other", ..transcript },
            Transcript { client: "other", ..transcript },
            Transcript { name: "Bob", ..transcript },
            Transcript { device: "e", ..transcript },
        ] {
            assert!(!verify_proof(&expected, &keys.password_proof(&changed)));
        }
        assert!(!verify_proof(&expected, &decoded.keys("wrong").password_proof(&transcript)));
        assert_eq!(
            decoded.keys("wrong").invitation_proof(&transcript),
            decoded.keys("masked password").invitation_proof(&transcript)
        );
        assert!(Invitation::decode(&"x".repeat(4096)).is_err());
    }

    #[test]
    fn invitations_of_other_versions_give_clean_refusals() {
        let encode = |value: &serde_json::Value| {
            format!("{INVITATION_PREFIX}{}", URL_SAFE_NO_PAD.encode(serde_json::to_vec(value).unwrap()))
        };
        let mut value = serde_json::to_value(invitation()).unwrap();
        value["version"] = 2.into();
        value["future"] = true.into();
        let error = Invitation::decode(&encode(&value)).unwrap_err();
        assert_eq!(error.downcast_ref::<Refused>().map(|refused| refused.refusal), Some(Refusal::InvitationNewer));
        value["version"] = 0.into();
        assert!(Invitation::decode(&encode(&value)).unwrap_err().downcast_ref::<Refused>().is_none());
        let mut relay = invitation();
        relay.mode = ConnectionMode::Relay;
        assert!(Invitation::decode(&relay.encode().unwrap()).is_err());
        let mut bad = invitation();
        bad.endpoint = "not a key".into();
        assert!(Invitation::decode(&bad.encode().unwrap()).is_err());
        assert!(!format!("{:?}", invitation()).contains("token"));
    }

    #[test]
    fn presence_names_keys_and_reasons_are_bounded() {
        assert!(valid_name("Alice"));
        assert!(!valid_name("\nAlice"));
        assert!(!Presence { position: [f64::NAN, 0.0, 0.0], ..Presence::default() }.validate());
        assert!(!Presence { direction: [0.0, f64::INFINITY, 0.0], ..Presence::default() }.validate());
        for key in ["solid:1", "entity:1999999999"] {
            assert!(valid_selection_key(key), "{key}");
        }
        for key in ["world", "metadata", "solid:0", "solid:01", "solid:+1", "solid:2000000000", "entity:", "x:1"] {
            assert!(!valid_selection_key(key), "{key}");
        }
        assert!(valid_change_key("world") && valid_change_key("metadata") && !valid_change_key("World"));
        let selected = vec!["world".to_owned()];
        assert!(!Presence { selected, ..Presence::default() }.validate());
        let reason = sanitize(&format!("a\nb{}", "x".repeat(1000)));
        assert_eq!(reason.chars().count(), MAX_REASON_CHARS);
        assert!(reason.starts_with("a b"));
        let refused = Refused::with(Refusal::Banned, "spam", Some(5));
        assert_eq!(refused.to_string(), "Banned from this session (5 s): spam");
        assert_eq!(Refused::new(Refusal::Banned).to_string(), "Banned from this session (until the session ends): ");
    }

    #[test]
    fn participant_roles_are_consistent() {
        let row = |id, role| Participant { id, name: "A".into(), role, presence: Presence::default(), latency_ms: 0 };
        assert!(row(1, Role::Host).validate());
        assert!(row(2, Role::Viewer).validate());
        assert!(!row(2, Role::Host).validate());
        assert!(!row(1, Role::Editor).validate());
        assert!(!row(0, Role::Editor).validate());
    }

    #[test]
    fn notes_and_activity_are_bounded_and_omitted_when_empty() {
        let quiet = serde_json::to_string(&Presence::default()).unwrap();
        assert!(!quiet.contains("notes") && !quiet.contains("activity"), "{quiet}");
        let note = |text: &str, marker: Option<[f64; 3]>| Note { serial: 1, text: text.into(), marker };
        let presence = |notes: Vec<Note>| Presence { notes, ..Presence::default() };
        assert!(presence(vec![note("hello", None)]).validate());
        assert!(presence(vec![note("", Some([1.0, 2.0, 3.0]))]).validate());
        assert!(!presence(vec![note("", None)]).validate());
        assert!(!presence(vec![note("a\nb", None)]).validate());
        assert!(!presence(vec![note(&"x".repeat(MAX_NOTE_BYTES + 1), None)]).validate());
        assert!(!presence(vec![note("", Some([f64::INFINITY, 0.0, 0.0]))]).validate());
        assert!(!presence(vec![Note { serial: 0, ..note("x", None) }]).validate());
        assert!(!presence(vec![note("x", None); MAX_NOTES + 1]).validate());
        let busy = Presence {
            notes: vec![note("hi", Some([0.0; 3]))],
            activity: Activity { upload: 10, download: 20, round_trip_ms: 30 },
            ..Presence::default()
        };
        let text = serde_json::to_string(&busy).unwrap();
        assert_eq!(serde_json::from_str::<Presence>(&text).unwrap(), busy);
        assert!(serde_json::from_str::<Presence>(&text.replace("\"serial\"", "\"other\"")).is_err());
    }
}
