use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use hp_app::i18n::TrTable;
use hp_app::state::RightTab;
use hp_app::{Action, Severity, ToolId, t, tf};
use hp_editor::{Document, EditError, FaceRef, Id, ObjRef, Owner, SelectOp};
use hp_geom::{CoverageSolid, HiddenFace, SolidGeom};

use super::Package;
use crate::app::HammerProApp;

pub(crate) mod func_detail;

pub(crate) struct Brush;

#[derive(Default)]
pub(crate) struct State {
    nodraw_scan: Option<NodrawScan>,
    nodraw_prompt: Option<NodrawPrompt>,
    pub(super) inset: super::inset::InsetPrompt,
    pub(crate) inset_modal: Option<hp_tools_create::InsetModal>,
    pub(crate) detail: func_detail::DetailState,
}

impl State {
    pub(crate) fn drop_inset_modal(&mut self) {
        self.inset_modal = None;
    }

    pub(crate) fn settle_inset_modal(&mut self) {
        if self.inset_modal.as_ref().is_some_and(|m| !m.is_active()) {
            self.inset_modal = None;
        }
    }
}

struct NodrawScan {
    ids: Vec<Id>,
    rx: Receiver<(Vec<CoverageSolid>, Vec<HiddenFace>)>,
}

struct NodrawPrompt {
    faces: Vec<FaceRef>,
    deps: Vec<(Id, Arc<SolidGeom>)>,
}

const HANDLED: &[Action] = &[
    Action::TieToEntity,
    Action::MoveToWorld,
    Action::Carve,
    Action::MakeHollow,
    Action::MergeConvex,
    Action::MergeBrushes,
    Action::IntersectBrushes,
    Action::UnionBrushes,
    Action::TieEachToEntity,
    Action::ConvertToDetail,
    Action::SelectDetailCandidates,
    Action::ToggleDetailTint,
    Action::CurveEdit,
    Action::RoadEdit,
    Action::MakeHintBrush,
    Action::NodrawHiddenFaces,
    Action::InsetFace,
    Action::InsetInteractive,
    Action::ExtrudeInteractive,
];

impl Package for Brush {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::TieToEntity => app.tie_to_entity(),
            Action::TieEachToEntity => {
                if app.require_selection() {
                    let objs = app.selected();
                    let class = app.state.settings.edit.default_brush_entity.clone();
                    let kv = app.brush_entity_defaults(&class);
                    let origin = app.brush_entity_needs_origin(&class);
                    let result = app.state.doc.transaction("Tie each to entity", |doc| {
                        let ids = doc.tie_each_to_entity(&objs, &class, &kv)?;
                        if origin {
                            doc.add_missing_origins(&ids)?;
                        }
                        Ok::<_, hp_editor::EditError>(ids)
                    });
                    if app.report(action, result).is_some() {
                        app.show_panel(RightTab::Entity);
                    }
                }
            }
            Action::ConvertToDetail => app.make_func_detail(),
            Action::SelectDetailCandidates => app.open_detail_candidates(),
            Action::ToggleDetailTint => {
                let d = &mut app.state.settings.brush.detail;
                d.tint = !d.tint;
                app.state.settings_dirty = true;
            }
            Action::RoadEdit => match hp_tools_create::road::apply::selected_road_piece(&app.state.doc) {
                Some(id) => {
                    app.state.runtime.brush.road_edit = Some(id);
                    app.state.set_tool(ToolId::Road);
                }
                None => app.state.notify(Severity::Info, t!("road.edit_need")),
            },
            Action::CurveEdit => match app.state.doc.selected_curve_piece() {
                Some(id) if hp_tools_create::road::apply::read_road(&app.state.doc, id).is_some() => {
                    app.state.runtime.brush.road_edit = Some(id);
                    app.state.set_tool(ToolId::Road);
                }
                Some(id) => {
                    app.state.runtime.brush.curve_edit = Some(id);
                    app.state.set_tool(ToolId::Curve);
                }
                None => app.state.notify(Severity::Info, t!("curve.edit_need")),
            },
            Action::MoveToWorld => {
                if app.require_selection() {
                    let objs = app.selected();
                    let result = app.state.doc.move_to_world(&objs);
                    app.report(action, result);
                }
            }
            Action::Carve => {
                let solids = app.selected_solids();
                if solids.is_empty() {
                    app.state.notify(Severity::Info, t!("app.need_solids"));
                } else {
                    let result = app.state.doc.carve(&solids);
                    app.report(action, result);
                }
            }
            Action::MakeHollow => {
                if app.selected_solids().is_empty() {
                    app.state.notify(Severity::Info, t!("app.need_solids"));
                } else {
                    app.hollow_prompt = Some(crate::prompts::HollowPrompt::new(32.0));
                }
            }
            Action::MergeConvex => {
                let solids = app.selected_solids();
                if solids.is_empty() {
                    app.state.notify(Severity::Info, t!("app.need_solids"));
                } else if solids.len() < 2 {
                    app.state.notify(Severity::Info, t!("app.need_two_solids"));
                } else {
                    let result = app.state.doc.merge_solids(&solids);
                    app.report(action, result);
                }
            }
            Action::MergeBrushes => app.merge_brushes(),
            Action::IntersectBrushes => app.boolean_brushes(action),
            Action::UnionBrushes => app.boolean_brushes(action),
            Action::MakeHintBrush => {
                let faces = app.state.doc.selected_faces();
                if faces.is_empty() {
                    app.state.notify(Severity::Info, t!("brush.hint_need_face"));
                } else {
                    let result = app.state.doc.make_hint_brushes(&faces);
                    if let Some(n) = app.report(action, result) {
                        app.state.log(Severity::Info, tf!("brush.hint_done", n));
                    }
                }
            }
            Action::NodrawHiddenFaces => app.start_nodraw_scan(),
            Action::InsetFace => app.start_inset(),
            Action::InsetInteractive => app.start_inset_modal(ctx),
            Action::ExtrudeInteractive => app.start_extrude_modal(ctx),
            _ => return false,
        }
        true
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        app.poll_nodraw_scan(ctx);
        app.poll_func_detail(ctx);
    }

    fn dialogs(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        app.nodraw_dialog(ctx);
        app.inset_dialog(ctx);
        app.func_detail_dialogs(ctx);
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

impl HammerProApp {
    pub(crate) fn make_hollow(&mut self, thickness: f64) {
        let solids = self.selected_solids();
        if solids.is_empty() {
            self.state.notify(Severity::Info, t!("app.need_solids"));
            return;
        }
        let result = self.state.doc.hollow(&solids, thickness);
        self.report(Action::MakeHollow, result);
    }

    fn brush_entity_defaults(&self, class: &str) -> Vec<(String, String)> {
        self.state.assets.fgd().and_then(|f| f.class(class)).map(|c| c.default_keyvalues()).unwrap_or_default()
    }

    fn brush_entity_needs_origin(&self, class: &str) -> bool {
        hp_app::runtime::helpers::brush_entity_needs_origin(self.state.assets.fgd(), class)
    }

    fn tie_to_entity(&mut self) {
        if !self.require_selection() {
            return;
        }
        let objs = self.selected();
        if let Some(target) = self.state.doc.tie_target(&objs) {
            let result = self.state.doc.tie_to_existing(&objs, target);
            if self.report(Action::TieToEntity, result).is_some() {
                self.show_panel(RightTab::Entity);
            }
            return;
        }
        if single_brush_entity(&self.state.doc, &objs) {
            self.show_panel(RightTab::Entity);
            return;
        }
        let class = self.state.settings.edit.default_brush_entity.clone();
        let kv = self.brush_entity_defaults(&class);
        let origin = self.brush_entity_needs_origin(&class);
        let result = self.state.doc.transaction("Tie to entity", |doc| {
            let id = doc.tie_to_entity(&objs, &class, &kv)?;
            if origin {
                doc.add_missing_origins(&[id])?;
            }
            Ok::<_, hp_editor::EditError>(id)
        });
        if self.report(Action::TieToEntity, result).is_some() {
            self.show_panel(RightTab::Entity);
        }
    }

    fn merge_brushes(&mut self) {
        let solids = self.state.doc.selected_solid_ids();
        if solids.is_empty() {
            self.state.notify(Severity::Info, t!("app.need_solids"));
            return;
        }
        if solids.len() < 2 {
            self.state.notify(Severity::Info, t!("app.need_two_solids"));
            return;
        }
        match self.state.doc.merge_brushes(&solids) {
            Ok(_) => {}
            Err(EditError::Invalid(s)) if s == Document::MERGE_NOT_CONVEX => {
                self.state.notify(Severity::Warning, t!("brush.merge_not_convex"));
            }
            Err(EditError::Invalid(s)) if s == Document::MERGE_MIXED_OWNERS => {
                self.state.notify(Severity::Warning, t!("brush.merge_mixed_owners"));
            }
            Err(e) => {
                self.report(Action::MergeBrushes, Err::<(), _>(e));
            }
        }
    }
}

impl HammerProApp {
    fn boolean_brushes(&mut self, action: Action) {
        let solids = self.state.doc.selected_solid_ids();
        if solids.is_empty() {
            self.state.notify(Severity::Info, t!("app.need_solids"));
            return;
        }
        if solids.len() < 2 {
            self.state.notify(Severity::Info, t!("app.need_two_solids"));
            return;
        }
        let result = if action == Action::IntersectBrushes {
            self.state.doc.intersect_solids(&solids).map(|_| ())
        } else {
            self.state.doc.union_solids(&solids).map(|_| ())
        };
        match result {
            Ok(()) => {}
            Err(EditError::Invalid(s)) if s == Document::MERGE_MIXED_OWNERS => {
                self.state.notify(Severity::Warning, t!("brush.merge_mixed_owners"));
            }
            Err(EditError::Invalid(s)) if s == Document::BOOLEAN_NO_OVERLAP => {
                self.state.notify(Severity::Warning, t!("brush.intersect_empty"));
            }
            Err(EditError::Invalid(s)) if s == Document::UNION_UNCHANGED => {
                self.state.notify(Severity::Warning, t!("brush.union_unchanged"));
            }
            Err(e) => {
                self.report(action, Err::<(), _>(e));
            }
        }
    }

    pub(crate) fn bevel_shortcut(&mut self, ctx: &egui::Context) -> bool {
        if !matches!(self.state.tool, ToolId::Select | ToolId::Vertex) || !self.has_tool(ToolId::Bevel) {
            return false;
        }
        let picked = self.state.runtime.selection.fresh_edges(self.state.doc.revision()).to_vec();
        if !picked.is_empty() {
            self.state.runtime.brush.bevel_edges = picked;
            self.switch_tool(ToolId::Bevel);
            return true;
        }
        let Some((view, pos)) = self.viewports.hovered_view(&self.state, ctx) else { return false };
        if view.is_2d() || hp_tools_create::bevel::edge_at(&self.state, &view, pos, false).is_none() {
            return false;
        }
        self.state.runtime.brush.bevel_start = Some((view, pos));
        self.switch_tool(ToolId::Bevel);
        self.state.runtime.brush.bevel_start = None;
        true
    }

    fn start_nodraw_scan(&mut self) {
        if self.pkg.brush.nodraw_scan.is_some() {
            return;
        }
        self.pkg.brush.nodraw_prompt = None;
        let (ids, solids) = self.state.doc.hidden_face_scan();
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("hp-nodraw-scan".to_owned()).spawn(move || {
            let found = hp_editor::background::run(|| hp_geom::hidden_faces(&solids));
            let _ = tx.send((solids, found));
        });
        match spawned {
            Ok(_) => {
                self.pkg.brush.nodraw_scan = Some(NodrawScan { ids, rx });
                self.state.log(Severity::Info, t!("brush.nodraw_searching"));
            }
            Err(e) => self.state.notify(Severity::Error, tf!("brush.nodraw_failed", e)),
        }
    }

    fn poll_nodraw_scan(&mut self, ctx: &egui::Context) {
        let Some(scan) = self.pkg.brush.nodraw_scan.take() else { return };
        let (solids, found) = match scan.rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => {
                self.pkg.brush.nodraw_scan = Some(scan);
                ctx.request_repaint_after(std::time::Duration::from_millis(30));
                return;
            }
            Err(TryRecvError::Disconnected) => {
                self.state.notify(Severity::Error, tf!("brush.nodraw_failed", "worker stopped"));
                return;
            }
        };
        let mut faces = Vec::with_capacity(found.len());
        let mut deps: Vec<(Id, Arc<SolidGeom>)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for h in &found {
            let (Some(id), Some(s)) = (scan.ids.get(h.solid), solids.get(h.solid)) else { continue };
            let Some(face) = s.geom.faces.get(h.face) else { continue };
            faces.push(FaceRef { solid: *id, side: face.side_index });
            for i in std::iter::once(h.solid).chain(h.covered_by.iter().copied()) {
                if seen.insert(i)
                    && let (Some(id), Some(s)) = (scan.ids.get(i), solids.get(i))
                {
                    deps.push((*id, s.geom.clone()));
                }
            }
        }
        if !geometry_unchanged(&self.state.doc, &deps) {
            self.state.notify(Severity::Warning, t!("brush.nodraw_changed"));
            return;
        }
        if faces.is_empty() {
            self.state.notify(Severity::Info, t!("brush.nodraw_none"));
            return;
        }
        self.state.set_tool(ToolId::Texture);
        self.state.doc.select_faces(&faces, SelectOp::Replace);
        self.pkg.brush.nodraw_prompt = Some(NodrawPrompt { faces, deps });
    }

    fn nodraw_dialog(&mut self, ctx: &egui::Context) {
        let Some(prompt) = &self.pkg.brush.nodraw_prompt else { return };
        let count = prompt.faces.len();
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        hp_app::chrome::Window::new(t!("brush.nodraw_title"))
            .id(egui::Id::unique("hp_brush_nodraw"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
            .show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.label(tf!("brush.nodraw_found", count));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    apply = ui.button(t!("brush.nodraw_apply")).clicked();
                    cancel = ui.button(t!("common.cancel")).clicked();
                });
            });
        if apply {
            let Some(prompt) = self.pkg.brush.nodraw_prompt.take() else { return };
            if geometry_unchanged(&self.state.doc, &prompt.deps) {
                let result = self.state.doc.apply_nodraw(&prompt.faces);
                if let Some(n) = self.report(Action::NodrawHiddenFaces, result) {
                    self.state.log(Severity::Info, tf!("brush.nodraw_done", n));
                }
            } else {
                self.state.notify(Severity::Warning, t!("brush.nodraw_changed"));
            }
        } else if cancel || !open {
            self.pkg.brush.nodraw_prompt = None;
        }
    }
}

fn geometry_unchanged(doc: &Document, deps: &[(Id, Arc<SolidGeom>)]) -> bool {
    deps.iter().all(|(id, geom)| doc.scene().solid(*id).is_some_and(|n| Arc::ptr_eq(&n.geom, geom)))
}

fn single_brush_entity(doc: &Document, objs: &[ObjRef]) -> bool {
    let mut owner = None;
    for o in objs {
        let e = match *o {
            ObjRef::Entity(e) if doc.scene().entity_solid_count(e) > 0 => e,
            ObjRef::Solid(s) => match doc.scene().solid(s).map(|n| n.owner) {
                Some(Owner::Entity(e)) => e,
                _ => return false,
            },
            ObjRef::Entity(_) => return false,
        };
        if owner.is_some_and(|o| o != e) {
            return false;
        }
        owner = Some(e);
    }
    owner.is_some()
}

const TR: TrTable = &[
    (
        "brush.merge_not_convex",
        "The brushes do not form a convex shape. \"Merge into convex solid\" wraps them in their convex hull instead.",
        "Les brushes ne forment pas une forme convexe. « Fusionner en un solide convexe » utilise plutôt leur enveloppe convexe.",
    ),
    (
        "brush.intersect_empty",
        "The brushes do not overlap: there is no shared volume to keep",
        "Les brushes ne se chevauchent pas : il n'y a aucun volume commun à conserver",
    ),
    (
        "brush.union_unchanged",
        "Nothing to join: the brushes do not overlap and no touching pair forms a convex shape",
        "Rien à joindre : les brushes ne se chevauchent pas et aucune paire en contact ne forme une forme convexe",
    ),
    (
        "brush.merge_mixed_owners",
        "The brushes must all be world brushes or belong to the same entity",
        "Les brushes doivent tous être des brushes du monde ou appartenir à la même entité",
    ),
    (
        "brush.hint_need_face",
        "Select the faces that become hint faces with the texture tool first",
        "Sélectionnez d'abord les faces qui deviennent des faces hint avec l'outil de texture",
    ),
    ("brush.hint_done", "{0} hint brush(es) made", "{0} brush(es) hint créé(s)"),
    ("inset.need_face", "Select the faces to inset first", "Sélectionnez d'abord les faces à décaler"),
    ("inset.faces", "{0} face(s) selected", "{0} face(s) sélectionnée(s)"),
    ("inset.distance", "Distance", "Distance"),
    (
        "inset.distance_tip",
        "How far the edges of the face move inward, in units",
        "De combien les arêtes de la face reculent vers l'intérieur, en unités",
    ),
    ("inset.depth", "Depth", "Profondeur"),
    (
        "inset.depth_tip",
        "Raise (positive) or sink (negative) the inner block in the same step, 0 keeps it flush",
        "Surélève (positif) ou enfonce (négatif) le bloc intérieur dans la même opération, 0 le laisse au ras",
    ),
    ("inset.snap", "Snap to grid", "Aligner sur la grille"),
    (
        "inset.hint",
        "The brush is split into a frame of quads and an inner block, all convex. Live version: press I in the 3D view (Tools > Brush operations > Interactive inset).",
        "Le brush est découpé en un cadre de quadrilatères et un bloc intérieur, tous convexes. En direct : touche I dans la vue 3D (Outils > Opérations sur les brushes > Décalage interactif).",
    ),
    ("inset.done", "{0} brush(es) after the inset", "{0} brush(es) après le décalage"),
    ("inset.err_face", "The side is not a face of the brush", "Le côté n'est pas une face du brush"),
    ("inset.err_disp", "The face is a displacement", "La face est une displacement"),
    (
        "inset.err_distance",
        "The distance must be from 0.125 to 16384 units",
        "La distance doit aller de 0,125 à 16384 unités",
    ),
    ("inset.err_large", "The distance is larger than the face", "La distance dépasse la taille de la face"),
    (
        "inset.err_deep",
        "The sink depth must stay under the brush thickness",
        "La profondeur d'enfoncement doit rester sous l'épaisseur du brush",
    ),
    ("inset.err_limits", "The result goes past the map limits", "Le résultat dépasse les limites de la carte"),
    (
        "inset.err_region",
        "The selected faces do not form one simple region (hole or single point contact)",
        "Les faces sélectionnées ne forment pas une région simple (trou ou contact en un seul point)",
    ),
    ("brush.already_detail", "The selection is already a func_detail", "La sélection est déjà un func_detail"),
    (
        "brush.detail.nothing",
        "Nothing to turn into func_detail: select world brushes.",
        "Rien à transformer en func_detail : sélectionnez des brushes du monde.",
    ),
    (
        "brush.detail.skipped_already",
        "{0} already in a func_detail (left as is).",
        "{0} déjà dans un func_detail (laissés tels quels).",
    ),
    (
        "brush.detail.skipped_entity",
        "{0} belong to another brush entity (left as is: Move to world first).",
        "{0} appartiennent à une autre entité brush (laissés tels quels : Vers le monde d'abord).",
    ),
    (
        "brush.detail.skipped_disp",
        "{0} displacements skipped (they never cut visleaves).",
        "{0} displacements ignorés (ils ne découpent jamais les visleaves).",
    ),
    (
        "brush.detail.checking",
        "Checking which brushes seal the map...",
        "Vérification des brushes qui scellent la map...",
    ),
    ("brush.detail.failed", "Make func_detail failed: {0}", "Échec de la transformation en func_detail : {0}"),
    (
        "brush.detail.made",
        "Made func_detail: {0} brushes in {1} func_detail.",
        "func_detail créé : {0} brushes dans {1} func_detail.",
    ),
    (
        "brush.detail.kept_sealing",
        "{0} kept in the world because they seal the map.",
        "{0} gardés dans le monde car ils scellent la map.",
    ),
    (
        "brush.detail.unverified_leak",
        "The map leaks, so the sealing could not be checked (Map > Check for leaks).",
        "La map fuit, l'étanchéité n'a donc pas pu être vérifiée (Map > Vérifier les fuites).",
    ),
    (
        "brush.detail.unverified_complex",
        "The map is too complex for the sealing check: review the result.",
        "La map est trop complexe pour la vérification d'étanchéité : contrôlez le résultat.",
    ),
    (
        "brush.detail.changed",
        "The map changed during the check: try again.",
        "La map a changé pendant la vérification : réessayez.",
    ),
    (
        "brush.detail.busy",
        "Another edit is in progress: try again.",
        "Une autre modification est en cours : réessayez.",
    ),
    (
        "brush.detail.all_sealing",
        "Nothing converted: the {0} brushes seal the map (they are part of the outer shell).",
        "Rien de converti : les {0} brushes scellent la map (ils font partie de la coque extérieure).",
    ),
    ("brush.detail.seal_title", "Make func_detail", "Transformer en func_detail"),
    (
        "brush.detail.seal_count",
        "{0} of the {1} selected brushes seal the map",
        "{0} des {1} brushes sélectionnés scellent la map",
    ),
    (
        "brush.detail.seal_why",
        "They are part of the outer shell between the playable space and the void. A func_detail does not seal, so converting them would make the map leak: no visibility and no lighting in the compile.",
        "Ils font partie de la coque extérieure entre l'espace jouable et le vide. Un func_detail ne scelle pas : les convertir ferait fuir la map, sans visibilité ni éclairage à la compilation.",
    ),
    ("brush.detail.seal_others", "Convert the other {0}", "Convertir les {0} autres"),
    ("brush.detail.seal_all", "Convert all anyway", "Tout convertir quand même"),
    (
        "brush.detail.seal_all_tip",
        "The map will leak until the hull is closed another way",
        "La map fuira tant que la coque n'est pas fermée autrement",
    ),
    ("brush.detail.seal_select", "Select the sealing brushes", "Sélectionner les brushes qui scellent"),
    ("brush.detail.cand_title", "func_detail candidates", "Candidats func_detail"),
    (
        "brush.detail.cand_intro",
        "Selects the world brushes that should be func_detail, with the rules of Map > Optimize: small and decorative pieces, steps, trims, pillars, rubble, never part of the sealing hull. Review the selection, then press Make func_detail.",
        "Sélectionne les brushes du monde qui devraient être des func_detail, avec les règles de Map > Optimiser : petites pièces décoratives, marches, moulures, piliers, gravats, jamais dans la coque étanche. Vérifiez la sélection puis cliquez sur Transformer en func_detail.",
    ),
    ("brush.detail.cand_max_size", "Largest size", "Taille maximale"),
    (
        "brush.detail.cand_max_size_tip",
        "Brushes longer than this on any axis are left out",
        "Les brushes plus longs que cela sur un axe sont exclus",
    ),
    ("brush.detail.cand_steps", "Steps and stairs", "Marches et escaliers"),
    ("brush.detail.cand_trims", "Trims and frames", "Moulures et encadrements"),
    ("brush.detail.cand_pillars", "Pillars and posts", "Piliers et poteaux"),
    ("brush.detail.cand_small", "Small pieces and rubble", "Petites pièces et gravats"),
    ("brush.detail.cand_complex", "Complex shapes (cylinders, arches)", "Formes complexes (cylindres, arches)"),
    ("brush.detail.cand_in_selection", "Only inside the current selection", "Seulement dans la sélection actuelle"),
    ("brush.detail.cand_running", "Checking the sealing...", "Vérification de l'étanchéité..."),
    ("brush.detail.cand_select", "Select candidates", "Sélectionner les candidats"),
    (
        "brush.detail.cand_make_tip",
        "Turn the selected candidates into func_detail",
        "Transformer les candidats sélectionnés en func_detail",
    ),
    ("brush.detail.cand_none", "No func_detail candidate found.", "Aucun candidat func_detail trouvé."),
    ("brush.detail.cand_selected", "{0} candidates selected.", "{0} candidats sélectionnés."),
    (
        "brush.detail.cand_kept",
        "{0} more kept as world because they seal the map.",
        "{0} autres gardés dans le monde car ils scellent la map.",
    ),
    ("brush.nodraw_title", "Nodraw on hidden faces", "Nodraw sur les faces cachées"),
    ("brush.nodraw_searching", "Searching for hidden faces...", "Recherche des faces cachées..."),
    (
        "brush.nodraw_found",
        "{0} face(s) are entirely covered by other brushes and can never be seen. They are selected: check them in the views, then apply tools/toolsnodraw to save their rendering and lightmap cost.",
        "{0} face(s) sont entièrement couvertes par d'autres brushes et ne sont jamais visibles. Elles sont sélectionnées : vérifiez-les dans les vues, puis appliquez tools/toolsnodraw pour économiser leur rendu et leur lightmap.",
    ),
    ("brush.nodraw_apply", "Apply nodraw", "Appliquer nodraw"),
    ("brush.nodraw_done", "Nodraw applied to {0} face(s)", "Nodraw appliqué à {0} face(s)"),
    ("brush.nodraw_none", "No hidden face found", "Aucune face cachée trouvée"),
    (
        "brush.nodraw_changed",
        "The map changed since the search: run it again",
        "La map a changé depuis la recherche : relancez-la",
    ),
    (
        "brush.nodraw_failed",
        "Could not search for hidden faces: {0}",
        "Impossible de rechercher les faces cachées : {0}",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty(), "{:#?}", hp_app::i18n::lint_table(TR));
        assert!(TR.iter().all(|(k, _, _)| k.starts_with("brush.") || k.starts_with("inset.")));
    }
}
