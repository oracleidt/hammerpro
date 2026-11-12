use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use egui::{RichText, vec2};
use glam::DVec3;
use hp_app::runtime::{CombineRequest, PropCreationRequest};
use hp_app::settings::{CombineOrigin, ModelLodSettings};
use hp_app::{AppState, Severity, t, tf, theme};
use hp_compile::{CompileEvent, CompilePlan, Step, StepKind};
use hp_editor::prop_export::{EditableCollision, PropDirectives, PropGeometry, PropOptions, PropTri, merge_prop_keys};
use hp_editor::unprop::PropTransform;
use hp_editor::{Document, Id};
use hp_fs::GameFs;
use hp_geom::Aabb;

use super::{PreparedProp, collision_pieces, creation, int_key, model_material, prepare_creation};
use crate::dialogs::MapDialog;
use crate::util::{escape_pressed, icon_title};

pub const MAX_TRIANGLES: usize = 150_000;
pub const MODEL_FOLDER: &str = "hammerpro/combined";

#[derive(Clone, Debug, PartialEq)]
pub struct CombineSettings {
    pub origin: CombineOrigin,
    pub keep_originals: bool,
    pub collision: bool,
    pub max_pieces: usize,
    pub model: String,
    pub destination: PathBuf,
    pub source_dir: PathBuf,
    pub lod: ModelLodSettings,
}

impl CombineSettings {
    pub fn from_state(state: &AppState, props: &[Id]) -> Self {
        let lab = &state.settings.lab;
        Self {
            origin: lab.combine_origin,
            keep_originals: lab.combine_keep_originals,
            collision: lab.combine_collision,
            max_pieces: lab.combine_pieces(),
            model: default_model(state, props),
            destination: default_destination(state),
            source_dir: hp_app::settings::Settings::cache_dir().join("propsrc"),
            lod: lab.model_lod,
        }
    }
}

pub fn default_model(state: &AppState, props: &[Id]) -> String {
    let mut hash = 0x811c_9dc5u32;
    for byte in props.iter().flat_map(|id| id.to_le_bytes()) {
        hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
    }
    let stem = state.doc.path().and_then(|path| path.file_stem()).map_or("map".into(), |s| s.to_string_lossy());
    let stem: String = stem.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(40).collect();
    format!("{MODEL_FOLDER}/{}_{hash:08x}", stem.to_ascii_lowercase())
}

pub fn default_destination(state: &AppState) -> PathBuf {
    state
        .assets
        .fs()
        .and_then(|fs| fs.game_dir())
        .map(PathBuf::from)
        .or_else(|| state.settings.game.gmod_root.as_ref().map(|root| root.join("garrysmod")))
        .unwrap_or_else(|| hp_app::settings::Settings::cache_dir().join("prop_exports"))
}

struct LoadedProp {
    tris: Vec<PropTri>,
    pieces: Vec<Vec<DVec3>>,
    points: Vec<DVec3>,
    materials: BTreeSet<String>,
    surface: String,
}

fn load_prop(fs: &GameFs, entity: &hp_vmf::Entity, collision: bool) -> Result<LoadedProp, String> {
    let model_path = hp_mdl::normalize_material_name(entity.get("model").unwrap_or_default());
    let model_path = if model_path.ends_with(".mdl") { model_path } else { format!("{model_path}.mdl") };
    let Some(mdl) = fs.read(&model_path) else {
        return Err(tf!("propper.model_missing", model_path));
    };
    let body = int_key(entity, "body").unwrap_or(0);
    let opts = hp_mdl::LoadOptions { body_groups: hp_mdl::BodyGroupSelect::Body(body), ..Default::default() };
    let model = hp_mdl::load_from(&model_path, |p| fs.read(p), &opts)
        .map_err(|e| tf!("propper.model_bad", model_path.clone(), e.to_string()))?;
    if model.bones.len() > 1 {
        return Err(tf!("propper.combine_bones", model_path, model.bones.len()));
    }
    let transform = PropTransform::of(entity);
    let skin = int_key(entity, "skin").unwrap_or(0) as usize;
    let mut out = LoadedProp {
        tris: Vec::new(),
        pieces: Vec::new(),
        points: Vec::new(),
        materials: BTreeSet::new(),
        surface: model.surface_prop.clone(),
    };
    let normal_of = |n: [f32; 3]| {
        let n = DVec3::new(f64::from(n[0]), f64::from(n[1]), f64::from(n[2]));
        (transform.rotation * n).try_normalize().unwrap_or(DVec3::Z)
    };
    for mesh in &model.meshes {
        let material = model_material(fs, &model, mesh.skin_ref, skin);
        if material.is_empty() {
            continue;
        }
        out.materials.insert(material.clone());
        for t in mesh.indices.chunks_exact(3) {
            let Some(v) = t.iter().map(|&i| mesh.vertices.get(i as usize)).collect::<Option<Vec<_>>>() else {
                continue;
            };
            let pos = [transform.apply(v[0].pos), transform.apply(v[1].pos), transform.apply(v[2].pos)];
            out.points.extend(pos);
            out.tris.push(PropTri {
                material: material.clone(),
                pos,
                normal: [normal_of(v[0].normal), normal_of(v[1].normal), normal_of(v[2].normal)],
                uv: [v[0].uv, v[1].uv, v[2].uv].map(|uv| [f64::from(uv[0]), f64::from(uv[1])]),
                disp: None,
            });
        }
    }
    let solid = entity.get("solid").is_none_or(|s| s.trim() != "0");
    if collision && solid {
        out.pieces = fs
            .read(&hp_mdl::phy_path(&model_path))
            .and_then(|phy| hp_mdl::parse_phy_for_model(&phy, &mdl, hp_mdl::Pose::FirstSequence).ok())
            .map(|c| collision_pieces(&c, &transform))
            .unwrap_or_default();
        if out.pieces.is_empty() && out.points.len() >= 4 {
            out.pieces.push(out.points.clone());
        }
    }
    Ok(out)
}

fn hull_triangles(points: &[DVec3]) -> Option<Vec<[DVec3; 3]>> {
    let hull = EditableCollision::new(points.to_vec()).ok()?;
    let tris = hull.triangles().to_vec();
    (!tris.is_empty()).then_some(tris)
}

pub fn reduce_pieces(per_prop: Vec<Vec<Vec<DVec3>>>, max: usize) -> Vec<Vec<DVec3>> {
    let max = max.max(1);
    let total: usize = per_prop.iter().map(Vec::len).sum();
    if total <= max {
        return per_prop.into_iter().flatten().collect();
    }
    let mut merged: Vec<Vec<DVec3>> =
        per_prop.into_iter().filter(|p| !p.is_empty()).map(|p| p.into_iter().flatten().collect()).collect();
    if merged.len() <= max {
        return merged;
    }
    let bounds = Aabb::from_points(merged.iter().flatten().copied());
    let size = bounds.size();
    let axis = if size.x >= size.y && size.x >= size.z {
        0
    } else if size.y >= size.z {
        1
    } else {
        2
    };
    let center = |p: &Vec<DVec3>| p.iter().map(|v| v[axis]).sum::<f64>() / p.len().max(1) as f64;
    merged.sort_by(|a, b| center(a).total_cmp(&center(b)));
    let per = merged.len().div_ceil(max);
    merged.chunks(per).map(|c| c.iter().flatten().copied().collect()).collect()
}

pub fn plan(fs: &GameFs, doc: &Document, props: &[Id], settings: &CombineSettings) -> Result<CombineRequest, String> {
    let model = creation::model_path(&settings.model).ok_or_else(|| t!("propper.invalid_model_path").to_owned())?;
    let mut unique: Vec<Id> = Vec::new();
    for id in props {
        if !unique.contains(id) {
            unique.push(*id);
        }
    }
    let mut entities = Vec::with_capacity(unique.len());
    for id in &unique {
        let node = doc.scene().entity(*id).ok_or_else(|| tf!("propper.combine_missing", id))?;
        let e = &node.entity;
        if !e.classname.eq_ignore_ascii_case("prop_static") || !hp_app::actions::is_model_prop(e) {
            return Err(tf!("propper.combine_not_static", id, e.classname.clone()));
        }
        entities.push(e.clone());
    }
    if entities.len() < 2 {
        return Err(t!("propper.combine_select").to_owned());
    }
    let mut tris = Vec::new();
    let mut pieces = Vec::new();
    let mut before = 0usize;
    let mut materials = BTreeSet::new();
    let mut surfaces = Vec::new();
    for e in &entities {
        let loaded = load_prop(fs, e, settings.collision)?;
        before += loaded.materials.len();
        materials.extend(loaded.materials.iter().cloned());
        surfaces.push(loaded.surface.clone());
        tris.extend(loaded.tris);
        pieces.push(loaded.pieces);
    }
    if tris.is_empty() {
        return Err(t!("propper.empty").to_owned());
    }
    if tris.len() > MAX_TRIANGLES {
        return Err(tf!("propper.combine_too_big", tris.len(), MAX_TRIANGLES));
    }
    let props_kv: Vec<&[(String, String)]> = entities.iter().map(|e| e.props.as_slice()).collect();
    let merged = merge_prop_keys(&props_kv);
    if !merged.conflicts.is_empty() {
        return Err(tf!("propper.combine_conflict", merged.conflicts.join(", ")));
    }
    let bounds = Aabb::from_points(tris.iter().flat_map(|t| t.pos));
    let c = bounds.center();
    let origin = match settings.origin {
        CombineOrigin::Center => c,
        CombineOrigin::Bottom => DVec3::new(c.x, c.y, bounds.min.z),
    }
    .round();
    for t in &mut tris {
        for p in &mut t.pos {
            *p -= origin;
        }
    }
    let pieces: Vec<Vec<[DVec3; 3]>> = reduce_pieces(pieces, settings.max_pieces)
        .into_iter()
        .filter_map(|points| hull_triangles(&points.iter().map(|p| *p - origin).collect::<Vec<_>>()))
        .collect();
    let collision = settings.collision && merged.solid && !pieces.is_empty();
    let mut keys = merged.keys.clone();
    if let Some(slot) = keys.iter_mut().find(|(k, _)| k == "solid") {
        slot.1 = if collision { "6" } else { "0" }.to_owned();
    }
    let mut warnings = Vec::new();
    if !merged.dropped.is_empty() {
        warnings.push(tf!("propper.combine_fade_dropped", merged.dropped.join(", ")));
    }
    let known = fs.surface_properties();
    let surface = surfaces
        .iter()
        .find(|s| known.iter().any(|k| k.eq_ignore_ascii_case(s)))
        .cloned()
        .unwrap_or_else(|| "default".to_owned());
    let geometry =
        PropGeometry { tris, disps: Vec::new(), pieces: if collision { pieces } else { Vec::new() }, origin };
    let creation = PropCreationRequest {
        source_serial: doc.revision(),
        solid_ids: Vec::new(),
        geometry,
        options: PropOptions { physics: false, static_prop: true, collision, mass: 10.0 },
        directives: PropDirectives::default(),
        model,
        source_dir: settings.source_dir.clone(),
        destination: settings.destination.clone(),
        surface_prop: surface,
        reuse_materials: true,
        lod: settings.lod,
    };
    Ok(CombineRequest {
        props: unique,
        creation,
        keys,
        keep_originals: settings.keep_originals,
        draw_calls: (before, materials.len()),
        warnings,
    })
}

pub fn summary(request: &CombineRequest) -> String {
    tf!(
        "propper.combine_summary",
        request.props.len(),
        request.creation.geometry.tris.len(),
        request.draw_calls.0,
        request.draw_calls.1,
        request.creation.geometry.pieces.len()
    )
}

pub fn build(
    fs: &GameFs,
    request: PropCreationRequest,
    exe: &Path,
    extra_args: &str,
    log: &mut dyn FnMut(hp_compile::Severity, String),
) -> Result<PreparedProp, String> {
    if hp_compile::sandbox::is_sandboxed() {
        return Err(t!("propper.combine_sandboxed").to_owned());
    }
    let mut prepared = prepare_creation(fs, request)?;
    let args = hp_app::model_compiler::args(&prepared.game_dir, &prepared.qc, hp_compile::split_args(extra_args));
    let mut plan = CompilePlan::new(&prepared.qc);
    plan.steps.push(Step::new(StepKind::Custom, exe, args));
    let dir = prepared.qc.parent().map(Path::to_path_buf).unwrap_or_default();
    let handle = hp_compile::start(plan, &dir);
    let mut success = false;
    let mut errors = 0usize;
    loop {
        match handle.events.recv() {
            Ok(CompileEvent::StepStarted { command_line, .. }) => log(hp_compile::Severity::Info, command_line),
            Ok(CompileEvent::Log { line, severity, .. }) => {
                if line.trim_start().to_ascii_uppercase().starts_with("ERROR") {
                    errors += 1;
                }
                log(severity, line);
            }
            Ok(CompileEvent::Finished { success: ok, .. }) => {
                success = ok;
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    if !success {
        let name = exe.file_name().map_or_else(|| exe.display().to_string(), |n| n.to_string_lossy().into_owned());
        return Err(tf!("propper.combine_compile_failed", name, errors));
    }
    prepared.finish_creation()?;
    Ok(prepared)
}

type Planned = (u64, Vec<Id>, CombineSettings, Result<CombineRequest, String>);

#[derive(Default)]
pub(crate) struct CombineDialog {
    settings: Option<CombineSettings>,
    props: Vec<Id>,
    planned: Option<Planned>,
}

impl CombineDialog {
    fn refresh(&mut self, state: &AppState) {
        let props = hp_app::actions::selected_static_props(state);
        if self.settings.is_none() || props != self.props {
            self.settings = Some(CombineSettings::from_state(state, &props));
            self.props = props;
        }
        let Some(settings) = self.settings.clone() else { return };
        let key = state.doc.revision() ^ state.assets.generation();
        let same = |s: &CombineSettings| {
            (s.origin, s.collision, s.max_pieces, s.keep_originals)
                == (settings.origin, settings.collision, settings.max_pieces, settings.keep_originals)
        };
        let stale = self.planned.as_ref().is_none_or(|(k, p, s, _)| *k != key || *p != self.props || !same(s));
        if stale {
            let result = match state.assets.fs() {
                Some(fs) => plan(fs, &state.doc, &self.props, &settings),
                None => Err(t!("propper.no_content").to_owned()),
            };
            self.planned = Some((key, self.props.clone(), settings, result));
        }
    }

    fn body(&mut self, ui: &mut egui::Ui, state: &mut AppState) -> bool {
        let mut start = false;
        ui.label(RichText::new(t!("propper.combine_intro")).weak());
        ui.add_space(4.0);
        let Some(s) = self.settings.as_mut() else { return false };
        let mut changed = false;
        egui::Grid::new("hp_combine_props").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label(t!("propper.combine_model"));
            ui.add(egui::TextEdit::singleline(&mut s.model).desired_width(280.0).font(egui::TextStyle::Monospace));
            ui.end_row();
            ui.label(t!("propper.destination"));
            let mut dest = s.destination.to_string_lossy().into_owned();
            if ui.add(egui::TextEdit::singleline(&mut dest).desired_width(280.0)).changed() {
                s.destination = PathBuf::from(dest);
            }
            ui.end_row();
            ui.label(t!("propper.combine_origin"));
            ui.horizontal(|ui| {
                changed |= ui
                    .selectable_value(&mut s.origin, CombineOrigin::Center, t!("propper.combine_origin_center"))
                    .changed();
                changed |= ui
                    .selectable_value(&mut s.origin, CombineOrigin::Bottom, t!("propper.combine_origin_bottom"))
                    .changed();
            });
            ui.end_row();
            ui.label(t!("propper.collision"));
            changed |= ui.checkbox(&mut s.collision, "").changed();
            ui.end_row();
            ui.label(t!("propper.combine_pieces"));
            let mut pieces = s.max_pieces as u32;
            if ui
                .add_enabled(
                    s.collision,
                    egui::DragValue::new(&mut pieces).range(1..=hp_app::settings::LabSettings::MAX_COMBINE_PIECES),
                )
                .on_hover_text(t!("propper.combine_pieces_tip"))
                .changed()
            {
                s.max_pieces = pieces as usize;
                changed = true;
            }
            ui.end_row();
            ui.label(t!("propper.combine_keep"));
            changed |= ui.checkbox(&mut s.keep_originals, "").on_hover_text(t!("propper.combine_keep_tip")).changed();
            ui.end_row();
        });
        if changed {
            let lab = &mut state.settings.lab;
            lab.combine_origin = s.origin;
            lab.combine_collision = s.collision;
            lab.combine_max_pieces = s.max_pieces as u32;
            lab.combine_keep_originals = s.keep_originals;
            state.settings_dirty = true;
        }
        ui.separator();
        let compiler = hp_app::model_compiler::name(&state.settings);
        match self.planned.as_ref().map(|(_, _, _, r)| r) {
            Some(Ok(request)) => {
                ui.label(summary(request));
                for w in &request.warnings {
                    ui.label(RichText::new(w).color(theme::warning()));
                }
                if s.keep_originals && state.settings.compile.compile_hidden {
                    ui.label(RichText::new(t!("propper.combine_hidden_compiled")).color(theme::warning()));
                }
                ui.label(RichText::new(tf!("propper.combine_compiler", compiler)).small().weak());
            }
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(theme::warning()));
            }
            None => {}
        }
        ui.separator();
        ui.horizontal(|ui| {
            let ready = matches!(self.planned, Some((_, _, _, Ok(_)))) && !state.runtime.lab.combine_busy;
            if ui.add_enabled(ready, egui::Button::new(t!("propper.combine_go"))).clicked() {
                start = true;
            }
            if state.runtime.lab.combine_busy {
                ui.spinner();
                ui.label(RichText::new(t!("propper.combine_running")).weak());
            }
        });
        start
    }
}

impl MapDialog for CombineDialog {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.runtime.lab.combine_open {
            self.settings = None;
            self.planned = None;
            return;
        }
        self.refresh(state);
        let mut open = true;
        let mut start = false;
        let response = hp_app::chrome::Window::new(icon_title("🧩", t!("propper.combine_title")))
            .id(egui::Id::unique("hp_combine_props"))
            .default_pos(crate::util::centered_pos(ctx, vec2(440.0, 320.0)))
            .open(&mut open)
            .default_width(440.0)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| start = self.body(ui, state));
        let escape = response.as_ref().is_some_and(|r| escape_pressed(ctx, &r.response));
        if start && let Some(settings) = self.settings.clone() {
            let fresh = match state.assets.fs() {
                Some(fs) => plan(fs, &state.doc, &self.props, &settings),
                None => Err(t!("propper.no_content").to_owned()),
            };
            match fresh {
                Ok(request) => {
                    state.console.info(summary(&request));
                    state.runtime.lab.combine_request = Some(Box::new(request));
                    open = false;
                }
                Err(e) => state.notify(Severity::Warning, e),
            }
        }
        if !open || escape {
            state.runtime.lab.combine_open = false;
        }
    }
}

pub fn report_done(state: &mut AppState, request: &CombineRequest, model: &str) {
    let mut text = tf!("propper.combine_done", request.props.len(), format!("models/{model}.mdl"));
    text.push(' ');
    text.push_str(&tf!("propper.combine_draw_calls", request.draw_calls.0, request.draw_calls.1));
    state.notify(Severity::Success, text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn cube(at: f64) -> Vec<DVec3> {
        let mut out = Vec::new();
        for i in 0..8 {
            out.push(DVec3::new(
                at + f64::from(i & 1) * 8.0,
                f64::from((i >> 1) & 1) * 8.0,
                f64::from((i >> 2) & 1) * 8.0,
            ));
        }
        out
    }

    #[test]
    fn collision_pieces_are_capped() {
        let per_prop: Vec<Vec<Vec<DVec3>>> = (0..10).map(|i| vec![cube(f64::from(i) * 20.0); 3]).collect();
        assert_eq!(reduce_pieces(per_prop.clone(), 64).len(), 30, "under the cap nothing is merged");
        assert_eq!(reduce_pieces(per_prop.clone(), 12).len(), 10, "one hull per prop");
        let few = reduce_pieces(per_prop, 4);
        assert!(few.len() <= 4 && !few.is_empty());
        for piece in &few {
            assert!(hull_triangles(piece).is_some());
        }
    }

    #[test]
    #[ignore = "requires Garry's Mod and its studiomdl; set HP_TEST_GMOD and HP_TEST_STUDIOMDL"]
    fn props_combine_into_one_compiled_static_model() {
        let root = PathBuf::from(std::env::var_os("HP_TEST_GMOD").expect("HP_TEST_GMOD"));
        let exe = PathBuf::from(std::env::var_os("HP_TEST_STUDIOMDL").expect("HP_TEST_STUDIOMDL"));
        let out =
            PathBuf::from(std::env::var_os("HP_TEST_OUT").unwrap_or_else(|| std::env::temp_dir().into_os_string()))
                .join("hp_combine");
        let fs = GameFs::for_gmod(&root, &hp_fs::MountOptions::default()).expect("game");
        let mut doc = Document::new_empty();
        let mut ids = Vec::new();
        for (model, origin, skin, yaw) in [
            ("models/props_c17/oildrum001.mdl", DVec3::new(-80.0, -40.0, 0.0), "0", "0"),
            ("models/props_c17/oildrum001.mdl", DVec3::new(-40.0, -40.0, 0.0), "1", "45"),
            ("models/props_junk/wood_crate001a.mdl", DVec3::new(40.0, 30.0, 20.0), "0", "20"),
            ("models/props_borealis/bluebarrel001.mdl", DVec3::new(0.0, 60.0, 24.0), "0", "0"),
        ] {
            let kv: Vec<(String, String)> = [("model", model), ("skin", skin), ("angles", &format!("0 {yaw} 0"))]
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect();
            ids.push(doc.create_point_entity("prop_static", origin, &kv).unwrap());
        }
        let settings = CombineSettings {
            origin: CombineOrigin::Bottom,
            keep_originals: true,
            collision: true,
            max_pieces: 32,
            model: "hammerpro/combined/test".to_owned(),
            destination: out.join("game"),
            source_dir: out.join("src"),
            lod: ModelLodSettings::default(),
        };
        let request = plan(&fs, &doc, &ids, &settings).unwrap();
        assert!(request.draw_calls.1 <= request.draw_calls.0);
        assert!(!request.creation.geometry.pieces.is_empty());
        let tris = request.creation.geometry.tris.len();
        let mut lines = Vec::new();
        let prepared = build(&fs, request.creation.clone(), &exe, "", &mut |_, l| lines.push(l)).unwrap_or_else(|e| {
            panic!(
                "{e}
{}",
                lines.join(
                    "
"
                )
            );
        });
        let base = out.join("game").join("models/hammerpro/combined/test");
        assert!(base.with_extension("mdl").is_file() && base.with_extension("phy").is_file());
        let model = hp_mdl::load_from(
            "models/hammerpro/combined/test.mdl",
            |name| std::fs::read(out.join("game").join(name)).ok(),
            &hp_mdl::LoadOptions::default(),
        )
        .unwrap();
        assert!(model.is_static_prop);
        assert_eq!(model.triangle_count(), tris);
        let materials: BTreeSet<usize> = model.meshes.iter().map(|m| m.material_index).collect();
        assert_eq!(materials.len(), request.draw_calls.1);
        let id = doc.combine_props(&request.props, &prepared.model, prepared.origin, &request.keys, true).unwrap();
        assert_eq!(doc.scene().entity(id).unwrap().entity.get("model"), Some("models/hammerpro/combined/test.mdl"));
        if std::env::var_os("HP_TEST_KEEP").is_none() {
            let _ = std::fs::remove_dir_all(&out);
        }
    }
}
