use egui::{DragValue, Grid, RichText, Ui};
use hp_app::i18n::TrTable;
use hp_app::runtime::instances::{InstanceCache, selected_instances};
use hp_app::{Action, AppState, Severity, t, tf, theme};
use hp_editor::instances::{
    InstanceParm, InstanceSpec, LoadRequest, Loaded, is_replace_key, next_free_replace_key, parm_value, split_replace,
};
use hp_fgd::PropertyType;
use hp_vmf::Entity;

use crate::edit::{Op, apply};
use crate::view::Target;

pub const TR: TrTable = &[
    ("entities.instance.parameters", "Instance parameters", "Paramètres de l'instance"),
    (
        "entities.instance.none_declared",
        "This instance declares no parameter.",
        "Cette instance ne déclare aucun paramètre.",
    ),
    ("entities.instance.loading", "Loading the instance file...", "Chargement du fichier de l'instance..."),
    ("entities.instance.not_set", "Not set: the instance keeps {0}", "Non défini : l'instance garde {0}"),
    ("entities.instance.default", "Default: {0}", "Par défaut : {0}"),
    ("entities.instance.type", "Type: {0}", "Type : {0}"),
    ("entities.instance.remove", "Remove the value", "Retirer la valeur"),
    ("entities.instance.other", "Other replacements", "Autres remplacements"),
    ("entities.instance.add", "Add a replacement ($variable value)", "Ajouter un remplacement ($variable valeur)"),
    ("entities.instance.add_hint", "$variable value", "$variable valeur"),
    (
        "entities.instance.select",
        "Select a func_instance to see its file and its parameters.",
        "Sélectionnez une func_instance pour voir son fichier et ses paramètres.",
    ),
    (
        "entities.instance.several",
        "{0} instances selected: the first one is shown.",
        "{0} instances sélectionnées : la première est affichée.",
    ),
    ("entities.instance.file", "File: {0}", "Fichier : {0}"),
    ("entities.instance.no_file", "No file set (key \"file\").", "Aucun fichier défini (clé « file »)."),
    ("entities.instance.missing", "File not found: {0}", "Fichier introuvable : {0}"),
    ("entities.instance.failed", "Cannot read {0}: {1}", "Impossible de lire {0} : {1}"),
    (
        "entities.instance.in_content",
        "Read from the game content (read only)",
        "Lu depuis le contenu du jeu (lecture seule)",
    ),
    (
        "entities.instance.edit_tip",
        "Open the instance file to edit it",
        "Ouvrir le fichier de l'instance pour le modifier",
    ),
    (
        "entities.instance.collapse_tip",
        "Replace the instance with its contents (undo restores it)",
        "Remplacer l'instance par son contenu (annuler la rétablit)",
    ),
];

const MAX_VALUE: usize = 1024;

fn clean_value(s: &str) -> String {
    let mut out: String = s.chars().filter(|c| *c != '"' && *c != '\n' && *c != '\r').collect();
    if out.len() > MAX_VALUE {
        let mut cut = MAX_VALUE;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
    }
    out
}

fn text_field(ui: &mut Ui, id: egui::Id, current: &str, hint: &str) -> Option<String> {
    let focused = ui.memory(|m| m.has_focus(id));
    let mut buf = if focused {
        ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| current.to_owned())
    } else {
        current.to_owned()
    };
    let resp = ui.add(egui::TextEdit::singleline(&mut buf).id(id).hint_text(hint).desired_width(f32::INFINITY));
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, buf.clone()));
    }
    if resp.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
        let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
        let buf = clean_value(&buf);
        if !cancelled && buf != current {
            return Some(buf);
        }
    }
    None
}

fn numbers(v: &str) -> Vec<f64> {
    v.split_ascii_whitespace().map_while(|t| t.parse::<f64>().ok()).collect()
}

fn slot_for(entity: &Entity, name: &str, taken: &mut Vec<String>) -> String {
    if let Some((key, _)) = parm_value(entity, name) {
        return key.to_owned();
    }
    let mut probe = entity.clone();
    for k in taken.iter() {
        probe.set(k, "x");
    }
    let key = next_free_replace_key(&probe);
    taken.push(key.clone());
    key
}

pub fn parameter_rows(ui: &mut Ui, entity: &Entity, parms: Option<&[InstanceParm]>, ops: &mut Vec<Op>) {
    let target = Target::Entities(vec![entity.id]);
    let base = ui.make_persistent_id(("hp_instance_parms", entity.id));
    ui.label(RichText::new(t!("entities.instance.parameters")).strong());
    let Some(parms) = parms else {
        ui.label(RichText::new(t!("entities.instance.loading")).weak());
        return;
    };
    let mut taken: Vec<String> = Vec::new();
    if parms.is_empty() {
        ui.label(RichText::new(t!("entities.instance.none_declared")).weak());
    } else {
        Grid::new(base.with("grid")).num_columns(2).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
            for p in parms {
                parm_row(ui, base, entity, p, &target, &mut taken, ops);
                ui.end_row();
            }
        });
    }
    let others: Vec<(&str, &str)> = entity
        .props
        .iter()
        .filter(|(k, _)| is_replace_key(k))
        .filter(|(_, v)| {
            split_replace(v).is_none_or(|(var, _)| !parms.iter().any(|p| p.name.eq_ignore_ascii_case(&var)))
        })
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    ui.add_space(4.0);
    ui.label(RichText::new(t!("entities.instance.other")).weak());
    Grid::new(base.with("raw")).num_columns(2).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
        for (key, value) in others {
            let label = grid_label(ui, key);
            label.context_menu(|ui| {
                if ui.button(t!("entities.instance.remove")).clicked() {
                    ops.push(Op::RemoveKey { target: target.clone(), key: key.to_owned() });
                    ui.close();
                }
            });
            if let Some(v) = text_field(ui, base.with(("raw", key)), value, "") {
                ops.push(Op::SetKey { target: target.clone(), key: key.to_owned(), value: v, merge: false });
            }
            ui.end_row();
        }
        ui.label("+").on_hover_text(t!("entities.instance.add"));
        if let Some(v) = text_field(ui, base.with("add"), "", t!("entities.instance.add_hint"))
            && split_replace(v.trim()).is_some()
        {
            let key = slot_for(entity, "", &mut taken);
            ops.push(Op::SetKey { target: target.clone(), key, value: v.trim().to_owned(), merge: false });
        }
        ui.end_row();
    });
}

pub fn instance_section(
    ui: &mut Ui,
    doc: &hp_editor::Document,
    id: hp_editor::Id,
    ops: &mut Vec<Op>,
) -> Option<Action> {
    let entity = doc.scene().entity(id).map(|n| &n.entity)?;
    let spec = InstanceSpec::of(entity)?;
    let mut readable = false;
    let parms = if spec.file.is_empty() {
        Some(Vec::new())
    } else {
        let cache = InstanceCache::shared();
        let req = LoadRequest { file: &spec.file, containing: None, depth: 0 };
        match cache.load(&req, doc.path()) {
            Loaded::Ready { path, .. } => cache.file(&path).map(|f| {
                readable = !f.is_content();
                f.parms.clone()
            }),
            Loaded::Pending => {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                None
            }
            Loaded::Missing | Loaded::Failed(_) => Some(Vec::new()),
        }
    };
    ui.separator();
    let action = buttons(ui, readable);
    parameter_rows(ui, entity, parms.as_deref(), ops);
    action
}

fn buttons(ui: &mut Ui, readable: bool) -> Option<Action> {
    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        let edit = ui.add_enabled(readable, egui::Button::new(Action::EditInstance.label()));
        if edit.on_hover_text(t!("entities.instance.edit_tip")).clicked() {
            action = Some(Action::EditInstance);
        }
        let collapse = ui.button(Action::CollapseInstances.label());
        if collapse.on_hover_text(t!("entities.instance.collapse_tip")).clicked() {
            action = Some(Action::CollapseInstances);
        }
    });
    action
}

fn grid_label(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    let size = egui::vec2((ui.available_width() * 0.4).max(40.0), ui.spacing().interact_size.y);
    let layout = egui::Layout::left_to_right(egui::Align::Center);
    ui.allocate_ui_with_layout(size, layout, |ui| ui.add(egui::Label::new(text).truncate())).inner
}

fn parm_row(
    ui: &mut Ui,
    base: egui::Id,
    entity: &Entity,
    p: &InstanceParm,
    target: &Target,
    taken: &mut Vec<String>,
    ops: &mut Vec<Op>,
) {
    let current = parm_value(entity, &p.name);
    let label_text = p.name.trim_start_matches('$');
    let mut label = RichText::new(label_text);
    if current.is_none() {
        label = label.italics().weak();
    }
    let resp = grid_label(ui, label).on_hover_ui(|ui| {
        ui.label(tf!("entities.instance.type", &p.kind));
        if let Some(d) = &p.default {
            ui.label(tf!("entities.instance.default", d));
        }
        if current.is_none() {
            ui.label(tf!("entities.instance.not_set", &p.name));
        }
    });
    if let Some((key, _)) = current {
        resp.context_menu(|ui| {
            if ui.button(t!("entities.instance.remove")).clicked() {
                ops.push(Op::RemoveKey { target: target.clone(), key: key.to_owned() });
                ui.close();
            }
        });
    }
    let value = current.map(|(_, v)| v.to_owned()).or_else(|| p.default.clone()).unwrap_or_default();
    let id = base.with(("parm", p.name.to_ascii_lowercase()));
    let mut edit: Option<(String, bool)> = None;
    match PropertyType::from_name(&p.kind) {
        PropertyType::Boolean => {
            let mut on =
                numbers(&value).first().is_some_and(|n| *n != 0.0) || value.trim().eq_ignore_ascii_case("true");
            if ui.checkbox(&mut on, "").changed() {
                edit = Some((if on { "1" } else { "0" }.to_owned(), false));
            }
        }
        PropertyType::Integer => {
            let mut n = numbers(&value).first().copied().unwrap_or(0.0).round() as i64;
            let r = ui.add(DragValue::new(&mut n).speed(1.0));
            if r.changed() {
                edit = Some((n.to_string(), true));
            }
        }
        PropertyType::Float => {
            let mut n = numbers(&value).first().copied().filter(|v| v.is_finite()).unwrap_or(0.0);
            let r = ui.add(DragValue::new(&mut n).speed(0.1).max_decimals(4));
            if r.changed() && n.is_finite() {
                edit = Some((crate::values::fmt_num_rounded(n, 4), true));
            }
        }
        PropertyType::Color255 => {
            let nums = numbers(&value);
            let c = |i: usize| nums.get(i).copied().unwrap_or(255.0).clamp(0.0, 255.0) as u8;
            let mut rgb = [c(0), c(1), c(2)];
            ui.horizontal(|ui| {
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    let extra = nums.get(3).map(|b| format!(" {b}")).unwrap_or_default();
                    edit = Some((format!("{} {} {}{extra}", rgb[0], rgb[1], rgb[2]), true));
                }
                if let Some(v) = text_field(ui, id, &value, "") {
                    edit = Some((v, false));
                }
            });
        }
        _ => {
            let hint = p.default.as_deref().unwrap_or("");
            let shown = current.map(|(_, v)| v).unwrap_or("");
            if let Some(v) = text_field(ui, id, shown, hint) {
                edit = Some((v, false));
            }
        }
    }
    if let Some((v, merge)) = edit {
        let key = slot_for(entity, &p.name, taken);
        ops.push(Op::SetKey { target: target.clone(), key, value: format!("{} {}", p.name, v), merge });
    }
}

#[derive(Debug, Default)]
pub struct InstancePanel;

impl InstancePanel {
    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let ids = selected_instances(&state.doc);
        let Some(&id) = ids.first() else {
            ui.label(RichText::new(t!("entities.instance.select")).weak());
            return;
        };
        if ids.len() > 1 {
            ui.label(RichText::new(tf!("entities.instance.several", ids.len())).weak());
        }
        let Some(entity) = state.doc.scene().entity(id).map(|n| n.entity.clone()) else { return };
        let Some(spec) = InstanceSpec::of(&entity) else { return };
        let cache = InstanceCache::shared();
        let mut parms: Option<Vec<InstanceParm>> = None;
        let mut readable = false;
        if spec.file.is_empty() {
            ui.label(RichText::new(t!("entities.instance.no_file")).color(theme::error()));
            parms = Some(Vec::new());
        } else {
            let req = LoadRequest { file: &spec.file, containing: None, depth: 0 };
            match cache.load(&req, state.doc.path()) {
                Loaded::Ready { path, .. } => {
                    ui.add(egui::Label::new(tf!("entities.instance.file", path.display())).truncate())
                        .on_hover_text(spec.file.as_str());
                    if let Some(f) = cache.file(&path) {
                        if f.is_content() {
                            ui.label(RichText::new(t!("entities.instance.in_content")).weak());
                        } else {
                            readable = true;
                        }
                        parms = Some(f.parms.clone());
                    }
                }
                Loaded::Pending => {
                    ui.label(RichText::new(t!("entities.instance.loading")).weak());
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                }
                Loaded::Missing => {
                    ui.label(RichText::new(tf!("entities.instance.missing", &spec.file)).color(theme::error()));
                    parms = Some(Vec::new());
                }
                Loaded::Failed(e) => {
                    ui.label(RichText::new(tf!("entities.instance.failed", &spec.file, e)).color(theme::error()));
                    parms = Some(Vec::new());
                }
            }
        }
        if let Some(action) = buttons(ui, readable) {
            state.request(action);
        }
        ui.separator();
        let mut ops = Vec::new();
        egui::ScrollArea::vertical().id_salt("hp_instance_panel").auto_shrink([false, false]).show(ui, |ui| {
            parameter_rows(ui, &entity, parms.as_deref(), &mut ops);
        });
        let AppState { doc, assets, .. } = state;
        let mut failed = None;
        for op in ops {
            if let Err(e) = apply(doc, assets.fgd(), op) {
                failed = Some(e);
            }
        }
        if let Some(e) = failed {
            state.notify(Severity::Warning, tf!("entities.edit_failed", e));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(props: &[(&str, &str)]) -> Entity {
        Entity {
            id: 7,
            classname: "func_instance".into(),
            props: props.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
            ..Entity::default()
        }
    }

    #[test]
    fn values_are_cleaned_and_slots_found() {
        assert_eq!(clean_value("a\"b\nc"), "abc");
        assert_eq!(clean_value(&"é".repeat(MAX_VALUE)).len(), MAX_VALUE);
        assert_eq!(numbers("255 0 x 3"), vec![255.0, 0.0]);
        let e = instance(&[("replace01", "$a 1"), ("replace03", "$b 2")]);
        let mut taken = Vec::new();
        assert_eq!(slot_for(&e, "$B", &mut taken), "replace03");
        assert_eq!(slot_for(&e, "$c", &mut taken), "replace02");
        assert_eq!(slot_for(&e, "$d", &mut taken), "replace04", "slots given this frame are taken");
    }

    #[test]
    fn rows_draw_every_kind() {
        let parms = vec![
            InstanceParm { name: "$on".into(), kind: "boolean".into(), default: None },
            InstanceParm { name: "$n".into(), kind: "integer".into(), default: Some("4".into()) },
            InstanceParm { name: "$f".into(), kind: "float".into(), default: None },
            InstanceParm { name: "$c".into(), kind: "color255".into(), default: None },
            InstanceParm { name: "$s".into(), kind: "string".into(), default: Some("x".into()) },
        ];
        let e = instance(&[("replace01", "$n 9"), ("replace02", "$other y")]);
        let mut state = hp_app::testing::test_state();
        let mut ops = Vec::new();
        hp_app::testing::run_ui(&mut state, |ui, _| parameter_rows(ui, &e, Some(&parms), &mut ops));
        assert!(ops.is_empty(), "drawing alone edits nothing");
        hp_app::testing::run_ui(&mut state, |ui, _| parameter_rows(ui, &e, None, &mut ops));
        assert!(ops.is_empty());
    }

    #[test]
    fn panel_shows_the_file_and_its_parameters() {
        hp_app::i18n::register(TR);
        let dir = std::env::temp_dir().join(format!("hp_instance_panel_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let text = "world
{
	\"id\" \"1\"
	\"classname\" \"worldspawn\"
}
entity
{
	\"id\" \"2\"
	\"classname\" \"func_instance_parms\"
	\"parm1\" \"$speed integer 100\"
}
";
        std::fs::write(dir.join("door.vmf"), text).expect("write");
        let mut state = hp_app::testing::test_state();
        state.doc.set_path(Some(dir.join("main.vmf")));
        let id = state
            .doc
            .create_point_entity("func_instance", glam::DVec3::ZERO, &[("file".into(), "door.vmf".into())])
            .expect("instance");
        let mut panel = InstancePanel;
        state.doc.set_selection(&[]);
        hp_app::testing::run_ui(&mut state, |ui, state| panel.ui(ui, state));
        state.doc.set_selection(&[hp_editor::ObjRef::Entity(id)]);
        let cache = InstanceCache::shared();
        for _ in 0..50 {
            hp_app::testing::run_ui(&mut state, |ui, state| panel.ui(ui, state));
            cache.wait_idle(std::time::Duration::from_millis(100));
        }
        let req = LoadRequest { file: "door.vmf", containing: None, depth: 0 };
        let Loaded::Ready { path, .. } = cache.load(&req, state.doc.path()) else { panic!("not loaded") };
        assert_eq!(cache.file(&path).expect("file").parms.len(), 1);
        assert!(state.requested_actions.is_empty(), "drawing requests nothing");
        hp_app::testing::run_ui(&mut state, |ui, state| {
            let mut ops = Vec::new();
            assert_eq!(instance_section(ui, &state.doc, id, &mut ops), None);
            assert_eq!(instance_section(ui, &state.doc, 999_999, &mut ops), None);
            assert!(ops.is_empty());
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn translations_are_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty(), "{:?}", hp_app::i18n::lint_table(TR));
        for (k, _, _) in TR {
            assert!(k.starts_with("entities.instance."), "{k}");
            assert!(!crate::TR.iter().any(|(c, _, _)| c == k), "{k} collides");
        }
    }
}
