pub const NONE: f32 = 0.0;
pub const NORM: f32 = 75.0;
pub const NORMAL_CLIP_DIST: f32 = 1000.0;
const REF_DB: f32 = 60.0;
const REF_DIST: f32 = 36.0;
const GAIN_COMP_THRESH: f32 = 0.59;
const GAIN_MIN: f32 = 0.01;

const NAMED: &[(&str, f32)] = &[
    ("SNDLVL_NONE", 0.0),
    ("SNDLVL_20dB", 20.0),
    ("SNDLVL_25dB", 25.0),
    ("SNDLVL_30dB", 30.0),
    ("SNDLVL_35dB", 35.0),
    ("SNDLVL_40dB", 40.0),
    ("SNDLVL_45dB", 45.0),
    ("SNDLVL_50dB", 50.0),
    ("SNDLVL_55dB", 55.0),
    ("SNDLVL_IDLE", 60.0),
    ("SNDLVL_60dB", 60.0),
    ("SNDLVL_65dB", 65.0),
    ("SNDLVL_STATIC", 66.0),
    ("SNDLVL_70dB", 70.0),
    ("SNDLVL_NORM", 75.0),
    ("SNDLVL_75dB", 75.0),
    ("SNDLVL_80dB", 80.0),
    ("SNDLVL_TALKING", 80.0),
    ("SNDLVL_85dB", 85.0),
    ("SNDLVL_90dB", 90.0),
    ("SNDLVL_95dB", 95.0),
    ("SNDLVL_100dB", 100.0),
    ("SNDLVL_105dB", 105.0),
    ("SNDLVL_110dB", 110.0),
    ("SNDLVL_120dB", 120.0),
    ("SNDLVL_130dB", 130.0),
    ("SNDLVL_GUNFIRE", 140.0),
    ("SNDLVL_140dB", 140.0),
    ("SNDLVL_150dB", 150.0),
    ("SNDLVL_180dB", 180.0),
];

pub fn names() -> impl Iterator<Item = (&'static str, f32)> {
    NAMED.iter().copied()
}

pub fn parse(text: &str) -> Option<f32> {
    let t = text.trim();
    if t.len() > 7 && t[..7].eq_ignore_ascii_case("SNDLVL_") {
        return NAMED.iter().find(|(n, _)| n.eq_ignore_ascii_case(t)).map(|(_, v)| *v);
    }
    let i = crate::script::Interval::parse(t)?;
    Some(i.mid().clamp(0.0, 255.0).trunc())
}

pub fn to_text(level: f32) -> String {
    let l = level.round();
    if l <= 0.0 {
        return "SNDLVL_NONE".to_owned();
    }
    match NAMED.iter().find(|(n, v)| (*v - l).abs() < 0.01 && n.ends_with("dB")) {
        Some((n, _)) => (*n).to_owned(),
        None => format!("{l:.0}"),
    }
}

pub fn from_attenuation(attenuation: f32) -> f32 {
    if attenuation > 0.0 && attenuation.is_finite() { (50.0 + 20.0 / attenuation).trunc() } else { NONE }
}

pub fn from_radius(radius: f32, everywhere: bool) -> f32 {
    if everywhere || radius <= 0.0 || !radius.is_finite() {
        return NONE;
    }
    from_attenuation(NORMAL_CLIP_DIST / radius)
}

pub fn dist_mult(level: f32) -> f32 {
    if level <= 0.0 { 0.0 } else { 10.0_f32.powf((REF_DB - level) / 20.0) / REF_DIST }
}

pub fn gain(volume: f32, level: f32, distance: f32) -> f32 {
    let volume = if volume.is_finite() { volume.max(0.0) } else { 0.0 };
    if volume <= 0.0 {
        return 0.0;
    }
    let mult = dist_mult(level);
    if mult <= 0.0 {
        return volume.min(1.0);
    }
    let relative = distance.max(0.0) * mult;
    let mut g = volume * if relative > 0.1 { 1.0 / relative } else { 10.0 };
    if g > GAIN_COMP_THRESH {
        let y = -1.0 / (GAIN_COMP_THRESH * (GAIN_COMP_THRESH - 1.0));
        g = 1.0 - 1.0 / (y * g);
    }
    if g < GAIN_MIN {
        g = GAIN_MIN * (2.0 - relative * GAIN_MIN);
        if g <= 0.0 {
            return 0.0;
        }
    }
    g
}

pub fn audible_distance(level: f32) -> Option<f32> {
    let mult = dist_mult(level);
    (mult > 0.0).then(|| 2.0 / (GAIN_MIN * mult))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_numbers_parse() {
        assert_eq!(parse("SNDLVL_NORM"), Some(75.0));
        assert_eq!(parse("sndlvl_70db"), Some(70.0));
        assert_eq!(parse("SNDLVL_WHAT"), None);
        assert_eq!(parse("85"), Some(85.0));
        assert_eq!(parse("80,90"), Some(85.0));
        assert_eq!(to_text(70.0), "SNDLVL_70dB");
        assert_eq!(to_text(0.0), "SNDLVL_NONE");
        assert_eq!(to_text(77.0), "77");
    }

    #[test]
    fn ambient_generic_radius_maps_to_a_sound_level() {
        assert_eq!(from_radius(1250.0, false), 75.0, "the default radius is SNDLVL_NORM");
        assert_eq!(from_radius(500.0, false), 60.0);
        assert_eq!(from_radius(1250.0, true), NONE, "play everywhere");
        assert_eq!(from_radius(0.0, false), NONE);
        assert_eq!(from_attenuation(1.0), 70.0);
    }

    #[test]
    fn gain_falls_with_distance_like_the_engine() {
        assert!((gain(0.5, NONE, 5000.0) - 0.5).abs() < 1e-6, "SNDLVL_NONE does not attenuate");
        let near = gain(1.0, 75.0, 16.0);
        let mid = gain(1.0, 75.0, 600.0);
        let far = gain(1.0, 75.0, 3000.0);
        assert!(near > mid && mid > far && far > 0.0, "{near} {mid} {far}");
        assert!(near <= 1.0);
        assert!(gain(1.0, 90.0, 600.0) > mid, "louder sound levels carry further");
        let edge = audible_distance(75.0).unwrap();
        assert_eq!(gain(1.0, 75.0, edge * 1.01), 0.0);
        assert!(gain(1.0, 75.0, edge * 0.9) > 0.0);
        assert_eq!(gain(f32::NAN, 75.0, 10.0), 0.0);
    }
}
