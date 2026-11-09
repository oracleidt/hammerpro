use std::net::IpAddr;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BanRow {
    pub id: u64,
    pub nickname: String,
    pub reason: String,
    pub remaining_seconds: Option<u64>,
}

struct Ban {
    id: u64,
    endpoint: String,
    ip: Option<IpAddr>,
    nickname: String,
    reason: String,
    until: Option<Instant>,
}

#[derive(Default)]
pub struct BanList {
    next_id: u64,
    entries: Vec<Ban>,
}

impl BanList {
    pub fn add(
        &mut self,
        endpoint: String,
        ip: Option<IpAddr>,
        nickname: String,
        reason: String,
        duration: Option<Duration>,
        now: Instant,
    ) {
        self.next_id += 1;
        self.entries.push(Ban {
            id: self.next_id,
            endpoint,
            ip,
            nickname,
            reason,
            until: duration.map(|duration| now + duration),
        });
    }

    pub fn remove(&mut self, id: u64) {
        self.entries.retain(|entry| entry.id != id);
    }

    pub fn denied(&mut self, endpoint: &str, ip: Option<IpAddr>, now: Instant) -> Option<String> {
        self.entries.retain(|entry| entry.until.is_none_or(|until| until > now));
        self.entries.iter().find(|entry| entry.endpoint == endpoint || (ip.is_some() && entry.ip == ip)).map(|entry| {
            let duration = entry.until.map_or_else(
                || "until this session ends".to_owned(),
                |until| format!("{} seconds remaining", until.saturating_duration_since(now).as_secs().max(1)),
            );
            format!("Banned from this session ({duration}): {}", entry.reason)
        })
    }

    pub fn rows(&mut self, now: Instant) -> Vec<BanRow> {
        self.entries.retain(|entry| entry.until.is_none_or(|until| until > now));
        self.entries
            .iter()
            .map(|entry| BanRow {
                id: entry.id,
                nickname: entry.nickname.clone(),
                reason: entry.reason.clone(),
                remaining_seconds: entry.until.map(|until| until.saturating_duration_since(now).as_secs().max(1)),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bans_match_installation_and_observed_ip_expire_and_can_be_removed() {
        let now = Instant::now();
        let ip = "192.0.2.10".parse().unwrap();
        let mut bans = BanList::default();
        bans.add("device".into(), Some(ip), "Alice".into(), "spam".into(), Some(Duration::from_secs(60)), now);
        assert!(bans.denied("device", None, now).unwrap().contains("spam"));
        assert!(bans.denied("other", Some(ip), now).is_some());
        assert!(bans.denied("other", None, now).is_none());
        assert!(bans.denied("device", Some(ip), now + Duration::from_secs(61)).is_none());
        bans.add("device".into(), None, "Alice".into(), "session ban".into(), None, now);
        let id = bans.rows(now)[0].id;
        bans.remove(id);
        assert!(bans.rows(now).is_empty());
        assert!(BanList::default().denied("device", Some(ip), now).is_none());
    }
}
