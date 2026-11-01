use std::collections::HashSet;

use glam::DVec3;
use hp_editor::{Document, Id};
use hp_fgd::{PropertyType, ResolvedClass, ResolvedFgd};
use hp_vmf::Entity;

use super::names::NameIndex;

pub type Rgb = [u8; 3];

const MAX_EXTENT: f64 = 65536.0;
const MAX_LINES: usize = 4096;
pub const MAX_LINKS_PER_KEY: usize = 64;
const MAX_TEXT: usize = 256;
pub const GRAVITY: f64 = 600.0;
const PLAYER_HULL: (DVec3, DVec3) = (DVec3::new(-16.0, -16.0, 0.0), DVec3::new(16.0, 16.0, 72.0));
pub const TARGET_COLOR: Rgb = [255, 255, 255];
pub const LINK_VALID: Rgb = [80, 220, 90];
pub const LINK_MISSING: Rgb = [255, 70, 60];
pub const LINK_UNKNOWN: Rgb = [255, 210, 40];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HelperLine {
    pub a: DVec3,
    pub b: DVec3,
    pub color: Rgb,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HelperSphere {
    pub center: DVec3,
    pub radius: f64,
    pub color: Rgb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HandleKind {
    Origin,
    VecLine,
    AxisStart,
    AxisEnd,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HelperHandle {
    pub key: String,
    pub kind: HandleKind,
    pub pos: DVec3,
}

impl HelperHandle {
    pub fn value_for(&self, current: &str, pos: DVec3) -> String {
        let fmt = |p: DVec3| hp_vmf::fmt_vec3(clean_vec(p));
        match self.kind {
            HandleKind::Origin | HandleKind::VecLine => fmt(pos),
            HandleKind::AxisStart | HandleKind::AxisEnd => {
                let (a, b) = parse_axis(current).unwrap_or((pos, pos));
                if self.kind == HandleKind::AxisStart {
                    format!("{}, {}", fmt(pos), fmt(b))
                } else {
                    format!("{}, {}", fmt(a), fmt(pos))
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HelperText {
    pub pos: DVec3,
    pub text: String,
    pub size: f64,
    pub color: Rgb,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HelperGeometry {
    pub lines: Vec<HelperLine>,
    pub spheres: Vec<HelperSphere>,
    pub handles: Vec<HelperHandle>,
    pub texts: Vec<HelperText>,
    pub sides: Vec<Id>,
}

impl HelperGeometry {
    pub fn clear(&mut self) {
        self.lines.clear();
        self.spheres.clear();
        self.handles.clear();
        self.texts.clear();
        self.sides.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
            && self.spheres.is_empty()
            && self.handles.is_empty()
            && self.texts.is_empty()
            && self.sides.is_empty()
    }

    fn line(&mut self, a: DVec3, b: DVec3, color: Rgb) {
        if self.lines.len() < MAX_LINES && a.is_finite() && b.is_finite() && (a - b).length_squared() > 1e-12 {
            self.lines.push(HelperLine { a, b, color });
        }
    }

    fn polyline(&mut self, pts: &[DVec3], closed: bool, color: Rgb) {
        for w in pts.windows(2) {
            self.line(w[0], w[1], color);
        }
        if closed && pts.len() > 2 {
            self.line(pts[pts.len() - 1], pts[0], color);
        }
    }

    fn circle(&mut self, center: DVec3, normal: DVec3, radius: f64, segments: usize, color: Rgb) {
        let Some(n) = normal.try_normalize() else { return };
        let (a, b) = n.any_orthonormal_pair();
        let pts: Vec<DVec3> = (0..segments)
            .map(|i| {
                let t = i as f64 / segments as f64 * std::f64::consts::TAU;
                center + (a * t.cos() + b * t.sin()) * radius
            })
            .collect();
        self.polyline(&pts, true, color);
    }

    fn arrow_head(&mut self, tip: DVec3, dir: DVec3, size: f64, color: Rgb) {
        let Some(d) = dir.try_normalize() else { return };
        let (a, b) = d.any_orthonormal_pair();
        let back = tip - d * size;
        for side in [a, -a, b, -b] {
            self.line(tip, back + side * size * 0.45, color);
        }
    }

    fn directed_line(&mut self, a: DVec3, b: DVec3, color: Rgb) {
        self.line(a, b, color);
        let len = (b - a).length();
        if len > 4.0 {
            self.arrow_head(
                a.lerp(b, 0.5) + (b - a) / len * (len * 0.04).min(4.0),
                b - a,
                (len * 0.08).clamp(2.0, 8.0),
                color,
            );
        }
    }

    fn arrow(&mut self, a: DVec3, b: DVec3, color: Rgb) {
        self.line(a, b, color);
        let len = (b - a).length();
        if len > 1.0 {
            self.arrow_head(b, b - a, (len * 0.2).clamp(2.0, 16.0), color);
        }
    }

    fn box_lines(&mut self, corners: [DVec3; 8], color: Rgb) {
        for (i, j) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
            self.line(corners[i], corners[j], color);
        }
    }
}

fn clean_vec(v: DVec3) -> DVec3 {
    let c = |x: f64| {
        if !x.is_finite() {
            return 0.0;
        }
        let r = x.round();
        if (x - r).abs() < 1e-4 { r + 0.0 } else { x + 0.0 }
    };
    DVec3::new(c(v.x), c(v.y), c(v.z))
}

fn box_corners(min: DVec3, max: DVec3) -> [DVec3; 8] {
    std::array::from_fn(|i| {
        DVec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        )
    })
}

fn finite(v: DVec3) -> Option<DVec3> {
    (v.is_finite() && v.abs().max_element() <= MAX_EXTENT).then_some(v)
}

fn key_num(e: &Entity, key: &str) -> Option<f64> {
    e.get(key).and_then(|s| s.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
}

fn key_vec(e: &Entity, key: &str) -> Option<DVec3> {
    e.get(key).and_then(hp_vmf::parse_vec3).and_then(finite)
}

fn parse_axis(s: &str) -> Option<(DVec3, DVec3)> {
    let (a, b) = s.split_once(',')?;
    Some((finite(hp_vmf::parse_vec3(a)?)?, finite(hp_vmf::parse_vec3(b)?)?))
}

fn parse_rgb(s: &str) -> Option<Rgb> {
    let v: Vec<f64> = s.split_ascii_whitespace().take(4).filter_map(|t| t.parse::<f64>().ok()).collect();
    match v.as_slice() {
        [r, g, b, ..] if [r, g, b].iter().all(|c| c.is_finite()) => {
            Some([*r, *g, *b].map(|c| c.round().clamp(0.0, 255.0) as u8))
        }
        _ => None,
    }
}

fn arg_color(arg: Option<&str>) -> Option<Rgb> {
    arg.and_then(parse_rgb)
}

pub fn entity_color(e: &Entity, class: Option<&ResolvedClass>) -> Rgb {
    class.and_then(ResolvedClass::color).unwrap_or(e.editor.color)
}

pub fn entity_anchor(doc: &Document, id: Id) -> Option<DVec3> {
    let scene = doc.scene();
    let node = scene.entity(id)?;
    if scene.is_brush_entity(id) {
        return scene.entity_solid_bounds(id).filter(|b| !b.is_empty()).map(|b| b.center()).and_then(finite);
    }
    node.entity.origin().and_then(finite)
}

pub fn light_direction(e: &Entity) -> DVec3 {
    let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
    let angle = key_num(e, "angle").unwrap_or(0.0);
    if angle == -1.0 {
        return DVec3::Z;
    }
    if angle == -2.0 {
        return DVec3::NEG_Z;
    }
    let yaw = if angle != 0.0 { angle } else { angles.y };
    let pitch = key_num(e, "pitch").filter(|p| *p != 0.0).unwrap_or(angles.x);
    let (sp, cp) = pitch.to_radians().sin_cos();
    let (sy, cy) = yaw.to_radians().sin_cos();
    DVec3::new(cp * cy, cp * sy, sp)
}

pub fn light_range(e: &Entity) -> f64 {
    let range = light_falloff_range(e);
    match key_num(e, "_distance").filter(|v| *v > 0.0) {
        Some(max) => range.min(max.max(1.0)),
        None => range,
    }
}

fn light_falloff_range(e: &Entity) -> f64 {
    let positive = |k: &str| key_num(e, k).filter(|v| *v > 0.0);
    if let Some(z) = positive("_zero_percent_distance") {
        return z.clamp(64.0, 16384.0);
    }
    if let Some(f) = positive("_fifty_percent_distance") {
        return (f * 3.0).clamp(64.0, 16384.0);
    }
    let v: Vec<f64> = e
        .get("_light")
        .unwrap_or("255 255 255 200")
        .split_ascii_whitespace()
        .take(4)
        .filter_map(|t| t.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .collect();
    let (rgb, brightness) = match v.as_slice() {
        [c] => ([*c; 3], 255.0),
        [r, g, b] => ([*r, *g, *b], 255.0),
        [r, g, b, br, ..] => ([*r, *g, *b], *br),
        _ => ([255.0; 3], 200.0),
    };
    let peak = rgb.iter().map(|c| (c / 255.0).max(0.0).powf(2.2)).fold(0.0, f64::max) * brightness.max(0.0);
    (100.0 * peak.sqrt()).clamp(64.0, 16384.0)
}

fn forward(angles: DVec3) -> DVec3 {
    hp_geom::angles_to_vectors(angles).0
}

fn key_angles(e: &Entity, key: &str) -> Option<DVec3> {
    e.get(key).and_then(hp_vmf::parse_vec3).filter(|a| a.is_finite())
}

#[derive(Clone, Copy)]
pub struct HelperContext<'a> {
    pub doc: &'a Document,
    pub fgd: Option<&'a ResolvedFgd>,
    pub names: &'a NameIndex,
}

impl<'a> HelperContext<'a> {
    pub fn class_of(&self, e: &Entity) -> Option<&'a ResolvedClass> {
        self.fgd.and_then(|f| f.class(&e.classname))
    }

    fn find(&self, key: &str, value: &str, out: &mut Vec<Id>) {
        if key.eq_ignore_ascii_case("targetname") {
            self.names.resolve_name(value, out);
        } else if key.eq_ignore_ascii_case("classname") {
            out.extend(self.names.of_class(value).iter().take(super::names::MAX_RESOLVED));
        }
    }
}

fn lower_buf<'a>(s: &str, buf: &'a mut [u8; 48]) -> &'a str {
    let b = s.as_bytes();
    if b.len() > buf.len() {
        return "";
    }
    for (d, c) in buf.iter_mut().zip(b) {
        *d = c.to_ascii_lowercase();
    }
    std::str::from_utf8(&buf[..b.len()]).unwrap_or("")
}

const DRAWN_HELPERS: [&str; 14] = [
    "sphere",
    "lightcone",
    "frustum",
    "line",
    "cylinder",
    "origin",
    "vecline",
    "axis",
    "wirebox",
    "obb",
    "sweptplayerhull",
    "quadbounds",
    "sidelist",
    "worldtext",
];

const VISUALIZED: [&str; 20] = [
    "func_door",
    "func_button",
    "func_movelinear",
    "func_conveyor",
    "func_water_analog",
    "trigger_push",
    "trigger_apply_impulse",
    "env_blood",
    "trigger_wind",
    "phys_thruster",
    "shadow_control",
    "env_physimpact",
    "env_entity_maker",
    "info_projecteddecal",
    "light_environment",
    "light_directional",
    "trigger_catapult",
    "point_spotlight",
    "env_beam",
    "func_fish_pool",
];

pub fn class_has_helpers(class: Option<&ResolvedClass>, classname: &str) -> bool {
    let name = classname.trim();
    if VISUALIZED.iter().chain(&["env_laser", "editor_text", "point_worldtext"]).any(|c| c.eq_ignore_ascii_case(name)) {
        return true;
    }
    class.is_some_and(|c| {
        c.helpers.iter().any(|h| DRAWN_HELPERS.iter().any(|d| h.is(d)))
            || c.props
                .iter()
                .any(|p| matches!(p.kind, PropertyType::TargetDestination | PropertyType::TargetNameOrClass))
    })
}

pub fn entity_helpers(cx: &HelperContext<'_>, id: Id, out: &mut HelperGeometry) {
    let Some(node) = cx.doc.scene().entity(id) else { return };
    let e = &node.entity;
    let Some(origin) = entity_anchor(cx.doc, id) else { return };
    let class = cx.class_of(e);
    let color = entity_color(e, class);
    let mut linked: HashSet<Id> = HashSet::new();
    let mut found: Vec<Id> = Vec::new();
    if let Some(class) = class {
        for h in &class.helpers {
            let mut buf = [0u8; 48];
            let name = lower_buf(&h.name, &mut buf);
            match name {
                "sphere" => {
                    let key = h.first_arg().unwrap_or("radius");
                    if let Some(r) = key_num(e, key).filter(|r| *r > 0.0) {
                        let c = arg_color(h.arg(1)).unwrap_or(color);
                        out.spheres.push(HelperSphere { center: origin, radius: r.min(MAX_EXTENT), color: c });
                    }
                }
                "lightcone" => light_cone(e, origin, out),
                "frustum" => frustum(e, h, origin, out),
                "line" | "cylinder" => link_helper(cx, e, id, origin, h, name, &mut linked, &mut found, out),
                "origin" | "vecline" => {
                    let Some(key) = h.first_arg() else { continue };
                    let Some(p) = key_vec(e, key) else { continue };
                    let kind = if name == "origin" { HandleKind::Origin } else { HandleKind::VecLine };
                    if kind == HandleKind::VecLine {
                        out.line(origin, p, color);
                    }
                    out.handles.push(HelperHandle { key: key.to_owned(), kind, pos: p });
                }
                "axis" => {
                    let Some(key) = h.first_arg() else { continue };
                    let Some((a, b)) = e.get(key).and_then(parse_axis) else { continue };
                    out.line(a, b, color);
                    out.handles.push(HelperHandle { key: key.to_owned(), kind: HandleKind::AxisStart, pos: a });
                    out.handles.push(HelperHandle { key: key.to_owned(), kind: HandleKind::AxisEnd, pos: b });
                }
                "wirebox" | "obb" => {
                    let (Some(kmin), Some(kmax)) = (h.arg(0), h.arg(1)) else { continue };
                    let (Some(lo), Some(hi)) = (key_vec(e, kmin), key_vec(e, kmax)) else { continue };
                    let (lo, hi) = (lo.min(hi), lo.max(hi));
                    let mut corners = box_corners(lo, hi);
                    let rot = if name == "obb" {
                        hp_geom::angle_matrix(e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO))
                    } else {
                        glam::DMat3::IDENTITY
                    };
                    for c in &mut corners {
                        *c = origin + rot * *c;
                    }
                    out.box_lines(corners, color);
                }
                "sweptplayerhull" => {
                    let (Some(a), Some(b)) = (key_vec(e, "point0"), key_vec(e, "point1")) else { continue };
                    let ca = box_corners(a + PLAYER_HULL.0, a + PLAYER_HULL.1);
                    let cb = box_corners(b + PLAYER_HULL.0, b + PLAYER_HULL.1);
                    out.box_lines(ca, color);
                    out.box_lines(cb, color);
                    for (p, q) in ca.iter().zip(&cb) {
                        out.line(*p, *q, color);
                    }
                }
                "quadbounds" => {
                    let pts: Option<Vec<DVec3>> =
                        ["lowerleft", "upperleft", "upperright", "lowerright"].iter().map(|k| key_vec(e, k)).collect();
                    if let Some(pts) = pts {
                        out.polyline(&pts, true, color);
                    }
                }
                "sidelist" => {
                    let key = h.first_arg().unwrap_or("sides");
                    if let Some(v) = e.get(key) {
                        for s in hp_editor::side_index::parse_side_list(v) {
                            if !out.sides.contains(&s) && out.sides.len() < hp_editor::side_index::MAX_SIDE_LIST {
                                out.sides.push(s);
                            }
                        }
                    }
                }
                "worldtext" => text_helper(e, origin, out),
                _ => {}
            }
        }
        let rope_shown = super::scene_display().ropes && (class.has_helper("keyframe") || is_rope_class(&e.classname));
        for p in &class.props {
            if !matches!(p.kind, PropertyType::TargetDestination | PropertyType::TargetNameOrClass) {
                continue;
            }
            if rope_shown && p.name.eq_ignore_ascii_case("NextKey") {
                continue;
            }
            let Some(value) = e.get(&p.name).map(str::trim).filter(|v| !v.is_empty()) else { continue };
            found.clear();
            if p.kind == PropertyType::TargetNameOrClass {
                cx.names.resolve(value, &mut found);
            } else {
                cx.names.resolve_name(value, &mut found);
            }
            for target in found.iter().take(MAX_LINKS_PER_KEY) {
                if *target == id || !linked.insert(*target) {
                    continue;
                }
                if let Some(b) = entity_anchor(cx.doc, *target) {
                    out.directed_line(origin, b, color);
                }
            }
        }
    }
    visualizers(cx, e, id, origin, &mut linked, out);
}

pub fn class_has_path_links(class: Option<&ResolvedClass>, classname: &str) -> bool {
    match class {
        Some(c) => c.has_helper("line") || c.has_helper("cylinder"),
        None => hp_editor::Document::known_path_next_key(classname) == Some("target"),
    }
}

pub fn path_links(cx: &HelperContext<'_>, id: Id, out: &mut HelperGeometry) {
    let Some(node) = cx.doc.scene().entity(id) else { return };
    let e = &node.entity;
    let Some(origin) = entity_anchor(cx.doc, id) else { return };
    let mut linked: HashSet<Id> = HashSet::new();
    let mut found: Vec<Id> = Vec::new();
    match cx.class_of(e) {
        Some(class) => {
            for h in &class.helpers {
                let mut buf = [0u8; 48];
                let name = lower_buf(&h.name, &mut buf);
                if name == "line" || name == "cylinder" {
                    link_helper(cx, e, id, origin, h, name, &mut linked, &mut found, out);
                }
            }
        }
        None => {
            if hp_editor::Document::known_path_next_key(&e.classname) != Some("target") {
                return;
            }
            let Some(value) = e.get("target").map(str::trim).filter(|v| !v.is_empty()) else { return };
            cx.names.resolve_name(value, &mut found);
            for target in found.iter().take(MAX_LINKS_PER_KEY) {
                if *target != id
                    && let Some(b) = entity_anchor(cx.doc, *target)
                {
                    out.line(origin, b, TARGET_COLOR);
                }
            }
        }
    }
}

fn is_rope_class(classname: &str) -> bool {
    let c = classname.trim();
    c.eq_ignore_ascii_case("move_rope") || c.eq_ignore_ascii_case("keyframe_rope")
}

pub fn rope_link(cx: &HelperContext<'_>, id: Id, out: &mut HelperGeometry) {
    if !super::scene_display().ropes {
        return;
    }
    let Some(node) = cx.doc.scene().entity(id) else { return };
    let e = &node.entity;
    let class = cx.class_of(e);
    if !(is_rope_class(&e.classname) || class.is_some_and(|c| c.has_helper("keyframe"))) {
        return;
    }
    let Some(value) = e.get("NextKey").map(str::trim).filter(|v| !v.is_empty()) else { return };
    let Some(origin) = entity_anchor(cx.doc, id) else { return };
    let mut found = Vec::new();
    cx.names.resolve_name(value, &mut found);
    let entities = cx.doc.scene().entities();
    let first = found.into_iter().min_by_key(|t| entities.get_index_of(t).unwrap_or(usize::MAX));
    if let Some(b) = first.filter(|t| *t != id).and_then(|t| entity_anchor(cx.doc, t)) {
        out.directed_line(origin, b, entity_color(e, class));
    }
}

#[allow(clippy::too_many_arguments)]
fn link_helper(
    cx: &HelperContext<'_>,
    e: &Entity,
    id: Id,
    origin: DVec3,
    h: &hp_fgd::Helper,
    name: &str,
    linked: &mut HashSet<Id>,
    found: &mut Vec<Id>,
    out: &mut HelperGeometry,
) {
    let cylinder = name == "cylinder";
    let color = arg_color(h.arg(0)).unwrap_or(TARGET_COLOR);
    let (start_key, start_value, start_radius) =
        if cylinder { (h.arg(1), h.arg(2), h.arg(3)) } else { (h.arg(1), h.arg(2), None) };
    let (end_key, end_value, end_radius) =
        if cylinder { (h.arg(4), h.arg(5), h.arg(6)) } else { (h.arg(3), h.arg(4), None) };
    let ends = |key: Option<&str>, value_key: Option<&str>, found: &mut Vec<Id>| -> Option<Vec<(Id, DVec3)>> {
        let (key, value_key) = (key?, value_key?);
        if key.eq_ignore_ascii_case("targetname") && value_key.eq_ignore_ascii_case("targetname") {
            return Some(vec![(id, origin)]);
        }
        let value = e.get(value_key).map(str::trim).filter(|v| !v.is_empty())?;
        found.clear();
        cx.find(key, value, found);
        Some(found.iter().take(MAX_LINKS_PER_KEY).filter_map(|t| entity_anchor(cx.doc, *t).map(|p| (*t, p))).collect())
    };
    let Some(starts) = ends(start_key, start_value, found) else { return };
    let finish = match (end_key, end_value) {
        (Some(_), Some(_)) => ends(end_key, end_value, found).unwrap_or_default(),
        _ => vec![(id, origin)],
    };
    let radius = |k: Option<&str>, of: Id| {
        let ent = if of == id { Some(e) } else { cx.doc.scene().entity(of).map(|n| &n.entity) };
        k.zip(ent).and_then(|(k, ent)| key_num(ent, k)).filter(|r| *r > 0.0).map(|r| r.min(MAX_EXTENT))
    };
    for (sid, a) in &starts {
        for (eid, b) in &finish {
            if sid == eid {
                continue;
            }
            let other = if *sid == id { *eid } else { *sid };
            linked.insert(other);
            out.line(*b, *a, color);
            if cylinder {
                let axis = *b - *a;
                if let Some(r) = radius(start_radius, *sid) {
                    out.circle(*a, axis, r, 16, color);
                }
                if let Some(r) = radius(end_radius, *eid) {
                    out.circle(*b, axis, r, 16, color);
                }
            }
        }
    }
}

fn light_cone(e: &Entity, origin: DVec3, out: &mut HelperGeometry) {
    let dir = light_direction(e);
    let length = light_range(e);
    let outer = key_num(e, "_cone").unwrap_or(45.0).clamp(1.0, 89.0);
    let inner = key_num(e, "_inner_cone").unwrap_or(30.0).clamp(0.0, outer);
    let color = e.get("_light").and_then(parse_rgb).unwrap_or([255, 255, 255]);
    cone(origin, dir, length, outer, 8, color, out);
    if inner > 0.5 {
        let dim = color.map(|c| (u16::from(c) * 3 / 5) as u8);
        cone(origin, dir, length, inner, 4, dim, out);
    }
}

fn cone(
    apex: DVec3,
    dir: DVec3,
    length: f64,
    half_angle: f64,
    generators: usize,
    color: Rgb,
    out: &mut HelperGeometry,
) {
    let Some(d) = dir.try_normalize() else { return };
    let (s, c) = half_angle.to_radians().sin_cos();
    let center = apex + d * length * c;
    let radius = length * s;
    out.circle(center, d, radius, 24, color);
    let (a, b) = d.any_orthonormal_pair();
    for i in 0..generators {
        let t = i as f64 / generators as f64 * std::f64::consts::TAU;
        out.line(apex, center + (a * t.cos() + b * t.sin()) * radius, color);
    }
}

fn frustum(e: &Entity, h: &hp_fgd::Helper, origin: DVec3, out: &mut HelperGeometry) {
    let num = |i: usize, default: f64| h.arg(i).and_then(|k| key_num(e, k)).unwrap_or(default);
    let fov = num(0, 90.0).clamp(1.0, 179.0);
    let near = num(1, 4.0).clamp(0.0, MAX_EXTENT);
    let far = num(2, 750.0).clamp(near + 1.0, MAX_EXTENT);
    let color = h.arg(3).and_then(|k| e.get(k)).and_then(parse_rgb).unwrap_or([255, 255, 255]);
    let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
    let (f, r, u) = hp_geom::angles_to_vectors(angles);
    let half = (fov.to_radians() * 0.5).tan();
    let rect = |d: f64| {
        let (x, y) = (r * d * half, u * d * half);
        let c = origin + f * d;
        [c - x - y, c + x - y, c + x + y, c - x + y]
    };
    let (n, fa) = (rect(near), rect(far));
    out.polyline(&n, true, color);
    out.polyline(&fa, true, color);
    for i in 0..4 {
        out.line(n[i], fa[i], color);
        out.line(origin, n[i], color);
    }
}

fn text_helper(e: &Entity, origin: DVec3, out: &mut HelperGeometry) {
    let Some(msg) = e.get("message").map(str::trim).filter(|m| !m.is_empty()) else { return };
    if out.texts.iter().any(|t| t.pos == origin) {
        return;
    }
    let text: String = msg.chars().take(MAX_TEXT).collect();
    let size = key_num(e, "textsize").filter(|s| *s > 0.0).unwrap_or(10.0).min(1024.0);
    let color =
        e.get("color").and_then(parse_rgb).or_else(|| e.get("rendercolor").and_then(parse_rgb)).unwrap_or([255; 3]);
    out.texts.push(HelperText { pos: origin, text, size, color });
}

fn extent_along(doc: &Document, id: Id, dir: DVec3) -> Option<f64> {
    let b = doc.scene().entity_solid_bounds(id).filter(|b| !b.is_empty())?;
    let size = b.size();
    Some((size * dir.abs()).element_sum())
}

fn visualizers(
    cx: &HelperContext<'_>,
    e: &Entity,
    id: Id,
    origin: DVec3,
    linked: &mut HashSet<Id>,
    out: &mut HelperGeometry,
) {
    let mut buf = [0u8; 48];
    let class = lower_buf(e.classname.trim(), &mut buf);
    if class.is_empty() {
        return;
    }
    let angles_key = |k: &str| key_angles(e, k);
    let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
    const DIR: Rgb = [255, 200, 60];
    match class {
        "func_door" | "func_button" | "func_movelinear" | "func_conveyor" | "func_water_analog" => {
            let d = forward(angles_key("movedir").unwrap_or(DVec3::ZERO));
            let len = match class {
                "func_door" => {
                    let lip = key_num(e, "lip").unwrap_or(0.0);
                    extent_along(cx.doc, id, d).map_or(64.0, |x| (x - lip).max(8.0))
                }
                "func_movelinear" | "func_water_analog" => key_num(e, "movedistance").unwrap_or(64.0).abs().max(8.0),
                _ => 64.0,
            };
            out.arrow(origin, origin + d * len.min(MAX_EXTENT), DIR);
        }
        "trigger_push" => {
            let d = forward(angles_key("pushdir").unwrap_or(DVec3::ZERO));
            out.arrow(origin, origin + d * 64.0, DIR);
        }
        "trigger_apply_impulse" => {
            let d = forward(angles_key("impulse_dir").unwrap_or(DVec3::ZERO));
            out.arrow(origin, origin + d * 64.0, DIR);
        }
        "env_blood" => {
            let d = forward(angles_key("spraydir").unwrap_or(DVec3::ZERO));
            out.arrow(origin, origin + d * 48.0, DIR);
        }
        "trigger_wind"
        | "phys_thruster"
        | "shadow_control"
        | "env_physimpact"
        | "env_entity_maker"
        | "info_projecteddecal" => {
            let len = if class == "shadow_control" { 128.0 } else { 64.0 };
            out.arrow(origin, origin + forward(angles) * len, DIR);
        }
        "light_environment" | "light_directional" => {
            out.arrow(origin, origin + light_direction(e) * 128.0, DIR);
        }
        "trigger_catapult" => catapult(cx, e, origin, out),
        "point_spotlight" => {
            let len = key_num(e, "spotlightlength").unwrap_or(500.0).clamp(1.0, MAX_EXTENT);
            let width = key_num(e, "spotlightwidth").unwrap_or(50.0).clamp(1.0, MAX_EXTENT);
            let c = e.get("rendercolor").and_then(parse_rgb).unwrap_or([255; 3]);
            let half = (width * 0.5 / len).atan().to_degrees().clamp(0.5, 80.0);
            cone(origin, forward(angles), len, half, 4, c, out);
        }
        "env_beam" => {
            let c = e.get("rendercolor").and_then(parse_rgb).unwrap_or([255; 3]);
            let ends = |key: &str| -> Vec<(Id, DVec3)> {
                let mut f = Vec::new();
                match e.get(key).map(str::trim).filter(|v| !v.is_empty()) {
                    Some(v) => cx.names.resolve_name(v, &mut f),
                    None => f.push(id),
                }
                f.iter().take(MAX_LINKS_PER_KEY).filter_map(|t| entity_anchor(cx.doc, *t).map(|p| (*t, p))).collect()
            };
            let (starts, ends) = (ends("lightningstart"), ends("lightningend"));
            for (sid, a) in &starts {
                for (eid, b) in &ends {
                    if sid != eid {
                        out.line(*a, *b, c);
                    }
                }
            }
        }
        "env_laser" => {
            let c = e.get("rendercolor").and_then(parse_rgb).unwrap_or([255, 0, 0]);
            if let Some(v) = e.get("lasertarget").map(str::trim).filter(|v| !v.is_empty()) {
                let mut f = Vec::new();
                cx.names.resolve_name(v, &mut f);
                for t in f.iter().take(MAX_LINKS_PER_KEY) {
                    if *t != id
                        && linked.insert(*t)
                        && let Some(b) = entity_anchor(cx.doc, *t)
                    {
                        out.line(origin, b, c);
                    }
                }
            }
        }
        "editor_text" | "point_worldtext" => text_helper(e, origin, out),
        "func_fish_pool" => {
            let r = super::fish::fish_range(e);
            if r > 0.0 {
                let c = entity_color(e, cx.class_of(e));
                out.spheres.push(HelperSphere { center: origin, radius: r, color: c });
                out.circle(origin, DVec3::Z, r, 48, c);
            }
        }
        _ => {}
    }
}

pub fn ballistic(start: DVec3, velocity: DVec3, duration: f64, steps: usize) -> Vec<DVec3> {
    let steps = steps.clamp(2, 256);
    (0..=steps)
        .map(|i| {
            let t = duration * i as f64 / steps as f64;
            start + velocity * t - DVec3::Z * (0.5 * GRAVITY * t * t)
        })
        .collect()
}

fn catapult(cx: &HelperContext<'_>, e: &Entity, origin: DVec3, out: &mut HelperGeometry) {
    const ARC: Rgb = [120, 220, 255];
    let speed = key_num(e, "playerspeed").unwrap_or(450.0).clamp(1.0, 10_000.0);
    let target = e.get("launchtarget").map(str::trim).filter(|v| !v.is_empty()).and_then(|v| {
        let mut f = Vec::new();
        cx.names.resolve_name(v, &mut f);
        f.first().and_then(|t| entity_anchor(cx.doc, *t))
    });
    let pts = match target {
        Some(t) => {
            let flat = DVec3::new(t.x - origin.x, t.y - origin.y, 0.0);
            let dist = flat.length();
            if dist < 1.0 {
                vec![origin, t]
            } else {
                let time = dist / speed;
                let vz = (t.z - origin.z + 0.5 * GRAVITY * time * time) / time;
                ballistic(origin, flat / dist * speed + DVec3::Z * vz, time, 48)
            }
        }
        None => {
            let dir = forward(key_angles(e, "launchdirection").unwrap_or(DVec3::ZERO));
            ballistic(origin, dir * speed, 2.0, 48)
        }
    };
    out.polyline(&pts, false, ARC);
    if let [.., a, b] = pts.as_slice() {
        out.arrow_head(*b, *b - *a, 12.0, ARC);
    }
}

pub fn entity_handles(doc: &Document, fgd: Option<&ResolvedFgd>, id: Id) -> Vec<HelperHandle> {
    let Some(node) = doc.scene().entity(id) else { return Vec::new() };
    let e = &node.entity;
    let Some(class) = fgd.and_then(|f| f.class(&e.classname)) else { return Vec::new() };
    let mut out = Vec::new();
    for h in &class.helpers {
        let Some(key) = h.first_arg() else { continue };
        if h.is("origin") || h.is("vecline") {
            if let Some(p) = key_vec(e, key) {
                let kind = if h.is("origin") { HandleKind::Origin } else { HandleKind::VecLine };
                out.push(HelperHandle { key: key.to_owned(), kind, pos: p });
            }
        } else if h.is("axis")
            && let Some((a, b)) = e.get(key).and_then(parse_axis)
        {
            out.push(HelperHandle { key: key.to_owned(), kind: HandleKind::AxisStart, pos: a });
            out.push(HelperHandle { key: key.to_owned(), kind: HandleKind::AxisEnd, pos: b });
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LinkState {
    Valid,
    MissingTarget,
    UnknownOutput,
}

impl LinkState {
    pub fn color(self) -> Rgb {
        match self {
            LinkState::Valid => LINK_VALID,
            LinkState::MissingTarget => LINK_MISSING,
            LinkState::UnknownOutput => LINK_UNKNOWN,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConnectionLink {
    pub from: Id,
    pub to: Option<Id>,
    pub a: DVec3,
    pub b: DVec3,
    pub state: LinkState,
}

pub fn entity_outputs(cx: &HelperContext<'_>, id: Id, out: &mut Vec<ConnectionLink>) {
    let Some(node) = cx.doc.scene().entity(id) else { return };
    let e = &node.entity;
    if e.connections.is_empty() {
        return;
    }
    let Some(a) = entity_anchor(cx.doc, id) else { return };
    let class = cx.class_of(e);
    let mut found = Vec::new();
    for c in &e.connections {
        if NameIndex::is_special(&c.target) || c.target.trim().is_empty() {
            continue;
        }
        let known = class.is_none_or(|cl| cl.output(c.output.trim()).is_some());
        found.clear();
        cx.names.resolve(&c.target, &mut found);
        if found.is_empty() {
            out.push(ConnectionLink { from: id, to: None, a, b: a, state: LinkState::MissingTarget });
            continue;
        }
        let state = if known { LinkState::Valid } else { LinkState::UnknownOutput };
        for t in found.iter().take(MAX_LINKS_PER_KEY) {
            if let Some(b) = entity_anchor(cx.doc, *t) {
                out.push(ConnectionLink { from: id, to: Some(*t), a, b, state });
            }
        }
    }
}

pub fn incoming_outputs(cx: &HelperContext<'_>, ids: &[Id], out: &mut Vec<ConnectionLink>) {
    let wanted: HashSet<Id> = ids.iter().copied().collect();
    let mut keys: HashSet<String> = HashSet::new();
    for id in ids {
        if let Some(n) = cx.doc.scene().entity(*id) {
            keys.insert(n.entity.classname.trim().to_ascii_lowercase());
            if let Some(t) = n.entity.get("targetname").map(str::trim).filter(|t| !t.is_empty()) {
                keys.insert(t.to_ascii_lowercase());
            }
        }
    }
    if keys.is_empty() {
        return;
    }
    let mut links = Vec::new();
    for (src, node) in cx.doc.scene().entities() {
        if wanted.contains(src) {
            continue;
        }
        let hits = node.entity.connections.iter().any(|c| {
            let mut buf = [0u8; 48];
            let raw = c.target.trim();
            let owned;
            let t: &str = if raw.len() <= buf.len() {
                lower_buf(raw, &mut buf)
            } else {
                owned = raw.to_ascii_lowercase();
                &owned
            };
            keys.contains(t) || t.strip_suffix('*').is_some_and(|p| keys.iter().any(|k| k.starts_with(p)))
        });
        if !hits {
            continue;
        }
        links.clear();
        entity_outputs(cx, *src, &mut links);
        out.extend(links.iter().filter(|l| l.to.is_some_and(|t| wanted.contains(&t))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FGD: &str = r#"
        @BaseClass = Targetname [ targetname(target_source) : "Name" ]
        @PointClass base(Targetname) color(255 255 0) sphere(_fifty_percent_distance) sphere(radius) = light
        [ _light(color255) : "Brightness" : "255 255 255 200" ]
        @PointClass base(Targetname) lightcone() = light_spot [ _cone(integer) : "Outer" : 45 ]
        @PointClass base(Targetname) frustum(lightfov,nearz,farz,lightcolor,-1) = env_projectedtexture []
        @PointClass base(Targetname) color(0 255 0) line(255 0 255, targetname, target) = path_track
        [ target(target_destination) : "Next" output OnPass(void) : "" ]
        @PointClass base(Targetname) vecline(endpoint) origin(other) axis(hinge) = test_handles []
        @PointClass base(Targetname) wirebox(mins, maxs) obb(mins, maxs) sidelist(sides) = test_boxes []
        @PointClass base(Targetname) worldtext() = point_worldtext []
        @PointClass cylinder(255 255 255, targetname, start, radius, targetname, end, radius) = test_cylinder []
        @SolidClass base(Targetname) = trigger_multiple [ output OnTrigger(void) : "" ]
    "#;

    fn setup() -> (crate::AppState, ResolvedFgd) {
        let mut state = crate::testing::test_state();
        type Ent<'a> = (Id, &'a str, &'a [(&'a str, &'a str)]);
        let ents: [Ent<'_>; 9] = [
            (100, "path_track", &[("targetname", "p1"), ("target", "p2"), ("origin", "0 0 0"), ("radius", "24")]),
            (101, "path_track", &[("targetname", "p2"), ("target", "p1"), ("origin", "128 0 0"), ("radius", "32")]),
            (
                102,
                "test_handles",
                &[("origin", "0 0 0"), ("endpoint", "0 0 64"), ("other", "8 8 8"), ("hinge", "0 0 0, 0 0 32")],
            ),
            (103, "test_boxes", &[("origin", "10 0 0"), ("mins", "-8 -8 -8"), ("maxs", "8 8 8"), ("sides", "1 2 x 2")]),
            (104, "light_spot", &[("origin", "0 0 64"), ("pitch", "-90"), ("_light", "255 0 0 200")]),
            (105, "env_projectedtexture", &[("origin", "0 0 0"), ("lightfov", "90"), ("farz", "100")]),
            (106, "point_worldtext", &[("origin", "1 2 3"), ("message", "Hello"), ("textsize", "20")]),
            (107, "trigger_catapult", &[("origin", "0 0 0"), ("playerSpeed", "600"), ("launchTarget", "p2")]),
            (108, "test_cylinder", &[("origin", "0 0 0"), ("start", "p1"), ("end", "p2"), ("radius", "16")]),
        ];
        let mut extra = String::new();
        for (id, class, keys) in ents {
            extra.push_str(&format!(
                "entity
{{
	\"id\" \"{id}\"
	\"classname\" \"{class}\"
"
            ));
            for (k, v) in keys {
                extra.push_str(&format!(
                    "	\"{k}\" \"{v}\"
"
                ));
            }
            extra.push_str(
                "}
",
            );
        }
        let vmf = crate::testing::TEST_VMF.replacen(
            "cordon
{",
            &format!(
                "{extra}cordon
{{"
            ),
            1,
        );
        state.doc = hp_editor::Document::from_map(hp_vmf::parse(&vmf).expect("test map"));
        state.doc.edit_entity(5, |e| e.set("_fifty_percent_distance", "100")).expect("light");
        (state, hp_fgd::Fgd::parse(FGD, None).resolve())
    }

    fn helpers_of(state: &crate::AppState, fgd: &ResolvedFgd, id: Id) -> HelperGeometry {
        let names = NameIndex::build(&state.doc);
        let cx = HelperContext { doc: &state.doc, fgd: Some(fgd), names: &names };
        let mut g = HelperGeometry::default();
        entity_helpers(&cx, id, &mut g);
        g
    }

    #[test]
    fn spheres_handles_boxes_and_sides() {
        let (state, fgd) = setup();
        let g = helpers_of(&state, &fgd, 5);
        assert_eq!(
            g.spheres,
            vec![HelperSphere { center: DVec3::new(0.0, 0.0, 96.0), radius: 100.0, color: [255, 255, 0] }]
        );
        let g = helpers_of(&state, &fgd, 102);
        assert_eq!(g.handles.len(), 4);
        assert_eq!(g.handles[0].kind, HandleKind::VecLine);
        assert_eq!(g.handles[0].pos, DVec3::new(0.0, 0.0, 64.0));
        assert_eq!(g.lines.len(), 2, "vecline and axis lines");
        let axis_end = &g.handles[3];
        assert_eq!(axis_end.kind, HandleKind::AxisEnd);
        assert_eq!(axis_end.value_for("0 0 0, 0 0 32", DVec3::new(1.0, 2.0, 3.0)), "0 0 0, 1 2 3");
        assert_eq!(entity_handles(&state.doc, Some(&fgd), 102), g.handles);
        assert!(entity_handles(&state.doc, None, 102).is_empty());
        let g = helpers_of(&state, &fgd, 103);
        assert_eq!(g.lines.len(), 24, "wirebox and obb");
        assert_eq!(g.sides, vec![1, 2]);
    }

    #[test]
    fn fish_pools_show_their_range() {
        let (mut state, fgd) = setup();
        let keys = [("max_range".to_owned(), "300".to_owned())];
        let id = state.doc.create_point_entity("func_fish_pool", DVec3::new(0.0, 0.0, -32.0), &keys).expect("pool");
        assert!(class_has_helpers(None, "func_fish_pool"));
        let g = helpers_of(&state, &fgd, id);
        assert_eq!(g.spheres.len(), 1);
        assert_eq!((g.spheres[0].center, g.spheres[0].radius), (DVec3::new(0.0, 0.0, -32.0), 255.0));
        assert_eq!(g.lines.len(), 48, "the circle of the range");
        assert!(g.lines.iter().all(|l| (l.a.z + 32.0).abs() < 1e-9));
        state.doc.edit_entity(id, |e| e.set("max_range", "0")).expect("edit");
        assert!(helpers_of(&state, &fgd, id).is_empty());
    }

    #[test]
    fn cones_frusta_and_text() {
        let (state, fgd) = setup();
        let g = helpers_of(&state, &fgd, 104);
        assert!(!g.lines.is_empty());
        assert!(g.lines.iter().all(|l| l.a.z <= 64.0 + 1e-9 && l.b.z <= 64.0 + 1e-9), "points down");
        assert!(g.lines.iter().any(|l| l.color == [255, 0, 0]));
        let g = helpers_of(&state, &fgd, 105);
        assert_eq!(g.lines.len(), 16);
        let far = g.lines.iter().map(|l| l.a.x.max(l.b.x)).fold(f64::MIN, f64::max);
        assert!((far - 100.0).abs() < 1e-9, "far plane along +X: {far}");
        let g = helpers_of(&state, &fgd, 106);
        assert_eq!(g.texts.len(), 1);
        assert_eq!((g.texts[0].text.as_str(), g.texts[0].size), ("Hello", 20.0));
    }

    #[test]
    fn lines_to_targets_and_chains() {
        let (state, fgd) = setup();
        let g = helpers_of(&state, &fgd, 100);
        let to_p2: Vec<&HelperLine> = g
            .lines
            .iter()
            .filter(|l| l.a == DVec3::new(128.0, 0.0, 0.0) || l.b == DVec3::new(128.0, 0.0, 0.0))
            .collect();
        assert!(!to_p2.is_empty());
        assert!(to_p2.iter().all(|l| l.color == [255, 0, 255]), "{to_p2:?}");
        let g = helpers_of(&state, &fgd, 108);
        assert!(g.lines.len() > 30, "line plus two circles");
        let ring = |center: DVec3| -> Vec<f64> {
            g.lines
                .iter()
                .filter(|l| (l.a.x - center.x).abs() < 1e-6 && (l.b.x - center.x).abs() < 1e-6)
                .map(|l| (l.a - center).length())
                .collect()
        };
        let at_p1 = ring(DVec3::ZERO);
        let at_p2 = ring(DVec3::new(128.0, 0.0, 0.0));
        assert_eq!((at_p1.len(), at_p2.len()), (16, 16));
        assert!(at_p1.iter().all(|r| (r - 24.0).abs() < 1e-6), "the radius at p1 is the one of p1: {at_p1:?}");
        assert!(at_p2.iter().all(|r| (r - 32.0).abs() < 1e-6), "the radius at p2 is the one of p2: {at_p2:?}");
        let g = helpers_of(&state, &fgd, 107);
        let last = g.lines.iter().map(|l| l.b).find(|b| (b.x - 128.0).abs() < 1e-6);
        assert!(last.is_some_and(|b| b.z.abs() < 1e-6), "the arc lands on the target: {last:?}");
        assert!(g.lines.iter().any(|l| l.a.z > 1.0), "it goes up first");
    }

    #[test]
    fn path_links_are_the_chain_lines_only() {
        let (state, fgd) = setup();
        let names = NameIndex::build(&state.doc);
        let cx = HelperContext { doc: &state.doc, fgd: Some(&fgd), names: &names };
        assert!(class_has_path_links(fgd.class("path_track"), "path_track"));
        assert!(class_has_path_links(fgd.class("test_cylinder"), "test_cylinder"));
        assert!(!class_has_path_links(fgd.class("light"), "light"));
        assert!(class_has_path_links(None, "path_corner"), "known chains without an FGD");
        assert!(!class_has_path_links(None, "keyframe_rope"), "ropes are drawn by the rope preview");
        let mut g = HelperGeometry::default();
        path_links(&cx, 100, &mut g);
        assert_eq!(g.lines.len(), 1);
        assert_eq!(
            (g.lines[0].a, g.lines[0].b),
            (DVec3::ZERO, DVec3::new(128.0, 0.0, 0.0)),
            "from the node to its next"
        );
        assert!(g.spheres.is_empty() && g.handles.is_empty());
        let mut g = HelperGeometry::default();
        path_links(&cx, 104, &mut g);
        assert!(g.is_empty(), "no chain on a light_spot");
        let cx = HelperContext { doc: &state.doc, fgd: None, names: &names };
        let mut g = HelperGeometry::default();
        path_links(&cx, 101, &mut g);
        assert_eq!(g.lines.len(), 1, "path_track links without an FGD");
        assert_eq!(g.lines[0].color, TARGET_COLOR);
    }

    #[test]
    fn connections() {
        let (mut state, fgd) = setup();
        state.doc.edit_entity(100, |e| e.connections.push(hp_vmf::Connection::parse("OnPass", "p2,Toggle,,0,-1"))).ok();
        state
            .doc
            .edit_entity(101, |e| {
                e.connections.push(hp_vmf::Connection::parse("OnBogus", "p1,Toggle,,0,-1"));
                e.connections.push(hp_vmf::Connection::parse("OnPass", "nobody,Toggle,,0,-1"));
                e.connections.push(hp_vmf::Connection::parse("OnPass", "!activator,Kill,,0,-1"));
            })
            .ok();
        let names = NameIndex::build(&state.doc);
        let cx = HelperContext { doc: &state.doc, fgd: Some(&fgd), names: &names };
        let mut links = Vec::new();
        entity_outputs(&cx, 100, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!((links[0].to, links[0].state), (Some(101), LinkState::Valid));
        links.clear();
        entity_outputs(&cx, 101, &mut links);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].state, LinkState::UnknownOutput);
        assert_eq!((links[1].to, links[1].state), (None, LinkState::MissingTarget));
        links.clear();
        incoming_outputs(&cx, &[5], &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!((links[0].from, links[0].to), (6, Some(5)));
        assert_eq!(LinkState::MissingTarget.color(), LINK_MISSING);
        let cx = HelperContext { doc: &state.doc, fgd: None, names: &names };
        links.clear();
        entity_outputs(&cx, 101, &mut links);
        assert_eq!(links[0].state, LinkState::Valid);
    }

    #[test]
    fn hostile_keys_never_panic() {
        let (mut state, fgd) = setup();
        state
            .doc
            .edit_entity(103, |e| {
                e.set("mins", "nan nan nan");
                e.set("maxs", "1e308 0 0");
            })
            .ok();
        state.doc.edit_entity(5, |e| e.set("_fifty_percent_distance", "1e300")).ok();
        state.doc.edit_entity(102, |e| e.set("hinge", "0 0 0")).ok();
        for id in [5, 102, 103, 104, 105, 106, 107, 108, 999] {
            let g = helpers_of(&state, &fgd, id);
            assert!(g.lines.iter().all(|l| l.a.is_finite() && l.b.is_finite()));
            assert!(g.spheres.iter().all(|s| s.radius <= MAX_EXTENT));
        }
    }

    #[test]
    fn big_map_is_fast() {
        let (_, fgd) = setup();
        let mut extra = String::new();
        for i in 0..12_000u32 {
            let id = 1000 + i;
            let (class, keys) = match i % 4 {
                0 => ("light", format!("\"_fifty_percent_distance\" \"{}\"", 64 + i % 100)),
                1 => ("path_track", format!("\"targetname\" \"p{i}\" \"target\" \"p{}\"", i + 4)),
                2 => ("prop_static", "\"model\" \"models/x.mdl\"".to_owned()),
                _ => ("info_target", String::new()),
            };
            extra.push_str(&format!(
                "entity
{{
	\"id\" \"{id}\"
	\"classname\" \"{class}\"
	{keys}
	\"origin\" \"{} {} 0\"
}}
",
                i % 100 * 32,
                i / 100 * 32
            ));
        }
        let vmf = crate::testing::TEST_VMF.replacen(
            "cordon
{",
            &format!(
                "{extra}cordon
{{"
            ),
            1,
        );
        let doc = hp_editor::Document::from_map(hp_vmf::parse(&vmf).expect("map"));
        let mut best = (std::time::Duration::MAX, std::time::Duration::MAX);
        let mut g = HelperGeometry::default();
        for _ in 0..5 {
            let start = std::time::Instant::now();
            let names = NameIndex::build(&doc);
            let names_time = start.elapsed();
            let cx = HelperContext { doc: &doc, fgd: Some(&fgd), names: &names };
            let mut relevance: std::collections::HashMap<&str, bool> = std::collections::HashMap::new();
            g.clear();
            for (id, n) in doc.scene().entities() {
                let class = n.entity.classname.as_str();
                let r = *relevance.entry(class).or_insert_with(|| class_has_helpers(cx.class_of(&n.entity), class));
                if r {
                    entity_helpers(&cx, *id, &mut g);
                }
            }
            best = (best.0.min(start.elapsed()), best.1.min(names_time));
        }
        let (elapsed, names_time) = best;
        eprintln!(
            "12 000 entities: {elapsed:?} (names {names_time:?}), {} lines, {} spheres",
            g.lines.len(),
            g.spheres.len()
        );
        assert_eq!(g.spheres.len(), 3000);
        assert_eq!(g.lines.len(), 2999, "one line per path_track link");
        assert!(elapsed.as_millis() < 250, "{elapsed:?}");
    }

    #[test]
    fn ballistic_arc() {
        let pts = ballistic(DVec3::ZERO, DVec3::new(100.0, 0.0, 300.0), 1.0, 10);
        assert_eq!(pts.len(), 11);
        assert!((pts[10].z - (300.0 - 300.0)).abs() < 1e-9);
        assert!((pts[10].x - 100.0).abs() < 1e-9);
    }
}
