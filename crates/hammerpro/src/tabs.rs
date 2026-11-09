use std::collections::HashSet;
use std::path::Path;

use glam::DVec3;
use hp_app::runtime::{MapTabInfo, TabRequest};
use hp_app::sidecar::Sidecar;
use hp_app::state::{Cameras, Pointfile};
use hp_app::{AppState, Severity, t, tf};
use hp_editor::Document;

use crate::app::HammerProApp;
use crate::files::{self, PendingOp};

pub(crate) type TabId = u64;

pub(crate) struct Parked {
    pub doc: Document,
    pub cameras: Cameras,
    pub sidecar: Sidecar,
    pub pointfile: Option<Pointfile>,
    pub pivot: Option<DVec3>,
}

struct Tab {
    id: TabId,
    parked: Option<Parked>,
}

pub(crate) struct Tabs {
    tabs: Vec<Tab>,
    active: TabId,
    next_id: TabId,
    version: u64,
}

impl Default for Tabs {
    fn default() -> Self {
        Self { tabs: vec![Tab { id: 1, parked: None }], active: 1, next_id: 2, version: 0 }
    }
}

fn swap_into_state(state: &mut AppState, incoming: Parked) -> Parked {
    let Parked { doc, cameras, sidecar, pointfile, pivot } = incoming;
    let shown = Parked {
        doc: std::mem::replace(&mut state.doc, doc),
        cameras: std::mem::replace(&mut state.cameras, cameras),
        sidecar: std::mem::replace(&mut state.sidecar, sidecar),
        pointfile: std::mem::replace(&mut state.pointfile, pointfile),
        pivot: std::mem::replace(&mut state.pivot, pivot),
    };
    hp_app::games::apply_map_game(state);
    shown
}

fn park_scene(state: &mut AppState) {
    let AppState { assets, doc, gfx, .. } = state;
    if let Some(gfx) = gfx.as_mut() {
        assets.park_scene(doc, gfx);
    }
}

fn after_switch(state: &mut AppState) {
    state.doc.mark_everything_changed();
    state.pick = None;
    state.hover_world = None;
}

impl Tabs {
    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn active_id(&self) -> TabId {
        self.active
    }

    pub(crate) fn with_document(&mut self, state: &mut AppState, id: TabId, f: impl FnOnce(&mut AppState)) -> bool {
        if id == self.active {
            f(state);
            return true;
        }
        let Some(parked) = self.tabs.iter_mut().find(|tab| tab.id == id).and_then(|tab| tab.parked.as_mut()) else {
            return false;
        };
        std::mem::swap(&mut state.doc, &mut parked.doc);
        std::mem::swap(&mut state.cameras, &mut parked.cameras);
        f(state);
        std::mem::swap(&mut state.doc, &mut parked.doc);
        std::mem::swap(&mut state.cameras, &mut parked.cameras);
        true
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    fn active_index(&self) -> usize {
        self.index_of(self.active).unwrap_or(0)
    }

    pub fn contains(&self, id: TabId) -> bool {
        self.index_of(id).is_some()
    }

    pub fn ids(&self) -> Vec<TabId> {
        self.tabs.iter().map(|t| t.id).collect()
    }

    pub fn parked(&self) -> impl Iterator<Item = (TabId, &Parked)> {
        self.tabs.iter().filter_map(|t| t.parked.as_ref().map(|p| (t.id, p)))
    }

    pub fn parked_mut(&mut self) -> impl Iterator<Item = (TabId, &mut Parked)> {
        self.tabs.iter_mut().filter_map(|t| t.parked.as_mut().map(|p| (t.id, p)))
    }

    pub fn doc<'a>(&'a self, state: &'a AppState, id: TabId) -> Option<&'a Document> {
        if id == self.active {
            return Some(&state.doc);
        }
        self.tabs.iter().find(|t| t.id == id).and_then(|t| t.parked.as_ref()).map(|p| &p.doc)
    }

    pub fn map_view<'a>(&'a self, state: &'a AppState, id: TabId) -> Option<(&'a Document, &'a Cameras)> {
        if id == self.active {
            return Some((&state.doc, &state.cameras));
        }
        self.tabs.iter().find(|t| t.id == id).and_then(|t| t.parked.as_ref()).map(|p| (&p.doc, &p.cameras))
    }

    pub fn activate(&mut self, state: &mut AppState, id: TabId) -> bool {
        if id == self.active {
            return false;
        }
        let Some(i) = self.index_of(id) else { return false };
        let Some(incoming) = self.tabs[i].parked.take() else { return false };
        park_scene(state);
        let outgoing = swap_into_state(state, incoming);
        let a = self.active_index();
        self.tabs[a].parked = Some(outgoing);
        self.active = id;
        after_switch(state);
        self.version += 1;
        true
    }

    pub fn open_new(&mut self, state: &mut AppState, sidecar: Sidecar) -> TabId {
        let incoming =
            Parked { doc: Document::new_empty(), cameras: Cameras::default(), sidecar, pointfile: None, pivot: None };
        park_scene(state);
        let outgoing = swap_into_state(state, incoming);
        let a = self.active_index();
        self.tabs[a].parked = Some(outgoing);
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.insert(a + 1, Tab { id, parked: None });
        self.active = id;
        after_switch(state);
        self.version += 1;
        id
    }

    pub fn close_active(&mut self, state: &mut AppState) -> Option<Parked> {
        if self.tabs.len() < 2 {
            return None;
        }
        let a = self.active_index();
        let next = if a + 1 < self.tabs.len() { a + 1 } else { a - 1 };
        let incoming = self.tabs[next].parked.take()?;
        let outgoing = swap_into_state(state, incoming);
        self.active = self.tabs[next].id;
        self.tabs.remove(a);
        after_switch(state);
        self.version += 1;
        Some(outgoing)
    }

    pub fn touch(&mut self) {
        self.version += 1;
    }

    pub fn neighbor(&self, step: isize) -> Option<TabId> {
        let n = self.tabs.len();
        if n < 2 {
            return None;
        }
        let a = self.active_index() as isize;
        let i = (a + step).rem_euclid(n as isize) as usize;
        Some(self.tabs[i].id)
    }

    pub fn find_map(&self, state: &AppState, path: &Path) -> Option<TabId> {
        self.tabs.iter().find_map(|t| {
            let doc = match &t.parked {
                Some(p) => &p.doc,
                None => &state.doc,
            };
            doc.path().is_some_and(|p| same_file(p, path)).then_some(t.id)
        })
    }

    pub fn first_dirty(&self, state: &AppState, skip: &HashSet<TabId>) -> Option<TabId> {
        self.tabs.iter().find_map(|t| {
            let dirty = match &t.parked {
                Some(p) => p.doc.is_dirty(),
                None => state.doc.is_dirty(),
            };
            (dirty && !skip.contains(&t.id)).then_some(t.id)
        })
    }

    pub fn infos(&self, state: &AppState) -> Vec<MapTabInfo> {
        self.tabs
            .iter()
            .map(|t| {
                let doc = match &t.parked {
                    Some(p) => &p.doc,
                    None => &state.doc,
                };
                MapTabInfo {
                    id: t.id,
                    title: files::doc_display_name(doc),
                    path: doc.path().map(Path::to_path_buf),
                    dirty: doc.is_dirty(),
                }
            })
            .collect()
    }
}

pub(crate) fn same_file(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
        let s = abs.to_string_lossy().replace('\\', "/");
        if cfg!(windows) { s.to_lowercase() } else { s }
    };
    norm(a) == norm(b)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabAction {
    Switch(TabId),
    Close(TabId),
    CloseOthers(TabId),
    New,
    CopyPath(TabId),
    ShowInFolder(TabId),
}

pub(crate) const TAB_BAR_HEIGHT: f32 = 24.0;
const MAX_TAB_WIDTH: f32 = 220.0;

pub(crate) fn tab_bar(ui: &mut egui::Ui, tabs: &[MapTabInfo], active: TabId) -> Option<TabAction> {
    let mut action = None;
    egui::ScrollArea::horizontal().id_salt("hp_map_tabs").auto_shrink([false, true]).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for tab in tabs {
                if let Some(a) = tab_ui(ui, tab, tab.id == active, tabs.len()) {
                    action = Some(a);
                }
                ui.separator();
            }
            if ui.small_button("+").on_hover_text(hp_app::Action::NewMap.label()).clicked() {
                action = Some(TabAction::New);
            }
        });
    });
    action
}

fn tab_ui(ui: &mut egui::Ui, tab: &MapTabInfo, active: bool, count: usize) -> Option<TabAction> {
    let mut action = None;
    let text = if tab.dirty { format!("{}*", tab.title) } else { tab.title.clone() };
    let mut rich = egui::RichText::new(text);
    if active {
        rich = rich.strong();
    }
    let label = ui
        .scope(|ui| {
            ui.set_max_width(MAX_TAB_WIDTH);
            ui.add(egui::Button::selectable(active, rich).truncate().min_size(egui::vec2(0.0, TAB_BAR_HEIGHT - 4.0)))
        })
        .inner;
    let tip = match &tab.path {
        Some(p) => p.display().to_string(),
        None => t!("tabs.not_saved").to_owned(),
    };
    let label = label.on_hover_text(tip);
    if label.clicked() {
        action = Some(TabAction::Switch(tab.id));
    }
    if label.middle_clicked() {
        action = Some(TabAction::Close(tab.id));
    }
    label.context_menu(|ui| {
        if ui.button(hp_app::Action::CloseMap.label()).clicked() {
            action = Some(TabAction::Close(tab.id));
        }
        if ui.add_enabled(count > 1, egui::Button::new(hp_app::Action::CloseOtherMaps.label())).clicked() {
            action = Some(TabAction::CloseOthers(tab.id));
        }
        ui.separator();
        let saved = tab.path.is_some();
        if ui.add_enabled(saved, egui::Button::new(t!("tabs.copy_path"))).clicked() {
            action = Some(TabAction::CopyPath(tab.id));
        }
        if ui.add_enabled(saved, egui::Button::new(t!("tabs.show_in_folder"))).clicked() {
            action = Some(TabAction::ShowInFolder(tab.id));
        }
    });
    let close = ui
        .add(egui::Button::new(egui::RichText::new("✖").small()).frame(false))
        .on_hover_text(hp_app::Action::CloseMap.label());
    if close.clicked() {
        action = Some(TabAction::Close(tab.id));
    }
    action
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PublishedTabs {
    version: u64,
    dirty: bool,
    sidecar: u64,
    lang: hp_app::i18n::Lang,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenTarget {
    Replace(TabId),
    NewTab,
}

pub(crate) fn is_pristine(doc: &Document) -> bool {
    doc.path().is_none() && doc.opened_from().is_none() && !doc.is_dirty() && doc.history_len() == (0, 0)
}

impl HammerProApp {
    fn fresh_sidecar(&self) -> Sidecar {
        Sidecar::open(self.state.sidecar.dir(), None)
    }

    pub(crate) fn switch_to_tab(&mut self, id: TabId) {
        if id == self.tabs.active_id() || !self.tabs.contains(id) {
            return;
        }
        self.deactivate_tool();
        files::remember_view(&mut self.state);
        self.tabs.activate(&mut self.state, id);
        files::apply_doc_options(&mut self.state);
        self.activate_tool();
    }

    pub(crate) fn cycle_tabs(&mut self, step: isize) {
        if let Some(id) = self.tabs.neighbor(step) {
            self.switch_to_tab(id);
        }
    }

    pub(crate) fn new_tab(&mut self) {
        self.deactivate_tool();
        files::remember_view(&mut self.state);
        let sidecar = self.fresh_sidecar();
        self.tabs.open_new(&mut self.state, sidecar);
        files::apply_doc_options(&mut self.state);
        files::frame_map(&mut self.state, self.central_size);
        self.activate_tool();
    }

    pub(crate) fn new_map(&mut self, ctx: &egui::Context) {
        if self.state.settings.workflow.open_in_new_tab && !is_pristine(&self.state.doc) {
            if self.state.doc.is_dirty() {
                self.pending = Some(PendingOp::NewTab);
            } else {
                self.new_tab();
            }
        } else {
            self.guard_unsaved(ctx, PendingOp::New);
        }
    }

    pub(crate) fn request_open(&mut self, ctx: &egui::Context, path: std::path::PathBuf) {
        if let Some(id) = self.tabs.find_map(&self.state, &path) {
            self.switch_to_tab(id);
            return;
        }
        if self.state.settings.workflow.open_in_new_tab && !is_pristine(&self.state.doc) {
            if self.state.doc.is_dirty() {
                self.pending = Some(PendingOp::OpenTab(path));
            } else {
                self.start_open(ctx, &path, files::OpenKind::Map, OpenTarget::NewTab);
            }
        } else {
            self.guard_unsaved(ctx, PendingOp::Open(path));
        }
    }

    pub(crate) fn recover_target(&self, original: Option<&Path>) -> OpenTarget {
        if let Some(id) = original.and_then(|p| self.tabs.find_map(&self.state, p)) {
            return OpenTarget::Replace(id);
        }
        if self.state.settings.workflow.open_in_new_tab && !is_pristine(&self.state.doc) {
            OpenTarget::NewTab
        } else {
            OpenTarget::Replace(self.tabs.active_id())
        }
    }

    pub(crate) fn enter_open_target(&mut self, target: OpenTarget) {
        match target {
            OpenTarget::Replace(id) if self.tabs.contains(id) => self.switch_to_tab(id),
            OpenTarget::Replace(_) | OpenTarget::NewTab => self.new_tab(),
        }
    }

    pub(crate) fn close_active_tab(&mut self) {
        self.deactivate_tool();
        files::flush_sidecar(&mut self.state);
        match self.tabs.close_active(&mut self.state) {
            Some(closed) => {
                files::drop_in_background(closed.doc);
                files::apply_doc_options(&mut self.state);
            }
            None => {
                if self.open_target == Some(OpenTarget::Replace(self.tabs.active_id())) {
                    self.opener.cancel();
                    self.open_target = None;
                }
                files::new_map(&mut self.state, self.central_size);
                self.tabs.touch();
            }
        }
        self.activate_tool();
    }

    pub(crate) fn close_tab(&mut self, id: TabId) {
        self.close_queue.clear();
        self.close_queue.push_back(id);
        self.close_keep = None;
        self.process_close_queue();
    }

    pub(crate) fn close_other_tabs(&mut self, keep: TabId) {
        self.close_queue = self.tabs.ids().into_iter().filter(|id| *id != keep).collect();
        self.close_keep = Some(keep);
        self.process_close_queue();
    }

    pub(crate) fn process_close_queue(&mut self) {
        while self.pending.is_none() {
            let Some(id) = self.close_queue.pop_front() else {
                if let Some(keep) = self.close_keep.take() {
                    self.switch_to_tab(keep);
                }
                return;
            };
            if !self.tabs.contains(id) {
                continue;
            }
            self.switch_to_tab(id);
            if self.state.doc.is_dirty() {
                self.pending = Some(PendingOp::Close);
                return;
            }
            self.close_active_tab();
        }
    }

    pub(crate) fn finish_close(&mut self) {
        self.close_active_tab();
        self.process_close_queue();
    }

    pub(crate) fn cancel_closing(&mut self) {
        self.particle_exit = crate::particle_exit::Controller::default();
        self.update.disarm();
        self.close_queue.clear();
        self.close_keep = None;
        self.exit_answered.clear();
    }

    pub(crate) fn continue_exit(&mut self, ctx: &egui::Context) {
        let answered = self
            .exit_answered
            .iter()
            .filter_map(|(&id, &revision)| {
                self.tabs.doc(&self.state, id).filter(|doc| doc.revision() == revision).map(|_| id)
            })
            .collect();
        match self.tabs.first_dirty(&self.state, &answered) {
            Some(id) => {
                self.switch_to_tab(id);
                self.pending = Some(PendingOp::Exit);
            }
            None => {
                if !crate::particle_exit::gate(self, ctx) {
                    return;
                }
                self.exit_answered.clear();
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    pub(crate) fn any_map_dirty(&self) -> bool {
        self.tabs.first_dirty(&self.state, &HashSet::new()).is_some()
    }

    pub(crate) fn tab_action(&mut self, ctx: &egui::Context, action: TabAction) {
        match action {
            TabAction::Switch(id) => self.switch_to_tab(id),
            TabAction::Close(id) => self.close_tab(id),
            TabAction::CloseOthers(id) => self.close_other_tabs(id),
            TabAction::New => self.new_tab(),
            TabAction::CopyPath(id) => {
                if let Some(path) = self.tabs.doc(&self.state, id).and_then(Document::path) {
                    ctx.copy_text(path.display().to_string());
                    self.state.notify(Severity::Info, tf!("tabs.path_copied", path.display()));
                }
            }
            TabAction::ShowInFolder(id) => {
                if let Some(dir) = self.tabs.doc(&self.state, id).and_then(Document::path).and_then(Path::parent) {
                    crate::app::open_folder(dir);
                }
            }
        }
    }

    pub(crate) fn tabs_tick(&mut self, ctx: &egui::Context) {
        if let Some(request) = self.state.runtime.workflow.tab_request.take()
            && self.pending.is_none()
        {
            let action = match request {
                TabRequest::Switch(id) => TabAction::Switch(id),
                TabRequest::Close(id) => TabAction::Close(id),
                TabRequest::CloseOthers(id) => TabAction::CloseOthers(id),
            };
            self.tab_action(ctx, action);
        }
        self.publish_tabs();
    }

    pub(crate) fn publish_tabs(&mut self) {
        let stamp = PublishedTabs {
            version: self.tabs.version(),
            dirty: self.state.doc.is_dirty(),
            sidecar: self.state.sidecar.serial(),
            lang: hp_app::i18n::lang(),
        };
        if let Some((published, path)) = &self.published_tabs
            && *published == stamp
            && path.as_deref() == self.state.doc.path()
        {
            return;
        }
        self.published_tabs = Some((stamp, self.state.doc.path().map(Path::to_path_buf)));
        self.state.runtime.workflow.tabs = self.tabs.infos(&self.state);
        self.state.runtime.workflow.active_tab = self.tabs.active_id();
    }

    pub(crate) fn map_tabs_ui(&mut self, ui: &mut egui::Ui) {
        let active = self.tabs.active_id();
        if let Some(action) = tab_bar(ui, &self.state.runtime.workflow.tabs, active) {
            self.tab_bar_action = Some(action);
        }
    }
}

pub(crate) const TR: hp_app::i18n::TrTable = &[
    ("tabs.not_saved", "Not saved yet", "Pas encore enregistrée"),
    ("tabs.copy_path", "Copy the path", "Copier le chemin"),
    ("tabs.show_in_folder", "Show in the folder", "Afficher dans le dossier"),
    ("tabs.path_copied", "Copied: {0}", "Copié : {0}"),
    (
        "tabs.close_question",
        "Save the changes to {0} before closing it?",
        "Enregistrer les modifications de {0} avant de la fermer ?",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use hp_editor::{ObjRef, SelectOp};

    fn state() -> AppState {
        hp_app::register_translations();
        hp_app::i18n::register(crate::app::TR);
        hp_app::i18n::register(TR);
        hp_app::testing::test_state()
    }

    #[test]
    fn collaboration_background_updates_only_the_chosen_parked_map() {
        let mut state = state();
        let mut tabs = Tabs::default();
        let shared = tabs.active_id();
        state.doc.set_path(Some("shared.vmf".into()));
        let local = tabs.open_new(&mut state, Sidecar::in_memory());
        state.doc.set_path(Some("local.vmf".into()));
        state.doc.set_world_key("message", "local only").unwrap();
        let local_vmf = state.doc.to_vmf_string();
        let camera = state.cameras.cam3d.pos;
        assert!(tabs.with_document(&mut state, shared, |state| {
            state.doc.set_world_key("message", "remote edit").unwrap();
            state.cameras.cam3d.pos = DVec3::splat(123.0);
        }));
        assert_eq!(tabs.active_id(), local);
        assert_eq!(state.doc.to_vmf_string(), local_vmf);
        assert_eq!(state.cameras.cam3d.pos, camera);
        assert_eq!(state.doc.path().unwrap().to_str(), Some("local.vmf"));
        assert!(tabs.doc(&state, shared).unwrap().to_vmf_string().contains("remote edit"));
        assert!(!tabs.with_document(&mut state, 9999, |_| panic!("unknown tab callback")));
        tabs.activate(&mut state, shared);
        assert_eq!(state.doc.path().unwrap().to_str(), Some("shared.vmf"));
        assert_eq!(state.cameras.cam3d.pos, DVec3::splat(123.0));
    }

    #[test]
    fn switching_swaps_the_maps_and_what_belongs_to_them() {
        let mut state = state();
        let mut tabs = Tabs::default();
        let first = tabs.active_id();
        let solids = state.doc.scene().solids().len();
        state.doc.select(&[ObjRef::Solid(2)], SelectOp::Replace);
        state.cameras.cam3d.pos = DVec3::new(1.0, 2.0, 3.0);
        state.pointfile = Some(Pointfile { points: vec![DVec3::ZERO, DVec3::X], ..Pointfile::default() });
        state.pivot = Some(DVec3::splat(8.0));
        state.sidecar.set(&hp_app::runtime::WorkflowMapMemory { grid: Some(4.0) });
        let first_revision = state.doc.revision();
        let _ = state.doc.take_changes();

        let second = tabs.open_new(&mut state, Sidecar::in_memory());
        assert_ne!(second, first);
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs.active_id(), second);
        assert!(state.doc.scene().solids().is_empty(), "a new empty map");
        assert!(state.pointfile.is_none() && state.pivot.is_none());
        assert_eq!(state.cameras.cam3d.pos, Cameras::default().cam3d.pos);
        assert_eq!(state.sidecar.get::<hp_app::runtime::WorkflowMapMemory>().grid, None);
        assert_ne!(state.doc.revision(), first_revision, "revisions never repeat between maps");
        assert!(state.doc.take_changes().everything, "the views draw the new map");
        state.pick = Some(hp_app::state::PickRequest {
            kind: hp_app::state::PickKind::Entity,
            requester: "x".into(),
            result: None,
        });

        assert!(tabs.activate(&mut state, first));
        assert!(!tabs.activate(&mut state, first), "already shown");
        assert!(!tabs.activate(&mut state, 999), "not open");
        assert_eq!(state.doc.scene().solids().len(), solids);
        assert_eq!(state.doc.selected_objects(), vec![ObjRef::Solid(2)]);
        assert_eq!(state.cameras.cam3d.pos, DVec3::new(1.0, 2.0, 3.0));
        assert_eq!(state.pointfile.as_ref().map(|p| p.points.len()), Some(2));
        assert_eq!(state.pivot, Some(DVec3::splat(8.0)));
        assert_eq!(state.sidecar.get::<hp_app::runtime::WorkflowMapMemory>().grid, Some(4.0));
        assert!(state.pick.is_none(), "a pick request of the other map is dropped");
        assert!(state.doc.take_changes().everything);
        assert_eq!(tabs.parked().count(), 1);
        assert!(tabs.doc(&state, second).is_some_and(|d| d.scene().solids().is_empty()));
        assert!(tabs.map_view(&state, first).is_some_and(|(d, _)| d.scene().solids().len() == solids));
    }

    #[test]
    fn closing_shows_the_next_tab_and_keeps_the_last_one() {
        let mut state = state();
        let mut tabs = Tabs::default();
        let a = tabs.active_id();
        let b = tabs.open_new(&mut state, Sidecar::in_memory());
        let c = tabs.open_new(&mut state, Sidecar::in_memory());
        assert_eq!(tabs.ids(), vec![a, b, c], "new tabs open after the active one");
        assert_eq!(tabs.neighbor(1), Some(a), "wraps around");
        assert_eq!(tabs.neighbor(-1), Some(b));
        tabs.activate(&mut state, b);
        let closed = tabs.close_active(&mut state).expect("closed");
        assert!(closed.doc.scene().solids().is_empty());
        assert_eq!(tabs.ids(), vec![a, c]);
        assert_eq!(tabs.active_id(), c);
        tabs.close_active(&mut state).expect("closed");
        assert_eq!(tabs.ids(), vec![a]);
        assert_eq!(tabs.active_id(), a);
        assert!(!state.doc.scene().solids().is_empty(), "the test map is shown again");
        assert!(tabs.close_active(&mut state).is_none(), "the only tab stays");
        assert_eq!(tabs.neighbor(1), None);
    }

    #[test]
    fn maps_are_found_by_path_and_by_unsaved_changes() {
        let mut state = state();
        let mut tabs = Tabs::default();
        let dir = std::env::temp_dir().join(format!("hammerpro_tabs_{}", std::process::id()));
        let a_path = dir.join("Room.vmf");
        state.doc.set_path(Some(a_path.clone()));
        let a = tabs.active_id();
        let b = tabs.open_new(&mut state, Sidecar::in_memory());
        assert_eq!(tabs.find_map(&state, &a_path), Some(a));
        assert_eq!(tabs.find_map(&state, &dir.join("other.vmf")), None);
        if cfg!(windows) {
            assert_eq!(tabs.find_map(&state, &dir.join("ROOM.VMF")), Some(a), "Windows paths ignore case");
        }
        assert_eq!(tabs.first_dirty(&state, &HashSet::new()), None);
        state.doc.mark_dirty();
        assert_eq!(tabs.first_dirty(&state, &HashSet::new()), Some(b));
        assert_eq!(tabs.first_dirty(&state, &HashSet::from([b])), None);
        let infos = tabs.infos(&state);
        assert_eq!(infos.iter().map(|i| i.id).collect::<Vec<_>>(), vec![a, b]);
        assert_eq!(infos[0].title, "Room.vmf");
        assert_eq!(infos[0].path.as_deref(), Some(a_path.as_path()));
        assert!(!infos[0].dirty && infos[1].dirty);
        assert_eq!(infos[1].title, t!("app.untitled"));
        assert!(infos[1].path.is_none());
    }

    #[test]
    fn only_the_start_map_is_pristine() {
        let mut doc = Document::new_empty();
        assert!(is_pristine(&doc));
        doc.select_all();
        assert!(is_pristine(&doc), "selecting is not editing");
        let mut edited = Document::new_empty();
        edited.create_point_entity("info_target", DVec3::ZERO, &[]).expect("entity");
        assert!(!is_pristine(&edited));
        edited.undo();
        assert!(!is_pristine(&edited), "an undone edit can be redone");
        let mut saved = Document::new_empty();
        saved.set_path(Some(std::path::PathBuf::from("C:/maps/a.vmf")));
        assert!(!is_pristine(&saved));
    }

    fn nodes(out: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
        let mut nodes = Vec::new();
        if let Some(update) = &out.platform_output.accesskit_update {
            for (_, node) in &update.nodes {
                let text = [node.label(), node.value()].into_iter().flatten().collect::<Vec<_>>().join(" ");
                if let Some(b) = node.bounds() {
                    let min = egui::pos2(b.x0 as f32, b.y0 as f32);
                    let max = egui::pos2(b.x1 as f32, b.y1 as f32);
                    nodes.push((text, egui::Rect::from_min_max(min, max)));
                }
            }
        }
        nodes
    }

    #[test]
    fn the_tab_bar_answers_clicks() {
        let _lang = hp_app::testing::lang_lock();
        let _ = state();
        let infos = vec![
            MapTabInfo { id: 1, title: "first.vmf".into(), path: None, dirty: false },
            MapTabInfo { id: 7, title: "second.vmf".into(), path: Some("C:/maps/second.vmf".into()), dirty: true },
        ];
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut time = 0.0;
        let mut run = |events: Vec<egui::Event>| {
            time += 0.5;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 60.0))),
                time: Some(time),
                events,
                ..Default::default()
            };
            let mut found = None;
            let mut out = ctx.run_ui(input, |ui| found = tab_bar(ui, &infos, 1));
            out.textures_delta.clear();
            (found, nodes(&out))
        };
        let (_, nodes) = run(Vec::new());
        let second = nodes.iter().find(|(t, _)| t.contains("second.vmf*")).map(|(_, r)| *r);
        let second = second.unwrap_or_else(|| panic!("{nodes:?}"));
        let plus = nodes.iter().find(|(t, _)| t == "+").map(|(_, r)| *r);
        let plus = plus.unwrap_or_else(|| panic!("{nodes:?}"));
        let mut results = Vec::new();
        for (pos, button) in [
            (second.center(), egui::PointerButton::Primary),
            (second.center(), egui::PointerButton::Middle),
            (plus.center(), egui::PointerButton::Primary),
        ] {
            let mut seen = None;
            for events in [
                vec![egui::Event::PointerMoved(pos)],
                vec![egui::Event::PointerButton { pos, button, pressed: true, modifiers: egui::Modifiers::NONE }],
                vec![egui::Event::PointerButton { pos, button, pressed: false, modifiers: egui::Modifiers::NONE }],
            ] {
                if let (Some(a), _) = run(events) {
                    seen = Some(a);
                }
            }
            results.push(seen);
        }
        assert_eq!(results, vec![Some(TabAction::Switch(7)), Some(TabAction::Close(7)), Some(TabAction::New)]);
    }
}
