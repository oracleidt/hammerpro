use std::net::IpAddr;
use std::time::{Duration, Instant};

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_INVITATION_MINUTES: u32 = 10_080;
pub const MAX_BAN_SECONDS: u64 = 604_800;
pub const MAX_BANS: usize = 256;
const FREE_PASSWORD_FAILURES: u32 = 5;
const FAILURE_DECAY: Duration = Duration::from_secs(300);
const FIRST_LOCKOUT: Duration = Duration::from_secs(30);
const LONGEST_LOCKOUT: Duration = Duration::from_secs(900);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
        self.expire(now);
        if self.entries.len() >= MAX_BANS {
            self.entries.remove(0);
        }
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

    fn expire(&mut self, now: Instant) {
        self.entries.retain(|entry| entry.until.is_none_or(|until| until > now));
    }

    pub fn denied(&mut self, endpoint: &str, ip: Option<IpAddr>, now: Instant) -> Option<(String, Option<u64>)> {
        self.expire(now);
        self.entries.iter().find(|entry| entry.endpoint == endpoint || (ip.is_some() && entry.ip == ip)).map(|entry| {
            (entry.reason.clone(), entry.until.map(|until| until.saturating_duration_since(now).as_secs().max(1)))
        })
    }

    pub fn ip_denied(&mut self, ip: IpAddr, now: Instant) -> Option<(String, Option<u64>)> {
        self.expire(now);
        self.entries.iter().find(|entry| entry.ip == Some(ip)).map(|entry| {
            (entry.reason.clone(), entry.until.map(|until| until.saturating_duration_since(now).as_secs().max(1)))
        })
    }

    pub fn rows(&mut self, now: Instant) -> Vec<BanRow> {
        self.expire(now);
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AccessPolicy {
    pub locked: bool,
    pub approval: bool,
    pub read_only_guests: bool,
    pub invitation_minutes: u32,
    pub refuse_scripts: bool,
}

impl AccessPolicy {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.invitation_minutes <= MAX_INVITATION_MINUTES,
            "Invitation lifetime must be 0 to {MAX_INVITATION_MINUTES} minutes"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingRow {
    pub id: u32,
    pub name: String,
    pub waiting_seconds: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessState {
    pub policy: AccessPolicy,
    pub pending: Vec<PendingRow>,
    pub failed_passwords: u64,
    pub invalid_attempts: u64,
    pub refused_joins: u64,
    pub rate_limited: u64,
    pub resyncs: u64,
    pub lockout_seconds: Option<u64>,
    pub new_devices_seconds: Option<u64>,
    pub known_devices: usize,
}

pub struct PasswordLimiter {
    count: u32,
    since: Instant,
    locked_until: Option<Instant>,
}

impl PasswordLimiter {
    pub fn new(now: Instant) -> Self {
        Self { count: 0, since: now, locked_until: None }
    }

    fn decay(&mut self, now: Instant) {
        let steps = now.saturating_duration_since(self.since).as_secs() / FAILURE_DECAY.as_secs();
        if steps > 0 {
            self.count = self.count.saturating_sub(u32::try_from(steps).unwrap_or(u32::MAX));
            self.since += FAILURE_DECAY * u32::try_from(steps).unwrap_or(u32::MAX).min(1 << 16);
        }
        if self.count == 0 {
            self.since = now;
        }
    }

    pub fn locked(&mut self, now: Instant) -> Option<Duration> {
        self.decay(now);
        self.locked_until.map(|until| until.saturating_duration_since(now)).filter(|left| !left.is_zero())
    }

    pub fn fail(&mut self, now: Instant) {
        self.decay(now);
        self.count = self.count.saturating_add(1);
        if self.count >= FREE_PASSWORD_FAILURES {
            let doublings = (self.count - FREE_PASSWORD_FAILURES).min(5);
            let lockout = (FIRST_LOCKOUT * (1 << doublings)).min(LONGEST_LOCKOUT);
            self.locked_until = Some(now + lockout);
        }
    }

    pub fn failures(&self) -> u32 {
        self.count
    }
}

pub struct Window {
    start: Instant,
    count: u32,
}

impl Window {
    pub fn new(now: Instant) -> Self {
        Self { start: now, count: 0 }
    }

    pub fn allow(&mut self, now: Instant, limit: u32, period: Duration) -> bool {
        if now.saturating_duration_since(self.start) >= period {
            self.start = now;
            self.count = 0;
        }
        self.count = self.count.saturating_add(1);
        self.count <= limit
    }
}

pub struct Bucket {
    tokens: f64,
    capacity: f64,
    rate: f64,
    updated: Instant,
}

impl Bucket {
    pub fn new(now: Instant, capacity: f64, rate: f64) -> Self {
        Self { tokens: capacity, capacity, rate, updated: now }
    }

    pub fn charge(&mut self, now: Instant, amount: f64) -> Duration {
        let elapsed = now.saturating_duration_since(self.updated).as_secs_f64();
        self.updated = now;
        self.tokens = (self.tokens + elapsed * self.rate).min(self.capacity) - amount;
        if self.tokens >= 0.0 { Duration::ZERO } else { Duration::from_secs_f64(-self.tokens / self.rate) }
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
        assert_eq!(bans.denied("device", None, now).unwrap().0, "spam");
        assert!(bans.denied("other", Some(ip), now).is_some());
        assert!(bans.ip_denied(ip, now).is_some());
        assert!(bans.denied("other", None, now).is_none());
        assert!(bans.denied("device", Some(ip), now + Duration::from_secs(61)).is_none());
        bans.add("device".into(), None, "Alice".into(), "session ban".into(), None, now);
        assert_eq!(bans.denied("device", None, now), Some(("session ban".into(), None)));
        let id = bans.rows(now)[0].id;
        bans.remove(id);
        assert!(bans.rows(now).is_empty());
        assert!(BanList::default().denied("device", Some(ip), now).is_none());
        for index in 0..MAX_BANS + 10 {
            bans.add(format!("d{index}"), None, "N".into(), "r".into(), None, now);
        }
        assert_eq!(bans.rows(now).len(), MAX_BANS);
    }

    #[test]
    fn password_failures_lock_out_with_exponential_backoff_and_decay() {
        let start = Instant::now();
        let mut limiter = PasswordLimiter::new(start);
        for _ in 0..4 {
            limiter.fail(start);
            assert!(limiter.locked(start).is_none());
        }
        limiter.fail(start);
        assert_eq!(limiter.locked(start), Some(Duration::from_secs(30)));
        assert!(limiter.locked(start + Duration::from_secs(31)).is_none());
        limiter.fail(start + Duration::from_secs(31));
        assert_eq!(limiter.locked(start + Duration::from_secs(31)), Some(Duration::from_secs(60)));
        let mut later = start;
        for _ in 0..20 {
            later += Duration::from_secs(1);
            limiter.fail(later);
        }
        assert_eq!(limiter.locked(later), Some(LONGEST_LOCKOUT));
        let quiet = later + FAILURE_DECAY * 40;
        assert!(limiter.locked(quiet).is_none());
        assert_eq!(limiter.failures(), 0);
    }

    #[test]
    fn windows_and_pacing_buckets_refill() {
        let now = Instant::now();
        let mut window = Window::new(now);
        assert!((0..10).all(|_| window.allow(now, 10, Duration::from_secs(1))));
        assert!(!window.allow(now, 10, Duration::from_secs(1)));
        assert!(window.allow(now + Duration::from_secs(1), 10, Duration::from_secs(1)));
        let mut bucket = Bucket::new(now, 10.0, 5.0);
        assert_eq!(bucket.charge(now, 10.0), Duration::ZERO);
        assert_eq!(bucket.charge(now, 5.0), Duration::from_secs(1));
        let later = now + Duration::from_secs(1);
        assert_eq!(bucket.charge(later, 0.0), Duration::ZERO);
        assert_eq!(bucket.charge(later, 30.0), Duration::from_secs(6));
        assert_eq!(bucket.charge(later + Duration::from_secs(60), 1.0), Duration::ZERO, "refill stops at the burst");
        assert_eq!(bucket.charge(later + Duration::from_secs(60), 10.0), Duration::from_secs_f64(0.2));
        assert!(
            AccessPolicy { invitation_minutes: MAX_INVITATION_MINUTES + 1, ..Default::default() }.validate().is_err()
        );
    }
}
