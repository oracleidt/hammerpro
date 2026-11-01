use glam::DVec3;

use super::clean_vec;
use crate::document::Document;
use crate::ids::{IdSet, id_set};
use crate::selection::ObjRef;
use crate::{EditError, EditResult, Id};

pub const MAX_JITTER_DEG: f64 = 45.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandomRotate {
    pub min_yaw: f64,
    pub max_yaw: f64,
    pub snap: f64,
    pub jitter: f64,
    pub seed: u64,
}

impl Default for RandomRotate {
    fn default() -> Self {
        Self { min_yaw: 0.0, max_yaw: 360.0, snap: 0.0, jitter: 0.0, seed: 1 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RandomRotateReport {
    pub rotated: usize,
    pub skipped: usize,
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range(&mut self, min: f64, max: f64) -> f64 {
        min + (max - min) * self.next()
    }
}

impl RandomRotate {
    pub fn check(&self) -> EditResult<()> {
        let finite = [self.min_yaw, self.max_yaw, self.snap, self.jitter].iter().all(|v| v.is_finite());
        if !finite {
            return Err(EditError::Invalid("the random rotation values must be numbers".into()));
        }
        if self.min_yaw > self.max_yaw {
            return Err(EditError::Invalid("the minimum yaw is above the maximum yaw".into()));
        }
        if self.max_yaw - self.min_yaw > 360.0 {
            return Err(EditError::Invalid("the yaw range is wider than 360 degrees".into()));
        }
        if !(0.0..=180.0).contains(&self.snap) {
            return Err(EditError::Invalid("the snap step is from 0 (free) to 180 degrees".into()));
        }
        if !(0.0..=MAX_JITTER_DEG).contains(&self.jitter) {
            return Err(EditError::Invalid(format!("the pitch and roll jitter is from 0 to {MAX_JITTER_DEG} degrees")));
        }
        Ok(())
    }

    fn yaw(&self, rng: &mut Rng) -> f64 {
        let raw = rng.range(self.min_yaw, self.max_yaw);
        if self.snap < 1e-6 {
            return raw;
        }
        let lo = (self.min_yaw / self.snap).ceil();
        let hi = (self.max_yaw / self.snap).floor();
        if hi < lo {
            return (raw / self.snap).round() * self.snap;
        }
        let full_turn = self.max_yaw - self.min_yaw >= 360.0 - 1e-9;
        let hi = if full_turn && ((hi - lo) * self.snap) >= 360.0 - 1e-9 { hi - 1.0 } else { hi };
        let steps = (hi - lo) as u64 + 1;
        let pick = ((rng.next() * steps as f64) as u64).min(steps - 1);
        (lo + pick as f64) * self.snap
    }
}

fn wrap(deg: f64) -> f64 {
    let w = deg.rem_euclid(360.0);
    if (w - 360.0).abs() < 1e-9 { 0.0 } else { w }
}

fn tidy(v: f64) -> f64 {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { 0.0 } else { r }
}

impl Document {
    pub fn random_rotate(&mut self, objs: &[ObjRef], opts: &RandomRotate) -> EditResult<RandomRotateReport> {
        opts.check()?;
        let mut seen: IdSet = id_set(objs.len());
        let mut targets: Vec<(Id, DVec3)> = Vec::new();
        let mut skipped = 0;
        for o in objs {
            let id = match *o {
                ObjRef::Entity(id) => id,
                ObjRef::Solid(s) => match self.scene.object_of_solid(s) {
                    ObjRef::Entity(id) => id,
                    ObjRef::Solid(_) => {
                        skipped += 1;
                        continue;
                    }
                },
            };
            if !seen.insert(id) {
                continue;
            }
            let Some(node) = self.scene.entities.get(&id) else {
                skipped += 1;
                continue;
            };
            let angles = node.entity.angles().filter(|a| a.is_finite());
            match angles {
                Some(a) if !self.scene.is_brush_entity(id) => targets.push((id, a)),
                _ => skipped += 1,
            }
        }
        targets.sort_by_key(|(id, _)| *id);
        let mut rng = Rng(opts.seed);
        let updates: Vec<(Id, DVec3)> = targets
            .into_iter()
            .map(|(id, a)| {
                let yaw = wrap(a.y + opts.yaw(&mut rng));
                let (pitch, roll) = if opts.jitter > 0.0 {
                    (a.x + rng.range(-opts.jitter, opts.jitter), a.z + rng.range(-opts.jitter, opts.jitter))
                } else {
                    (a.x, a.z)
                };
                (id, clean_vec(DVec3::new(tidy(pitch), tidy(yaw), tidy(roll))))
            })
            .collect();
        let rotated = updates.len();
        if rotated == 0 {
            return Ok(RandomRotateReport { rotated, skipped });
        }
        self.tx("Random rotation", |doc| {
            for (id, a) in updates {
                doc.update_entity(id, |e| e.set("angles", hp_vmf::fmt_vec3(a)));
            }
            Ok(())
        })?;
        Ok(RandomRotateReport { rotated, skipped })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: &str = r#"world { "id" "1" "classname" "worldspawn"
solid { "id" "9"
side { "id" "1" "plane" "(0 64 64) (64 64 64) (64 0 64)" "material" "A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
side { "id" "2" "plane" "(0 0 0) (64 0 0) (64 64 0)" "material" "A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
side { "id" "3" "plane" "(0 64 64) (0 0 64) (0 0 0)" "material" "A" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
side { "id" "4" "plane" "(64 64 0) (64 0 0) (64 0 64)" "material" "A" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
side { "id" "5" "plane" "(64 64 64) (0 64 64) (0 64 0)" "material" "A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
side { "id" "6" "plane" "(64 0 0) (0 0 0) (0 0 64)" "material" "A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
} }
entity { "id" "2" "classname" "prop_static" "model" "models/a.mdl" "angles" "0 0 0" "origin" "0 0 0" }
entity { "id" "3" "classname" "prop_physics" "model" "models/b.mdl" "angles" "10 30 0" "origin" "64 0 0" }
entity { "id" "4" "classname" "info_target" "origin" "5 5 5" }
entity { "id" "5" "classname" "prop_dynamic" "model" "models/c.mdl" "angles" "0 90 0" "origin" "128 0 0" }
"#;

    fn doc() -> Document {
        Document::from_map(hp_vmf::parse(MAP).expect("map"))
    }

    fn angles(doc: &Document, id: Id) -> DVec3 {
        doc.scene().entity(id).and_then(|n| n.entity.angles()).expect("angles")
    }

    #[test]
    fn rotates_each_entity_with_angles_in_one_step() {
        let mut d = doc();
        let steps = d.history_len().0;
        let objs = [ObjRef::Entity(2), ObjRef::Entity(3), ObjRef::Entity(4), ObjRef::Entity(5), ObjRef::Solid(9)];
        let opts = RandomRotate { seed: 7, ..RandomRotate::default() };
        let r = d.random_rotate(&objs, &opts).expect("rotate");
        assert_eq!(r, RandomRotateReport { rotated: 3, skipped: 2 });
        assert_eq!(d.history_len().0, steps + 1);
        assert_eq!(d.undo_label(), Some("Random rotation"));
        let a3 = angles(&d, 3);
        assert_eq!((a3.x, a3.z), (10.0, 0.0), "pitch and roll stay without jitter");
        let yaws = [angles(&d, 2).y, a3.y, angles(&d, 5).y];
        assert!(yaws.iter().all(|y| (0.0..360.0).contains(y)), "{yaws:?}");
        let turns = [yaws[0], (yaws[1] - 30.0).rem_euclid(360.0), (yaws[2] - 90.0).rem_euclid(360.0)];
        assert!(turns[0] != turns[1] && turns[1] != turns[2], "each gets its own yaw: {turns:?}");
        assert_eq!(d.scene().entity(4).and_then(|n| n.entity.get("angles")), None, "no angles key: skipped");
        d.undo();
        assert_eq!(angles(&d, 2), DVec3::ZERO);
        assert_eq!(angles(&d, 3), DVec3::new(10.0, 30.0, 0.0));
    }

    #[test]
    fn snap_range_and_jitter() {
        let mut d = doc();
        let objs = [ObjRef::Entity(2), ObjRef::Entity(5)];
        for seed in 1..40 {
            let opts = RandomRotate { snap: 90.0, seed, ..RandomRotate::default() };
            d.random_rotate(&objs, &opts).expect("rotate");
            for id in [2, 5] {
                let y = angles(&d, id).y;
                assert!([0.0, 90.0, 180.0, 270.0].contains(&y), "{y}");
            }
        }
        let before = angles(&d, 2).y;
        let opts = RandomRotate { min_yaw: 10.0, max_yaw: 20.0, snap: 0.0, seed: 3, ..RandomRotate::default() };
        d.random_rotate(&[ObjRef::Entity(2)], &opts).expect("rotate");
        let delta = (angles(&d, 2).y - before).rem_euclid(360.0);
        assert!((10.0..=20.0).contains(&delta), "{delta}");
        let opts = RandomRotate { min_yaw: 0.0, max_yaw: 0.0, jitter: 5.0, seed: 4, ..RandomRotate::default() };
        let b = angles(&d, 5);
        d.random_rotate(&[ObjRef::Entity(5)], &opts).expect("rotate");
        let a = angles(&d, 5);
        assert_eq!(a.y, b.y);
        assert!((a.x - b.x).abs() <= 5.0 && (a.z - b.z).abs() <= 5.0);
        assert!(a.x != b.x || a.z != b.z);
    }

    #[test]
    fn same_seed_same_result_and_bad_options() {
        let objs = [ObjRef::Entity(2), ObjRef::Entity(3)];
        let opts = RandomRotate { seed: 42, snap: 15.0, ..RandomRotate::default() };
        let (mut a, mut b) = (doc(), doc());
        a.random_rotate(&objs, &opts).expect("a");
        b.random_rotate(&objs, &opts).expect("b");
        assert_eq!(angles(&a, 2), angles(&b, 2));
        assert_eq!(angles(&a, 3), angles(&b, 3));
        assert_eq!(angles(&a, 2).y % 15.0, 0.0);
        let mut d = doc();
        for bad in [
            RandomRotate { min_yaw: 90.0, max_yaw: 10.0, ..RandomRotate::default() },
            RandomRotate { max_yaw: 720.0, ..RandomRotate::default() },
            RandomRotate { snap: -1.0, ..RandomRotate::default() },
            RandomRotate { jitter: 90.0, ..RandomRotate::default() },
            RandomRotate { min_yaw: f64::NAN, ..RandomRotate::default() },
        ] {
            assert!(d.random_rotate(&objs, &bad).is_err(), "{bad:?}");
        }
        let steps = d.history_len().0;
        let r = d.random_rotate(&[ObjRef::Entity(4)], &RandomRotate::default()).expect("nothing");
        assert_eq!(r, RandomRotateReport { rotated: 0, skipped: 1 });
        assert_eq!(d.history_len().0, steps, "no undo step when nothing turns");
    }
}
