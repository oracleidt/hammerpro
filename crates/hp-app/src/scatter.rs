use std::collections::{HashMap, HashSet};

use glam::DVec3;
use hp_editor::prefab::SprinkleItem;
use hp_editor::{Document, Id, ObjRef, PickOptions};
use hp_geom::{Aabb, DispMesh, Ray};

use crate::grid::GridFrame;
use crate::state::AppState;

pub const MAX_ITEMS: usize = 64;
pub const MAX_COUNT: usize = 20_000;
pub const DEFAULT_MIN_BLEND: f64 = 0.5;
pub const SCALE_KEY: &str = "modelscale";

#[derive(Clone, Debug, PartialEq)]
pub struct ScatterItem {
    pub classname: String,
    pub keyvalues: Vec<(String, String)>,
    pub weight: f64,
    pub texture: Option<String>,
    pub min_blend: f64,
}

impl ScatterItem {
    pub fn prop(model: &str) -> Self {
        Self {
            classname: "prop_static".to_owned(),
            keyvalues: vec![("model".to_owned(), model.trim().replace('\\', "/"))],
            weight: 1.0,
            texture: None,
            min_blend: DEFAULT_MIN_BLEND,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Area {
    Circle { center: DVec3, radius: f64 },
    Box(Aabb),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScatterSpec {
    pub items: Vec<ScatterItem>,
    pub count: usize,
    pub area: Area,
    pub spacing: f64,
    pub seed: u64,
    pub random_yaw: bool,
    pub align: bool,
    pub max_slope: f64,
    pub scale: (f64, f64),
    pub offset: (f64, f64),
    pub grid: Option<GridFrame>,
    pub targets: Option<Vec<Id>>,
}

impl ScatterSpec {
    pub fn new(items: Vec<ScatterItem>, count: usize, area: Area) -> Self {
        Self {
            items,
            count,
            area,
            spacing: 0.0,
            seed: 0,
            random_yaw: true,
            align: false,
            max_slope: 90.0,
            scale: (1.0, 1.0),
            offset: (0.0, 0.0),
            grid: None,
            targets: None,
        }
    }

    pub fn uses_textures(&self) -> bool {
        self.items.iter().any(|i| i.texture.is_some())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceLayers {
    pub material: String,
    pub base: Option<String>,
    pub blend: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialLookup {
    Ready(SurfaceLayers),
    Loading,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScatterError {
    Invalid(String),
    Loading(Vec<String>),
    NoSurface,
}

impl std::fmt::Display for ScatterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScatterError::Invalid(s) => f.write_str(s),
            ScatterError::Loading(m) => {
                write!(f, "the materials are still loading ({}): try again in a moment", m.join(", "))
            }
            ScatterError::NoSurface => {
                f.write_str("no surface was found below the area that fits the rules (surface, slope, texture)")
            }
        }
    }
}

impl std::error::Error for ScatterError {}

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub item: usize,
    pub origin: DVec3,
    pub angles: DVec3,
    pub scale: f64,
    pub blend: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScatterPlan {
    pub placements: Vec<Placement>,
    pub asked: usize,
    pub attempts: usize,
    pub per_item: Vec<usize>,
    pub missed: usize,
    pub too_steep: usize,
    pub too_close: usize,
    pub wrong_texture: usize,
}

impl ScatterPlan {
    pub fn items(&self, spec: &ScatterSpec) -> Vec<SprinkleItem> {
        self.placements
            .iter()
            .filter_map(|p| {
                let item = spec.items.get(p.item)?;
                let mut kv = item.keyvalues.clone();
                if !kv.iter().any(|(k, _)| k.eq_ignore_ascii_case("angles")) {
                    kv.push(("angles".to_owned(), hp_vmf::fmt_vec3(p.angles)));
                }
                if (p.scale - 1.0).abs() > 1e-6 && !kv.iter().any(|(k, _)| k.eq_ignore_ascii_case(SCALE_KEY)) {
                    kv.push((SCALE_KEY.to_owned(), format!("{:.3}", p.scale)));
                }
                Some(SprinkleItem { classname: item.classname.clone(), origin: p.origin, keyvalues: kv })
            })
            .collect()
    }
}

pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn norm(path: &str) -> String {
    let p = path.trim().replace('\\', "/").to_ascii_lowercase();
    let p = p.strip_prefix("materials/").unwrap_or(&p);
    p.strip_suffix(".vmt").or_else(|| p.strip_suffix(".vtf")).unwrap_or(p).to_owned()
}

fn glob(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn texture_matches(filter: &str, texture: &str) -> bool {
    let f = norm(filter);
    let t = norm(texture);
    if f.is_empty() {
        return false;
    }
    if f.contains(['*', '?']) {
        return glob(&f, &t);
    }
    t == f || t.ends_with(&format!("/{f}"))
}

pub fn layer_weight(filter: &str, layers: &SurfaceLayers, alpha: f64) -> f64 {
    if texture_matches(filter, &layers.material) {
        return 1.0;
    }
    let base = layers.base.as_deref().is_some_and(|t| texture_matches(filter, t));
    let blend = layers.blend.as_deref().is_some_and(|t| texture_matches(filter, t));
    let a = alpha.clamp(0.0, 1.0);
    let mut w: f64 = 0.0;
    if base {
        w = w.max(if layers.blend.is_some() { 1.0 - a } else { 1.0 });
    }
    if blend {
        w = w.max(a);
    }
    w
}

fn disp_alpha(mesh: &DispMesh, p: DVec3) -> f64 {
    let mut best = (f64::NEG_INFINITY, 0.0);
    for tri in mesh.indices.chunks_exact(3) {
        let (Some(&a), Some(&b), Some(&c)) = (
            mesh.positions.get(tri[0] as usize),
            mesh.positions.get(tri[1] as usize),
            mesh.positions.get(tri[2] as usize),
        ) else {
            continue;
        };
        let (v0, v1, v2) = (b - a, c - a, p - a);
        let (d00, d01, d11) = (v0.dot(v0), v0.dot(v1), v1.dot(v1));
        let (d20, d21) = (v2.dot(v0), v2.dot(v1));
        let denom = d00 * d11 - d01 * d01;
        if denom.abs() < 1e-12 {
            continue;
        }
        let v = (d11 * d20 - d01 * d21) / denom;
        let w = (d00 * d21 - d01 * d20) / denom;
        let u = 1.0 - v - w;
        let score = u.min(v).min(w);
        if score > best.0 {
            let al = |i: u32| mesh.alphas.get(i as usize).copied().unwrap_or(0.0);
            best = (score, u * al(tri[0]) + v * al(tri[1]) + w * al(tri[2]));
        }
    }
    best.1.clamp(0.0, 1.0)
}

pub fn blend_at(doc: &Document, solid: Id, side: usize, point: DVec3) -> f64 {
    doc.scene()
        .solid(solid)
        .and_then(|n| n.disp.get(side))
        .and_then(Option::as_ref)
        .map_or(0.0, |mesh| disp_alpha(mesh, point))
}

struct Spacing {
    cell: f64,
    cells: HashMap<(i64, i64, i64), Vec<DVec3>>,
}

impl Spacing {
    fn key(&self, p: DVec3) -> (i64, i64, i64) {
        ((p.x / self.cell).floor() as i64, (p.y / self.cell).floor() as i64, (p.z / self.cell).floor() as i64)
    }

    fn blocked(&self, p: DVec3, distance: f64) -> bool {
        let (kx, ky, kz) = self.key(p);
        for x in kx - 1..=kx + 1 {
            for y in ky - 1..=ky + 1 {
                for z in kz - 1..=kz + 1 {
                    if self.cells.get(&(x, y, z)).is_some_and(|v| v.iter().any(|q| q.distance(p) < distance)) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn add(&mut self, p: DVec3) {
        let k = self.key(p);
        self.cells.entry(k).or_default().push(p);
    }
}

pub fn validate(spec: &ScatterSpec) -> Result<(), ScatterError> {
    let bad = |s: &str| Err(ScatterError::Invalid(s.to_owned()));
    if spec.items.is_empty() || spec.items.len() > MAX_ITEMS {
        return bad("give 1 to 64 assets");
    }
    if spec.count == 0 || spec.count > MAX_COUNT {
        return Err(ScatterError::Invalid(format!("the count must be from 1 to {MAX_COUNT}")));
    }
    for i in &spec.items {
        if i.classname.trim().is_empty() || i.classname.contains(char::is_whitespace) {
            return bad("an asset has an invalid class name");
        }
        if !(i.weight.is_finite() && i.weight > 0.0 && i.min_blend.is_finite()) {
            return bad("asset weights must be positive and blend thresholds numbers");
        }
    }
    let finite = [spec.spacing, spec.scale.0, spec.scale.1, spec.offset.0, spec.offset.1, spec.max_slope];
    if finite.iter().any(|v| !v.is_finite()) || spec.spacing < 0.0 || spec.scale.0 <= 0.0 || spec.scale.1 < spec.scale.0
    {
        return bad("spacing, scale, offset and slope must be valid numbers (scale above 0, max not below min)");
    }
    Ok(())
}

pub fn plan(
    doc: &Document,
    spec: &ScatterSpec,
    lookup: &mut dyn FnMut(&str) -> MaterialLookup,
) -> Result<ScatterPlan, ScatterError> {
    validate(spec)?;
    let targets: Option<HashSet<Id>> = spec.targets.as_ref().map(|t| t.iter().copied().collect());
    if let Some(t) = &targets
        && !t.iter().any(|id| doc.scene().solid(*id).is_some())
    {
        return Err(ScatterError::Invalid("none of the target ids is a solid of the map".to_owned()));
    }
    let area = match spec.area {
        Area::Circle { center, radius } => {
            Aabb::new(center - DVec3::new(radius, radius, 0.0), center + DVec3::new(radius, radius, 0.0))
        }
        Area::Box(b) => b,
    };
    let (center, radius) = match spec.area {
        Area::Circle { center, radius } => (center, radius),
        Area::Box(b) => (b.center(), 0.0),
    };
    let top = doc
        .scene()
        .solids()
        .values()
        .filter(|n| targets.as_ref().is_none_or(|t| t.contains(&n.solid.id)))
        .map(|n| n.bounds.max.z)
        .fold(area.max.z, f64::max)
        + 1.0;
    let min_dot = spec.max_slope.to_radians().cos() - 1e-9;
    let filter = |o: ObjRef| match o {
        ObjRef::Solid(id) => targets.as_ref().is_none_or(|t| t.contains(&id)),
        ObjRef::Entity(_) => false,
    };
    let opts = PickOptions { include_entities: false, filter: Some(&filter), ..PickOptions::default() };
    let mut rng = Rng(spec.seed);
    let mut out = ScatterPlan { asked: spec.count, per_item: vec![0; spec.items.len()], ..Default::default() };
    let mut spacing = (spec.spacing > 0.0).then(|| Spacing { cell: spec.spacing, cells: HashMap::new() });
    let mut taken: HashSet<(u64, u64)> = HashSet::new();
    let mut pending: Vec<String> = Vec::new();
    let mut cache: HashMap<String, MaterialLookup> = HashMap::new();
    let textured = spec.uses_textures();
    let attempts = spec.count * if textured { 60 } else { 30 };
    for _ in 0..attempts {
        if out.placements.len() >= spec.count {
            break;
        }
        out.attempts += 1;
        let (mut x, mut y) = if radius > 0.0 {
            let (r, t) = (radius * rng.next().sqrt(), rng.next() * std::f64::consts::TAU);
            (center.x + r * t.cos(), center.y + r * t.sin())
        } else {
            (area.min.x + rng.next() * (area.max.x - area.min.x), area.min.y + rng.next() * (area.max.y - area.min.y))
        };
        if let Some(g) = &spec.grid {
            let q = g.snap_point(DVec3::new(x, y, top));
            if q.is_finite() {
                (x, y) = (q.x, q.y);
            }
            if !taken.insert((x.to_bits(), y.to_bits())) {
                out.too_close += 1;
                continue;
            }
        }
        let ray = Ray::new(DVec3::new(x, y, top), DVec3::NEG_Z);
        let Some(hit) = doc.pick_ray(&ray, &opts, &|_| None) else {
            out.missed += 1;
            continue;
        };
        if hit.point.z < area.min.z - 1e-6 && radius == 0.0 {
            out.missed += 1;
            continue;
        }
        if hit.normal.z < min_dot {
            out.too_steep += 1;
            continue;
        }
        if let Some(s) = &spacing
            && s.blocked(hit.point, spec.spacing)
        {
            out.too_close += 1;
            continue;
        }
        let mut eligible: Vec<(usize, f64)> = Vec::with_capacity(spec.items.len());
        let mut blend = 0.0;
        if textured {
            let sample = match (hit.obj, hit.face) {
                (ObjRef::Solid(id), Some(side)) => doc.scene().solid(id).and_then(|n| {
                    let material = n.solid.sides.get(side)?.material.clone();
                    Some((material, blend_at(doc, id, side, hit.point)))
                }),
                _ => None,
            };
            let Some((material, alpha)) = sample else {
                out.wrong_texture += 1;
                continue;
            };
            blend = alpha;
            let key = norm(&material);
            let found = cache.entry(key.clone()).or_insert_with(|| lookup(&key)).clone();
            let layers = match found {
                MaterialLookup::Ready(l) => l,
                MaterialLookup::Loading => {
                    if !pending.contains(&key) {
                        pending.push(key);
                    }
                    continue;
                }
            };
            for (i, item) in spec.items.iter().enumerate() {
                let ok = match &item.texture {
                    None => true,
                    Some(f) => {
                        let w = layer_weight(f, &layers, alpha);
                        w > 1e-6 && w + 1e-9 >= item.min_blend
                    }
                };
                if ok {
                    eligible.push((i, item.weight));
                }
            }
            if eligible.is_empty() {
                out.wrong_texture += 1;
                continue;
            }
        } else {
            eligible.extend(spec.items.iter().enumerate().map(|(i, it)| (i, it.weight)));
        }
        let total: f64 = eligible.iter().map(|e| e.1).sum();
        let mut pick = rng.next() * total;
        let chosen = eligible
            .iter()
            .find(|(_, w)| {
                pick -= *w;
                pick < 0.0
            })
            .or(eligible.last())
            .map_or(0, |e| e.0);
        let yaw = if spec.random_yaw { (rng.next() * 360.0).floor() } else { 0.0 };
        let n = hit.normal;
        let angles = if spec.align {
            DVec3::new(-n.x.atan2(n.z).to_degrees(), yaw, n.y.atan2(n.z).to_degrees())
        } else {
            DVec3::new(0.0, yaw, 0.0)
        };
        let scale = if spec.scale.1 > spec.scale.0 {
            spec.scale.0 + rng.next() * (spec.scale.1 - spec.scale.0)
        } else {
            spec.scale.0
        };
        let offset = if spec.offset.1 > spec.offset.0 {
            spec.offset.0 + rng.next() * (spec.offset.1 - spec.offset.0)
        } else {
            spec.offset.0
        };
        let up = if spec.align { n } else { DVec3::Z };
        if let Some(s) = &mut spacing {
            s.add(hit.point);
        }
        out.per_item[chosen] += 1;
        out.placements.push(Placement { item: chosen, origin: hit.point + up * offset, angles, scale, blend });
    }
    if out.placements.is_empty() {
        if !pending.is_empty() {
            return Err(ScatterError::Loading(pending));
        }
        return Err(ScatterError::NoSurface);
    }
    Ok(out)
}

pub fn plan_in_state(state: &mut AppState, spec: &ScatterSpec) -> Result<ScatterPlan, ScatterError> {
    let assets = &mut state.assets;
    let mut lookup = |name: &str| match assets.material_info(name) {
        Some(info) => MaterialLookup::Ready(SurfaceLayers {
            material: norm(name),
            base: info.base_texture.clone(),
            blend: info.base_texture2.clone(),
        }),
        None => MaterialLookup::Loading,
    };
    plan(&state.doc, spec, &mut lookup)
}

pub fn commit(state: &mut AppState, spec: &ScatterSpec, plan: &ScatterPlan) -> Result<Vec<Id>, ScatterError> {
    let items = plan.items(spec);
    state.doc.transaction("Sprinkle", |doc| doc.sprinkle(&items, &[])).map_err(|e| ScatterError::Invalid(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layers(base: &str, blend: Option<&str>) -> SurfaceLayers {
        SurfaceLayers {
            material: "nature/blend_a".to_owned(),
            base: Some(base.to_owned()),
            blend: blend.map(str::to_owned),
        }
    }

    #[test]
    fn texture_names_match_by_path_tail_or_glob() {
        assert!(texture_matches("grass1", "nature/grass1"));
        assert!(texture_matches("GRASS1", "Materials\\Nature\\Grass1.vtf"));
        assert!(!texture_matches("grass", "nature/grass1"));
        assert!(texture_matches("nature/*", "nature/grass1"));
        assert!(texture_matches("*grass?", "nature/grass1"));
        assert!(!texture_matches("", "nature/grass1"));
    }

    #[test]
    fn blend_weights_follow_the_layer() {
        let l = layers("nature/grass1", Some("nature/dirt1"));
        assert_eq!(layer_weight("grass1", &l, 0.0), 1.0);
        assert!((layer_weight("grass1", &l, 0.25) - 0.75).abs() < 1e-9);
        assert!((layer_weight("dirt1", &l, 0.25) - 0.25).abs() < 1e-9);
        assert_eq!(layer_weight("dirt1", &l, 1.0), 1.0);
        assert_eq!(layer_weight("rock", &l, 0.5), 0.0);
        assert_eq!(layer_weight("blend_a", &l, 0.9), 1.0, "the material name covers the whole face");
        let single = layers("nature/grass1", None);
        assert_eq!(layer_weight("grass1", &single, 0.9), 1.0, "a material without a blend ignores alpha");
    }

    #[test]
    fn globs() {
        assert!(glob("a*b*c", "aXXbYYc"));
        assert!(!glob("a*b*c", "aXXbYY"));
        assert!(glob("*", ""));
    }

    fn blend_state(alpha_of: impl Fn(usize) -> f64) -> AppState {
        let mut s = crate::testing::test_state();
        let face = hp_editor::FaceRef::new(2, 0);
        s.doc.create_displacements(&[face], 2).expect("displacement");
        let mut info = *s.doc.scene().solid(2).and_then(|n| n.solid.sides[0].disp.clone()).expect("disp info");
        for k in 0..info.alphas.len() {
            info.alphas[k] = alpha_of(k);
        }
        s.doc.set_disp_data("test", vec![(face, info)]).expect("alphas");
        s
    }

    fn grass_dirt() -> impl FnMut(&str) -> MaterialLookup {
        |_| MaterialLookup::Ready(layers("nature/grass1", Some("nature/dirt1")))
    }

    fn floor() -> Area {
        Area::Box(Aabb::new(DVec3::new(-200.0, -200.0, 0.0), DVec3::new(200.0, 200.0, 0.0)))
    }

    fn two_items() -> Vec<ScatterItem> {
        let mut grass = ScatterItem::prop("models/grass.mdl");
        grass.texture = Some("grass1".to_owned());
        let mut dirt = ScatterItem::prop("models/rock.mdl");
        dirt.texture = Some("dirt1".to_owned());
        vec![grass, dirt]
    }

    #[test]
    fn assets_follow_the_vertex_alpha_of_a_displacement() {
        let s = blend_state(|k| 255.0 * (k % 5) as f64 / 4.0);
        let mut spec = ScatterSpec::new(two_items(), 120, floor());
        spec.seed = 7;
        spec.targets = Some(vec![2]);
        let made = plan(&s.doc, &spec, &mut grass_dirt()).expect("plan");
        assert!(made.placements.len() > 40, "{}", made.placements.len());
        let (mut grass, mut dirt) = (0, 0);
        for p in &made.placements {
            if p.item == 0 {
                grass += 1;
                assert!(p.blend <= 0.5 + 1e-9, "grass at blend {}", p.blend);
            } else {
                dirt += 1;
                assert!(p.blend >= 0.5 - 1e-9, "dirt at blend {}", p.blend);
            }
        }
        assert!(grass > 0 && dirt > 0, "{grass} {dirt}");
        assert_eq!(made.per_item, vec![grass, dirt]);

        let all_dirt = blend_state(|_| 255.0);
        let only = plan_for(&all_dirt, &spec);
        assert!(only.placements.iter().all(|p| p.item == 1));
        let all_grass = blend_state(|_| 0.0);
        let only = plan_for(&all_grass, &spec);
        assert!(only.placements.iter().all(|p| p.item == 0));

        let mut strict = spec.clone();
        strict.items[0].min_blend = 1.0;
        let s2 = blend_state(|_| 128.0);
        let made = plan(&s2.doc, &strict, &mut grass_dirt()).expect("plan");
        assert!(made.placements.iter().all(|p| p.item == 1), "grass needs 100 percent, the surface is at 50");
    }

    fn plan_for(s: &AppState, spec: &ScatterSpec) -> ScatterPlan {
        plan(&s.doc, spec, &mut grass_dirt()).expect("plan")
    }

    #[test]
    fn spacing_scale_offset_grid_and_targets() {
        let s = crate::testing::test_state();
        let mut spec = ScatterSpec::new(vec![ScatterItem::prop("models/a.mdl")], 60, floor());
        spec.spacing = 48.0;
        spec.targets = Some(vec![2]);
        spec.seed = 3;
        spec.scale = (0.5, 1.5);
        spec.offset = (-4.0, -4.0);
        let p = plan(&s.doc, &spec, &mut grass_dirt()).expect("plan");
        assert!(p.placements.len() > 10);
        for (i, a) in p.placements.iter().enumerate() {
            assert!((0.5..=1.5).contains(&a.scale));
            assert!((a.origin.z + 4.0).abs() < 1e-9, "{:?}", a);
            for b in &p.placements[i + 1..] {
                assert!(a.origin.distance(b.origin) >= 48.0 - 1e-9);
            }
        }
        let items = p.items(&spec);
        assert!(items.iter().all(|i| i.keyvalues.iter().any(|(k, _)| k == "modelscale")));
        assert!(items.iter().all(|i| i.keyvalues.iter().any(|(k, _)| k == "angles")));

        let mut snapped = spec.clone();
        snapped.spacing = 0.0;
        snapped.grid = Some(GridFrame::uniform(32.0));
        let p = plan(&s.doc, &snapped, &mut grass_dirt()).expect("plan");
        assert!(p.placements.iter().all(|a| (a.origin.x / 32.0).fract() == 0.0 && (a.origin.y / 32.0).fract() == 0.0));
        let unique: HashSet<(u64, u64)> =
            p.placements.iter().map(|a| (a.origin.x.to_bits(), a.origin.y.to_bits())).collect();
        assert_eq!(unique.len(), p.placements.len(), "one asset per grid point");

        let mut target = ScatterSpec::new(
            vec![ScatterItem::prop("models/a.mdl")],
            20,
            Area::Box(Aabb::new(DVec3::new(-100.0, 256.0, 0.0), DVec3::new(100.0, 272.0, 128.0))),
        );
        target.targets = Some(vec![3]);
        let p = plan(&s.doc, &target, &mut grass_dirt()).expect("plan");
        assert!(p.placements.iter().all(|a| (a.origin.z - 128.0).abs() < 1e-6 && a.origin.y >= 256.0));
        target.targets = Some(vec![99]);
        assert!(matches!(plan(&s.doc, &target, &mut grass_dirt()), Err(ScatterError::Invalid(_))));
    }

    #[test]
    fn pending_materials_are_reported_and_bad_specs_refused() {
        let s = blend_state(|_| 0.0);
        let spec = ScatterSpec::new(two_items(), 10, floor());
        let loading = plan(&s.doc, &spec, &mut |_: &str| MaterialLookup::Loading);
        assert!(matches!(loading, Err(ScatterError::Loading(m)) if !m.is_empty()));
        let mut bad = spec.clone();
        bad.items.clear();
        assert!(matches!(plan(&s.doc, &bad, &mut grass_dirt()), Err(ScatterError::Invalid(_))));
        bad = spec.clone();
        bad.count = 0;
        assert!(validate(&bad).is_err());
        bad = spec.clone();
        bad.scale = (2.0, 1.0);
        assert!(validate(&bad).is_err());
        bad = spec;
        bad.area = Area::Circle { center: DVec3::new(90_000.0, 0.0, 0.0), radius: 10.0 };
        assert!(matches!(plan(&s.doc, &bad, &mut grass_dirt()), Err(ScatterError::NoSurface)));
    }

    #[test]
    fn commit_is_one_undo_step() {
        let mut s = crate::testing::test_state();
        let spec = ScatterSpec::new(vec![ScatterItem::prop("models/a.mdl")], 25, floor());
        let p = plan_in_state(&mut s, &spec).expect("plan");
        let before = s.doc.stats().entities;
        let (undo, _) = s.doc.history_len();
        let ids = commit(&mut s, &spec, &p).expect("commit");
        assert_eq!(ids.len(), p.placements.len());
        assert_eq!(s.doc.stats().entities, before + ids.len());
        assert_eq!(s.doc.history_len().0, undo + 1);
        assert!(s.doc.undo());
        assert_eq!(s.doc.stats().entities, before);
    }
}
