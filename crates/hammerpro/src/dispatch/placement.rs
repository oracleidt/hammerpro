use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};

use glam::DVec3;
use hp_app::dnd::DragPayload;
use hp_app::i18n::TrTable;
use hp_app::runtime::{CreatePrefabRequest, PrefabLoad};
use hp_app::state::RightTab;
use hp_app::{Action, Severity, Tool, ViewKind, t, tf};
use hp_editor::prefab::PrefabPlacement;
use hp_ui_viewports::placement::{self as vp, Target};

use super::{DockTab, Package};
use crate::app::HammerProApp;

pub(crate) const TAB: &str = "prefabs";

pub(crate) struct Placement;

type Write = Receiver<(PathBuf, Result<(), String>)>;

#[derive(Default)]
pub(crate) struct State {
    prefabs: hp_ui_mapping::prefabs::PrefabTab,
    pending_insert: Option<PendingInsert>,
    writes: Vec<Write>,
    pub(crate) ops: super::placement_ops::State,
}

#[derive(Clone, Debug)]
enum InsertAt {
    ViewCenter,
    Drop { target: Target, axes: Option<(usize, usize)> },
}

#[derive(Clone, Debug)]
struct PendingInsert {
    path: PathBuf,
    at: InsertAt,
}

const HANDLED: &[Action] = &[
    Action::ShowPrefabs,
    Action::CreatePrefab,
    Action::InsertOriginalPrefab,
    Action::ScatterByMaterial,
    Action::PlaceOnTop,
    Action::PlaceAgainstWall,
    Action::PlaceAgainstWallAway,
    Action::PlaceNextTo,
    Action::SettleFloating,
    Action::FitToGround,
    Action::ShowShapePreview,
    Action::ToggleRealDimensions,
];

impl Package for Placement {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::ShowPrefabs => show_prefabs(app),
            Action::ScatterByMaterial => app.state.ui.show_scatter = true,
            Action::CreatePrefab => {
                if app.require_selection() {
                    app.state.runtime.placement.create_open = true;
                }
            }
            Action::InsertOriginalPrefab => {
                let rt = &app.state.runtime.placement;
                match rt.selected_prefab.clone().or_else(|| rt.armed_prefab.clone()) {
                    Some(path) => {
                        app.state.runtime.placement.prefabs.refresh(&path, Some(ctx));
                        app.pkg.placement.pending_insert = Some(PendingInsert { path, at: InsertAt::ViewCenter });
                        insert_pending(app);
                    }
                    None => app.state.notify(Severity::Info, t!("placement.no_prefab")),
                }
            }
            a if super::placement_ops::HANDLED.contains(&a) => return super::placement_ops::handle(app, a),
            _ => return false,
        }
        true
    }

    fn map_loaded(&self, app: &mut HammerProApp) {
        super::placement_ops::map_loaded(app);
    }

    fn dialogs(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        super::placement_ops::dialogs(app, ctx);
    }

    fn init(&self, app: &mut HammerProApp) {
        app.viewports.add_layer(Box::new(super::placement_ops::RealDimsLayer));
        app.shell.add_status_segment(Box::new(super::placement_ops::RealDimsStatus));
        if hp_compile::sandbox::is_sandboxed() && app.state.settings.placement.prefab_library.is_none() {
            let dir = std::env::temp_dir().join(format!("hammerpro_sandbox_{}", std::process::id())).join("prefabs");
            app.state.settings.placement.prefab_library = Some(dir);
        }
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        app.state.runtime.placement.prefabs.poll();
        take_prefab_drop(app, ctx);
        if app.pkg.placement.pending_insert.is_some() {
            insert_pending(app);
        }
        if let Some(req) = app.state.runtime.placement.create_request.take() {
            start_write(app, req);
        }
        poll_writes(app);
        if let Some(path) = app.state.runtime.placement.open_path.take() {
            open_path(&path);
        }
        super::placement_ops::tick(app, ctx);
    }

    fn dock_tabs(&self, app: &HammerProApp, tabs: &mut Vec<DockTab>) {
        if app.state.settings.placement.show_prefabs {
            tabs.push(DockTab { id: TAB, label: t!("prefabs.tab").to_owned() });
        }
    }

    fn dock_ui(&self, app: &mut HammerProApp, ui: &mut egui::Ui, id: &'static str) -> bool {
        if id != TAB {
            return false;
        }
        app.pkg.placement.prefabs.ui(ui, &mut app.state);
        true
    }

    fn tools(&self, out: &mut Vec<Box<dyn Tool>>) {
        out.push(Box::new(hp_tools_create::SprinkleTool::new()));
        out.push(Box::new(hp_tools_create::FoliageTool::new()));
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

fn show_prefabs(app: &mut HammerProApp) {
    let tab = RightTab::Package(TAB);
    let s = &mut app.state.settings.placement;
    if s.show_prefabs && app.state.ui.right_tab == tab {
        s.show_prefabs = false;
    } else {
        s.show_prefabs = true;
        app.state.ui.right_tab = tab;
    }
    app.state.settings_dirty = true;
}

fn target_view(app: &HammerProApp) -> ViewKind {
    let focused = app.state.ui.focused_view.and_then(|i| app.viewports.view_rects().get(i).copied().flatten());
    match focused {
        Some((kind, _)) if kind.is_2d() => kind,
        _ => ViewKind::Top,
    }
}

fn take_prefab_drop(app: &mut HammerProApp, ctx: &egui::Context) {
    let Some(payload) = egui::DragAndDrop::payload::<DragPayload>(ctx) else { return };
    let DragPayload::Prefab(path) = payload.as_ref() else { return };
    let Some(pos) = ctx.input(|i| if i.pointer.any_released() { i.pointer.latest_pos() } else { None }) else {
        return;
    };
    if ctx.layer_id_at(pos).is_some_and(|l| l.order != egui::Order::Background) {
        return;
    }
    let rects = app.viewports.view_rects();
    let Some((index, (kind, rect))) =
        rects.iter().enumerate().find_map(|(i, v)| v.filter(|(_, r)| r.contains(pos)).map(|v| (i, v)))
    else {
        return;
    };
    let path = path.clone();
    egui::DragAndDrop::clear_payload(ctx);
    let cfg = hp_ui_viewports::params::RenderConfig::from_state(&app.state);
    let view = hp_ui_viewports::params::view_info(index, kind, rect, &app.state.cameras, &cfg, ctx.pixels_per_point());
    let depth = match kind.axes_2d() {
        Some((_, _, d)) => {
            let state = &app.state;
            state
                .doc
                .selection_center(&|e| state.assets.entity_bounds(e))
                .filter(|c| c.is_finite())
                .map_or(0.0, |c| c[d])
        }
        None => 0.0,
    };
    let target = vp::target_at(&app.state, &view, pos, depth);
    let axes = kind.axes_2d().map(|(r, u, _)| (r, u));
    app.state.ui.focused_view = Some(index);
    app.state.runtime.placement.selected_prefab = Some(path.clone());
    app.state.runtime.placement.prefabs.refresh(&path, Some(ctx));
    app.pkg.placement.pending_insert = Some(PendingInsert { path, at: InsertAt::Drop { target, axes } });
    insert_pending(app);
}

fn drop_offset(app: &HammerProApp, bounds: &hp_geom::Aabb, target: &Target, axes: Option<(usize, usize)>) -> DVec3 {
    let mut offset = match axes {
        Some(axes) => vp::box_offset_2d(bounds.min, bounds.max, target.point, axes),
        None if target.on_surface => vp::box_rest_offset(bounds.min, bounds.max, target.point, target.normal),
        None => target.point - bounds.center(),
    };
    let keep = vp::aligned_axis(target.normal);
    for k in 0..3 {
        if Some(k) != keep || axes.is_some() {
            offset[k] = app.state.snap(offset[k]);
        }
    }
    offset
}

fn insert_pending(app: &mut HammerProApp) {
    let Some(PendingInsert { path, at }) = app.pkg.placement.pending_insert.clone() else { return };
    let prefab = match app.state.runtime.placement.prefabs.get(&path, None) {
        PrefabLoad::Loading => return,
        PrefabLoad::Failed(e) => {
            app.pkg.placement.pending_insert = None;
            let name = hp_tools_create::prefab_place::file_label(&path);
            app.state.notify(Severity::Error, tf!("placement.load_failed", name, e));
            return;
        }
        PrefabLoad::Ready(p) => p,
    };
    app.pkg.placement.pending_insert = None;
    let Some(bounds) = prefab.bounds() else {
        app.state.notify(Severity::Warning, t!("placement.no_position"));
        return;
    };
    let offset = match at {
        InsertAt::ViewCenter => {
            let kind = target_view(app);
            let Some((r, u, _)) = kind.axes_2d() else { return };
            let center = app.state.cameras.cam2d(kind).center;
            let c = bounds.center();
            let mut offset = DVec3::ZERO;
            offset[r] = app.state.snap(center.x - c[r]);
            offset[u] = app.state.snap(center.y - c[u]);
            offset
        }
        InsertAt::Drop { target, axes } => drop_offset(app, &bounds, &target, axes),
    };
    if !offset.is_finite() {
        return;
    }
    match hp_tools_create::insert_prefab(&mut app.state, &prefab, PrefabPlacement::Offset(offset)) {
        Ok(_) => {}
        Err(e) => {
            let text = super::edit_error_text(&e);
            app.state.notify(Severity::Error, tf!("app.op_failed", Action::InsertOriginalPrefab.label(), text));
        }
    }
}

fn start_write(app: &mut HammerProApp, req: CreatePrefabRequest) {
    if app.state.doc.selected_objects().is_empty() {
        app.state.notify(Severity::Info, t!("app.nothing_selected"));
        return;
    }
    let mut map = match crate::files::export_map(&app.state.doc) {
        Ok(m) => m,
        Err(e) => {
            app.state.notify(Severity::Error, tf!("placement.save_failed", req.path.display(), e));
            return;
        }
    };
    map.version_info.prefab = true;
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new().name("hp-prefab-write".to_owned()).spawn(move || {
        let result = write_prefab(&map, &req.path, req.overwrite);
        let _ = tx.send((req.path, result));
    });
    match spawned {
        Ok(_) => app.pkg.placement.writes.push(rx),
        Err(e) => app.state.notify(Severity::Error, tf!("placement.save_failed", "", e)),
    }
}

fn write_prefab(map: &hp_vmf::Map, path: &Path, overwrite: bool) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    if !overwrite && path.exists() {
        return Err("the file already exists".to_owned());
    }
    let opts = hp_vmf::SaveOptions { backup: false, sync: true };
    hp_vmf::save_with(map, path, &opts).map(|_| ()).map_err(|e| e.to_string())
}

fn poll_writes(app: &mut HammerProApp) {
    let mut done = Vec::new();
    app.pkg.placement.writes.retain(|rx| match rx.try_recv() {
        Ok(r) => {
            done.push(r);
            false
        }
        Err(TryRecvError::Empty) => true,
        Err(TryRecvError::Disconnected) => false,
    });
    for (path, result) in done {
        match result {
            Ok(()) => {
                let rt = &mut app.state.runtime.placement;
                rt.library_serial = rt.library_serial.wrapping_add(1);
                rt.selected_prefab = Some(path.clone());
                let name = hp_tools_create::prefab_place::file_label(&path);
                app.state.notify(Severity::Success, tf!("placement.saved", name));
                app.state.log(Severity::Info, tf!("placement.saved_path", path.display()));
            }
            Err(e) => app.state.notify(Severity::Error, tf!("placement.save_failed", path.display(), e)),
        }
    }
}

fn open_path(path: &Path) {
    let is_text = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt"));
    if path.is_file() && is_text {
        let program = if cfg!(windows) {
            std::env::var_os("SystemRoot").map(|r| PathBuf::from(r).join("System32").join("notepad.exe"))
        } else if cfg!(target_os = "macos") {
            Some(PathBuf::from("/usr/bin/open"))
        } else {
            Some(PathBuf::from("/usr/bin/xdg-open"))
        };
        match program.filter(|p| p.is_file()) {
            Some(program) => {
                if let Err(e) = std::process::Command::new(program).arg(path).spawn() {
                    log::warn!("cannot open {}: {e}", path.display());
                }
            }
            None => {
                if let Some(dir) = path.parent() {
                    crate::app::open_folder(dir);
                }
            }
        }
    } else if path.is_file() {
        if let Some(dir) = path.parent() {
            crate::app::open_folder(dir);
        }
    } else {
        crate::app::open_folder(path);
    }
}

const TR: TrTable = &[
    (
        "placement.no_prefab",
        "Select a prefab in the Prefabs tab first",
        "Sélectionnez d'abord un prefab dans l'onglet Prefabs",
    ),
    ("placement.load_failed", "Cannot read the prefab {0}: {1}", "Impossible de lire le prefab {0} : {1}"),
    ("placement.no_position", "The prefab has no object to place", "Le prefab n'a aucun objet à placer"),
    ("placement.saved", "Prefab {0} saved", "Prefab {0} enregistré"),
    ("placement.saved_path", "Prefab saved to {0}", "Prefab enregistré dans {0}"),
    ("placement.save_failed", "Cannot save the prefab {0}: {1}", "Impossible d'enregistrer le prefab {0} : {1}"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty(), "{:#?}", hp_app::i18n::lint_table(TR));
        for (k, _, _) in TR {
            assert!(k.starts_with("placement."));
        }
    }

    #[test]
    fn prefabs_are_written_atomically_and_never_overwritten_by_surprise() {
        let dir = std::env::temp_dir().join(format!("hp_placement_write_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut state = hp_app::testing::test_state();
        state.doc.set_selection(&[hp_editor::ObjRef::Solid(2)]);
        let mut map = crate::files::export_map(&state.doc).expect("export");
        map.version_info.prefab = true;
        let path = dir.join("sub").join("floor.vmf");
        write_prefab(&map, &path, false).expect("written");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("\"prefab\" \"1\""), "{text}");
        assert!(write_prefab(&map, &path, false).is_err(), "no surprise overwrite");
        write_prefab(&map, &path, true).expect("replaced");
        let leftovers: Vec<_> = std::fs::read_dir(dir.join("sub")).expect("dir").flatten().collect();
        assert_eq!(leftovers.len(), 1, "no temporary or backup file");
        let prefab = hp_editor::prefab::Prefab::load(&path).expect("loads back");
        assert_eq!(prefab.solid_count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
