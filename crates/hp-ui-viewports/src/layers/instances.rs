use egui::Align2;
use glam::DVec3;
use hp_app::AppState;
use hp_app::i18n::TrTable;
use hp_app::runtime::instances::{InstanceCache, instance_bounds, selected_instances};
use hp_app::tf;
use hp_app::view::ViewInfo;
use hp_editor::instances::{InstanceSpec, LoadRequest, Loaded};
use hp_geom::Aabb;
use hp_render::Overlay;

use super::OverlayLayer;
use crate::overlay::{box_lines_on_top, cross_marker};
use crate::paint::{overlay_color, text_pill};

pub const INSTANCE_COLOR: [f32; 4] = [0.35, 0.75, 1.0, 1.0];
const BROKEN_COLOR: [f32; 4] = [1.0, 0.3, 0.25, 1.0];
const MAX_OUTLINED: usize = 64;
const MAX_MARKED: usize = 512;
const MAX_LABEL: usize = 80;

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(InstanceLayer::default())]
}

pub(crate) const TR: TrTable = &[
    ("viewport.instance_missing", "Instance not found: {0}", "Instance introuvable : {0}"),
    ("viewport.instance_broken", "Instance unreadable: {0}", "Instance illisible : {0}"),
    ("viewport.instance_context", "Shown inside {0} (read only)", "Affiché dans {0} (lecture seule)"),
];

fn short(s: &str) -> &str {
    match s.char_indices().nth(MAX_LABEL) {
        Some((cut, _)) => &s[..cut],
        None => s,
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Broken {
    origin: DVec3,
    label: String,
}

#[derive(Default)]
pub struct InstanceLayer {
    key: Option<(u64, u64, Option<std::path::PathBuf>)>,
    broken: Vec<Broken>,
    selected: Vec<(Aabb, String)>,
    selected_key: Option<SelectedKey>,
    context: Option<String>,
    context_key: Option<(Option<hp_app::runtime::instances::InstanceContext>, hp_app::i18n::Lang)>,
}

type SelectedKey = (u64, u64, u64, bool);

impl InstanceLayer {
    fn refresh_broken(&mut self, state: &AppState, cache: &InstanceCache) {
        let key = (state.doc.revision(), cache.generation(), state.doc.path().map(std::path::Path::to_path_buf));
        if self.key.as_ref() == Some(&key) {
            return;
        }
        self.broken.clear();
        let main = state.doc.path();
        for id in state.doc.instance_entities() {
            if self.broken.len() >= MAX_MARKED {
                break;
            }
            if !state.doc.is_entity_visible(id) {
                continue;
            }
            let Some(e) = state.doc.scene().entity(id).map(|n| &n.entity) else { continue };
            let Some(spec) = InstanceSpec::of(e) else { continue };
            if spec.file.is_empty() || !spec.origin.is_finite() {
                continue;
            }
            let req = LoadRequest { file: &spec.file, containing: None, depth: 0 };
            let label = match cache.load(&req, main) {
                Loaded::Missing => tf!("viewport.instance_missing", short(&spec.file)),
                Loaded::Failed(_) => tf!("viewport.instance_broken", short(&spec.file)),
                Loaded::Ready { .. } | Loaded::Pending => continue,
            };
            self.broken.push(Broken { origin: spec.origin, label });
        }
        self.key = Some(key);
    }
}

impl OverlayLayer for InstanceLayer {
    fn name(&self) -> &'static str {
        "instances"
    }

    fn prepare(&mut self, state: &AppState) {
        let cache = InstanceCache::shared();
        self.refresh_broken(state, &cache);
        let context = hp_app::runtime::instances::context().filter(|_| state.settings.instances.show_context);
        let context_key = (context, hp_app::i18n::lang());
        if self.context_key.as_ref() != Some(&context_key) {
            self.context = context_key
                .0
                .as_ref()
                .and_then(|c| c.parent.file_name().map(|n| short(&n.to_string_lossy()).to_owned()))
                .map(|parent| tf!("viewport.instance_context", parent));
            self.context_key = Some(context_key);
        }
        let outline = state.settings.instances.outline_selected;
        let key = (state.doc.revision(), cache.generation(), state.assets.generation(), outline);
        if self.selected_key == Some(key) {
            return;
        }
        self.selected_key = Some(key);
        self.selected.clear();
        if !outline {
            return;
        }
        let main = state.doc.path();
        let fgd = state.assets.fgd();
        for id in selected_instances(&state.doc).into_iter().take(MAX_OUTLINED) {
            let Some(e) = state.doc.scene().entity(id).map(|n| &n.entity) else { continue };
            let Some(b) = instance_bounds(&cache, e, main, fgd) else { continue };
            let file = e.get("file").map(str::trim).unwrap_or("");
            self.selected.push((b, short(file).to_owned()));
        }
    }

    fn add(&self, _state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        for (b, _) in &self.selected {
            box_lines_on_top(out, b.min, b.max, INSTANCE_COLOR);
        }
        for m in &self.broken {
            cross_marker(out, view, m.origin, 10.0, BROKEN_COLOR);
        }
    }

    fn paint(&self, _state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        if let Some(text) = &self.context
            && !view.is_2d()
        {
            let pos = egui::pos2(view.rect.center().x, view.rect.top() + 8.0);
            text_pill(painter, pos, Align2::CENTER_TOP, text.clone(), overlay_color(INSTANCE_COLOR));
        }
        for (b, file) in &self.selected {
            if file.is_empty() {
                continue;
            }
            let mut corner: Option<egui::Pos2> = None;
            for i in 0..8 {
                let c = DVec3::new(
                    if i & 1 != 0 { b.max.x } else { b.min.x },
                    if i & 2 != 0 { b.max.y } else { b.min.y },
                    if i & 4 != 0 { b.max.z } else { b.min.z },
                );
                if let Some(p) = view.world_to_screen(c) {
                    corner = Some(corner.map_or(p, |q| egui::pos2(q.x.min(p.x), q.y.min(p.y))));
                }
            }
            if let Some(p) = corner {
                let p = egui::pos2(p.x.max(view.rect.left() + 4.0), p.y.max(view.rect.top() + 18.0));
                if view.rect.contains(p) {
                    text_pill(painter, p, Align2::LEFT_BOTTOM, file.clone(), overlay_color(INSTANCE_COLOR));
                }
            }
        }
        for m in &self.broken {
            if let Some(p) = view.world_to_screen(m.origin)
                && view.rect.contains(p)
            {
                let pos = p + egui::vec2(8.0, -8.0);
                text_pill(painter, pos, Align2::LEFT_BOTTOM, m.label.clone(), overlay_color(BROKEN_COLOR));
            }
        }
    }
}
