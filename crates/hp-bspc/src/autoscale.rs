use ahash::AHashMap;
use glam::DVec3;
use hp_geom::{Aabb, Plane};
use rayon::prelude::*;

use crate::materials::{MaterialSource, contents, surf};

pub const EYE_HEIGHT: f64 = 72.0;
pub const DEFAULT_SCALE: i32 = 16;
pub const FINE_SCALE: i32 = 8;
pub const SKYBOX_SCALE: i32 = 32;
const WALKABLE_NORMAL_Z: f64 = 0.7;
const FINE_DISTANCE: f64 = 64.0;
const FINE_LIGHT_DISTANCE: f64 = 256.0;
const FINE_MAX_EXTENT: f64 = 256.0;
const UNLIT: u32 = surf::NODRAW | surf::SKY | surf::SKY2D | surf::NOLIGHT | surf::TRIGGER | surf::HINT | surf::SKIP;
const STANDABLE: u32 =
    contents::SOLID | contents::WINDOW | contents::GRATE | contents::PLAYERCLIP | contents::MASK_WATER;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AutoScaleReport {
    pub faces: usize,
    pub coarser: usize,
    pub finer: usize,
    pub custom: usize,
    pub skybox: usize,
    pub luxels_before: f64,
    pub luxels_after: f64,
}

impl AutoScaleReport {
    pub fn bytes_before(&self) -> u64 {
        (self.luxels_before * 4.0) as u64
    }

    pub fn bytes_after(&self) -> u64 {
        (self.luxels_after * 4.0) as u64
    }

    pub fn saved_percent(&self) -> f64 {
        if self.luxels_before <= 0.0 { 0.0 } else { 100.0 * (1.0 - self.luxels_after / self.luxels_before) }
    }

    pub fn summary(&self) -> String {
        format!(
            "Automatic lightmap scale (-autolightmapscale): {} lit faces, {} coarser ({} in the 3D skybox), {} finer, {} with a custom scale kept; lightmap memory about {} KB -> {} KB ({:.0}% less)",
            self.faces,
            self.coarser,
            self.skybox,
            self.finer,
            self.custom,
            self.bytes_before().div_ceil(1024),
            self.bytes_after().div_ceil(1024),
            self.saved_percent()
        )
    }
}

pub fn scale_for_distance(distance: f64) -> i32 {
    if distance <= 256.0 {
        DEFAULT_SCALE
    } else if distance <= 1024.0 {
        32
    } else if distance <= 2048.0 {
        64
    } else {
        128
    }
}

struct Face {
    entity: usize,
    solid: usize,
    side: usize,
    plane: Plane,
    bounds: Aabb,
    area: f64,
    scale: i32,
    lit: bool,
    disp: bool,
}

fn eligible(class: &str) -> bool {
    class.eq_ignore_ascii_case("worldspawn") || class.eq_ignore_ascii_case("func_detail")
}

fn origin_of(e: &hp_vmf::Entity) -> Option<DVec3> {
    let v: Vec<f64> = e.get("origin")?.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    (v.len() == 3).then(|| DVec3::new(v[0], v[1], v[2]))
}

fn box_distance(a: &Aabb, b: &Aabb) -> f64 {
    let gap = (a.min - b.max).max(b.min - a.max).max(DVec3::ZERO);
    gap.length()
}

fn in_front(plane: &Plane, b: &Aabb) -> bool {
    b.corners().iter().any(|&c| plane.distance(c) > 1.0)
}

fn disp_reach(info: &hp_vmf::DispInfo) -> f64 {
    let d = info.distances.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    let o = info.offsets.iter().fold(0.0f64, |m, x| m.max(x.length()));
    d + o + info.elevation.abs()
}

fn skybox_bounds(map: &hp_vmf::Map, faces: &[Face], facts: &dyn Fn(&str) -> (u32, u32)) -> Option<Aabb> {
    let camera = map.entities.iter().find(|e| e.classname.eq_ignore_ascii_case("sky_camera")).and_then(origin_of)?;
    let players: Vec<DVec3> = map
        .entities
        .iter()
        .filter(|e| {
            let c = e.classname.to_ascii_lowercase();
            c.starts_with("info_player_") || c == "info_spawnpoint"
        })
        .filter_map(origin_of)
        .collect();
    let mut solids: Vec<Aabb> = Vec::new();
    let mut index: AHashMap<(usize, usize), usize> = AHashMap::new();
    for f in faces {
        let k = *index.entry((f.entity, f.solid)).or_insert_with(|| {
            solids.push(Aabb::EMPTY);
            solids.len() - 1
        });
        solids[k] = solids[k].union(&f.bounds);
    }
    let sky_solids: Vec<bool> = {
        let mut flags = vec![false; solids.len()];
        for f in faces {
            let (s, _) = facts(&map_material(map, f));
            if s & (surf::SKY | surf::SKY2D) != 0 {
                flags[index[&(f.entity, f.solid)]] = true;
            }
        }
        flags
    };
    let n = solids.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    let shells: Vec<usize> = (0..n).filter(|&i| sky_solids[i]).collect();
    for (a, &i) in shells.iter().enumerate() {
        for &j in &shells[a + 1..] {
            if solids[i].expand(1.0).intersects(&solids[j]) {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                parent[ri] = rj;
            }
        }
    }
    let mut groups: AHashMap<usize, Aabb> = AHashMap::new();
    for &i in &shells {
        let r = find(&mut parent, i);
        let g = groups.entry(r).or_insert(Aabb::EMPTY);
        *g = g.union(&solids[i]);
    }
    groups
        .into_values()
        .filter(|b| b.contains_point(camera) && !players.iter().any(|p| b.contains_point(*p)))
        .min_by(|a, b| a.volume().total_cmp(&b.volume()))
}

fn map_material(map: &hp_vmf::Map, f: &Face) -> String {
    let e = if f.entity == 0 { &map.world } else { &map.entities[f.entity - 1] };
    e.solids[f.solid].sides[f.side].material.clone()
}

pub fn apply(map: &mut hp_vmf::Map, materials: &dyn MaterialSource) -> AutoScaleReport {
    let mut cache: AHashMap<String, (u32, u32)> = AHashMap::new();
    let mut names: Vec<String> = Vec::new();
    for e in std::iter::once(&map.world).chain(map.entities.iter()).filter(|e| eligible(&e.classname)) {
        for s in e.solids.iter().flat_map(|s| s.sides.iter()) {
            let k = s.material.to_ascii_lowercase();
            if !cache.contains_key(&k) {
                cache.insert(k.clone(), (0, 0));
                names.push(s.material.clone());
            }
        }
    }
    let looked: Vec<(String, (u32, u32))> = names
        .par_iter()
        .map(|n| {
            let f = materials.facts(n);
            (n.to_ascii_lowercase(), (f.surf, f.contents))
        })
        .collect();
    cache.extend(looked);
    let facts = |name: &str| cache.get(&name.to_ascii_lowercase()).copied().unwrap_or((0, contents::SOLID));

    let mut jobs: Vec<(usize, usize, &hp_vmf::Solid)> = Vec::new();
    for (ei, e) in std::iter::once(&map.world).chain(map.entities.iter()).enumerate() {
        if !eligible(&e.classname) {
            continue;
        }
        for (si, s) in e.solids.iter().enumerate() {
            jobs.push((ei, si, s));
        }
    }
    let faces: Vec<Face> = jobs
        .par_iter()
        .flat_map_iter(|&(ei, si, solid)| {
            let geom = hp_geom::build_solid(&solid.sides);
            let mut out = Vec::new();
            if !geom.valid {
                return out;
            }
            for g in &geom.faces {
                let side = &solid.sides[g.side_index];
                let (s, _) = facts(&side.material);
                let mut bounds = Aabb::from_points(g.vertices.iter().copied());
                let disp = side.disp.is_some();
                if let Some(info) = &side.disp {
                    bounds = bounds.expand(disp_reach(info));
                }
                out.push(Face {
                    entity: ei,
                    solid: si,
                    side: g.side_index,
                    plane: g.plane,
                    bounds,
                    area: g.area(),
                    scale: side.lightmap_scale,
                    lit: s & UNLIT == 0,
                    disp,
                });
            }
            out
        })
        .collect();

    let walk: Vec<(Aabb, usize)> = faces
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            let (s, c) = facts(&map_material(map, f));
            (f.plane.normal.z >= WALKABLE_NORMAL_Z || f.disp)
                && c & STANDABLE != 0
                && s & (surf::SKY | surf::SKY2D) == 0
        })
        .map(|(i, f)| {
            let mut b = f.bounds;
            b.max.z += EYE_HEIGHT;
            (b, i)
        })
        .collect();
    let lights: Vec<DVec3> = map
        .entities
        .iter()
        .filter(|e| e.classname.eq_ignore_ascii_case("light") || e.classname.eq_ignore_ascii_case("light_spot"))
        .filter_map(origin_of)
        .collect();
    let skybox = skybox_bounds(map, &faces, &facts);

    let decided: Vec<Option<(i32, bool)>> = faces
        .par_iter()
        .enumerate()
        .map(|(i, f)| {
            if !f.lit {
                return None;
            }
            let custom = f.scale != 0 && f.scale != DEFAULT_SCALE;
            if custom {
                return Some((f.scale, false));
            }
            if skybox.is_some_and(|b| b.contains_point(f.bounds.center())) {
                return Some((SKYBOX_SCALE, true));
            }
            let mut best = f64::INFINITY;
            for (b, wi) in &walk {
                if *wi == i {
                    best = 0.0;
                    break;
                }
                let d = box_distance(&f.bounds, b);
                if d < best && in_front(&f.plane, b) {
                    best = d;
                    if best == 0.0 {
                        break;
                    }
                }
            }
            let mut scale = scale_for_distance(best);
            let size = f.bounds.size();
            let small = !f.disp && size.x.max(size.y).max(size.z) <= FINE_MAX_EXTENT;
            if scale == DEFAULT_SCALE
                && best <= FINE_DISTANCE
                && small
                && lights.iter().any(|l| {
                    box_distance(&f.bounds, &Aabb::new(*l, *l)) <= FINE_LIGHT_DISTANCE && f.plane.distance(*l) > 0.0
                })
            {
                scale = FINE_SCALE;
            }
            Some((scale, false))
        })
        .collect();

    let mut report = AutoScaleReport::default();
    for (f, d) in faces.iter().zip(decided) {
        let Some((scale, sky)) = d else { continue };
        report.faces += 1;
        let before = if f.scale == 0 { DEFAULT_SCALE } else { f.scale };
        report.luxels_before += f.area / f64::from(before * before);
        report.luxels_after += f.area / f64::from(scale * scale);
        if scale == before {
            if f.scale != 0 && f.scale != DEFAULT_SCALE {
                report.custom += 1;
            }
            continue;
        }
        if scale > before {
            report.coarser += 1;
            if sky {
                report.skybox += 1;
            }
        } else {
            report.finer += 1;
        }
        let e = if f.entity == 0 { &mut map.world } else { &mut map.entities[f.entity - 1] };
        e.solids[f.solid].sides[f.side].lightmap_scale = scale;
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials::MaterialFacts;

    struct Names;

    impl MaterialSource for Names {
        fn facts(&self, name: &str) -> MaterialFacts {
            crate::materials::facts_by_name(name)
        }
    }

    fn block(min: [f64; 3], max: [f64; 3], material: &str, id: &mut u32) -> hp_vmf::Solid {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        let v = DVec3::new;
        let planes = [
            [v(x0, y1, z1), v(x1, y1, z1), v(x1, y0, z1)],
            [v(x0, y0, z0), v(x1, y0, z0), v(x1, y1, z0)],
            [v(x0, y1, z1), v(x0, y0, z1), v(x0, y0, z0)],
            [v(x1, y1, z0), v(x1, y0, z0), v(x1, y0, z1)],
            [v(x1, y1, z1), v(x0, y1, z1), v(x0, y1, z0)],
            [v(x1, y0, z0), v(x0, y0, z0), v(x0, y0, z1)],
        ];
        *id += 1;
        let mut solid = hp_vmf::Solid { id: *id, ..hp_vmf::Solid::default() };
        for plane in planes {
            *id += 1;
            solid.sides.push(hp_vmf::Side {
                id: *id,
                plane,
                material: material.to_owned(),
                lightmap_scale: 16,
                ..hp_vmf::Side::default()
            });
        }
        solid
    }

    #[test]
    fn far_faces_get_coarser_and_custom_scales_stay() {
        let mut id = 0;
        let mut map = hp_vmf::Map::default();
        map.world.classname = "worldspawn".into();
        map.world.solids.push(block([-512.0, -512.0, -16.0], [512.0, 512.0, 0.0], "concrete/floor", &mut id));
        map.world.solids.push(block([-512.0, -512.0, 1200.0], [512.0, 512.0, 1216.0], "concrete/ceiling", &mut id));
        map.world.solids.push(block([1936.0, -64.0, 1100.0], [2064.0, 64.0, 1116.0], "concrete/ceiling", &mut id));
        if let Some(s) = map.world.solids[2].sides.first_mut() {
            s.lightmap_scale = 4;
        }
        let report = apply(&mut map, &Names);
        let floor_top = map.world.solids[0]
            .sides
            .iter()
            .find(|s| hp_geom::Plane::from_side(s).is_some_and(|p| p.normal.z > 0.9))
            .map(|s| s.lightmap_scale);
        assert_eq!(floor_top, Some(16));
        let ceiling_bottom = map.world.solids[1]
            .sides
            .iter()
            .find(|s| hp_geom::Plane::from_side(s).is_some_and(|p| p.normal.z < -0.9))
            .map(|s| s.lightmap_scale);
        assert_eq!(ceiling_bottom, Some(64), "1200 units above the floor: {report:?}");
        assert_eq!(map.world.solids[2].sides[0].lightmap_scale, 4, "the mapper's scale is kept");
        assert!(report.coarser > 0 && report.custom == 1, "{report:?}");
        assert!(report.luxels_after < report.luxels_before);
        assert!(report.summary().contains("less"));
        assert_eq!(scale_for_distance(10.0), 16);
        assert_eq!(scale_for_distance(5000.0), 128);
    }
}
