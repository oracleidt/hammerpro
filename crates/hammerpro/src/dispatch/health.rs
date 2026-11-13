pub(crate) mod cubemaps;
mod leak_confirm;
pub(crate) mod optimize;
mod placement;
pub(super) mod portals;

use hp_app::i18n::TrTable;
use hp_app::runtime::HealthRuntime;
use hp_app::{Action, Severity, t, tf};
use hp_editor::ObjRef;

use super::Package;
use crate::app::HammerProApp;

pub(crate) struct Health;

#[derive(Default)]
pub(crate) struct State {
    portals: portals::Portals,
    leak_report_wanted: bool,
    compiling: bool,
    compile_started: Option<std::time::SystemTime>,
    vis_tried: bool,
    dup_serial: u64,
    optimize: Option<optimize::Job>,
    placement: placement::PlacementChecks,
    cube_revision: u64,
    cube_changed: Option<std::time::Instant>,
}

const HANDLED: &[Action] = &[
    Action::CheckProblems,
    Action::MapInfo,
    Action::CheckLeaks,
    Action::LoadPortalFile,
    Action::UnloadPortalFile,
    Action::TogglePvs,
    Action::ShowPortalViewer,
    Action::BuildPortals,
    Action::SealAreaportal,
    Action::ShowDetailSuggestions,
    Action::ShowOptimize,
    Action::AnalyzeOptimization,
    Action::ToggleVisleaves,
    Action::ShowCubemaps,
    Action::ToggleCubemapAssignment,
];

impl Package for Health {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn init(&self, _app: &mut HammerProApp) {
        hp_app::extend::set_leak_confirm(leak_confirm::confirm);
    }

    fn handle(&self, app: &mut HammerProApp, ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::CheckProblems => {
                app.state.ui.show_problems = true;
                if !app.state.settings.health.live_checks {
                    app.state.runtime.health.recheck = true;
                }
            }
            Action::MapInfo => app.state.ui.show_map_info = true,
            Action::CheckLeaks => {
                let rt = &mut app.state.runtime.health;
                rt.recheck = true;
                rt.leak_requested = true;
                rt.show_leak = true;
                app.pkg.health.leak_report_wanted = true;
                ctx.request_repaint();
            }
            Action::LoadPortalFile => {
                let state = &mut app.state;
                app.pkg.health.portals.load_or_pick(state);
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            Action::UnloadPortalFile => {
                app.pkg.health.portals.cancel(&mut app.state);
                app.state.runtime.health.clear_portals();
                app.state.runtime.health.show_portal_window = false;
            }
            Action::ShowPortalViewer => app.state.runtime.health.show_portal_window = true,
            Action::ShowDetailSuggestions => {
                HealthRuntime::show_detail_suggestions(&mut app.state);
                ctx.request_repaint();
            }
            Action::SealAreaportal => seal_opening(app),
            Action::ShowOptimize => {
                let rt = &mut app.state.runtime.optimize;
                rt.show = true;
                if rt.report.is_none() && !rt.running {
                    rt.request = true;
                }
                ctx.request_repaint();
            }
            Action::AnalyzeOptimization => {
                app.state.runtime.optimize.request = true;
                ctx.request_repaint();
            }
            Action::ToggleVisleaves => {
                let on = !app.state.runtime.optimize.view;
                if on && app.state.runtime.optimize.portals.is_none() {
                    app.state.runtime.optimize.view = true;
                    app.state.runtime.optimize.request = true;
                } else {
                    optimize::set_view(&mut app.state, on);
                }
                ctx.request_repaint();
            }
            Action::ShowCubemaps => {
                let rt = &mut app.state.runtime.cubemaps;
                rt.show = true;
                if rt.report.is_none() {
                    rt.request = true;
                }
                ctx.request_repaint();
            }
            Action::ToggleCubemapAssignment => {
                let rt = &mut app.state.runtime.cubemaps;
                rt.view = !rt.view;
                if rt.view && rt.report.is_none() {
                    rt.request = true;
                }
                ctx.request_repaint();
            }
            Action::BuildPortals => {
                app.state.runtime.health.portal_build_request = true;
                app.state.runtime.health.show_portal_window = true;
                ctx.request_repaint();
            }
            Action::TogglePvs => {
                let rt = &mut app.state.runtime.health;
                rt.pvs = !rt.pvs;
                if rt.pvs {
                    app.pkg.health.vis_tried = true;
                    let state = &mut app.state;
                    app.pkg.health.portals.ensure_vis(state, false);
                }
            }
            _ => return false,
        }
        true
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        watch_duplicates(app);
        if app.state.runtime.health.recheck {
            app.pkg.health.placement.force();
        }
        let busy = HealthRuntime::tick(&mut app.state);
        let placing = app.pkg.health.placement.tick(&mut app.state);
        if app.pkg.health.leak_report_wanted {
            report_leak(app);
        }
        let compiling = app.state.runtime.compile.progress.is_some();
        let pkg = &mut app.pkg.health;
        if compiling && !pkg.compiling {
            pkg.compile_started = Some(std::time::SystemTime::now());
        }
        if !compiling && pkg.compiling {
            let since = pkg.compile_started.take();
            pkg.portals.after_compile(&mut app.state, since);
            pkg.vis_tried = false;
        }
        pkg.compiling = compiling;
        let dropped = app.state.runtime.workflow.take_dropped(&["prt"]);
        if let Some(path) = dropped.into_iter().next() {
            app.pkg.health.portals.load(&mut app.state, path);
        }
        let rt = &app.state.runtime.health;
        let heat = app.state.settings.health.portal_heat && rt.portals.is_some();
        let playing = rt.play_from.is_some();
        if (rt.pvs || heat || playing) && rt.vis.is_none() && !app.pkg.health.vis_tried {
            let quiet = !rt.pvs;
            app.pkg.health.vis_tried = true;
            let state = &mut app.state;
            app.pkg.health.portals.ensure_vis(state, quiet);
        }
        let working = app.pkg.health.portals.tick(&mut app.state);
        let analyzing = tick_optimize(app);
        let cubes = tick_cubemaps(app);
        HealthRuntime::update_pvs(&mut app.state);
        HealthRuntime::update_play(&mut app.state);
        if busy || working || analyzing || cubes || placing || app.pkg.health.leak_report_wanted {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
    }

    fn map_loaded(&self, app: &mut HammerProApp) {
        HealthRuntime::map_loaded(&mut app.state);
        app.pkg.health.leak_report_wanted = false;
        app.pkg.health.vis_tried = false;
        app.pkg.health.dup_serial = app.state.doc.last_step_serial().unwrap_or(0);
        app.pkg.health.portals.map_loaded();
        if let Some(job) = app.pkg.health.optimize.take() {
            job.cancel();
        }
        let rt = &mut app.state.runtime.optimize;
        *rt = hp_app::runtime::OptimizeRuntime { show: rt.show, ..Default::default() };
        let cm = &mut app.state.runtime.cubemaps;
        *cm = hp_app::runtime::CubemapRuntime {
            show: cm.show,
            view: cm.view,
            size: cm.size,
            request: cm.show || cm.view,
            ..Default::default()
        };
        app.pkg.health.placement.map_loaded();
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

fn report_leak(app: &mut HammerProApp) {
    let state = &mut app.state;
    let rt = &state.runtime.health;
    if rt.checking || rt.leak_checking || rt.recheck {
        return;
    }
    let Some(result) = rt.leak.clone() else {
        if !rt.leak_requested {
            app.pkg.health.leak_report_wanted = false;
        }
        return;
    };
    if result.serial != state.doc.content_serial() {
        return;
    }
    app.pkg.health.leak_report_wanted = false;
    state.runtime.health.leak_requested = false;
    let report = &result.report;
    if report.incomplete {
        state.notify(Severity::Info, t!("app.health.leak_incomplete"));
    } else if report.no_world {
        state.notify(Severity::Info, t!("app.health.leak_no_world"));
    } else if let Some(leak) = &report.leak {
        let class = state.doc.scene().entity(leak.entity).map_or_else(String::new, |n| n.entity.classname.clone());
        state.notify(Severity::Warning, tf!("app.health.leak_found", leak.entity, class, report.leaking.len()));
        if state.doc.scene().contains(ObjRef::Entity(leak.entity)) {
            state.doc.set_selection(&[ObjRef::Entity(leak.entity)]);
            state.request(Action::Center2dOnSelection);
            state.request(Action::Center3dOnSelection);
        }
        state.runtime.health.show_leak = true;
    } else {
        state.notify(Severity::Success, tf!("app.health.sealed", result.ms));
    }
}

fn is_door(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    c.starts_with("func_door") || c == "prop_door_rotating" || c == "func_movelinear"
}

fn tick_optimize(app: &mut HammerProApp) -> bool {
    if let Some(job) = &app.pkg.health.optimize
        && job.poll(&mut app.state)
    {
        app.pkg.health.optimize = None;
    }
    if app.state.runtime.optimize.request && app.pkg.health.optimize.is_none() {
        app.state.runtime.optimize.request = false;
        app.pkg.health.optimize = optimize::Job::start(&mut app.state);
    }
    app.pkg.health.optimize.is_some()
}

fn tick_cubemaps(app: &mut HammerProApp) -> bool {
    let revision = app.state.doc.scene_revision();
    let pkg = &mut app.pkg.health;
    if revision != pkg.cube_revision {
        pkg.cube_revision = revision;
        pkg.cube_changed = Some(std::time::Instant::now());
    }
    let changed = pkg.cube_changed;
    cubemaps::tick(&mut app.state, changed)
}

fn seal_opening(app: &mut HammerProApp) {
    let state = &mut app.state;
    let doc = &state.doc;
    let door = doc.selected_objects().into_iter().find_map(|o| {
        let id = match o {
            ObjRef::Entity(id) => id,
            ObjRef::Solid(s) => match doc.scene().solid(s)?.owner {
                hp_editor::Owner::Entity(e) => e,
                hp_editor::Owner::World => return None,
            },
        };
        doc.scene().entity(id).filter(|n| is_door(&n.entity.classname)).map(|n| (id, n.entity.clone()))
    });
    let Some((id, entity)) = door else {
        state.notify(Severity::Info, t!("app.health.seal_select_door"));
        return;
    };
    let bounds = if doc.scene().is_brush_entity(id) {
        doc.scene().entity_solid_bounds(id)
    } else {
        state.entity_bounds(&entity)
    };
    let Some(portal) = bounds.and_then(|b| hp_editor::limits::areaportal::fit_opening(&state.doc, b)) else {
        state.notify(Severity::Warning, t!("app.health.seal_no_bounds"));
        return;
    };
    match state.doc.seal_opening(id, portal) {
        Ok(_) => state.notify(Severity::Success, tf!("app.health.sealed_opening", id, entity.classname)),
        Err(e) => state.notify(
            Severity::Warning,
            tf!("app.op_failed", Action::SealAreaportal.label(), super::edit_error_text(&e)),
        ),
    }
}

fn watch_duplicates(app: &mut HammerProApp) {
    let Some(serial) = app.state.doc.last_step_serial() else { return };
    if serial <= app.pkg.health.dup_serial {
        if serial < app.pkg.health.dup_serial && !app.state.doc.can_redo() {
            app.pkg.health.dup_serial = serial;
        }
        return;
    }
    app.pkg.health.dup_serial = serial;
    if !app.state.settings.health.warn_duplicates || app.state.doc.in_transaction() {
        return;
    }
    let Some((_, created)) = app.state.doc.last_step_created() else { return };
    if created.is_empty() {
        return;
    }
    let live = app.state.doc.duplicates_of(&created);
    if let Some(text) = duplicate_text(&live) {
        app.state.notify(Severity::Warning, text);
    }
}

fn duplicate_text(live: &hp_editor::duplicates::LiveDuplicates) -> Option<String> {
    let text = match (live.brushes.as_slice(), live.entities.as_slice()) {
        ([], []) => return None,
        ([(copy, original)], []) => tf!("app.health.duplicate_brush", copy, original),
        ([], [(copy, original)]) => tf!("app.health.duplicate_entity", copy, original),
        _ => tf!("app.health.duplicate_many", live.count()),
    };
    Some(text)
}

const TR: TrTable = &[
    (
        "app.health.duplicate_brush",
        "Brush {0} is an exact copy of brush {1} at the same place (Check for problems can delete it)",
        "Le brush {0} est une copie exacte du brush {1} au même endroit (Vérifier les problèmes peut le supprimer)",
    ),
    (
        "app.health.duplicate_entity",
        "Entity {0} is stacked on the identical entity {1} (Check for problems can delete it)",
        "L'entité {0} est empilée sur l'entité identique {1} (Vérifier les problèmes peut la supprimer)",
    ),
    (
        "app.health.duplicate_many",
        "{0} new objects are exact copies of objects at the same place (Check for problems can delete them)",
        "{0} nouveaux objets sont des copies exactes d'objets au même endroit (Vérifier les problèmes peut les supprimer)",
    ),
    (
        "app.health.seal_select_door",
        "Select a door (func_door, prop_door_rotating...) to seal its doorway",
        "Sélectionnez une porte (func_door, prop_door_rotating...) pour fermer son ouverture",
    ),
    ("app.health.seal_no_bounds", "The size of this door is not known", "La taille de cette porte est inconnue"),
    (
        "app.health.sealed_opening",
        "Areaportal created and linked to door {0} ({1})",
        "Areaportal créé et lié à la porte {0} ({1})",
    ),
    (
        "app.health.leak_found",
        "The map leaks: entity {0} ({1}) reaches the void ({2} entities in all). Its path is drawn in the views.",
        "La map fuit : l'entité {0} ({1}) atteint le vide ({2} entités en tout). Son chemin est tracé dans les vues.",
    ),
    (
        "app.health.sealed",
        "No leak: the map is sealed (checked in {0} ms)",
        "Aucune fuite : la map est étanche (vérifiée en {0} ms)",
    ),
    (
        "app.health.leak_no_world",
        "No world brush seals the map yet: nothing to check",
        "Aucun brush du monde ne ferme encore la map : rien à vérifier",
    ),
    (
        "app.health.leak_incomplete",
        "The map is too complex for the instant leak check: compile it to find leaks",
        "La map est trop complexe pour la vérification instantanée : compilez-la pour trouver les fuites",
    ),
    ("app.health.portals_loaded_one", "{0} portal loaded ({1} clusters)", "{0} portail chargé ({1} clusters)"),
    ("app.health.portals_loaded", "{0} portals loaded ({1} clusters)", "{0} portails chargés ({1} clusters)"),
    (
        "app.health.portals_failed",
        "Cannot read the portal file: {0}",
        "Impossible de lire le fichier de portails : {0}",
    ),
    ("app.health.portal_filter", "Portal files", "Fichiers de portails"),
    (
        "app.health.no_portal_file",
        "No portal file: compile the map first, or use \"Geometry only\"",
        "Aucun fichier de portails : compilez d'abord la map, ou utilisez « Géométrie seule »",
    ),
    (
        "app.health.no_bsp",
        "No compiled map with visibility: compile the map first",
        "Aucune map compilée avec visibilité : compilez d'abord la map",
    ),
    (
        "app.health.no_vis",
        "The compiled map has no visibility data (compiled without visibility)",
        "La map compilée n'a pas de visibilité (compilée sans visibilité)",
    ),
    ("app.health.bsp_failed", "Cannot read the compiled map: {0}", "Impossible de lire la map compilée : {0}"),
    ("app.health.vbsp_started", "Building the portals...", "Construction des portails..."),
    ("app.health.vbsp_failed", "The geometry build failed: {0}", "La construction de la géométrie a échoué : {0}"),
    (
        "app.health.vbsp_leak",
        "The map leaks: no portal file (Check for leaks shows where)",
        "La map fuit : pas de fichier de portails (Vérifier les fuites montre où)",
    ),
    (
        "app.health.no_compiler",
        "The game was not found: set it up in Settings",
        "Le jeu est introuvable : configurez-le dans les paramètres",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_clean_and_doors_are_known() {
        assert!(hp_app::i18n::lint_table(TR).is_empty(), "{:#?}", hp_app::i18n::lint_table(TR));
        assert!(TR.iter().all(|(k, _, _)| k.starts_with("app.health.")));
        assert!(is_door("func_door_rotating") && is_door("PROP_DOOR_ROTATING"));
        assert!(!is_door("func_brush"));
    }
}
