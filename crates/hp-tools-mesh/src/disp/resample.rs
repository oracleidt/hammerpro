use hp_app::state::AppState;
use hp_app::{Severity, tf};

use super::{DisplacementTool, plan_sew, selected_disp_faces};
use crate::common::edit_error_text;

const UNDO_RESAMPLE: &str = "Resample displacement power";

impl DisplacementTool {
    pub fn set_power_with(&mut self, state: &mut AppState, power: u32, keep_shape: bool) -> usize {
        if !keep_shape {
            return self.set_power(state, power);
        }
        let faces = selected_disp_faces(&state.doc);
        if faces.is_empty() {
            return 0;
        }
        let result = state.doc.transaction(UNDO_RESAMPLE, |doc| {
            let n = doc.resample_displacements(&faces, power)?;
            if n > 0 && faces.len() >= 2 {
                let plan = plan_sew(doc, &faces, &[]);
                let mut by_solid: Vec<(hp_editor::Id, Vec<(usize, hp_vmf::DispInfo)>)> = Vec::new();
                for (f, d) in plan.updates {
                    match by_solid.iter_mut().find(|(id, _)| *id == f.solid) {
                        Some((_, list)) => list.push((f.side, d)),
                        None => by_solid.push((f.solid, vec![(f.side, d)])),
                    }
                }
                for (id, list) in by_solid {
                    doc.edit_solid(id, |s| {
                        for (side, d) in list {
                            if let Some(sd) = s.sides.get_mut(side) {
                                sd.disp = Some(Box::new(d));
                            }
                        }
                    })?;
                }
            }
            Ok::<usize, hp_editor::EditError>(n)
        });
        match result {
            Ok(n) => {
                if n > 0 {
                    state.notify(Severity::Success, tf!("disp.resampled", n, power));
                }
                n
            }
            Err(e) => {
                let text = tf!("disp.failed", edit_error_text(&e));
                self.warn(state, text);
                0
            }
        }
    }
}
