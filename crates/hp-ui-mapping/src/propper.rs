use std::collections::{HashMap, HashSet};

use egui::{RichText, vec2};
use glam::{DVec2, DVec3};
use hp_app::settings::UnpropShape;
use hp_app::{AppState, Severity, t, tf, theme};
use hp_editor::Id;
use hp_editor::unprop::{self, ModelTri, PropTransform, UnpropError, Unpropped};
use hp_vmf::Entity;

use crate::dialogs::MapDialog;
use crate::util::{enter_pressed, escape_pressed, icon_title, plural};

pub mod bake;

pub(crate) fn dialogs() -> Vec<Box<dyn MapDialog>> {
    vec![Box::new(UnpropDialog::default()), Box::new(OptionsDialog)]
}

pub fn lod_options(s: hp_app::settings::ModelLodSettings) -> hp_meshio::lod::LodOptions {
    let s = s.clamped();
    hp_meshio::lod::LodOptions { enabled: s.enabled, levels: s.levels as usize, reduction: f64::from(s.reduction) }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedProp {
    pub qc: std::path::PathBuf,
    pub model: String,
    pub origin: DVec3,
    pub lod_triangles: Vec<usize>,
    pub pieces: usize,
    pub surface_prop: String,
    pub baked: Option<(usize, (u32, u32), bool)>,
}

impl PreparedProp {
    pub fn details(&self) -> String {
        let tris: Vec<String> = self.lod_triangles.iter().map(ToString::to_string).collect();
        let mut text = tf!(
            "propper.details",
            self.lod_triangles.len().saturating_sub(1),
            tris.join(" / "),
            self.pieces,
            self.surface_prop.clone()
        );
        if let Some((faces, (w, h), _)) = self.baked {
            text.push(' ');
            text.push_str(&tf!("propper.baked", faces, w, h));
        }
        text
    }
}

pub fn prepare_prop(
    fs: Option<&hp_fs::GameFs>,
    mut geo: hp_editor::prop_export::PropGeometry,
    model: &str,
    lod: hp_app::settings::ModelLodSettings,
    src_dir: &std::path::Path,
    game_dir: &std::path::Path,
) -> Result<PreparedProp, String> {
    let failed = |e: std::io::Error| tf!("propper.write_failed", e.to_string());
    let surface_prop =
        fs.and_then(|fs| bake::dominant_surface_prop(fs, &geo.tris)).unwrap_or_else(|| "default".to_owned());
    let folder = format!("models/{}", model.trim_matches('/'));
    let mut baked = None;
    if let Some(fs) = fs
        && let Some(b) = bake::bake_blends(fs, &mut geo, &folder, &surface_prop)
    {
        for (path, bytes) in &b.files {
            let p = game_dir.join(path);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(failed)?;
            }
            std::fs::write(&p, bytes).map_err(failed)?;
        }
        baked = Some((b.faces, b.size, b.bump));
    }
    let lods = prop_lods(&geo.tris, lod);
    let export = geo.files(model, &surface_prop, &lods);
    let stem = model.rsplit('/').next().unwrap_or("prop").to_owned();
    std::fs::create_dir_all(src_dir).map_err(failed)?;
    let qc = src_dir.join(format!("{stem}.qc"));
    std::fs::write(src_dir.join(format!("{stem}_ref.smd")), &export.smd).map_err(failed)?;
    if !export.phys_smd.is_empty() {
        std::fs::write(src_dir.join(format!("{stem}_phys.smd")), &export.phys_smd).map_err(failed)?;
    }
    for (i, smd) in export.lod_smds.iter().enumerate() {
        std::fs::write(src_dir.join(format!("{stem}_lod{}.smd", i + 1)), smd).map_err(failed)?;
    }
    std::fs::write(&qc, &export.qc).map_err(failed)?;
    let mut lod_triangles = vec![export.triangles];
    lod_triangles.extend(lods.iter().filter(|l| !l.tris.is_empty()).map(|l| l.tris.len()));
    Ok(PreparedProp {
        qc,
        model: model.to_owned(),
        origin: export.origin,
        lod_triangles,
        pieces: export.pieces,
        surface_prop,
        baked,
    })
}

pub fn prop_lods(
    tris: &[hp_editor::prop_export::PropTri],
    lod: hp_app::settings::ModelLodSettings,
) -> Vec<hp_editor::prop_export::PropLod> {
    let radius = tris.iter().flat_map(|t| t.pos.iter()).map(|p| p.length()).fold(0.0f64, f64::max);
    let levels = hp_meshio::lod::plan(&lod_options(lod), tris.len(), radius);
    if levels.is_empty() {
        return Vec::new();
    }
    let mut materials: Vec<String> = Vec::new();
    let soup: Vec<hp_meshio::Triangle> = tris
        .iter()
        .map(|t| {
            let material = match materials.iter().position(|m| *m == t.material) {
                Some(i) => i,
                None => {
                    materials.push(t.material.clone());
                    materials.len() - 1
                }
            };
            hp_meshio::Triangle { pos: t.pos, normal: t.normal, uv: t.uv.map(|u| DVec2::new(u[0], u[1])), material }
        })
        .collect();
    hp_meshio::lod::simplify_triangles(&soup, &levels)
        .into_iter()
        .zip(&levels)
        .map(|(level, l)| hp_editor::prop_export::PropLod {
            switch: l.switch,
            tris: level
                .into_iter()
                .map(|t| hp_editor::prop_export::PropTri {
                    material: materials[t.material].clone(),
                    pos: t.pos,
                    normal: t.normal,
                    uv: t.uv.map(|u| [u.x, u.y]),
                    disp: None,
                })
                .collect(),
        })
        .collect()
}

struct OptionsDialog;

impl MapDialog for OptionsDialog {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.runtime.lab.propper_options_open {
            return;
        }
        let mut open = true;
        let mut close = false;
        let response = hp_app::chrome::Window::new(icon_title("⚙", t!("propper.options_title")))
            .id(egui::Id::unique("hp_propper_options"))
            .default_pos(crate::util::centered_pos(ctx, vec2(400.0, 240.0)))
            .open(&mut open)
            .default_width(400.0)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(RichText::new(t!("propper.options_intro")).weak());
                ui.add_space(4.0);
                if hp_app::widgets::model_lod_ui(ui, &mut state.settings.lab.model_lod) {
                    state.settings_dirty = true;
                }
                ui.add_space(4.0);
                ui.label(RichText::new(t!("propper.options_collision")).weak());
                ui.label(RichText::new(t!("propper.options_blend")).weak());
                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t!("common.close")).clicked() {
                            close = true;
                        }
                    });
                });
            });
        if response.as_ref().is_some_and(|r| escape_pressed(ctx, &r.response)) {
            close = true;
        }
        if !open || close {
            state.runtime.lab.propper_options_open = false;
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PropSource {
    pub model: String,
    pub tris: Vec<ModelTri>,
    pub collision: Vec<Vec<DVec3>>,
    pub sizes: HashMap<String, (u32, u32)>,
}

#[derive(Clone, Debug)]
pub struct PropBrushes {
    pub brushes: Unpropped,
    pub approximate: usize,
    pub from_collision: bool,
}

pub fn collision_pieces(model: &hp_mdl::CollisionModel, transform: &PropTransform) -> Vec<Vec<DVec3>> {
    let mut pieces = Vec::new();
    for solid in &model.solids {
        for range in &solid.ledges {
            let mut seen = HashSet::new();
            let mut points = Vec::new();
            for t in solid.triangles.get(range.clone()).unwrap_or(&[]) {
                for &i in t {
                    if seen.insert(i)
                        && let Some(v) = solid.vertices.get(i as usize)
                    {
                        points.push(transform.apply(*v));
                    }
                }
            }
            if !points.is_empty() {
                pieces.push(points);
            }
        }
    }
    pieces
}

fn int_key(e: &Entity, key: &str) -> Option<u32> {
    e.get(key).and_then(|s| s.trim().parse::<f64>().ok()).filter(|v| v.is_finite() && *v >= 0.0).map(|v| v as u32)
}

pub fn read_prop_source(fs: &hp_fs::GameFs, entity: &Entity) -> Result<PropSource, String> {
    let model_path = hp_mdl::normalize_material_name(entity.get("model").unwrap_or_default());
    let model_path = if model_path.ends_with(".mdl") { model_path } else { format!("{model_path}.mdl") };
    let Some(mdl) = fs.read(&model_path) else {
        return Err(tf!("propper.model_missing", model_path));
    };
    let body = int_key(entity, "body").unwrap_or(0);
    let opts = hp_mdl::LoadOptions { body_groups: hp_mdl::BodyGroupSelect::Body(body), ..Default::default() };
    let model = hp_mdl::load_from(&model_path, |p| fs.read(p), &opts)
        .map_err(|e| tf!("propper.model_bad", model_path.clone(), e.to_string()))?;
    let transform = PropTransform::of(entity);
    let skin = int_key(entity, "skin").unwrap_or(0) as usize;
    let mut materials: HashMap<usize, String> = HashMap::new();
    let mut tris = Vec::new();
    for mesh in &model.meshes {
        let material = materials
            .entry(mesh.skin_ref)
            .or_insert_with(|| {
                let candidates = model.material_candidates(mesh.skin_ref, skin);
                candidates
                    .iter()
                    .find(|c| fs.exists(&format!("materials/{c}.vmt")))
                    .or_else(|| candidates.first())
                    .cloned()
                    .unwrap_or_default()
            })
            .clone();
        for t in mesh.indices.chunks_exact(3) {
            let Some(v) = t.iter().map(|&i| mesh.vertices.get(i as usize)).collect::<Option<Vec<_>>>() else {
                continue;
            };
            tris.push(ModelTri {
                material: material.clone(),
                pos: [transform.apply(v[0].pos), transform.apply(v[1].pos), transform.apply(v[2].pos)],
                uv: [v[0].uv, v[1].uv, v[2].uv].map(|uv| DVec2::new(f64::from(uv[0]), f64::from(uv[1]))),
            });
        }
    }
    let collision = fs
        .read(&hp_mdl::phy_path(&model_path))
        .and_then(|phy| hp_mdl::parse_phy_for_model(&phy, &mdl, hp_mdl::Pose::FirstSequence).ok())
        .map(|c| collision_pieces(&c, &transform))
        .unwrap_or_default();
    let mut sizes = HashMap::new();
    for t in &tris {
        if !sizes.contains_key(&t.material) {
            let size = fs.material_texture_size(&t.material).filter(|s| s.0 > 0 && s.1 > 0).unwrap_or((512, 512));
            sizes.insert(t.material.clone(), size);
        }
    }
    Ok(PropSource { model: model_path, tris, collision, sizes })
}

fn error_text(e: &UnpropError) -> String {
    match e {
        UnpropError::TooManyTriangles(n) => tf!("propper.too_many_tris", n, unprop::MAX_RENDER_TRIANGLES),
        UnpropError::TooManyPieces(n) => tf!("propper.too_many_pieces", n, unprop::MAX_PIECES),
        UnpropError::NoBrush => t!("propper.no_brush").to_owned(),
    }
}

pub fn build_brushes(source: &PropSource, shape: UnpropShape) -> Result<PropBrushes, String> {
    let from_collision = match shape {
        UnpropShape::Auto => !source.collision.is_empty(),
        UnpropShape::Collision if source.collision.is_empty() => return Err(t!("propper.no_phy").to_owned()),
        UnpropShape::Collision => true,
        UnpropShape::Render => false,
    };
    let (pieces, approximate) = if from_collision {
        (source.collision.clone(), 0)
    } else {
        let split = unprop::render_pieces(&source.tris).map_err(|e| error_text(&e))?;
        (split.pieces, split.approximate)
    };
    let size = |m: &str| source.sizes.get(m).copied().unwrap_or((512, 512));
    let brushes = unprop::brushes_from_pieces(&pieces, &source.tris, &size).map_err(|e| error_text(&e))?;
    Ok(PropBrushes { brushes, approximate, from_collision })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnpropOptions {
    pub detail: bool,
    pub keep_prop: bool,
    pub shape: UnpropShape,
}

impl UnpropOptions {
    pub fn from_settings(state: &AppState) -> Self {
        let s = &state.settings.lab;
        Self { detail: s.unprop_detail, keep_prop: s.unprop_keep_prop, shape: s.unprop_shape }
    }
}

pub fn report_text(made: &PropBrushes) -> String {
    let b = &made.brushes;
    let n = b.solids.len();
    let mut text = tf!(plural(n, "propper.done_one", "propper.done"), n, b.textured_faces, b.nodraw_faces);
    if made.approximate > 0 {
        text.push(' ');
        text.push_str(&tf!("propper.approximate", made.approximate));
    }
    if b.skipped > 0 {
        text.push(' ');
        text.push_str(&tf!("propper.skipped", b.skipped));
    }
    text
}

pub fn prop_to_brushes(state: &mut AppState, prop: Id, opts: &UnpropOptions) -> Result<(PropBrushes, Vec<Id>), String> {
    let source = prop_source(state, prop)?;
    let made = build_brushes(&source, opts.shape)?;
    let ids = state
        .doc
        .replace_prop_with_brushes(prop, made.brushes.solids.clone(), opts.detail, opts.keep_prop)
        .map_err(|e| tf!("propper.failed", e.to_string()))?;
    Ok((made, ids))
}

pub fn prop_source(state: &AppState, prop: Id) -> Result<PropSource, String> {
    let entity = match state.doc.scene().entity(prop) {
        Some(n) if hp_app::actions::is_model_prop(&n.entity) => n.entity.clone(),
        _ => return Err(t!("propper.select").to_owned()),
    };
    let Some(fs) = state.assets.fs() else {
        return Err(t!("propper.no_content").to_owned());
    };
    read_prop_source(fs, &entity)
}

type LoadedSource = ((Id, String, u64), Result<PropSource, String>);

#[derive(Default)]
struct UnpropDialog {
    was_open: bool,
    source: Option<LoadedSource>,
    error: Option<String>,
}

impl UnpropDialog {
    fn refresh(&mut self, state: &AppState) -> Option<Id> {
        let prop = hp_app::actions::selected_prop(state)?;
        let model = state.doc.scene().entity(prop).and_then(|n| n.entity.get("model")).unwrap_or_default().to_owned();
        let key = (prop, model, state.assets.generation());
        if self.source.as_ref().is_none_or(|(k, _)| *k != key) {
            self.source = Some((key, prop_source(state, prop)));
            self.error = None;
        }
        Some(prop)
    }

    fn convert(&mut self, state: &mut AppState, prop: Id) -> bool {
        let Some((_, Ok(source))) = &self.source else { return false };
        let opts = UnpropOptions::from_settings(state);
        let made = match build_brushes(source, opts.shape) {
            Ok(m) => m,
            Err(e) => {
                self.error = Some(e);
                return false;
            }
        };
        match state.doc.replace_prop_with_brushes(prop, made.brushes.solids.clone(), opts.detail, opts.keep_prop) {
            Ok(_) => {
                state.notify(Severity::Success, report_text(&made));
                self.source = None;
                true
            }
            Err(e) => {
                self.error = Some(tf!("propper.failed", e.to_string()));
                false
            }
        }
    }

    fn window_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, prop: Option<Id>) -> (bool, bool) {
        let mut ready = false;
        match (prop, &self.source) {
            (None, _) => {
                ui.label(RichText::new(t!("propper.select")).color(theme::warning()));
            }
            (Some(_), Some((_, Err(e)))) => {
                ui.label(RichText::new(e).color(theme::error()));
            }
            (Some(_), Some((_, Ok(source)))) => {
                ready = true;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t!("propper.model")).strong());
                    ui.label(&source.model);
                });
                ui.label(RichText::new(tf!("propper.triangles", source.tris.len())).weak());
                if source.collision.is_empty() {
                    ui.label(RichText::new(t!("propper.no_collision")).weak());
                } else {
                    ui.label(RichText::new(tf!("propper.has_collision", source.collision.len())).weak());
                }
            }
            (Some(_), None) => {}
        }
        ui.separator();
        let s = &mut state.settings.lab;
        let mut changed = false;
        ui.label(RichText::new(t!("propper.shape")).strong());
        for (shape, key, tip) in [
            (UnpropShape::Auto, "propper.shape_auto", "propper.shape_auto_tip"),
            (UnpropShape::Collision, "propper.shape_collision", "propper.shape_collision_tip"),
            (UnpropShape::Render, "propper.shape_render", "propper.shape_render_tip"),
        ] {
            changed |= ui.radio_value(&mut s.unprop_shape, shape, t!(key)).on_hover_text(t!(tip)).changed();
        }
        ui.add_space(4.0);
        ui.label(RichText::new(t!("propper.target")).strong());
        ui.horizontal(|ui| {
            changed |= ui.radio_value(&mut s.unprop_detail, true, "func_detail").changed();
            changed |= ui.radio_value(&mut s.unprop_detail, false, t!("propper.target_world")).changed();
        });
        ui.add_space(4.0);
        changed |= ui.checkbox(&mut s.unprop_keep_prop, t!("propper.keep")).changed();
        if changed {
            state.settings_dirty = true;
            self.error = None;
        }
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(theme::error()));
        }
        ui.separator();
        let (mut close, mut go) = (false, false);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let key = hp_app::actions::key_label;
                if ui.button(t!("common.cancel")).on_hover_text(key(egui::Key::Escape)).clicked() {
                    close = true;
                }
                if ui
                    .add_enabled(ready, egui::Button::new(t!("propper.convert")))
                    .on_hover_text(key(egui::Key::Enter))
                    .clicked()
                {
                    go = true;
                }
            });
        });
        (close, go && ready)
    }
}

impl MapDialog for UnpropDialog {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.runtime.lab.unprop_open {
            self.was_open = false;
            return;
        }
        if !self.was_open {
            self.was_open = true;
            self.error = None;
            self.source = None;
        }
        let prop = self.refresh(state);
        let mut open = true;
        let (mut close, mut go) = (false, false);
        let response = hp_app::chrome::Window::new(icon_title("📦", t!("propper.title")))
            .id(egui::Id::unique("hp_prop_to_brushes"))
            .default_pos(crate::util::centered_pos(ctx, vec2(380.0, 300.0)))
            .open(&mut open)
            .default_width(380.0)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| (close, go) = self.window_ui(ui, state, prop));
        if let Some(r) = &response {
            if escape_pressed(ctx, &r.response) {
                open = false;
            } else if enter_pressed(ctx, &r.response) {
                go = matches!(self.source, Some((_, Ok(_))));
            }
        }
        if go
            && let Some(prop) = prop
            && self.convert(state, prop)
        {
            close = true;
        }
        if !open || close {
            state.runtime.lab.unprop_open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_ledges_become_world_pieces() {
        let solid = hp_mdl::CollisionSolid {
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [5.0, 5.0, 5.0]],
            triangles: vec![[0, 1, 2], [0, 3, 1], [0, 2, 3], [1, 3, 2], [4, 2, 1]],
            ledges: std::iter::once(0..4).collect(),
        };
        let model = hp_mdl::CollisionModel { solids: vec![solid] };
        let mut e = Entity { classname: "prop_static".to_owned(), ..Entity::default() };
        e.set("origin", "0 0 10");
        let pieces = collision_pieces(&model, &PropTransform::of(&e));
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].len(), 4, "the hull ledge's point is not in it");
        assert!(pieces[0].contains(&DVec3::new(0.0, 0.0, 11.0)));
    }

    #[test]
    fn shapes_and_messages() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        let corners = |x: f64, y: f64, z: f64| DVec3::new(x, y, z) * 32.0;
        let quads = [
            [corners(0., 0., 1.), corners(1., 0., 1.), corners(1., 1., 1.), corners(0., 1., 1.)],
            [corners(0., 1., 0.), corners(1., 1., 0.), corners(1., 0., 0.), corners(0., 0., 0.)],
            [corners(1., 0., 0.), corners(1., 1., 0.), corners(1., 1., 1.), corners(1., 0., 1.)],
            [corners(0., 1., 0.), corners(0., 0., 0.), corners(0., 0., 1.), corners(0., 1., 1.)],
            [corners(1., 1., 0.), corners(0., 1., 0.), corners(0., 1., 1.), corners(1., 1., 1.)],
            [corners(0., 0., 0.), corners(1., 0., 0.), corners(1., 0., 1.), corners(0., 0., 1.)],
        ];
        let mut tris = Vec::new();
        for q in quads {
            for t in [[0, 1, 2], [0, 2, 3]] {
                tris.push(ModelTri {
                    material: "brick/wall".to_owned(),
                    pos: t.map(|i| q[i]),
                    uv: t.map(|i| DVec2::new(q[i].x / 128.0, q[i].y / 128.0)),
                });
            }
        }
        let source = PropSource { model: "models/a.mdl".to_owned(), tris, ..PropSource::default() };
        let made = build_brushes(&source, UnpropShape::Auto).expect("render pieces");
        assert!(!made.from_collision);
        assert_eq!(made.brushes.solids.len(), 1);
        assert!(report_text(&made).starts_with("Prop converted to 1 brush"), "{}", report_text(&made));
        assert_eq!(build_brushes(&source, UnpropShape::Collision).unwrap_err(), t!("propper.no_phy"));
        let with_phy = PropSource { collision: vec![quads.iter().flatten().copied().collect()], ..source };
        let made = build_brushes(&with_phy, UnpropShape::Auto).expect("collision pieces");
        assert!(made.from_collision);
        assert_eq!(made.brushes.textured_faces, 6);
    }

    #[test]
    fn the_window_converts_the_selected_prop_or_explains() {
        let _lang = crate::lang_lock();
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        let mut state = hp_app::testing::test_state();
        let prop = state
            .doc
            .create_point_entity("prop_static", DVec3::ZERO, &[("model".to_owned(), "models/a.mdl".to_owned())])
            .expect("prop");
        assert_eq!(hp_app::actions::selected_prop(&state), Some(prop));
        let opts = UnpropOptions::from_settings(&state);
        let err = prop_to_brushes(&mut state, prop, &opts).unwrap_err();
        assert_eq!(err, t!("propper.no_content"));
        assert!(state.doc.scene().entity(prop).is_some());
        state.doc.select(&[], hp_editor::SelectOp::Replace);
        assert!(prop_source(&state, prop).is_err());
        state.runtime.lab.unprop_open = true;
        let mut dialog = UnpropDialog::default();
        hp_app::testing::run_ui(&mut state, |ui, state| dialog.show(&ui.ctx().clone(), state));
        assert!(state.runtime.lab.unprop_open, "stays open with nothing selected");
        assert!(dialog.source.is_none());
    }
}
