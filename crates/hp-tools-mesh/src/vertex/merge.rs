use glam::DVec3;
use hp_app::actions::key_label;
use hp_app::settings::MergePrompt;
use hp_app::state::AppState;
use hp_app::{Severity, t, tf};

use super::VertexTool;
use super::model::{Reselect, SolidEdit, preview_solid};

pub const STACK_EPS: f64 = 0.01;
pub(crate) const PROMPT_ID: &str = "hp_vertex_merge_prompt";

pub fn stacked_groups(points: &[DVec3], base: &[DVec3]) -> Vec<Vec<usize>> {
    let moved = |i: usize| base.get(i).is_none_or(|b| *b != points[i]);
    let mut parent: Vec<usize> = (0..points.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for i in 0..points.len() {
        for j in (i + 1)..points.len() {
            if (moved(i) || moved(j)) && points[i].distance(points[j]) <= STACK_EPS {
                let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                if a != b {
                    parent[b.max(a)] = a.min(b);
                }
            }
        }
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut slot: Vec<Option<usize>> = vec![None; points.len()];
    for i in 0..points.len() {
        let r = root(&mut parent, i);
        match slot[r] {
            Some(k) => groups[k].push(i),
            None => {
                slot[r] = Some(groups.len());
                groups.push(vec![i]);
            }
        }
    }
    groups.retain(|g| g.len() > 1);
    groups
}

pub fn merged_count(groups: &[Vec<usize>]) -> usize {
    groups.iter().map(|g| g.len() - 1).sum()
}

pub fn weld_stacked(points: &[DVec3], base: &[DVec3], groups: &[Vec<usize>]) -> Vec<DVec3> {
    let mut out = points.to_vec();
    for g in groups {
        let still = g.iter().copied().find(|&i| base.get(i).is_some_and(|b| *b == points[i]));
        let to = points[still.unwrap_or(g[0])];
        for &i in g {
            out[i] = to;
        }
    }
    out
}

pub(crate) struct Stacked {
    pub edits: Vec<SolidEdit>,
    pub merged: usize,
    pub spots: Vec<DVec3>,
}

pub(crate) fn find_stacked(edits: &[SolidEdit]) -> Option<Stacked> {
    let mut merged = 0;
    let mut spots = Vec::new();
    let mut welded = Vec::with_capacity(edits.len());
    for e in edits {
        let groups = stacked_groups(&e.points, &e.base);
        merged += merged_count(&groups);
        let points = weld_stacked(&e.points, &e.base, &groups);
        spots.extend(groups.iter().filter_map(|g| points.get(g[0]).copied()));
        welded.push(SolidEdit { points, ..e.clone() });
    }
    (merged > 0).then_some(Stacked { edits: welded, merged, spots })
}

fn valid_edits(state: &AppState, edits: &[SolidEdit]) -> bool {
    edits.iter().all(|e| {
        state
            .doc
            .scene()
            .solid(e.id)
            .is_some_and(|n| preview_solid(&e.points, &n.solid.sides, e.topology.as_ref()).is_ok())
    })
}

pub(crate) struct PendingMerge {
    pub edits: Vec<SolidEdit>,
    pub reselect: Reselect,
    pub label: &'static str,
    pub tag: Option<String>,
    pub merged: usize,
    pub spots: Vec<DVec3>,
    pub edges: Vec<[DVec3; 2]>,
    pub revision: u64,
    pub always: bool,
    pub focused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MergeAnswer {
    Open,
    Yes,
    No,
}

impl VertexTool {
    pub(crate) fn apply_move(
        &mut self,
        state: &mut AppState,
        edits: &[SolidEdit],
        reselect: Reselect,
        label: &'static str,
        tag: Option<&str>,
    ) -> bool {
        if state.settings.vertex.merge_prompt == MergePrompt::Off {
            return self.apply(state, edits, reselect, label, tag);
        }
        let Some(stacked) = find_stacked(edits).filter(|st| valid_edits(state, &st.edits)) else {
            return self.apply(state, edits, reselect, label, tag);
        };
        match state.settings.vertex.merge_prompt {
            MergePrompt::Off | MergePrompt::Always => self.apply(state, &stacked.edits, reselect, label, tag),
            MergePrompt::Never => {
                state.notify(Severity::Info, tf!("vertex.merge_reverted", stacked.merged));
                false
            }
            MergePrompt::Ask => {
                let edges = stacked
                    .edits
                    .iter()
                    .filter_map(|e| {
                        let node = state.doc.scene().solid(e.id)?;
                        preview_solid(&e.points, &node.solid.sides, e.topology.as_ref()).ok()
                    })
                    .flatten()
                    .collect();
                self.merge_prompt = Some(PendingMerge {
                    edits: stacked.edits,
                    reselect,
                    label,
                    tag: tag.map(str::to_owned),
                    merged: stacked.merged,
                    spots: stacked.spots,
                    edges,
                    revision: state.doc.revision(),
                    always: false,
                    focused: false,
                });
                false
            }
        }
    }

    pub fn merge_prompt_open(&self) -> bool {
        self.merge_prompt.is_some()
    }

    pub fn answer_merge_prompt(&mut self, state: &mut AppState, yes: bool, always: bool) -> bool {
        let Some(p) = self.merge_prompt.take() else { return false };
        if !yes {
            state.notify(Severity::Info, tf!("vertex.merge_reverted", p.merged));
            return true;
        }
        if always {
            state.settings.vertex.merge_prompt = MergePrompt::Always;
            state.settings_dirty = true;
        }
        if state.doc.revision() != p.revision {
            state.notify(Severity::Warning, t!("vertex.merge_stale"));
            return true;
        }
        if self.apply(state, &p.edits, p.reselect, p.label, p.tag.as_deref()) {
            state.notify(Severity::Info, tf!("vertex.merge_done", p.merged));
        }
        true
    }

    pub(crate) fn merge_prompt_ui(&mut self, state: &mut AppState, ctx: &egui::Context) {
        let Some(p) = self.merge_prompt.as_mut() else { return };
        let answer = show_prompt(ctx, p);
        let always = p.always;
        match answer {
            MergeAnswer::Open => {}
            MergeAnswer::Yes => {
                self.answer_merge_prompt(state, true, always);
            }
            MergeAnswer::No => {
                self.answer_merge_prompt(state, false, false);
            }
        }
    }
}

fn show_prompt(ctx: &egui::Context, p: &mut PendingMerge) -> MergeAnswer {
    let mut yes = false;
    let mut no = false;
    let modal = egui::Modal::new(egui::Id::unique(PROMPT_ID)).show(ctx, |ui| {
        ui.set_max_width(420.0);
        ui.heading(t!("vertex.merge_title"));
        ui.add_space(4.0);
        ui.label(t!("vertex.merge_question"));
        ui.label(egui::RichText::new(tf!("vertex.merge_count", p.merged)).weak());
        ui.add_space(6.0);
        ui.checkbox(&mut p.always, t!("vertex.merge_always")).on_hover_text(t!("vertex.merge_always_tip"));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let y = ui.button(format!("{} ({})", t!("vertex.merge_yes"), key_label(egui::Key::Enter)));
            if !p.focused {
                y.request_focus();
                p.focused = true;
            }
            yes = y.clicked();
            no = ui.button(format!("{} ({})", t!("vertex.merge_no"), key_label(egui::Key::Escape))).clicked();
        });
    });
    let enter = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    let escape = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if no || escape || (modal.should_close() && !yes && !enter) {
        MergeAnswer::No
    } else if yes || enter {
        MergeAnswer::Yes
    } else {
        MergeAnswer::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacked_vertices_are_found_only_when_a_moved_vertex_lands_on_another() {
        let base = vec![DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0), DVec3::new(0.0, 64.0, 0.0)];
        assert!(stacked_groups(&base, &base).is_empty());
        let mut moved = base.clone();
        moved[1] = DVec3::new(0.004, 0.0, 0.0);
        let groups = stacked_groups(&moved, &base);
        assert_eq!(groups, vec![vec![0, 1]]);
        assert_eq!(merged_count(&groups), 1);
        let welded = weld_stacked(&moved, &base, &groups);
        assert_eq!(welded[1], DVec3::ZERO, "the moved vertex joins the one that stayed");
        moved[2] = DVec3::new(0.0, 0.005, 0.0);
        let groups = stacked_groups(&moved, &base);
        assert_eq!(groups, vec![vec![0, 1, 2]]);
        assert_eq!(merged_count(&groups), 2);
        moved[1] = DVec3::new(0.5, 0.0, 0.0);
        moved[2] = DVec3::new(0.5, 0.0, 0.0);
        let groups = stacked_groups(&moved, &base);
        assert_eq!(groups, vec![vec![1, 2]], "two moved vertices on top of each other");
        let split = vec![DVec3::ZERO, DVec3::X, DVec3::ZERO];
        assert_eq!(stacked_groups(&split, &split[..2]), vec![vec![0, 2]], "a new vertex counts as moved");
    }
}
