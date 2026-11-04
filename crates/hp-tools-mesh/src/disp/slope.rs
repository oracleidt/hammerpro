use glam::{DVec2, DVec3};
use hp_app::state::AppState;
use hp_app::tool::ToolOption;
use hp_app::{Severity, t, tf};
use hp_editor::{Document, FaceRef, ObjRef, SelectOp};
use hp_geom::{BlendFalloff, RampPivot, SlopeField, SlopeStats};

use super::blend::{FALLOFF_NAMES, falloff_name};
use super::commands::{reshape_doc, targets};
use super::paint::selected_disp_faces;
use super::seams::{UNDO_SEW, plan_sew};
use super::smooth::{SmoothPart, Surface, write_back};
use crate::common::edit_error_text;

pub const UNDO_LIMIT: &str = "Limit displacement slope";
pub const UNDO_RAMP: &str = "Displacement ramp";
pub const UNDO_UNIFORM: &str = "Uniform displacement slope";
pub const UNDO_PATH: &str = "Displacement slope along a path";
pub const UNDO_SMOOTH: &str = "Smooth displacement slope";

pub const PIVOT_NAMES: [&str; 3] = ["average", "lowest", "highest"];

pub fn pivot_name(p: RampPivot) -> &'static str {
    match p {
        RampPivot::Average => "average",
        RampPivot::Lowest => "lowest",
        RampPivot::Highest => "highest",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlopeOp {
    Limit,
    Ramp,
    Uniform,
    Path,
    Smooth,
}

impl SlopeOp {
    pub fn label(self) -> &'static str {
        match self {
            SlopeOp::Limit => UNDO_LIMIT,
            SlopeOp::Ramp => UNDO_RAMP,
            SlopeOp::Uniform => UNDO_UNIFORM,
            SlopeOp::Path => UNDO_PATH,
            SlopeOp::Smooth => UNDO_SMOOTH,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectMatch {
    Any,
    All,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlopeOptions {
    pub max_angle: f64,
    pub terrace: bool,
    pub terrace_step: f64,
    pub terrace_strength: f64,
    pub ramp_angle: f64,
    pub ramp_direction: f64,
    pub ramp_pivot: RampPivot,
    pub ramp_strength: f64,
    pub ramp_detail: f64,
    pub uniform_strength: f64,
    pub uniform_detail: f64,
    pub path: Vec<DVec3>,
    pub path_width: f64,
    pub path_falloff: BlendFalloff,
    pub path_strength: f64,
    pub smooth_passes: u32,
    pub smooth_strength: f64,
    pub select_angle: f64,
    pub select_above: bool,
    pub select_match: SelectMatch,
    pub select_op: SelectOp,
}

impl Default for SlopeOptions {
    fn default() -> Self {
        Self {
            max_angle: 45.0,
            terrace: false,
            terrace_step: 64.0,
            terrace_strength: 1.0,
            ramp_angle: 15.0,
            ramp_direction: 0.0,
            ramp_pivot: RampPivot::Average,
            ramp_strength: 1.0,
            ramp_detail: 0.0,
            uniform_strength: 1.0,
            uniform_detail: 0.0,
            path: Vec::new(),
            path_width: 128.0,
            path_falloff: BlendFalloff::Smooth,
            path_strength: 1.0,
            smooth_passes: 4,
            smooth_strength: 0.5,
            select_angle: hp_geom::SOURCE_WALKABLE_DEGREES,
            select_above: true,
            select_match: SelectMatch::Any,
            select_op: SelectOp::Replace,
        }
    }
}

fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_owned() } else { s.to_owned() }
}

pub fn path_text(path: &[DVec3]) -> String {
    path.iter().map(|p| format!("{} {} {}", num(p.x), num(p.y), num(p.z))).collect::<Vec<_>>().join("; ")
}

pub fn parse_path(text: &str) -> Option<Vec<DVec3>> {
    let mut out = Vec::new();
    for part in text.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let v: Vec<f64> =
            part.split_whitespace().map(|n| n.parse::<f64>().ok().filter(|n| n.is_finite())).collect::<Option<_>>()?;
        let [x, y, z] = <[f64; 3]>::try_from(v).ok()?;
        out.push(DVec3::new(x, y, z));
    }
    (out.len() <= 256).then_some(out)
}

impl SlopeOptions {
    pub fn values(&self) -> Vec<ToolOption> {
        vec![
            ToolOption::new(
                "slope_max_angle",
                num(self.max_angle),
                "slope limit: steepest triangle angle in degrees (1 to 89)",
            ),
            ToolOption::new(
                "slope_terrace",
                if self.terrace { "on" } else { "off" },
                "slope limit: first cut the heights into flat terraces, then limit the risers between them",
            ),
            ToolOption::new(
                "slope_terrace_step",
                num(self.terrace_step),
                "slope limit: height of a terrace step in units (1 to 4096)",
            ),
            ToolOption::new(
                "slope_terrace_strength",
                num(self.terrace_strength),
                "slope limit: how much the heights snap to the terraces (0 to 1)",
            ),
            ToolOption::new("slope_ramp_angle", num(self.ramp_angle), "ramp: exact slope angle in degrees (0 to 89)"),
            ToolOption::new(
                "slope_ramp_direction",
                num(self.ramp_direction),
                "ramp: direction the ramp climbs toward, in degrees in the map plane (0 is +X, 90 is +Y)",
            ),
            ToolOption::new(
                "slope_ramp_pivot",
                pivot_name(self.ramp_pivot),
                "ramp: what stays in place: average (the mean height), lowest or highest (the lowest or highest vertex)",
            ),
            ToolOption::new(
                "slope_ramp_strength",
                num(self.ramp_strength),
                "ramp: share of the way to the exact plane (0 to 1)",
            ),
            ToolOption::new(
                "slope_ramp_detail",
                num(self.ramp_detail),
                "ramp: share of the current relief kept on top of the plane (0 to 1)",
            ),
            ToolOption::new(
                "slope_uniform_strength",
                num(self.uniform_strength),
                "uniform slope: share of the way to a constant slope (0 to 1)",
            ),
            ToolOption::new(
                "slope_uniform_detail",
                num(self.uniform_detail),
                "uniform slope: share of the current relief kept (0 to 1)",
            ),
            ToolOption::new(
                "slope_path",
                path_text(&self.path),
                "path of the constant slope: points \"x y z; x y z\" in world units (at least 2); the first and last heights are the ends of the grade",
            ),
            ToolOption::new(
                "slope_path_width",
                num(self.path_width),
                "path: half width of the road in units (0.001 to 65536)",
            ),
            ToolOption::new(
                "slope_path_falloff",
                falloff_name(self.path_falloff),
                "path: edge curve: linear, smooth, ease_in or ease_out",
            ),
            ToolOption::new(
                "slope_path_strength",
                num(self.path_strength),
                "path: share of the way to the grade (0 to 1)",
            ),
            ToolOption::new(
                "slope_smooth_passes",
                self.smooth_passes.to_string(),
                "slope smoothing: passes (1 to 1000)",
            ),
            ToolOption::new(
                "slope_smooth_strength",
                num(self.smooth_strength),
                "slope smoothing: share moved to the neighbours average per pass (0 to 1)",
            ),
            ToolOption::new(
                "slope_select_angle",
                num(self.select_angle),
                "select by slope: angle in degrees from flat (0 to 90), default the Source walkable limit",
            ),
            ToolOption::new(
                "slope_select_mode",
                if self.select_above { "above" } else { "below" },
                "select by slope: above or below the angle",
            ),
            ToolOption::new(
                "slope_select_match",
                if self.select_match == SelectMatch::Any { "any" } else { "all" },
                "select by slope: a displacement counts when any or all of its triangles match",
            ),
            ToolOption::new(
                "slope_select_op",
                match self.select_op {
                    SelectOp::Add => "add",
                    SelectOp::Remove => "remove",
                    _ => "replace",
                },
                "select by slope: replace, add or remove",
            ),
        ]
    }

    pub fn set(&mut self, key: &str, value: &str) -> bool {
        let v = value.trim().to_ascii_lowercase();
        let number = v.parse::<f64>().ok().filter(|n| n.is_finite());
        let within = |lo: f64, hi: f64| number.filter(|n| (lo..=hi).contains(n));
        let flag = || match v.as_str() {
            "true" | "on" | "1" => Some(true),
            "false" | "off" | "0" => Some(false),
            _ => None,
        };
        match key {
            "slope_max_angle" => within(1.0, 89.0).map(|n| self.max_angle = n).is_some(),
            "slope_terrace" => flag().map(|b| self.terrace = b).is_some(),
            "slope_terrace_step" => within(1.0, 4096.0).map(|n| self.terrace_step = n).is_some(),
            "slope_terrace_strength" => within(0.0, 1.0).map(|n| self.terrace_strength = n).is_some(),
            "slope_ramp_angle" => within(0.0, 89.0).map(|n| self.ramp_angle = n).is_some(),
            "slope_ramp_direction" => {
                within(-360.0, 720.0).map(|n| self.ramp_direction = n.rem_euclid(360.0)).is_some()
            }
            "slope_ramp_pivot" => [RampPivot::Average, RampPivot::Lowest, RampPivot::Highest]
                .into_iter()
                .find(|p| pivot_name(*p) == v)
                .map(|p| self.ramp_pivot = p)
                .is_some(),
            "slope_ramp_strength" => within(0.0, 1.0).map(|n| self.ramp_strength = n).is_some(),
            "slope_ramp_detail" => within(0.0, 1.0).map(|n| self.ramp_detail = n).is_some(),
            "slope_uniform_strength" => within(0.0, 1.0).map(|n| self.uniform_strength = n).is_some(),
            "slope_uniform_detail" => within(0.0, 1.0).map(|n| self.uniform_detail = n).is_some(),
            "slope_path" => parse_path(value).map(|p| self.path = p).is_some(),
            "slope_path_width" => within(0.001, 65536.0).map(|n| self.path_width = n).is_some(),
            "slope_path_falloff" => FALLOFF_NAMES
                .iter()
                .position(|n| *n == v)
                .and_then(|i| BlendFalloff::ALL.get(i).copied())
                .map(|f| self.path_falloff = f)
                .is_some(),
            "slope_path_strength" => within(0.0, 1.0).map(|n| self.path_strength = n).is_some(),
            "slope_smooth_passes" => {
                v.parse::<u32>().ok().filter(|n| (1..=1000).contains(n)).map(|n| self.smooth_passes = n).is_some()
            }
            "slope_smooth_strength" => within(0.0, 1.0).map(|n| self.smooth_strength = n).is_some(),
            "slope_select_angle" => within(0.0, 90.0).map(|n| self.select_angle = n).is_some(),
            "slope_select_mode" => match v.as_str() {
                "above" => Some(true),
                "below" => Some(false),
                _ => None,
            }
            .map(|b| self.select_above = b)
            .is_some(),
            "slope_select_match" => match v.as_str() {
                "any" => Some(SelectMatch::Any),
                "all" => Some(SelectMatch::All),
                _ => None,
            }
            .map(|m| self.select_match = m)
            .is_some(),
            "slope_select_op" => match v.as_str() {
                "replace" => Some(SelectOp::Replace),
                "add" => Some(SelectOp::Add),
                "remove" => Some(SelectOp::Remove),
                _ => None,
            }
            .map(|o| self.select_op = o)
            .is_some(),
            _ => false,
        }
    }

    pub fn from_values(values: &[ToolOption]) -> Self {
        let mut out = Self::default();
        for o in values {
            out.set(o.key, &o.value);
        }
        out
    }
}

struct Graph {
    surface: Surface,
    field: SlopeField,
}

fn build_graph(parts: &[SmoothPart<'_>]) -> Graph {
    let surface = Surface::build(parts, |m, _| m.face_normal, |_, _| 1.0, Some(DVec3::Z));
    let xy = surface.nodes.iter().map(|n| n.pos.truncate()).collect();
    let h = surface.nodes.iter().map(|n| n.pos.z).collect();
    let mut tris = Vec::new();
    for (pi, part) in parts.iter().enumerate() {
        for t in part.mesh.indices.chunks_exact(3) {
            let n = |k: u32| surface.node(pi, k as usize);
            if let (Some(a), Some(b), Some(c)) = (n(t[0]), n(t[1]), n(t[2]))
                && a != b
                && b != c
                && a != c
            {
                tris.push([a, b, c]);
            }
        }
    }
    Graph { surface, field: SlopeField { xy, h, tris } }
}

fn commit(parts: &mut [SmoothPart<'_>], graph: &Graph) -> Vec<usize> {
    let mut changed = vec![0usize; parts.len()];
    let new_pos: Vec<DVec3> =
        graph.field.xy.iter().zip(&graph.field.h).map(|(xy, h)| DVec3::new(xy.x, xy.y, *h)).collect();
    write_back(parts, &graph.surface, &new_pos, &mut changed);
    changed
}

pub(crate) fn run_op(parts: &mut [SmoothPart<'_>], op: SlopeOp, o: &SlopeOptions) -> Vec<usize> {
    let mut graph = build_graph(parts);
    match op {
        SlopeOp::Limit => {
            if o.terrace {
                graph.field.terrace(o.terrace_step, o.terrace_strength);
            }
            graph.field.limit(o.max_angle);
        }
        SlopeOp::Ramp => {
            graph.field.ramp(o.ramp_angle, o.ramp_direction, o.ramp_pivot, o.ramp_strength, o.ramp_detail);
        }
        SlopeOp::Uniform => {
            graph.field.uniform(o.uniform_strength, o.uniform_detail);
        }
        SlopeOp::Path => {
            let xy: Vec<DVec2> = o.path.iter().map(|p| p.truncate()).collect();
            let ends = o.path.first().zip(o.path.last()).map(|(a, b)| (a.z, b.z));
            graph.field.along_path(&xy, ends, o.path_width, o.path_falloff, o.path_strength);
        }
        SlopeOp::Smooth => graph.field.smooth(o.smooth_passes, o.smooth_strength),
    }
    commit(parts, &graph)
}

fn presew(doc: &mut Document, faces: &[FaceRef]) -> Result<(), hp_editor::EditError> {
    let plan = plan_sew(doc, faces, &[]);
    if !plan.updates.is_empty() {
        doc.set_disp_data(UNDO_SEW, plan.updates)?;
    }
    Ok(())
}

pub fn apply(state: &mut AppState, op: SlopeOp, opts: &SlopeOptions) -> usize {
    if op == SlopeOp::Path && opts.path.len() < 2 {
        state.notify(Severity::Info, t!("disp.slope_path_need"));
        return 0;
    }
    let faces = targets(&state.doc);
    if faces.is_empty() {
        state.notify(Severity::Info, t!("disp.nothing_selected"));
        return 0;
    }
    state.doc.begin(op.label());
    let outcome = presew(&mut state.doc, &faces)
        .and_then(|()| reshape_doc(&mut state.doc, &faces, op.label(), |parts| run_op(parts, op, opts)));
    state.doc.commit();
    match outcome {
        Ok(moved) if moved > 0 => {
            state.notify(Severity::Success, tf!("disp.slope_done", faces.len()));
            moved
        }
        Ok(_) => {
            state.notify(Severity::Info, t!("disp.slope_nothing"));
            0
        }
        Err(e) => {
            state.notify(Severity::Warning, tf!("disp.failed", edit_error_text(&e)));
            0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measure {
    pub stats: SlopeStats,
    pub angle: f64,
    pub direction: f64,
    pub fitted: bool,
}

pub fn measure(doc: &Document, faces: &[FaceRef]) -> Option<Measure> {
    let mut work: Vec<(hp_geom::DispMesh, hp_vmf::DispInfo)> = Vec::new();
    for f in faces {
        let Some(node) = doc.scene().solid(f.solid) else { continue };
        let (Some(mesh), Some(disp)) =
            (node.disp_mesh(f.side), node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()))
        else {
            continue;
        };
        work.push(((**mesh).clone(), disp.clone()));
    }
    if work.is_empty() {
        return None;
    }
    let parts: Vec<SmoothPart<'_>> =
        work.iter_mut().map(|(mesh, disp)| SmoothPart { mesh, disp, pinned: false }).collect();
    let graph = build_graph(&parts);
    let stats = graph.field.stats();
    let fit = graph.field.fit_plane();
    Some(Measure {
        stats,
        angle: fit.map_or(0.0, |f| f.angle_degrees()),
        direction: fit.map_or(0.0, |f| f.azimuth_degrees()),
        fitted: fit.is_some(),
    })
}

pub fn face_matches(doc: &Document, face: FaceRef, opts: &SlopeOptions) -> bool {
    let Some(mesh) = doc.scene().solid(face.solid).and_then(|n| n.disp_mesh(face.side)) else { return false };
    let inclines = hp_geom::triangle_inclines(&mesh.positions, &mesh.indices);
    if inclines.is_empty() {
        return false;
    }
    let hit = |s: &f64| if opts.select_above { *s > opts.select_angle } else { *s <= opts.select_angle };
    match opts.select_match {
        SelectMatch::Any => inclines.iter().any(hit),
        SelectMatch::All => inclines.iter().all(hit),
    }
}

pub fn select_by_slope(state: &mut AppState, opts: &SlopeOptions) -> usize {
    let selected = selected_disp_faces(&state.doc);
    let candidates = if selected.is_empty() { state.doc.disp_faces() } else { selected };
    select_among(state, candidates, opts)
}

pub fn select_among(state: &mut AppState, candidates: Vec<FaceRef>, opts: &SlopeOptions) -> usize {
    if candidates.is_empty() {
        state.notify(Severity::Info, t!("disp.nothing_selected"));
        return 0;
    }
    let hits: Vec<FaceRef> = candidates.into_iter().filter(|f| face_matches(&state.doc, *f, opts)).collect();
    if hits.is_empty() {
        state.notify(Severity::Info, t!("disp.slope_select_none"));
        return 0;
    }
    state.doc.select_faces(&hits, opts.select_op);
    state.notify(Severity::Success, tf!("disp.slope_selected", hits.len()));
    hits.len()
}

pub fn path_from_entities(doc: &Document) -> Vec<DVec3> {
    doc.selected_objects()
        .into_iter()
        .filter_map(|o| match o {
            ObjRef::Entity(id) => doc.scene().entity(id).and_then(|e| e.entity.origin()),
            ObjRef::Solid(_) => None,
        })
        .collect()
}
