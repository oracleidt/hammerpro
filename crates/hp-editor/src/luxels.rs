use glam::{DVec2, DVec3};
use hp_vmf::{Side, TexAxis};
use rayon::prelude::*;

use crate::Id;
use crate::document::Document;
use crate::scene::{Owner, SolidNode};
use crate::selection::FaceRef;

pub const DEFAULT_LIGHTMAP_SCALE: f64 = 16.0;
pub const MAX_BRUSH_LIGHTMAP_DIM: u32 = 32;
pub const MAX_DISP_LIGHTMAP_DIM: u32 = 125;
pub const MAX_MAP_LIGHTING: u64 = hp_bsp::limits::LIGHTING.gmod;
pub const BYTES_PER_LUXEL: u64 = 4;
pub const BUMPED_LIGHTMAPS: u64 = 4;
pub const MAX_LIGHT_STYLES: u32 = 4;
const MAX_GRID_DIM: f64 = 1_048_576.0;

pub fn lightmap_scale(side: &Side) -> f64 {
    if side.lightmap_scale > 0 { f64::from(side.lightmap_scale) } else { DEFAULT_LIGHTMAP_SCALE }
}

fn texture_scale(a: &TexAxis) -> f64 {
    hp_geom::effective_scale(a.scale)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LuxelAxes {
    pub s: DVec3,
    pub t: DVec3,
    pub s_offset: f64,
    pub t_offset: f64,
    pub scale: f64,
}

impl LuxelAxes {
    pub fn for_side(side: &Side) -> Option<Self> {
        Self::new(&side.uaxis, &side.vaxis, lightmap_scale(side))
    }

    pub fn new(u: &TexAxis, v: &TexAxis, scale: f64) -> Option<Self> {
        if !(scale.is_finite() && scale > 0.0) {
            return None;
        }
        let ok = |a: DVec3| a.is_finite() && a.length_squared() > 1e-12;
        if !ok(u.axis) || !ok(v.axis) || !u.shift.is_finite() || !v.shift.is_finite() {
            return None;
        }
        let s = u.axis / scale;
        let t = v.axis / scale;
        let s_offset = u.shift * texture_scale(u) / scale;
        let t_offset = v.shift * texture_scale(v) / scale;
        (s.is_finite() && t.is_finite() && s_offset.is_finite() && t_offset.is_finite()).then_some(Self {
            s,
            t,
            s_offset,
            t_offset,
            scale,
        })
    }

    pub fn coords(&self, p: DVec3) -> DVec2 {
        DVec2::new(p.dot(self.s) + self.s_offset, p.dot(self.t) + self.t_offset)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LuxelGrid {
    pub mins: [i32; 2],
    pub size: [u32; 2],
}

impl LuxelGrid {
    pub fn from_points(axes: &LuxelAxes, points: &[DVec3]) -> Option<Self> {
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        for &p in points {
            let c = axes.coords(p);
            if !c.is_finite() {
                return None;
            }
            lo = lo.min(c);
            hi = hi.max(c);
        }
        if !(lo.is_finite() && hi.is_finite()) {
            return None;
        }
        let lo = (lo + DVec2::splat(1e-6)).floor();
        let hi = (hi - DVec2::splat(1e-6)).ceil().max(lo);
        let size = hi - lo;
        if size.x > MAX_GRID_DIM || size.y > MAX_GRID_DIM || lo.abs().max_element() > f64::from(i32::MAX / 2) {
            return None;
        }
        Some(Self { mins: [lo.x as i32, lo.y as i32], size: [size.x as u32, size.y as u32] })
    }

    pub fn width(&self) -> u32 {
        self.size[0].saturating_add(1)
    }

    pub fn height(&self) -> u32 {
        self.size[1].saturating_add(1)
    }

    pub fn luxels(&self) -> u64 {
        u64::from(self.width()) * u64::from(self.height())
    }

    pub fn local(&self, coords: DVec2) -> DVec2 {
        coords - DVec2::new(f64::from(self.mins[0]), f64::from(self.mins[1]))
    }

    pub fn luxels_after_split(&self, max_dim: u32) -> u64 {
        let max_dim = u64::from(max_dim.max(1));
        let axis = |size: u32| {
            let size = u64::from(size);
            let pieces = size.div_ceil(max_dim).max(1);
            size + pieces
        };
        axis(self.size[0]).saturating_mul(axis(self.size[1]))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LuxelFrame {
    pub origin: DVec3,
    pub ds: DVec3,
    pub dt: DVec3,
}

impl LuxelFrame {
    pub fn new(axes: &LuxelAxes, normal: DVec3, dist: f64) -> Option<Self> {
        let m = glam::DMat3::from_cols(axes.s, axes.t, normal).transpose();
        let det = m.determinant();
        let norm = axes.s.length() * axes.t.length();
        if !det.is_finite() || norm <= 0.0 || det.abs() < 1e-6 * norm {
            return None;
        }
        let inv = m.inverse();
        let (a, b, c) = (inv.x_axis, inv.y_axis, inv.z_axis);
        let origin = c * dist - a * axes.s_offset - b * axes.t_offset;
        (origin.is_finite() && a.is_finite() && b.is_finite()).then_some(Self { origin, ds: a, dt: b })
    }

    pub fn point(&self, s: f64, t: f64) -> DVec3 {
        self.origin + self.ds * s + self.dt * t
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LightmapKind {
    None,
    Flat,
    Bumped,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BudgetOptions {
    pub styles: u32,
    pub top: usize,
    pub visible_only: bool,
    pub tiny_scale: f64,
}

impl Default for BudgetOptions {
    fn default() -> Self {
        Self { styles: 1, top: 20, visible_only: false, tiny_scale: 4.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BudgetFace {
    pub face: FaceRef,
    pub luxels: u64,
    pub bytes: u64,
    pub scale: f64,
    pub dims: [u32; 2],
    pub displacement: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightmapBudget {
    pub faces: usize,
    pub luxels: u64,
    pub bytes: u64,
    pub tiny_faces: usize,
    pub wide_faces: usize,
    pub oversized_displacements: usize,
    pub top: Vec<BudgetFace>,
}

impl LightmapBudget {
    pub fn fraction(&self) -> f64 {
        self.bytes as f64 / MAX_MAP_LIGHTING as f64
    }

    pub fn over_limit(&self) -> bool {
        self.bytes > MAX_MAP_LIGHTING
    }
}

pub fn solid_luxel_faces(node: &SolidNode) -> Vec<(usize, LuxelGrid, bool)> {
    let has_disp = node.disp.iter().any(Option::is_some);
    let mut out = Vec::new();
    for face in &node.geom.faces {
        if face.vertices.len() < 3 {
            continue;
        }
        let is_disp = node.disp_mesh(face.side_index).is_some();
        if has_disp && !is_disp {
            continue;
        }
        let Some(side) = node.solid.sides.get(face.side_index) else { continue };
        let Some(axes) = LuxelAxes::for_side(side) else { continue };
        if let Some(grid) = LuxelGrid::from_points(&axes, &face.vertices) {
            out.push((face.side_index, grid, is_disp));
        }
    }
    out
}

fn face_budget(
    id: Id,
    side_index: usize,
    side: &Side,
    grid: &LuxelGrid,
    disp: bool,
    kind: LightmapKind,
    styles: u64,
) -> Option<BudgetFace> {
    let per_style = match kind {
        LightmapKind::None => return None,
        LightmapKind::Flat => 1,
        LightmapKind::Bumped => BUMPED_LIGHTMAPS,
    };
    let luxels = if disp { grid.luxels() } else { grid.luxels_after_split(MAX_BRUSH_LIGHTMAP_DIM) };
    let luxels = luxels.saturating_mul(styles);
    Some(BudgetFace {
        face: FaceRef::new(id, side_index),
        luxels,
        bytes: luxels.saturating_mul(per_style).saturating_mul(BYTES_PER_LUXEL),
        scale: lightmap_scale(side),
        dims: [grid.width(), grid.height()],
        displacement: disp,
    })
}

#[derive(Default)]
struct Acc {
    budget: LightmapBudget,
}

impl Acc {
    fn add(&mut self, f: BudgetFace, top: usize, tiny: f64) {
        let b = &mut self.budget;
        b.faces += 1;
        b.luxels = b.luxels.saturating_add(f.luxels);
        b.bytes = b.bytes.saturating_add(f.bytes);
        if f.scale <= tiny {
            b.tiny_faces += 1;
        }
        if f.displacement {
            if f.dims[0].max(f.dims[1]) > MAX_DISP_LIGHTMAP_DIM + 1 {
                b.oversized_displacements += 1;
            }
        } else if f.dims[0].max(f.dims[1]) > MAX_BRUSH_LIGHTMAP_DIM + 1 {
            b.wide_faces += 1;
        }
        if top > 0 {
            b.top.push(f);
            if b.top.len() > top * 4 {
                trim_top(&mut b.top, top);
            }
        }
    }

    fn merge(mut self, other: Acc, top: usize) -> Acc {
        let (a, b) = (&mut self.budget, other.budget);
        a.faces += b.faces;
        a.luxels = a.luxels.saturating_add(b.luxels);
        a.bytes = a.bytes.saturating_add(b.bytes);
        a.tiny_faces += b.tiny_faces;
        a.wide_faces += b.wide_faces;
        a.oversized_displacements += b.oversized_displacements;
        a.top.extend(b.top);
        trim_top(&mut a.top, top);
        self
    }
}

fn trim_top(list: &mut Vec<BudgetFace>, top: usize) {
    list.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.face.cmp(&b.face)));
    list.truncate(top);
}

impl Document {
    pub fn lightmap_budget(
        &self,
        opts: &BudgetOptions,
        kind: &(dyn Fn(&str) -> LightmapKind + Sync),
    ) -> LightmapBudget {
        let styles = u64::from(opts.styles.clamp(1, MAX_LIGHT_STYLES));
        let top = opts.top;
        let acc = self
            .scene
            .solids
            .par_values()
            .filter(|n| !opts.visible_only || self.is_solid_visible(n.solid.id))
            .fold(Acc::default, |mut acc, n| {
                for (side_index, grid, disp) in solid_luxel_faces(n) {
                    let Some(side) = n.solid.sides.get(side_index) else { continue };
                    if let Some(f) =
                        face_budget(n.solid.id, side_index, side, &grid, disp, kind(&side.material), styles)
                    {
                        acc.add(f, top, opts.tiny_scale);
                    }
                }
                acc
            })
            .reduce(Acc::default, |a, b| a.merge(b, top));
        let mut budget = acc.budget;
        trim_top(&mut budget.top, top);
        budget
    }

    pub fn faces_with_lightmap_scale_at_most(
        &self,
        max_scale: f64,
        kind: &(dyn Fn(&str) -> LightmapKind + Sync),
    ) -> Vec<FaceRef> {
        let mut out = Vec::new();
        for n in self.scene.solids.values() {
            if !self.is_solid_visible(n.solid.id) {
                continue;
            }
            for (side_index, _, _) in solid_luxel_faces(n) {
                let Some(side) = n.solid.sides.get(side_index) else { continue };
                if lightmap_scale(side) <= max_scale && kind(&side.material) != LightmapKind::None {
                    out.push(FaceRef::new(n.solid.id, side_index));
                }
            }
        }
        out
    }

    pub fn solid_owner_class(&self, id: Id) -> Option<&str> {
        match self.scene.solids.get(&id)?.owner {
            Owner::World => None,
            Owner::Entity(e) => self.scene.entities.get(&e).map(|n| n.entity.classname.as_str()),
        }
    }
}

pub const LIGHT_PLUS: &str = "light_plus";
const LIGHT_PLUS_LABEL: &str = "Edit lighting options";

#[derive(Clone, Debug, PartialEq)]
pub struct LightPlus {
    pub samples_sun: u32,
    pub samples_ambient: u32,
    pub samples_vis: u32,
    pub texlight: String,
    pub incremental_delay: f32,
    pub bake_dist: f32,
    pub radius_scale: f32,
    pub brightness_scale: f32,
    pub ao_scale: f32,
    pub bounced: bool,
    pub incremental: bool,
    pub supersample: bool,
    pub bleed_hack: bool,
    pub soften_cosine: bool,
    pub debug: bool,
    pub cubemap: bool,
}

impl Default for LightPlus {
    fn default() -> Self {
        Self {
            samples_sun: 6,
            samples_ambient: 40,
            samples_vis: 256,
            texlight: String::new(),
            incremental_delay: 0.0,
            bake_dist: 1024.0,
            radius_scale: 1.0,
            brightness_scale: 1.0,
            ao_scale: 0.0,
            bounced: true,
            incremental: true,
            supersample: false,
            bleed_hack: true,
            soften_cosine: false,
            debug: false,
            cubemap: true,
        }
    }
}

const MAX_SAMPLES: u32 = 1024;
const MAX_TEXLIGHT_LEN: usize = 260;

impl LightPlus {
    pub fn from_block(b: &hp_kv::Block) -> Self {
        let d = Self::default();
        let int = |k: &str, def: u32, max: u32| {
            b.get(k)
                .and_then(|v| v.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite())
                .map_or(def, |v| v.round().clamp(0.0, f64::from(max)) as u32)
        };
        let float = |k: &str, def: f32, lo: f32, hi: f32| {
            b.get(k)
                .and_then(|v| v.trim().parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .map_or(def, |v| v.clamp(lo, hi))
        };
        let flag = |k: &str, def: bool| b.get(k).and_then(|v| v.trim().parse::<f64>().ok()).map_or(def, |v| v != 0.0);
        let texlight = b.get("texlight").unwrap_or("").trim();
        Self {
            samples_sun: int("samples_sun", d.samples_sun, MAX_SAMPLES).max(1),
            samples_ambient: int("samples_ambient", d.samples_ambient, MAX_SAMPLES),
            samples_vis: int("samples_vis", d.samples_vis, 4096),
            texlight: if texlight.len() <= MAX_TEXLIGHT_LEN { texlight.to_owned() } else { String::new() },
            incremental_delay: float("incremental_delay", d.incremental_delay, 0.0, 60.0),
            bake_dist: float("bake_dist", d.bake_dist, 64.0, 65536.0),
            radius_scale: float("radius_scale", d.radius_scale, 0.05, 16.0),
            brightness_scale: float("brightness_scale", d.brightness_scale, 0.0, 16.0),
            ao_scale: float("ao_scale", d.ao_scale, 0.0, 4.0),
            bounced: flag("bounced", d.bounced),
            incremental: flag("incremental", d.incremental),
            supersample: flag("supersample", d.supersample),
            bleed_hack: flag("bleed_hack", d.bleed_hack),
            soften_cosine: flag("soften_cosine", d.soften_cosine),
            debug: flag("debug", d.debug),
            cubemap: flag("cubemap", d.cubemap),
        }
    }

    pub fn write_to(&self, b: &mut hp_kv::Block) {
        let flag = |v: bool| if v { "1" } else { "0" };
        let num = |v: f32| {
            let mut s = String::new();
            hp_kv::fmt_num(&mut s, f64::from(v));
            s
        };
        b.set("samples_sun", self.samples_sun.to_string());
        b.set("samples_ambient", self.samples_ambient.to_string());
        b.set("samples_vis", self.samples_vis.to_string());
        b.set("texlight", self.texlight.replace('"', ""));
        b.set("incremental_delay", num(self.incremental_delay));
        b.set("bake_dist", num(self.bake_dist));
        b.set("radius_scale", num(self.radius_scale));
        b.set("brightness_scale", num(self.brightness_scale));
        b.set("ao_scale", num(self.ao_scale));
        b.set("bounced", flag(self.bounced));
        b.set("incremental", flag(self.incremental));
        b.set("supersample", flag(self.supersample));
        b.set("bleed_hack", flag(self.bleed_hack));
        b.set("soften_cosine", flag(self.soften_cosine));
        b.set("debug", flag(self.debug));
        b.set("cubemap", flag(self.cubemap));
    }
}

impl Document {
    pub fn light_plus(&self) -> LightPlus {
        self.extra_block(LIGHT_PLUS).map(LightPlus::from_block).unwrap_or_default()
    }

    pub fn set_light_plus(&mut self, options: &LightPlus) -> crate::EditResult<()> {
        let mut block = self.extra_block(LIGHT_PLUS).cloned().unwrap_or_else(|| hp_kv::Block::new(LIGHT_PLUS));
        options.write_to(&mut block);
        if self.extra_block(LIGHT_PLUS) == Some(&block) {
            return Ok(());
        }
        self.begin_merge(LIGHT_PLUS_LABEL, LIGHT_PLUS);
        let result = self.set_extra_block(LIGHT_PLUS, block);
        match result {
            Ok(()) => {
                self.commit();
            }
            Err(_) => self.cancel(),
        }
        result
    }
}

pub const COLORCORRECTION_PLUS: &str = "colorcorrection_plus";
const COLORCORRECTION_LABEL: &str = "Edit color correction";
pub const COLOR_CORRECTION_SLOTS: usize = 4;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColorCorrectionSlot {
    pub name: String,
    pub weight: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColorCorrectionPlus {
    pub slots: [ColorCorrectionSlot; COLOR_CORRECTION_SLOTS],
}

impl ColorCorrectionPlus {
    pub fn from_block(b: &hp_kv::Block) -> Self {
        let mut out = Self::default();
        for (i, slot) in out.slots.iter_mut().enumerate() {
            let name = b.get(&format!("name{i}")).unwrap_or("").trim();
            if name.len() <= MAX_TEXLIGHT_LEN && !name.contains('"') {
                slot.name = name.to_owned();
            }
            slot.weight = b
                .get(&format!("weight{i}"))
                .and_then(|v| v.trim().parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .map_or(0.0, |v| v.clamp(0.0, 1.0));
        }
        out
    }

    pub fn write_to(&self, b: &mut hp_kv::Block) {
        for (i, slot) in self.slots.iter().enumerate() {
            b.set(format!("name{i}"), slot.name.replace('"', ""));
            let mut w = String::new();
            hp_kv::fmt_num(&mut w, f64::from(slot.weight));
            b.set(format!("weight{i}"), w);
        }
    }

    pub fn is_active(&self) -> bool {
        self.slots.iter().any(|s| !s.name.trim().is_empty() && s.weight > 0.0)
    }
}

impl Document {
    pub fn color_correction_plus(&self) -> ColorCorrectionPlus {
        self.extra_block(COLORCORRECTION_PLUS).map(ColorCorrectionPlus::from_block).unwrap_or_default()
    }

    pub fn set_color_correction_plus(&mut self, cc: &ColorCorrectionPlus) -> crate::EditResult<()> {
        let mut block =
            self.extra_block(COLORCORRECTION_PLUS).cloned().unwrap_or_else(|| hp_kv::Block::new(COLORCORRECTION_PLUS));
        cc.write_to(&mut block);
        if self.extra_block(COLORCORRECTION_PLUS) == Some(&block) {
            return Ok(());
        }
        self.begin_merge(COLORCORRECTION_LABEL, COLORCORRECTION_PLUS);
        let result = self.set_extra_block(COLORCORRECTION_PLUS, block);
        match result {
            Ok(()) => {
                self.commit();
            }
            Err(_) => self.cancel(),
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

    #[test]
    fn color_correction_round_trip() {
        let mut doc = Document::new_empty();
        assert!(!doc.color_correction_plus().is_active());
        let mut cc = ColorCorrectionPlus::default();
        cc.slots[0] = ColorCorrectionSlot { name: "materials/correction/cc_warm.raw".into(), weight: 0.5 };
        doc.set_color_correction_plus(&cc).unwrap();
        assert_eq!(doc.undo_label(), Some("Edit color correction"));
        assert_eq!(doc.color_correction_plus(), cc);
        assert!(cc.is_active());
        let block = doc.extra_block("colorcorrection_plus").unwrap();
        assert_eq!(block.get("name0"), Some("materials/correction/cc_warm.raw"));
        assert_eq!(block.get("weight0"), Some("0.5"));
        assert_eq!(block.get("weight3"), Some("0"));
        let mut bad = hp_kv::Block::new("colorcorrection_plus");
        bad.set("weight1", "7");
        bad.set("weight2", "nan");
        bad.set("name3", "x".repeat(4000));
        let cc = ColorCorrectionPlus::from_block(&bad);
        assert_eq!(cc.slots[1].weight, 1.0);
        assert_eq!(cc.slots[2].weight, 0.0);
        assert!(cc.slots[3].name.is_empty());
    }

    #[test]
    fn light_plus_round_trip() {
        let mut doc = Document::new_empty();
        assert_eq!(doc.light_plus(), LightPlus::default());
        let mut b = hp_kv::Block::new("light_plus");
        for (k, v) in [
            ("samples_sun", "12"),
            ("samples_ambient", "80"),
            ("texlight", "mylights.rad"),
            ("radius_scale", "0.5"),
            ("bounced", "0"),
            ("future_key", "7"),
        ] {
            b.set(k, v);
        }
        doc.set_extra_block("light_plus", b).unwrap();
        let lp = doc.light_plus();
        assert_eq!((lp.samples_sun, lp.samples_ambient, lp.radius_scale, lp.bounced), (12, 80, 0.5, false));
        assert_eq!(lp.texlight, "mylights.rad");
        let edited = LightPlus { ao_scale: 1.5, ..lp };
        doc.set_light_plus(&edited).unwrap();
        assert_eq!(doc.undo_label(), Some("Edit lighting options"));
        assert_eq!(doc.light_plus(), edited);
        let block = doc.extra_block("light_plus").unwrap();
        assert_eq!(block.get("future_key"), Some("7"), "unknown keys are kept");
        assert_eq!(block.get("ao_scale"), Some("1.5"));
        assert_eq!(block.get("bounced"), Some("0"));
        let (undo, _) = doc.history_len();
        doc.set_light_plus(&edited).unwrap();
        assert_eq!(doc.history_len().0, undo);
        let mut bad = hp_kv::Block::new("light_plus");
        bad.set("samples_sun", "99999999");
        bad.set("radius_scale", "nan");
        bad.set("bake_dist", "-5");
        bad.set("texlight", "x".repeat(5000));
        let lp = LightPlus::from_block(&bad);
        assert_eq!(lp.samples_sun, MAX_SAMPLES);
        assert_eq!(lp.radius_scale, 1.0);
        assert_eq!(lp.bake_dist, 64.0);
        assert!(lp.texlight.is_empty());
    }

    fn side(u: DVec3, v: DVec3, uscale: f64, ushift: f64, lm: i32) -> Side {
        Side {
            uaxis: TexAxis { axis: u, shift: ushift, scale: uscale },
            vaxis: TexAxis { axis: v, shift: 0.0, scale: 0.25 },
            lightmap_scale: lm,
            ..Default::default()
        }
    }

    #[test]
    fn axes_follow_vbsp() {
        let s = side(DVec3::X, DVec3::NEG_Y, 0.25, 32.0, 16);
        let a = LuxelAxes::for_side(&s).unwrap();
        assert_eq!(a.s, DVec3::new(1.0 / 16.0, 0.0, 0.0));
        assert_eq!(a.t, DVec3::new(0.0, -1.0 / 16.0, 0.0));
        assert!((a.s_offset - 0.5).abs() < 1e-12);
        assert_eq!(a.scale, 16.0);
        assert_eq!(LuxelAxes::for_side(&side(DVec3::X, DVec3::Y, 0.25, 0.0, 0)).unwrap().scale, 16.0);
        assert!(LuxelAxes::for_side(&side(DVec3::ZERO, DVec3::Y, 0.25, 0.0, 16)).is_none());
        assert!(LuxelAxes::for_side(&side(DVec3::X, DVec3::Y, 0.25, f64::NAN, 16)).is_none());
    }

    #[test]
    fn grid_is_floor_ceil_plus_one() {
        let a = LuxelAxes::for_side(&side(DVec3::X, DVec3::NEG_Y, 0.25, 0.0, 16)).unwrap();
        let pts = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(128.0, 0.0, 0.0),
            DVec3::new(128.0, -64.0, 0.0),
            DVec3::new(0.0, -64.0, 0.0),
        ];
        let g = LuxelGrid::from_points(&a, &pts).unwrap();
        assert_eq!(g.mins, [0, 0]);
        assert_eq!(g.size, [8, 4]);
        assert_eq!(g.luxels(), 45);
        let shifted: Vec<DVec3> = pts.iter().map(|p| *p + DVec3::new(4.0, -4.0, 0.0)).collect();
        let g = LuxelGrid::from_points(&a, &shifted).unwrap();
        assert_eq!(g.size, [9, 5]);
        assert!(LuxelGrid::from_points(&a, &[]).is_none());
        assert!(LuxelGrid::from_points(&a, &[DVec3::splat(f64::NAN)]).is_none());
        let tiny = LuxelAxes::new(
            &TexAxis { axis: DVec3::X, shift: 0.0, scale: 0.25 },
            &TexAxis { axis: DVec3::Y, shift: 0.0, scale: 0.25 },
            1e-9,
        );
        assert!(LuxelGrid::from_points(&tiny.unwrap(), &pts).is_none());
    }

    #[test]
    fn subdivision_adds_border_luxels() {
        let g = LuxelGrid { mins: [0, 0], size: [64, 10] };
        assert_eq!(g.luxels_after_split(32), 66 * 11);
        let small = LuxelGrid { mins: [0, 0], size: [8, 4] };
        assert_eq!(small.luxels_after_split(32), small.luxels());
    }

    #[test]
    fn frame_inverts_the_projection() {
        let s = side(DVec3::new(1.0, 1.0, 0.0).normalize(), DVec3::NEG_Z, 0.5, 13.0, 8);
        let a = LuxelAxes::for_side(&s).unwrap();
        let n = DVec3::new(1.0, -1.0, 0.0).normalize();
        let f = LuxelFrame::new(&a, n, 100.0).unwrap();
        for (si, ti) in [(0.0, 0.0), (3.5, -2.0), (-7.0, 11.25)] {
            let p = f.point(si, ti);
            assert!((p.dot(n) - 100.0).abs() < 1e-9);
            let c = a.coords(p);
            assert!((c.x - si).abs() < 1e-9 && (c.y - ti).abs() < 1e-9, "{c:?}");
        }
        assert!(LuxelFrame::new(&a, DVec3::new(1.0, 1.0, 0.0).normalize(), 0.0).is_none());
    }

    fn box_doc(lm: i32, material: &str) -> (Document, Id) {
        let mut doc = Document::new_empty();
        let params = hp_geom::PrimitiveParams::default();
        let ids = doc.create_primitive(&params, crate::Owner::World).unwrap();
        let id = ids[0];
        let sides = doc.scene().solid(id).unwrap().solid.sides.len();
        let faces: Vec<FaceRef> = (0..sides).map(|i| FaceRef::new(id, i)).collect();
        let edit =
            crate::FaceEdit { material: Some(material.to_owned()), lightmap_scale: Some(lm), ..Default::default() };
        doc.edit_faces(&faces, &edit).unwrap();
        (doc, id)
    }

    #[test]
    fn budget_counts_faces_bumps_and_styles() {
        let (doc, id) = box_doc(16, "brick/brickwall001a");
        let flat = |_: &str| LightmapKind::Flat;
        let b = doc.lightmap_budget(&BudgetOptions::default(), &flat);
        assert_eq!(b.faces, 6);
        assert!(b.luxels > 0);
        assert_eq!(b.bytes, b.luxels * BYTES_PER_LUXEL);
        assert_eq!(b.top.len(), 6);
        assert!(b.top.windows(2).all(|w| w[0].bytes >= w[1].bytes));
        assert!(b.top.iter().all(|f| f.face.solid == id));
        let bumped = |_: &str| LightmapKind::Bumped;
        let b2 = doc.lightmap_budget(&BudgetOptions { styles: 2, ..Default::default() }, &bumped);
        assert_eq!(b2.bytes, b.bytes * 4 * 2);
        let none = |_: &str| LightmapKind::None;
        assert_eq!(doc.lightmap_budget(&BudgetOptions::default(), &none).faces, 0);
        assert!(!b.over_limit());
        assert!(b.fraction() > 0.0 && b.fraction() < 1.0);
    }

    #[test]
    fn tiny_scale_faces() {
        let (doc, _) = box_doc(2, "brick/brickwall001a");
        let flat = |_: &str| LightmapKind::Flat;
        assert_eq!(doc.faces_with_lightmap_scale_at_most(4.0, &flat).len(), 6);
        assert_eq!(doc.lightmap_budget(&BudgetOptions::default(), &flat).tiny_faces, 6);
        let (doc, _) = box_doc(16, "brick/brickwall001a");
        assert!(doc.faces_with_lightmap_scale_at_most(4.0, &flat).is_empty());
    }
}
