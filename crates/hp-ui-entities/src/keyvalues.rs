use hp_app::runtime::KeyvalueClip;
use hp_app::{AppState, Severity, t, tf};
use hp_editor::Document;

use crate::edit::{Op, apply, is_protected_key};
use crate::view::SelectionSummary;

pub(crate) fn clip_of(doc: &Document, id: hp_editor::Id) -> Option<KeyvalueClip> {
    let e = &doc.scene().entity(id)?.entity;
    let keys = e
        .props
        .iter()
        .filter(|(k, _)| !is_protected_key(k) && !k.eq_ignore_ascii_case("targetname"))
        .cloned()
        .collect();
    let targetname = e.get("targetname").filter(|n| !n.trim().is_empty()).map(str::to_owned);
    Some(KeyvalueClip { class: e.classname.clone(), keys, targetname })
}

pub(crate) fn clip_text(clip: &KeyvalueClip) -> String {
    let mut out = format!("\"classname\" \"{}\"\n", clip.class);
    if let Some(n) = &clip.targetname {
        out.push_str(&format!("\"targetname\" \"{n}\"\n"));
    }
    for (k, v) in &clip.keys {
        out.push_str(&format!("\"{k}\" \"{v}\"\n"));
    }
    out
}

pub fn copy_keyvalues(ctx: &egui::Context, state: &mut AppState) -> bool {
    let summary = SelectionSummary::of(&state.doc);
    let Some(clip) = summary.entities.first().and_then(|id| clip_of(&state.doc, *id)) else {
        state.notify(Severity::Warning, t!("entities.kv_nothing_to_copy"));
        return false;
    };
    ctx.copy_text(clip_text(&clip));
    state.notify(Severity::Info, tf!("entities.kv_copied", clip.keys.len(), clip.class));
    state.runtime.entities.clipboard = Some(clip);
    true
}

pub fn can_paste_keyvalues(state: &AppState) -> bool {
    state.runtime.entities.clipboard.is_some() && !SelectionSummary::of(&state.doc).entities.is_empty()
}

pub fn paste_keyvalues(state: &mut AppState) -> usize {
    let Some(clip) = state.runtime.entities.clipboard.clone() else { return 0 };
    let ids = SelectionSummary::of(&state.doc).entities;
    if ids.is_empty() {
        state.notify(Severity::Warning, t!("entities.kv_nothing_selected"));
        return 0;
    }
    let mut keys = clip.keys.clone();
    if state.settings.entities.paste_targetname
        && let Some(n) = &clip.targetname
    {
        keys.push(("targetname".to_owned(), n.clone()));
    }
    let n = keys.len();
    match apply(&mut state.doc, state.assets.fgd(), Op::PasteKeys { ids: ids.clone(), keys }) {
        Ok(_) => {
            state.notify(Severity::Success, tf!("entities.kv_pasted", n, ids.len()));
            ids.len()
        }
        Err(e) => {
            state.notify(Severity::Error, tf!("entities.edit_failed", e));
            0
        }
    }
}

pub fn clean_selected_keyvalues(state: &mut AppState, reset: bool) -> usize {
    let ids = SelectionSummary::of(&state.doc).entities;
    if ids.is_empty() {
        state.notify(Severity::Warning, t!("entities.kv_nothing_selected"));
        return 0;
    }
    let fgd = state.assets.fgd();
    if fgd.is_none() {
        state.notify(Severity::Warning, t!("entities.no_fgd"));
        return 0;
    }
    match crate::edit::apply_clean_keys(&mut state.doc, fgd, &ids, reset) {
        Ok(report) => {
            let n = report.removed.iter().filter(|(_, k)| !k.is_empty()).count();
            state.notify(Severity::Info, crate::cx::clean_report_text(&report));
            if report.reset > 0 { ids.len() } else { n }
        }
        Err(e) => {
            state.notify(Severity::Error, tf!("entities.edit_failed", e));
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_skips_identity_keys_and_paste_is_one_step() {
        crate::register_translations();
        let mut s = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        assert!(!copy_keyvalues(&ctx, &mut s), "nothing selected");
        s.doc.set_selection(&[hp_editor::ObjRef::Entity(5)]);
        assert!(copy_keyvalues(&ctx, &mut s));
        let clip = s.runtime.entities.clipboard.clone().expect("copied");
        assert_eq!(clip.class, "light");
        assert_eq!(clip.keys, [("_light".to_owned(), "255 240 200 300".to_owned())]);
        assert_eq!(clip.targetname.as_deref(), Some("main_light"));
        let text = clip_text(&clip);
        assert!(text.starts_with("\"classname\" \"light\"\n") && text.contains("\"_light\" \"255 240 200 300\""));
        s.doc.set_selection(&[hp_editor::ObjRef::Entity(4), hp_editor::ObjRef::Entity(6)]);
        assert!(can_paste_keyvalues(&s));
        let before = s.doc.history_len().0;
        assert_eq!(paste_keyvalues(&mut s), 2);
        assert_eq!(s.doc.history_len().0, before + 1);
        assert_eq!(s.doc.undo_label(), Some("Paste keyvalues"));
        for id in [4, 6] {
            let e = &s.doc.scene().entity(id).unwrap().entity;
            assert_eq!(e.get("_light"), Some("255 240 200 300"));
            assert_eq!(e.get("targetname"), None, "the name is not pasted by default");
        }
        assert_eq!(s.doc.scene().entity(4).unwrap().entity.get("origin"), Some("0 -128 0"), "origin kept");
        s.settings.entities.paste_targetname = true;
        assert_eq!(paste_keyvalues(&mut s), 2);
        assert_eq!(s.doc.scene().entity(4).unwrap().entity.get("targetname"), Some("main_light"));
    }

    #[test]
    fn paste_never_writes_identity_keys() {
        let mut s = hp_app::testing::test_state();
        s.runtime.entities.clipboard = Some(KeyvalueClip {
            class: "x".into(),
            keys: vec![("origin".into(), "1 2 3".into()), ("ID".into(), "7".into()), ("speed".into(), "5".into())],
            targetname: None,
        });
        s.doc.set_selection(&[hp_editor::ObjRef::Entity(5)]);
        assert_eq!(paste_keyvalues(&mut s), 1);
        let e = &s.doc.scene().entity(5).unwrap().entity;
        assert_eq!(e.get("origin"), Some("0 0 96"));
        assert_eq!(e.get("speed"), Some("5"));
        assert_eq!(e.id, 5);
    }
}
