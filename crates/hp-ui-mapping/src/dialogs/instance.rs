use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use egui::{DragValue, RichText, Ui};
use glam::DVec3;
use hp_app::runtime::instances::{InstanceCache, key_fixup};
use hp_app::{AppState, Severity, t, tf, theme};
use hp_editor::ObjRef;
use hp_editor::instances::maps_folder;

use crate::dialogs::MapDialog;
use crate::util::{centered_pos, escape_pressed, focus_select_all, icon_title};

pub(crate) fn dialogs() -> Vec<Box<dyn MapDialog>> {
    vec![Box::new(CreateInstanceDialog::default()), Box::new(CollapseAllDialog)]
}

const MAX_NAME: usize = 120;

pub(crate) fn clean_file_name(name: &str) -> Option<String> {
    let mut n = name.trim();
    if n.len() > 4 && n[n.len() - 4..].eq_ignore_ascii_case(".vmf") {
        n = n[..n.len() - 4].trim_end();
    }
    let bad = |c: char| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control();
    let reserved = ["CON", "PRN", "AUX", "NUL"].iter().any(|r| n.eq_ignore_ascii_case(r))
        || (n.len() == 4 && ["COM", "LPT"].iter().any(|p| n[..3].eq_ignore_ascii_case(p)));
    if n.is_empty() || n.len() > MAX_NAME || n.chars().any(bad) || n.ends_with('.') || n.starts_with('.') || reserved {
        return None;
    }
    Some(n.to_owned())
}

fn strip_base(path: &Path, base: &Path) -> Option<PathBuf> {
    let mut p = path.components();
    for b in base.components() {
        let c = p.next()?;
        let same = c.as_os_str().to_string_lossy().to_lowercase() == b.as_os_str().to_string_lossy().to_lowercase();
        if !same {
            return None;
        }
    }
    let rest: PathBuf = p.collect();
    (!rest.as_os_str().is_empty() && rest.components().all(|c| matches!(c, Component::Normal(_)))).then_some(rest)
}

pub(crate) fn instance_file_key(map: Option<&Path>, file: &Path) -> String {
    let rel = map
        .and_then(|m| m.parent())
        .and_then(|dir| strip_base(file, dir))
        .or_else(|| map.and_then(maps_folder).and_then(|maps| strip_base(file, &maps)));
    rel.as_deref().unwrap_or(file).to_string_lossy().replace('\\', "/")
}

fn default_folder(state: &AppState) -> Option<PathBuf> {
    let setting = state.settings.instances.folder.trim();
    let setting = if setting.is_empty() { "instances" } else { setting };
    let p = Path::new(setting);
    if p.is_absolute() {
        return Some(p.to_path_buf());
    }
    state.doc.path().and_then(Path::parent).map(|d| d.join(p))
}

fn inside_game(state: &AppState, path: &Path) -> bool {
    let root = state
        .assets
        .fs()
        .and_then(|f| f.root())
        .map(Path::to_path_buf)
        .or_else(|| state.settings.game.gmod_root.clone());
    root.is_some_and(|r| strip_base(path, &r).is_some())
}

struct Writing {
    rx: Receiver<Result<(), String>>,
    path: PathBuf,
    origin: DVec3,
    objs: Vec<ObjRef>,
}

#[derive(Default)]
struct CreateInstanceDialog {
    shown: bool,
    name: String,
    folder: String,
    origin: [f64; 3],
    objs: Vec<ObjRef>,
    overwrite: bool,
    error: Option<String>,
    browse: Option<Receiver<Option<PathBuf>>>,
    writing: Option<Writing>,
    focus_name: bool,
}

impl CreateInstanceDialog {
    fn start(&mut self, state: &AppState) {
        self.objs = state.doc.selected_objects();
        self.error = None;
        self.overwrite = false;
        self.focus_name = true;
        let center = state
            .doc
            .selection_bounds(&|e| state.assets.entity_bounds(e))
            .map(|b| b.center())
            .filter(|c| c.is_finite())
            .unwrap_or(DVec3::ZERO);
        let g = state.grid_size();
        let snap = |v: f64| if g > 0.0 { (v / g).round() * g + 0.0 } else { v.round() + 0.0 };
        self.origin = [snap(center.x), snap(center.y), snap(center.z)];
        let folder = default_folder(state);
        self.folder = folder.as_ref().map(|f| f.display().to_string()).unwrap_or_default();
        self.name = (1..1000)
            .map(|i| format!("instance_{i}"))
            .find(|n| folder.as_ref().is_none_or(|f| !f.join(format!("{n}.vmf")).exists()))
            .unwrap_or_else(|| "instance".to_owned());
    }

    fn target(&self, state: &AppState) -> Result<PathBuf, String> {
        let name = clean_file_name(&self.name).ok_or_else(|| t!("instancing.bad_name").to_owned())?;
        let folder = PathBuf::from(self.folder.trim());
        let folder = if folder.as_os_str().is_empty() {
            return Err(t!("instancing.no_folder").to_owned());
        } else if folder.is_absolute() {
            folder
        } else {
            state
                .doc
                .path()
                .and_then(Path::parent)
                .map(|d| d.join(&folder))
                .ok_or_else(|| t!("instancing.no_folder").to_owned())?
        };
        Ok(folder.join(format!("{name}.vmf")))
    }

    fn create(&mut self, state: &mut AppState) {
        self.error = None;
        let path = match self.target(state) {
            Ok(p) => p,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        if path.exists() && !self.overwrite {
            self.error = Some(t!("instancing.exists").to_owned());
            return;
        }
        if hp_compile::sandbox::is_sandboxed() && inside_game(state, &path) {
            self.error = Some(t!("instancing.sandboxed").to_owned());
            return;
        }
        let origin = DVec3::from_array(self.origin);
        let objs: Vec<ObjRef> = self.objs.iter().copied().filter(|o| state.doc.scene().contains(*o)).collect();
        let fgd = state.assets.fgd();
        let kinds = |c: &str, k: &str| fgd.and_then(|f| key_fixup(f, c, k));
        let map = match state.doc.instance_map(&objs, origin, Some(&kinds)) {
            Ok(m) => m,
            Err(e) => {
                self.error = Some(tf!("instancing.failed", e));
                return;
            }
        };
        let (tx, rx) = channel();
        let target = path.clone();
        let spawned = std::thread::Builder::new().name("hp-instance-write".to_owned()).spawn(move || {
            let result = target
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .map_err(|e| e.to_string())
                .and_then(|()| hp_vmf::save(&map, &target).map_err(|e| e.to_string()));
            let _ = tx.send(result);
        });
        match spawned {
            Ok(_) => self.writing = Some(Writing { rx, path, origin, objs }),
            Err(e) => self.error = Some(tf!("instancing.failed", e)),
        }
    }

    fn poll_writing(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let Some(w) = &self.writing else { return false };
        let result = match w.rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
                return false;
            }
            Err(TryRecvError::Disconnected) => Err(t!("instancing.write_stopped").to_owned()),
        };
        let Some(w) = self.writing.take() else { return false };
        if let Err(e) = result {
            self.error = Some(tf!("instancing.write_failed", w.path.display(), e));
            return false;
        }
        InstanceCache::shared().invalidate(&w.path);
        let key = instance_file_key(state.doc.path(), &w.path);
        match state.doc.replace_with_instance(&w.objs, &key, w.origin) {
            Ok(_) => {
                state.notify(Severity::Info, tf!("instancing.created", w.path.display()));
                true
            }
            Err(e) => {
                self.error = Some(tf!("instancing.replace_failed", w.path.display(), e));
                false
            }
        }
    }

    fn window_ui(&mut self, ui: &mut Ui, state: &mut AppState) -> bool {
        let busy = self.writing.is_some();
        ui.add_enabled_ui(!busy, |ui| {
            egui::Grid::new("hp_instance_create_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label(t!("instancing.name"));
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(200.0));
                    if self.focus_name {
                        self.focus_name = false;
                        focus_select_all(&r, &self.name);
                    }
                    if r.changed() {
                        self.error = None;
                    }
                    ui.label(".vmf");
                });
                ui.end_row();
                ui.label(t!("instancing.folder"));
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut self.folder).desired_width(260.0));
                    if r.changed() {
                        self.error = None;
                    }
                    if ui.add_enabled(self.browse.is_none(), egui::Button::new(t!("instancing.browse"))).clicked() {
                        self.browse = spawn_folder_browse(ui.ctx(), &self.folder);
                    }
                });
                ui.end_row();
                ui.label(t!("instancing.origin"));
                ui.horizontal(|ui| {
                    for (axis, v) in ["X", "Y", "Z"].iter().zip(self.origin.iter_mut()) {
                        ui.label(*axis);
                        ui.add(DragValue::new(v).speed(1.0).max_decimals(3));
                    }
                });
                ui.end_row();
            });
            ui.label(RichText::new(t!("instancing.origin_tip")).weak().small());
            if let Ok(path) = self.target(state) {
                let key = instance_file_key(state.doc.path(), &path);
                ui.label(RichText::new(tf!("instancing.key", key)).weak().small());
                if path.exists() {
                    ui.checkbox(&mut self.overwrite, t!("instancing.overwrite"));
                }
            }
        });
        if state.doc.path().is_none() {
            ui.label(RichText::new(t!("instancing.unsaved")).color(theme::warning()));
        }
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(theme::error()));
        }
        ui.separator();
        let mut close = false;
        ui.horizontal(|ui| {
            let ready = !busy && !self.objs.is_empty();
            if ui.add_enabled(ready, egui::Button::new(t!("instancing.create"))).clicked() {
                self.create(state);
            }
            if busy {
                hp_app::widgets::spinner(ui);
                ui.label(t!("instancing.writing"));
            }
            if ui.add_enabled(!busy, egui::Button::new(t!("instancing.cancel"))).clicked() {
                close = true;
            }
        });
        close
    }
}

fn spawn_folder_browse(ctx: &egui::Context, start: &str) -> Option<Receiver<Option<PathBuf>>> {
    let (tx, rx) = channel();
    let ctx = ctx.clone();
    let start = PathBuf::from(start.trim());
    let title = t!("instancing.browse_title").to_owned();
    std::thread::Builder::new()
        .name("hp-instance-folder".to_owned())
        .spawn(move || {
            let mut dialog = rfd::FileDialog::new().set_title(title);
            if start.is_dir() {
                dialog = dialog.set_directory(&start);
            }
            let _ = tx.send(dialog.pick_folder());
            ctx.request_repaint();
        })
        .ok()?;
    Some(rx)
}

impl MapDialog for CreateInstanceDialog {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if self.writing.is_some() && self.poll_writing(ctx, state) {
            state.runtime.instances.create_open = false;
        }
        if !state.runtime.instances.create_open {
            self.shown = false;
            return;
        }
        if !self.shown {
            self.shown = true;
            if state.doc.selection().objects.is_empty() {
                state.runtime.instances.create_open = false;
                state.notify(Severity::Info, t!("instancing.nothing"));
                return;
            }
            self.start(state);
        }
        if let Some(rx) = &self.browse {
            match rx.try_recv() {
                Ok(picked) => {
                    if let Some(p) = picked {
                        self.folder = p.display().to_string();
                        self.error = None;
                    }
                    self.browse = None;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.browse = None,
            }
        }
        let mut open = true;
        let mut close = false;
        let response = hp_app::chrome::Window::new(icon_title("📦", t!("instancing.create_title")))
            .id(egui::Id::unique("hp_instance_create"))
            .default_pos(centered_pos(ctx, egui::vec2(460.0, 220.0)))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| close = self.window_ui(ui, state));
        if let Some(r) = &response
            && self.writing.is_none()
            && escape_pressed(ctx, &r.response)
        {
            open = false;
        }
        if (!open || close) && self.writing.is_none() {
            state.runtime.instances.create_open = false;
        }
    }
}

struct CollapseAllDialog;

impl MapDialog for CollapseAllDialog {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.runtime.instances.confirm_collapse_all {
            return;
        }
        let ids: Vec<hp_editor::Id> =
            state.doc.instance_entities().into_iter().filter(|id| state.doc.is_entity_visible(*id)).collect();
        if ids.is_empty() {
            state.runtime.instances.confirm_collapse_all = false;
            state.notify(Severity::Info, t!("instancing.none"));
            return;
        }
        let mut open = true;
        let mut answer: Option<bool> = None;
        let response = hp_app::chrome::Window::new(icon_title("📦", t!("instancing.collapse_title")))
            .id(egui::Id::unique("hp_instance_collapse_all"))
            .default_pos(centered_pos(ctx, egui::vec2(380.0, 120.0)))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(tf!("instancing.collapse_question", ids.len()));
                ui.label(RichText::new(t!("instancing.collapse_note")).weak());
                if state.settings.instances.collapse_skips_hidden {
                    ui.label(RichText::new(t!("instancing.collapse_hidden")).weak());
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(t!("instancing.collapse_yes")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(t!("instancing.cancel")).clicked() {
                        answer = Some(false);
                    }
                });
            });
        if let Some(r) = &response
            && escape_pressed(ctx, &r.response)
        {
            open = false;
        }
        if answer == Some(true) {
            state.runtime.instances.pending_collapse = Some(ids);
        }
        if !open || answer.is_some() {
            state.runtime.instances.confirm_collapse_all = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_checked() {
        assert_eq!(clean_file_name(" door.VMF "), Some("door".to_owned()));
        assert_eq!(clean_file_name("my room 2"), Some("my room 2".to_owned()));
        for bad in ["", " ", "a/b", "a\\b", "c:x", "a?", "con", "COM1", ".hidden", "x.", "a\u{7}"] {
            assert_eq!(clean_file_name(bad), None, "{bad:?}");
        }
        assert_eq!(clean_file_name(&"x".repeat(MAX_NAME + 1)), None);
    }

    #[test]
    fn file_keys_are_relative_when_possible() {
        let map = Path::new("C:/sdk/maps/city/city.vmf");
        assert_eq!(
            instance_file_key(Some(map), Path::new("C:/sdk/maps/city/instances/door.vmf")),
            "instances/door.vmf"
        );
        assert_eq!(
            instance_file_key(Some(map), Path::new("C:/SDK/Maps/City/instances/door.vmf")),
            "instances/door.vmf"
        );
        assert_eq!(instance_file_key(Some(map), Path::new("C:/sdk/maps/shared/door.vmf")), "shared/door.vmf");
        assert_eq!(instance_file_key(Some(map), Path::new("D:/other/door.vmf")), "D:/other/door.vmf");
        assert_eq!(instance_file_key(None, Path::new("D:/other/door.vmf")), "D:/other/door.vmf");
    }

    #[test]
    fn create_dialog_writes_the_file_and_places_the_instance() {
        let dir = std::env::temp_dir().join(format!("hp_instancing_dialog_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let mut state = hp_app::testing::test_state();
        state.doc.set_path(Some(dir.join("main.vmf")));
        let ids = state.doc.create_primitive(&hp_geom::PrimitiveParams::default(), hp_editor::Owner::World).unwrap();
        state.doc.select(&[ObjRef::Solid(ids[0])], hp_editor::SelectOp::Replace);
        state.runtime.instances.create_open = true;
        let mut dialog = CreateInstanceDialog::default();
        hp_app::testing::run_ctx(&mut state, |ctx, state| dialog.show(ctx, state));
        assert_eq!(dialog.name, "instance_1");
        assert!(dialog.folder.ends_with("instances"));
        dialog.create(&mut state);
        assert!(dialog.error.is_none(), "{:?}", dialog.error);
        for _ in 0..200 {
            hp_app::testing::run_ctx(&mut state, |ctx, state| dialog.show(ctx, state));
            if dialog.writing.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!state.runtime.instances.create_open, "{:?}", dialog.error);
        let file = dir.join("instances").join("instance_1.vmf");
        let map = hp_vmf::load(&file).expect("written");
        assert_eq!(map.world.solids.len(), 1);
        let inst = state.doc.scene().entities().values().find(|n| n.entity.classname == "func_instance").unwrap();
        assert_eq!(inst.entity.get("file"), Some("instances/instance_1.vmf"));
        assert!(state.doc.scene().solid(ids[0]).is_none(), "the selection was replaced");
        state.doc.undo();
        state.doc.select(&[ObjRef::Solid(ids[0])], hp_editor::SelectOp::Replace);
        let mut again = CreateInstanceDialog::default();
        again.start(&state);
        again.name = "instance_1".to_owned();
        again.create(&mut state);
        assert_eq!(again.error.as_deref(), Some(t!("instancing.exists")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
