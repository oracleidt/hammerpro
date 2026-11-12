use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, TryRecvError};

use egui::RichText;
use hp_app::runtime::HealthRuntime;
use hp_app::settings::{DetailSplitSetting, FuncDetailSettings};
use hp_app::{Action, AppState, Severity, t, tf};
use hp_editor::limits::leak::LeakInput;
use hp_editor::optimize::detail::{DetailJob, DetailKind, DetailSuggestion};
use hp_editor::optimize::sealing::{self, CandidateFilter, SealCheck};
use hp_editor::{DETAIL_CLASS, DetailPlan, DetailSplit, Id, ObjRef, Owner};

use crate::app::HammerProApp;

#[derive(Clone, Debug)]
pub(crate) struct Checked {
    pub(crate) plan: DetailPlan,
    pub(crate) split: DetailSplit,
    pub(crate) check: Option<SealCheck>,
    pub(crate) revision: u64,
}

impl Checked {
    pub(crate) fn sealing(&self) -> &[Id] {
        match &self.check {
            Some(c) if c.verified => &c.sealing,
            _ => &[],
        }
    }

    pub(crate) fn unverified(&self) -> Option<&SealCheck> {
        self.check.as_ref().filter(|c| !c.verified)
    }
}

pub(crate) struct Pending {
    plan: DetailPlan,
    split: DetailSplit,
    input: LeakInput,
    revision: u64,
}

impl Pending {
    pub(crate) fn run(self, cancel: &AtomicBool) -> Checked {
        let check = sealing::sealing_brushes(&self.input, &self.plan.world, None, cancel)
            .unwrap_or(SealCheck { incomplete: true, ..SealCheck::default() });
        Checked { plan: self.plan, split: self.split, check: Some(check), revision: self.revision }
    }
}

pub(crate) enum Begin {
    Nothing(DetailPlan),
    Ready(Checked),
    Check(Pending),
}

pub(crate) fn split_of(s: DetailSplitSetting) -> DetailSplit {
    match s {
        DetailSplitSetting::Selection => DetailSplit::Selection,
        DetailSplitSetting::Cluster => DetailSplit::Cluster,
    }
}

pub(crate) fn begin(state: &mut AppState, objs: &[ObjRef], split: DetailSplit, check: bool) -> Begin {
    let plan = state.doc.detail_plan(objs);
    let revision = state.doc.scene_revision();
    if plan.is_empty() {
        return Begin::Nothing(plan);
    }
    if !check {
        return Begin::Ready(Checked { plan, split, check: None, revision });
    }
    let safe = SealCheck { verified: true, ..SealCheck::default() };
    let input = HealthRuntime::seal_input(state);
    let could = sealing::could_seal(&input, &plan.world);
    if could.is_empty() || HealthRuntime::current_leak_report(state).is_some_and(|r| sealing::quick_safe(&could, r)) {
        return Begin::Ready(Checked { plan, split, check: Some(safe), revision });
    }
    Begin::Check(Pending { plan, split, input, revision })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Policy {
    Skip,
    Refuse,
    Allow,
}

impl Policy {
    pub(crate) fn parse(s: &str) -> Option<Policy> {
        match s {
            "skip" => Some(Policy::Skip),
            "refuse" => Some(Policy::Refuse),
            "allow" => Some(Policy::Allow),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MakeError {
    Changed,
    AllSealing(Vec<Id>),
    Refused(Vec<Id>),
    Busy,
    Edit(String),
}

impl MakeError {
    pub(crate) fn english(&self) -> String {
        match self {
            MakeError::Changed => "the map changed during the sealing check: run it again".to_owned(),
            MakeError::AllSealing(ids) => format!(
                "every brush ({}) seals the map (it is part of the outer shell between the playable space and the void): a func_detail does not seal, so the map would leak. Use sealing: \"allow\" to convert anyway",
                ids.len()
            ),
            MakeError::Refused(ids) => {
                format!("{} brush(es) seal the map ({ids:?}): nothing converted (sealing: \"refuse\")", ids.len())
            }
            MakeError::Busy => "Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned(),
            MakeError::Edit(e) => e.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Made {
    pub(crate) entities: Vec<Id>,
    pub(crate) converted: Vec<Id>,
    pub(crate) refused: Vec<Id>,
}

fn detail_defaults(state: &AppState) -> Vec<(String, String)> {
    state.assets.fgd().and_then(|f| f.class(DETAIL_CLASS)).map(|c| c.default_keyvalues()).unwrap_or_default()
}

pub(crate) fn apply(state: &mut AppState, c: &Checked, policy: Policy, label: Option<&str>) -> Result<Made, MakeError> {
    if state.doc.scene_revision() != c.revision {
        return Err(MakeError::Changed);
    }
    if state.doc.in_transaction() {
        return Err(MakeError::Busy);
    }
    let sealing = c.sealing().to_vec();
    let (convert, refused): (Vec<Id>, Vec<Id>) = match policy {
        Policy::Allow => (c.plan.world.clone(), Vec::new()),
        Policy::Refuse if !sealing.is_empty() => return Err(MakeError::Refused(sealing)),
        Policy::Refuse => (c.plan.world.clone(), Vec::new()),
        Policy::Skip => c.plan.world.iter().copied().partition(|id| sealing.binary_search(id).is_err()),
    };
    if convert.is_empty() {
        return Err(MakeError::AllSealing(refused));
    }
    let kv = detail_defaults(state);
    let targets = c.plan.targets.clone();
    let split = c.split;
    let run = |doc: &mut hp_editor::Document| doc.make_func_detail(&convert, &targets, split, &kv);
    let result = match label {
        Some(l) => state.doc.transaction(l, run),
        None => run(&mut state.doc),
    };
    let entities = result.map_err(|e| MakeError::Edit(e.to_string()))?;
    state.runtime.health.recheck = true;
    Ok(Made { entities, converted: convert, refused })
}

pub(crate) fn filter_from(s: &FuncDetailSettings, within: Option<Vec<Id>>) -> CandidateFilter {
    let kinds = [
        (DetailKind::Step, s.steps),
        (DetailKind::Trim, s.trims),
        (DetailKind::Pillar, s.pillars),
        (DetailKind::Small, s.small),
        (DetailKind::Complex, s.complex),
    ]
    .into_iter()
    .filter(|(_, on)| *on)
    .map(|(k, _)| k)
    .collect();
    CandidateFilter { max_size: s.max_size, kinds, within }
}

pub(crate) fn selection_world_solids(state: &AppState) -> Vec<Id> {
    let objs = state.doc.selected_objects();
    let scene = state.doc.scene();
    scene
        .expand_solids(&objs)
        .into_iter()
        .filter(|id| scene.solid(*id).is_some_and(|n| n.owner == Owner::World))
        .collect()
}

pub(crate) fn candidate_job(state: &mut AppState, filter: &CandidateFilter) -> (DetailJob, u64) {
    let mut job = sealing::candidate_job(&state.doc, filter);
    job.input = HealthRuntime::seal_input(state);
    (job, state.doc.scene_revision())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Picked {
    pub(crate) ids: Vec<Id>,
    pub(crate) kept_for_sealing: usize,
    pub(crate) verified: bool,
    pub(crate) map_leaks: bool,
    pub(crate) incomplete: bool,
}

pub(crate) fn pick(state: &AppState, s: &DetailSuggestion) -> Picked {
    let scene = state.doc.scene();
    let ids = s.ids().into_iter().filter(|id| scene.solid(*id).is_some_and(|n| n.owner == Owner::World)).collect();
    Picked {
        ids,
        kept_for_sealing: s.kept_for_sealing.len(),
        verified: s.verified,
        map_leaks: s.map_leaks,
        incomplete: s.incomplete,
    }
}

pub(crate) fn select_picked(state: &mut AppState, p: &Picked) {
    if p.ids.is_empty() {
        return;
    }
    let objs: Vec<ObjRef> = p.ids.iter().map(|id| ObjRef::Solid(*id)).collect();
    state.doc.set_selection(&objs);
}

pub(crate) fn nothing_text(plan: &DetailPlan) -> String {
    if !plan.already.is_empty() && plan.other_entity.is_empty() && plan.displacements.is_empty() {
        return t!("brush.already_detail").to_owned();
    }
    let mut parts: Vec<String> = vec![t!("brush.detail.nothing").to_owned()];
    parts.extend(skipped_parts(plan));
    parts.join(" ")
}

fn skipped_parts(plan: &DetailPlan) -> Vec<String> {
    let mut parts = Vec::new();
    if !plan.already.is_empty() {
        parts.push(tf!("brush.detail.skipped_already", plan.already.len()));
    }
    if !plan.other_entity.is_empty() {
        parts.push(tf!("brush.detail.skipped_entity", plan.other_entity.len()));
    }
    if !plan.displacements.is_empty() {
        parts.push(tf!("brush.detail.skipped_disp", plan.displacements.len()));
    }
    parts
}

struct CheckJob {
    rx: Receiver<Checked>,
}

struct CandidateJob {
    rx: Receiver<(DetailSuggestion, u64)>,
}

#[derive(Default)]
pub(crate) struct DetailState {
    check: Option<CheckJob>,
    prompt: Option<Checked>,
    candidates_open: bool,
    candidates: Option<CandidateJob>,
    picked: Option<Picked>,
}

impl HammerProApp {
    pub(crate) fn make_func_detail(&mut self) {
        if !self.require_selection() {
            return;
        }
        if self.pkg.brush.detail.check.is_some() {
            return;
        }
        self.pkg.brush.detail.prompt = None;
        let objs = self.selected();
        let s = &self.state.settings.brush.detail;
        let (split, check) = (split_of(s.split), s.check_sealing);
        match begin(&mut self.state, &objs, split, check) {
            Begin::Nothing(plan) => self.state.notify(Severity::Info, nothing_text(&plan)),
            Begin::Ready(c) => self.finish_detail(c),
            Begin::Check(p) => {
                let (tx, rx) = std::sync::mpsc::channel();
                let spawned = std::thread::Builder::new().name("hp-detail-seal".to_owned()).spawn(move || {
                    let cancel = AtomicBool::new(false);
                    let _ = tx.send(hp_editor::background::run(|| p.run(&cancel)));
                });
                match spawned {
                    Ok(_) => {
                        self.pkg.brush.detail.check = Some(CheckJob { rx });
                        self.state.log(Severity::Info, t!("brush.detail.checking"));
                    }
                    Err(e) => self.state.notify(Severity::Error, tf!("brush.detail.failed", e)),
                }
            }
        }
    }

    fn finish_detail(&mut self, c: Checked) {
        if !c.sealing().is_empty() {
            self.pkg.brush.detail.prompt = Some(c);
            return;
        }
        self.apply_detail(&c, Policy::Skip);
    }

    fn apply_detail(&mut self, c: &Checked, policy: Policy) {
        match apply(&mut self.state, c, policy, None) {
            Ok(made) => {
                let mut parts = vec![tf!("brush.detail.made", made.converted.len(), made.entities.len())];
                if !made.refused.is_empty() {
                    parts.push(tf!("brush.detail.kept_sealing", made.refused.len()));
                }
                parts.extend(skipped_parts(&c.plan));
                let warn = match c.unverified() {
                    Some(u) if u.map_leaks => Some(t!("brush.detail.unverified_leak")),
                    Some(_) => Some(t!("brush.detail.unverified_complex")),
                    None => None,
                };
                let severity = if warn.is_some() { Severity::Warning } else { Severity::Success };
                if let Some(w) = warn {
                    parts.push(w.to_owned());
                }
                self.state.notify(severity, parts.join(" "));
            }
            Err(MakeError::Changed) => self.state.notify(Severity::Warning, t!("brush.detail.changed")),
            Err(MakeError::Busy) => self.state.notify(Severity::Warning, t!("brush.detail.busy")),
            Err(MakeError::AllSealing(ids) | MakeError::Refused(ids)) => {
                self.state.notify(Severity::Warning, tf!("brush.detail.all_sealing", ids.len()));
            }
            Err(MakeError::Edit(e)) => {
                self.state.notify(Severity::Warning, tf!("app.op_failed", Action::ConvertToDetail.label(), e));
            }
        }
    }

    pub(crate) fn poll_func_detail(&mut self, ctx: &egui::Context) {
        if let Some(job) = self.pkg.brush.detail.check.take() {
            match job.rx.try_recv() {
                Ok(c) => self.finish_detail(c),
                Err(TryRecvError::Empty) => {
                    self.pkg.brush.detail.check = Some(job);
                    ctx.request_repaint_after(std::time::Duration::from_millis(30));
                }
                Err(TryRecvError::Disconnected) => {
                    self.state.notify(Severity::Error, tf!("brush.detail.failed", "worker stopped"));
                }
            }
        }
        if let Some(job) = self.pkg.brush.detail.candidates.take() {
            match job.rx.try_recv() {
                Ok((s, revision)) => {
                    if revision != self.state.doc.scene_revision() {
                        self.state.notify(Severity::Warning, t!("brush.detail.changed"));
                        return;
                    }
                    let picked = pick(&self.state, &s);
                    select_picked(&mut self.state, &picked);
                    let severity = if picked.verified { Severity::Info } else { Severity::Warning };
                    self.state.notify(severity, picked_text(&picked));
                    self.pkg.brush.detail.picked = Some(picked);
                }
                Err(TryRecvError::Empty) => {
                    self.pkg.brush.detail.candidates = Some(job);
                    ctx.request_repaint_after(std::time::Duration::from_millis(30));
                }
                Err(TryRecvError::Disconnected) => {
                    self.state.notify(Severity::Error, tf!("brush.detail.failed", "worker stopped"));
                }
            }
        }
    }

    pub(crate) fn open_detail_candidates(&mut self) {
        self.pkg.brush.detail.candidates_open = true;
        self.pkg.brush.detail.picked = None;
    }

    fn start_detail_candidates(&mut self) {
        if self.pkg.brush.detail.candidates.is_some() {
            return;
        }
        let s = self.state.settings.brush.detail.clone();
        let within = (s.in_selection && !self.state.doc.selected_objects().is_empty())
            .then(|| selection_world_solids(&self.state));
        let filter = filter_from(&s, within);
        let (job, revision) = candidate_job(&mut self.state, &filter);
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("hp-detail-candidates".to_owned()).spawn(move || {
            let cancel = AtomicBool::new(false);
            let _ = tx.send((hp_editor::background::run(|| job.run(None, &cancel)), revision));
        });
        match spawned {
            Ok(_) => {
                self.pkg.brush.detail.picked = None;
                self.pkg.brush.detail.candidates = Some(CandidateJob { rx });
            }
            Err(e) => self.state.notify(Severity::Error, tf!("brush.detail.failed", e)),
        }
    }

    pub(crate) fn func_detail_dialogs(&mut self, ctx: &egui::Context) {
        self.detail_seal_dialog(ctx);
        self.detail_candidates_window(ctx);
    }

    fn detail_seal_dialog(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.pkg.brush.detail.prompt else { return };
        let total = c.plan.world.len();
        let sealing = c.sealing().len();
        let others = total.saturating_sub(sealing);
        let mut open = true;
        let mut choice: Option<Policy> = None;
        let mut select = false;
        let mut cancel = false;
        let response = hp_app::chrome::Window::new(t!("brush.detail.seal_title"))
            .id(egui::Id::unique("hp_detail_seal"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.label(RichText::new(tf!("brush.detail.seal_count", sealing, total)).strong());
                ui.label(t!("brush.detail.seal_why"));
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if others > 0 && ui.button(tf!("brush.detail.seal_others", others)).clicked() {
                        choice = Some(Policy::Skip);
                    }
                    if ui.button(t!("brush.detail.seal_all")).on_hover_text(t!("brush.detail.seal_all_tip")).clicked() {
                        choice = Some(Policy::Allow);
                    }
                    select = ui.button(t!("brush.detail.seal_select")).clicked();
                    cancel = ui.button(t!("common.cancel")).clicked();
                });
            });
        if response.as_ref().is_some_and(|r| {
            hp_ui_shell::common::is_top_dialog(ctx, r.response.layer_id) && hp_ui_shell::common::escape_pressed(ctx)
        }) {
            cancel = true;
        }
        if select {
            let objs: Vec<ObjRef> = c.sealing().iter().map(|id| ObjRef::Solid(*id)).collect();
            self.state.doc.set_selection(&objs);
            self.state.request(Action::Center2dOnSelection);
            self.state.request(Action::Center3dOnSelection);
            self.pkg.brush.detail.prompt = None;
        } else if let Some(policy) = choice {
            if let Some(c) = self.pkg.brush.detail.prompt.take() {
                self.apply_detail(&c, policy);
            }
        } else if cancel || !open {
            self.pkg.brush.detail.prompt = None;
        }
    }

    fn detail_candidates_window(&mut self, ctx: &egui::Context) {
        if !self.pkg.brush.detail.candidates_open {
            return;
        }
        let mut open = true;
        let mut start = false;
        let mut make = false;
        let running = self.pkg.brush.detail.candidates.is_some();
        let has_selection = !self.state.doc.selected_objects().is_empty();
        let picked = self.pkg.brush.detail.picked.clone();
        let mut changed = false;
        let s = &mut self.state.settings.brush.detail;
        let response = hp_app::chrome::Window::new(t!("brush.detail.cand_title"))
            .id(egui::Id::unique("hp_detail_candidates"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(ctx.content_rect().center() - egui::vec2(180.0, 150.0))
            .show(ctx, |ui| {
                ui.set_max_width(360.0);
                ui.label(RichText::new(t!("brush.detail.cand_intro")).weak().small());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(t!("brush.detail.cand_max_size"));
                    changed |= ui
                        .add(egui::DragValue::new(&mut s.max_size).range(16.0..=16384.0).speed(8.0).suffix(" u"))
                        .on_hover_text(t!("brush.detail.cand_max_size_tip"))
                        .changed();
                });
                for (value, key) in [
                    (&mut s.steps, "brush.detail.cand_steps"),
                    (&mut s.trims, "brush.detail.cand_trims"),
                    (&mut s.pillars, "brush.detail.cand_pillars"),
                    (&mut s.small, "brush.detail.cand_small"),
                    (&mut s.complex, "brush.detail.cand_complex"),
                ] {
                    changed |= ui.checkbox(value, t!(key)).changed();
                }
                changed |= ui
                    .add_enabled(
                        has_selection,
                        egui::Checkbox::new(&mut s.in_selection, t!("brush.detail.cand_in_selection")),
                    )
                    .changed();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if running {
                        ui.spinner();
                        ui.label(t!("brush.detail.cand_running"));
                    } else {
                        start = ui.button(t!("brush.detail.cand_select")).clicked();
                    }
                    let n = picked.as_ref().map_or(0, |p| p.ids.len());
                    if n > 0 {
                        make = ui
                            .button(Action::ConvertToDetail.label())
                            .on_hover_text(t!("brush.detail.cand_make_tip"))
                            .clicked();
                    }
                });
                if let Some(p) = &picked {
                    ui.label(RichText::new(picked_text(p)).small());
                }
            });
        if changed {
            self.state.settings_dirty = true;
        }
        if response.as_ref().is_some_and(|r| {
            hp_ui_shell::common::is_top_dialog(ctx, r.response.layer_id) && hp_ui_shell::common::escape_pressed(ctx)
        }) {
            open = false;
        }
        if start {
            self.start_detail_candidates();
        }
        if make {
            self.state.request(Action::ConvertToDetail);
        }
        if !open {
            self.pkg.brush.detail.candidates_open = false;
        }
    }
}

pub(crate) fn picked_text(p: &Picked) -> String {
    let mut parts = vec![if p.ids.is_empty() {
        t!("brush.detail.cand_none").to_owned()
    } else {
        tf!("brush.detail.cand_selected", p.ids.len())
    }];
    if p.kept_for_sealing > 0 {
        parts.push(tf!("brush.detail.cand_kept", p.kept_for_sealing));
    }
    if !p.verified {
        parts.push(
            if p.map_leaks { t!("brush.detail.unverified_leak") } else { t!("brush.detail.unverified_complex") }
                .to_owned(),
        );
    }
    parts.join(" ")
}
