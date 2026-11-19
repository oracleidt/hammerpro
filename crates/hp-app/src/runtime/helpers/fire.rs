use glam::DVec3;
use hp_vmf::Entity;

pub const SF_INFINITE: i64 = 1;
pub const SF_SMOKELESS: i64 = 2;
pub const SF_START_ON: i64 = 4;
pub const SF_START_FULL: i64 = 8;
pub const SF_NO_GLOW: i64 = 32;
pub const DEFAULT_SIZE: f64 = 64.0;
pub const DEFAULT_ATTACK: f64 = 4.0;
pub const DEFAULT_HEALTH: f64 = 30.0;
pub const GLOW_COLOR: [f32; 3] = [1.0, 100.0 / 255.0, 10.0 / 255.0];
const MAX_SIZE: f64 = 4096.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireKind {
    Natural,
    Plasma,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fire {
    pub origin: DVec3,
    pub size: f64,
    pub attack: f64,
    pub health: f64,
    pub kind: FireKind,
    pub smoke: bool,
    pub glow: bool,
    pub start_on: bool,
    pub start_full: bool,
    pub infinite: bool,
}

pub fn is_fire(class: &str) -> bool {
    class.trim().eq_ignore_ascii_case("env_fire")
}

pub fn is_fire_trail(class: &str) -> bool {
    class.trim().eq_ignore_ascii_case("env_fire_trail")
}

fn number(e: &Entity, key: &str) -> Option<f64> {
    e.get(key).and_then(|s| s.split_ascii_whitespace().next()?.parse::<f64>().ok()).filter(|v| v.is_finite())
}

pub fn fire(e: &Entity) -> Option<Fire> {
    if !is_fire(&e.classname) {
        return None;
    }
    let origin = e.origin().filter(|o| o.is_finite())?;
    let flags = number(e, "spawnflags").unwrap_or(0.0) as i64;
    let kind = if number(e, "firetype").is_some_and(|t| t >= 0.5) { FireKind::Plasma } else { FireKind::Natural };
    Some(Fire {
        origin,
        size: number(e, "firesize").unwrap_or(DEFAULT_SIZE).clamp(0.0, MAX_SIZE),
        attack: number(e, "fireattack").unwrap_or(DEFAULT_ATTACK).clamp(0.0, 3600.0),
        health: number(e, "health").unwrap_or(DEFAULT_HEALTH).clamp(0.0, 1.0e6),
        kind,
        smoke: flags & SF_SMOKELESS == 0,
        glow: flags & SF_NO_GLOW == 0,
        start_on: flags & SF_START_ON != 0,
        start_full: flags & SF_START_FULL != 0,
        infinite: flags & SF_INFINITE != 0,
    })
}

pub fn particle_system(f: &Fire) -> Option<&'static str> {
    if f.kind != FireKind::Natural || f.size <= 0.0 {
        return None;
    }
    let tier = (f.size / 36.0).floor() as i64;
    Some(match (tier, f.smoke) {
        (..=0, false) => "env_fire_tiny",
        (..=0, true) => "env_fire_tiny_smoke",
        (1, false) => "env_fire_small",
        (1, true) => "env_fire_small_smoke",
        (2, false) => "env_fire_medium",
        (2, true) => "env_fire_medium_smoke",
        (_, false) => "env_fire_large",
        (_, true) => "env_fire_large_smoke",
    })
}

pub fn glow_radius(f: &Fire) -> f64 {
    (f.size * 4.0).clamp(64.0, 2048.0)
}

pub fn glow_flicker(seed: u32, time: f64) -> f32 {
    let p = f64::from(seed % 997) * 0.37;
    let w = (time * 11.0 + p).sin() * 0.5 + (time * 23.0 + p * 1.7).sin() * 0.3 + (time * 37.0 + p * 2.3).sin() * 0.2;
    (0.85 + 0.15 * w) as f32
}

pub fn lit(start_on: bool, id: hp_vmf::Id, states: &[(hp_vmf::Id, bool)], playing: bool) -> bool {
    if !playing {
        return true;
    }
    states.iter().find(|(i, _)| *i == id).map_or(start_on, |(_, on)| *on)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(keys: &[(&str, &str)]) -> Entity {
        let mut e = Entity { classname: "env_fire".to_owned(), ..Default::default() };
        e.set("origin", "0 0 0");
        for (k, v) in keys {
            e.set(k, *v);
        }
        e
    }

    #[test]
    fn keys_and_flags_follow_the_game() {
        let f = fire(&ent(&[])).expect("fire");
        assert_eq!((f.size, f.attack, f.health), (64.0, 4.0, 30.0));
        assert!(f.smoke && f.glow && !f.start_on && !f.infinite && !f.start_full);
        assert_eq!(f.kind, FireKind::Natural);
        let f = fire(&ent(&[("spawnflags", "47"), ("firetype", "1"), ("firesize", "200")])).expect("fire");
        assert!(!f.smoke && !f.glow && f.start_on && f.infinite && f.start_full);
        assert_eq!((f.kind, f.size), (FireKind::Plasma, 200.0));
        assert!(fire(&Entity { classname: "env_firesource".into(), ..Default::default() }).is_none());
    }

    #[test]
    fn the_size_picks_the_engine_particle_system() {
        let sys = |size: &str, flags: &str| {
            particle_system(&fire(&ent(&[("firesize", size), ("spawnflags", flags)])).unwrap())
        };
        assert_eq!(sys("20", "0"), Some("env_fire_tiny_smoke"));
        assert_eq!(sys("64", "2"), Some("env_fire_small"));
        assert_eq!(sys("64", "0"), Some("env_fire_small_smoke"));
        assert_eq!(sys("80", "2"), Some("env_fire_medium"));
        assert_eq!(sys("128", "0"), Some("env_fire_large_smoke"));
        assert_eq!(sys("0", "0"), None);
        assert_eq!(particle_system(&fire(&ent(&[("firetype", "1")])).unwrap()), None);
    }

    #[test]
    fn state_follows_the_player_view_inputs() {
        assert!(lit(false, 3, &[], false), "editor views always preview the fire");
        assert!(!lit(false, 3, &[], true));
        assert!(lit(true, 3, &[], true));
        assert!(lit(false, 3, &[(3, true)], true), "StartFire");
        assert!(!lit(true, 3, &[(3, false)], true), "Extinguish");
        let a = glow_flicker(5, 1.0);
        assert!((0.7..=1.0).contains(&a) && a != glow_flicker(5, 1.3));
    }
}
