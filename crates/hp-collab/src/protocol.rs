use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::document::{Change, Snapshot};

pub const VERSION: u32 = 1;
pub const ALPN: &[u8] = b"hammerpro/collaboration/1";
pub const MAX_PARTICIPANTS: usize = 8;

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

pub fn random_secret() -> Result<[u8; 32]> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| anyhow::anyhow!("Operating system randomness unavailable"))?;
    Ok(bytes)
}

impl Invitation {
    pub fn encode(&self) -> Result<String> {
        Ok(format!("hpjoin1:{}", URL_SAFE_NO_PAD.encode(serde_json::to_vec(self)?)))
    }

    pub fn decode(text: &str) -> Result<Self> {
        ensure!(text.len() <= 2048, "Invalid invitation");
        let encoded = text.trim().strip_prefix("hpjoin1:").ok_or_else(|| anyhow::anyhow!("Invalid invitation"))?;
        let value: Self = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded)?)?;
        ensure!(value.version == VERSION && value.endpoint.len() <= 128, "Unsupported invitation");
        value.room.validate()?;
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

    pub fn credential(&self, password: &str) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(*blake3::keyed_hash(&self.token, password.as_bytes()).as_bytes())
    }
}

pub fn proof(key: &[u8; 32], challenge: &[u8; 32], host: &str, client: &str, name: &str) -> [u8; 32] {
    let mut message = blake3::Hasher::new_keyed(key);
    message.update(ALPN);
    message.update(challenge);
    for text in [host, client, name] {
        message.update(&(text.len() as u64).to_le_bytes());
        message.update(text.as_bytes());
    }
    *message.finalize().as_bytes()
}

pub fn verify_proof(expected: &[u8; 32], provided: &[u8; 32]) -> bool {
    bool::from(expected.ct_eq(provided))
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 12
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Presence {
    pub position: [f64; 3],
    pub direction: [f64; 3],
    pub selected: Vec<String>,
}

impl Presence {
    pub fn validate(&self) -> bool {
        self.position.iter().chain(&self.direction).all(|n| n.is_finite() && n.abs() <= 1_000_000.0)
            && self.selected.len() <= 2048
            && self.selected.iter().all(|key| key.len() <= 64)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Participant {
    pub id: u32,
    pub name: String,
    pub presence: Presence,
    pub latency_ms: u64,
}

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
        proof: [u8; 32],
    },
    Welcome {
        participant: u32,
        id_start: u32,
        revision: u64,
        map: Snapshot,
    },
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
    Rejected {
        sequence: u64,
        reason: String,
        revision: u64,
        map: Snapshot,
    },
    Presence(Presence),
    Participants(Vec<Participant>),
    Reset {
        revision: u64,
        map: Snapshot,
    },
    Ping(u64),
    Pong(u64),
    Closed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invitation_roundtrip_and_authentication_binds_all_context() {
        let invitation = Invitation {
            version: VERSION,
            endpoint: "host".into(),
            relay: None,
            token: random_secret().unwrap(),
            room: Room { name: "Test".into() },
            mode: ConnectionMode::Relay,
        };
        let decoded = Invitation::decode(&invitation.encode().unwrap()).unwrap();
        let key = decoded.credential("masked password");
        let nonce = random_secret().unwrap();
        let expected = proof(&key, &nonce, "host", "guest", "Alice");
        assert!(verify_proof(
            &expected,
            &proof(&invitation.credential("masked password"), &nonce, "host", "guest", "Alice")
        ));
        for actual in [
            proof(&decoded.credential("wrong"), &nonce, "host", "guest", "Alice"),
            proof(&key, &[0; 32], "host", "guest", "Alice"),
            proof(&key, &nonce, "other", "guest", "Alice"),
            proof(&key, &nonce, "host", "other", "Alice"),
            proof(&key, &nonce, "host", "guest", "Bob"),
        ] {
            assert!(!verify_proof(&expected, &actual));
        }
        assert!(Invitation::decode(&"x".repeat(4096)).is_err());
    }

    #[test]
    fn presence_and_names_are_bounded() {
        assert!(valid_name("Alice"));
        assert!(!valid_name("\nAlice"));
        assert!(!Presence { position: [f64::NAN, 0.0, 0.0], ..Presence::default() }.validate());
    }
}
