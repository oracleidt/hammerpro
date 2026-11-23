use egui::{CornerRadius, RichText, ScrollArea, Sense, Ui, vec2};
use hp_app::state::{ModelInfo, ModelState, PreviewCamera};
use hp_app::{t, tf, theme};
use hp_editor::{Document, Id};

use crate::edit::Op;
use crate::models::{body_choices, body_value};
use crate::values::Tri;
use crate::view::{Agg, EntityView, Target};
use crate::widgets::{ROW_HEIGHT, empty_note, list_row};

pub(crate) fn studio_model(view: &EntityView) -> Option<String> {
    if view.target.is_world() {
        return None;
    }
    let row = view.props.iter().find(|r| r.key.eq_ignore_ascii_case("model"))?;
    let v = row.agg.value()?.trim().replace('\\', "/").to_ascii_lowercase();
    v.ends_with(".mdl").then_some(v)
}

fn int_value(view: &EntityView, key: &str) -> Option<i64> {
    match &view.props.iter().find(|r| r.key.eq_ignore_ascii_case(key))?.agg {
        Agg::Value(v) => Some(v.trim().parse::<f64>().ok().filter(|f| f.is_finite()).map_or(0, |f| f as i64)),
        Agg::Absent => Some(0),
        Agg::Mixed => None,
    }
}

pub(crate) const PREVIEW_SIZE: f32 = 160.0;

#[allow(clippy::too_many_arguments)]
pub(crate) fn model_tab(
    ui: &mut Ui,
    view: &EntityView,
    path: &str,
    info: &ModelState,
    preview: Option<egui::TextureId>,
    camera: &mut PreviewCamera,
    autoplay: &mut bool,
    ops: &mut Vec<Op>,
) {
    ui.label(RichText::new(crate::values::model_file_stem(path)).strong()).on_hover_text(path);
    ui.label(RichText::new(path).small().weak());
    let info = match info {
        ModelState::Ready(i) => i,
        ModelState::Loading => {
            ui.horizontal(|ui| {
                hp_app::widgets::spinner(ui);
                ui.label(t!("common.loading"));
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        ModelState::Failed(e) => {
            ui.label(RichText::new(tf!("entities.model_failed", e)).color(theme::error()));
            return;
        }
    };
    if std::mem::take(autoplay) && camera.anim.is_none() {
        let default = view.props.iter().find(|r| r.key.eq_ignore_ascii_case("DefaultAnim")).and_then(|r| r.agg.value());
        camera.anim = info.preview_sequence(default);
        camera.anim_time = 0.0;
    }
    ui.vertical_centered(|ui| {
        let size = vec2(PREVIEW_SIZE, PREVIEW_SIZE);
        let resp = match preview {
            Some(tex) => ui.add(egui::Image::new((tex, size)).sense(Sense::DRAG)),
            None => {
                let (rect, resp) = ui.allocate_exact_size(size, Sense::DRAG);
                ui.painter().rect_filled(rect, CornerRadius::same(4), ui.visuals().extreme_bg_color);
                resp
            }
        };
        if resp.dragged() {
            let d = resp.drag_delta();
            camera.yaw_deg = (camera.yaw_deg - d.x * 0.5).rem_euclid(360.0);
            camera.pitch_deg = (camera.pitch_deg + d.y * 0.5).clamp(-89.0, 89.0);
        }
        resp.on_hover_text(t!("entities.preview_tip"));
    });
    crate::models::preview_overlays(ui, camera, Some(info));
    ui.label(RichText::new(tf!("entities.model_tab_summary", info.triangles, info.stats.vertices)).small().weak());
    egui::CollapsingHeader::new(t!("entities.model_stats"))
        .id_salt("hp_ui_entities.model_tab_stats")
        .default_open(true)
        .show(ui, |ui| crate::models::model_info_grid(ui, info));
    let target = view.target.clone();
    let set = |ops: &mut Vec<Op>, key: &str, value: String| {
        ops.push(Op::SetKey { target: target.clone(), key: key.to_owned(), value, merge: false });
    };
    egui::Grid::new("hp_ui_entities.model_tab").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label(t!("entities.skins"));
        let skin = int_value(view, "skin");
        let text = skin.map_or_else(|| t!("common.multiple_values").to_owned(), |s| tf!("entities.skin_n", s));
        egui::ComboBox::from_id_salt("hp_ui_entities.model_tab_skin").selected_text(text).show_ui(ui, |ui| {
            for s in 0..info.skins.max(1) {
                if ui.selectable_label(skin == Some(i64::from(s)), tf!("entities.skin_n", s)).clicked() {
                    set(ops, "skin", s.to_string());
                }
            }
        });
        ui.end_row();
        let body = int_value(view, "body");
        let choices = body_choices(&info.body_groups, body.unwrap_or(0));
        for (i, g) in info.body_groups.iter().enumerate() {
            if g.choices.len() < 2 {
                continue;
            }
            ui.label(&g.name);
            let current = choices.get(i).copied().unwrap_or(0);
            let text = match body {
                Some(_) => g.choices.get(current).cloned().unwrap_or_default(),
                None => t!("common.multiple_values").to_owned(),
            };
            egui::ComboBox::from_id_salt(("hp_ui_entities.model_tab_body", i)).selected_text(text).show_ui(ui, |ui| {
                for (c, name) in g.choices.iter().enumerate() {
                    if ui.selectable_label(body.is_some() && c == current, name).clicked() {
                        let mut new = choices.clone();
                        if let Some(slot) = new.get_mut(i) {
                            *slot = c;
                        }
                        set(ops, "body", body_value(&info.body_groups, &new).to_string());
                    }
                }
            });
            ui.end_row();
        }
    });
    camera.skin = int_value(view, "skin").map_or(0, |s| u32::try_from(s.max(0)).unwrap_or(0));
    sequences(ui, view, info, camera, ops);
}

fn sequences(ui: &mut Ui, view: &EntityView, info: &ModelInfo, camera: &mut PreviewCamera, ops: &mut Vec<Op>) {
    let key = view.props.iter().find(|r| r.key.eq_ignore_ascii_case("DefaultAnim")).map(|r| r.key.clone());
    if camera.anim.is_some_and(|i| i >= info.sequences.len()) {
        camera.anim = None;
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tf!("entities.model_tab_sequences", info.sequences.len())).strong());
        if let Some(i) = camera.anim {
            if ui.small_button("⏹").on_hover_text(t!("entities.stop_sequence")).clicked() {
                camera.anim = None;
                camera.anim_time = 0.0;
            }
            if ui.small_button("↺").on_hover_text(t!("entities.restart_sequence")).clicked() {
                camera.anim_time = 0.0;
            }
            ui.label(RichText::new(&info.sequences[i]).small().weak());
        }
    });
    if info.sequences.is_empty() {
        ui.label(RichText::new(t!("entities.no_sequences")).weak());
        return;
    }
    if key.is_none() {
        ui.label(RichText::new(t!("entities.no_default_anim")).small().weak());
    }
    let current = key.as_ref().and_then(|k| view.props.iter().find(|r| &r.key == k)).and_then(|r| r.agg.value());
    ScrollArea::vertical().id_salt("hp_ui_entities.model_tab_sequences").max_height(200.0).show_rows(
        ui,
        ROW_HEIGHT,
        info.sequences.len(),
        |ui, range| {
            for i in range {
                let name = &info.sequences[i];
                let selected = current.is_some_and(|c| c.eq_ignore_ascii_case(name));
                let playing = camera.anim == Some(i);
                let mut row = None;
                ui.horizontal(|ui| {
                    let glyph = if playing { "⏸" } else { "▶" };
                    let play = ui.add(egui::Button::selectable(playing, glyph).small());
                    if play.on_hover_text(t!("entities.play_sequence_tip")).clicked() {
                        camera.anim = if playing { None } else { Some(i) };
                        camera.anim_time = 0.0;
                    }
                    let frames = info.sequence_frames.get(i).copied().unwrap_or(1);
                    let detail = if frames > 1 { tf!("entities.frames_n", frames) } else { String::new() };
                    row = Some(list_row(ui, name, &detail, selected, false));
                });
                if row.is_some_and(|r| r.clicked())
                    && let Some(k) = &key
                {
                    ops.push(Op::SetKey {
                        target: view.target.clone(),
                        key: k.clone(),
                        value: name.clone(),
                        merge: false,
                    });
                }
            }
        },
    );
}

fn flat_visgroups(doc: &Document) -> Vec<(Id, String, usize, [u8; 3])> {
    fn walk(groups: &[hp_vmf::VisGroup], depth: usize, out: &mut Vec<(Id, String, usize, [u8; 3])>) {
        for g in groups {
            out.push((g.id, g.name.clone(), depth, g.color));
            if depth < 32 {
                walk(&g.children, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(doc.scene().visgroups(), 0, &mut out);
    out
}

pub(crate) fn visgroups_tab(ui: &mut Ui, doc: &Document, view: &EntityView, ops: &mut Vec<Op>) {
    let Target::Entities(ids) = &view.target else { return };
    let groups = flat_visgroups(doc);
    if groups.is_empty() {
        empty_note(ui, t!("entities.no_visgroups"));
        return;
    }
    ScrollArea::vertical().id_salt("hp_ui_entities.visgroups_tab").auto_shrink([false, false]).show(ui, |ui| {
        crate::widgets::fit_scroll_content(ui);
        for (id, name, depth, color) in groups {
            let state = Tri::of_bools(
                ids.iter().map(|e| doc.scene().entity(*e).is_some_and(|n| n.entity.editor.visgroup_ids.contains(&id))),
            );
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 14.0);
                let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().rect_filled(
                    r,
                    CornerRadius::same(2),
                    egui::Color32::from_rgb(color[0], color[1], color[2]),
                );
                let mut checked = state == Tri::On;
                let resp = ui.add(egui::Checkbox::new(&mut checked, name.as_str()).indeterminate(state == Tri::Mixed));
                if resp.clicked() {
                    ops.push(Op::Visgroup { ids: ids.clone(), group: id, add: state.toggled() });
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visgroup_membership_from_the_tab() {
        crate::register_translations();
        let mut s = hp_app::testing::test_state();
        let g = s.doc.create_visgroup("lights", None, &[hp_editor::ObjRef::Entity(5)]).unwrap();
        s.doc.clear_history();
        let names = crate::MapNames::build(&s.doc);
        let view = EntityView::build(&s.doc, Target::Entities(vec![5, 4]), None, &names);
        let mut ops = Vec::new();
        hp_app::testing::run_ui(&mut s, |ui, s| visgroups_tab(ui, &s.doc, &view, &mut ops));
        assert!(ops.is_empty(), "drawing does not edit");
        crate::edit::apply(&mut s.doc, None, Op::Visgroup { ids: vec![5, 4], group: g, add: true }).unwrap();
        assert!(s.doc.scene().entity(4).unwrap().entity.editor.visgroup_ids.contains(&g));
        crate::edit::apply(&mut s.doc, None, Op::Visgroup { ids: vec![5, 4], group: g, add: false }).unwrap();
        assert!(!s.doc.scene().entity(5).unwrap().entity.editor.visgroup_ids.contains(&g));
        assert_eq!(s.doc.history_len().0, 2);
    }

    #[test]
    fn model_tab_reads_and_writes_skin_and_body() {
        crate::register_translations();
        let mut s = hp_app::testing::test_state();
        let kv =
            [("model".to_owned(), "Models\\Props_C17\\OilDrum001.mdl".to_owned()), ("body".to_owned(), "1".to_owned())];
        let id = s.doc.create_point_entity("prop_dynamic", glam::DVec3::ZERO, &kv).unwrap();
        let names = crate::MapNames::build(&s.doc);
        let view = EntityView::build(&s.doc, Target::Entities(vec![id]), None, &names);
        assert_eq!(studio_model(&view).as_deref(), Some("models/props_c17/oildrum001.mdl"));
        let info = s.assets.model_info("models/props_c17/oildrum001.mdl");
        let mut camera = PreviewCamera::default();
        let mut ops = Vec::new();
        hp_app::testing::run_ui(&mut s, |ui, _| {
            model_tab(ui, &view, "models/props_c17/oildrum001.mdl", &info, None, &mut camera, &mut false, &mut ops);
        });
        assert!(ops.is_empty());
        let world = EntityView::build(&s.doc, Target::World, None, &names);
        assert_eq!(studio_model(&world), None);
    }
}
