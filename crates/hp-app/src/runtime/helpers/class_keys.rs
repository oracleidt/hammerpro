use hp_fgd::{ResolvedClass, ResolvedFgd};

use crate::state::{AppState, ModelState};

pub const KEPT_KEYS: &[&str] = &["classname", "origin", "angles", "targetname", "hammerid"];

pub const PROP_TYPES: &[&str] = &["prop_static", "prop_physics", "prop_physics_multiplayer", "prop_dynamic"];

pub fn is_kept_key(key: &str) -> bool {
    let key = key.trim();
    KEPT_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key)) || key.to_ascii_lowercase().starts_with("hp_")
}

pub fn unused_keys(entity: &hp_vmf::Entity, class: &ResolvedClass) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (key, _) in &entity.props {
        if is_kept_key(key) || class.property(key).is_some() {
            continue;
        }
        if !out.iter().any(|k| k.eq_ignore_ascii_case(key)) {
            out.push(key.clone());
        }
    }
    out
}

pub fn unused_keys_in(fgd: Option<&ResolvedFgd>, entity: &hp_vmf::Entity) -> Vec<String> {
    fgd.and_then(|f| f.class(entity.classname.trim())).map(|c| unused_keys(entity, c)).unwrap_or_default()
}

pub fn default_resets(entity: &hp_vmf::Entity, class: &ResolvedClass) -> Vec<(String, String)> {
    class
        .props
        .iter()
        .filter(|p| !is_kept_key(&p.name))
        .filter_map(|p| {
            let value = p.default_value()?;
            if value.is_empty() || entity.get(&p.name) == Some(value.as_str()) {
                return None;
            }
            Some((p.name.clone(), value))
        })
        .collect()
}

pub fn has_physics_model(state: &AppState, path: &str) -> Option<bool> {
    let fs = state.assets.fs()?;
    Some(fs.exists(&hp_mdl::phy_path(path.trim())))
}

pub fn chosen_prop_type(state: &AppState) -> Option<&'static str> {
    let chosen = state.settings.browser.model_prop_class.trim();
    PROP_TYPES.iter().copied().find(|p| p.eq_ignore_ascii_case(chosen))
}

pub struct PropChoice {
    pub class: String,
    pub refused: Option<&'static str>,
}

pub fn prop_class_for_model(state: &mut AppState, path: &str, default: &str) -> PropChoice {
    let Some(mut chosen) = chosen_prop_type(state) else {
        return PropChoice { class: default.to_owned(), refused: None };
    };
    if chosen == "prop_physics_multiplayer" && state.assets.fgd().is_some_and(|f| f.class(chosen).is_none()) {
        chosen = "prop_physics";
    }
    if !chosen.starts_with("prop_physics") {
        return PropChoice { class: chosen.to_owned(), refused: None };
    }
    let is_static = match state.assets.model_info(path) {
        ModelState::Ready(info) => info.is_static_prop,
        _ => return PropChoice { class: chosen.to_owned(), refused: None },
    };
    if has_physics_model(state, path) != Some(false) {
        return PropChoice { class: chosen.to_owned(), refused: None };
    }
    let fallback = if is_static { "prop_static" } else { "prop_dynamic" };
    PropChoice { class: fallback.to_owned(), refused: Some(chosen) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class() -> ResolvedClass {
        let fgd = hp_fgd::Fgd::parse(
            r#"@PointClass = light [ targetname(target_source) : "Name" _light(color255) : "Brightness" : "255 255 255 200" spawnflags(flags) = [ 1 : "Initially dark" : 0 ] ]"#,
            None,
        )
        .resolve();
        fgd.class("light").expect("light").clone()
    }

    fn prop_entity() -> hp_vmf::Entity {
        let mut e = hp_vmf::Entity { classname: "light".to_owned(), ..Default::default() };
        for (k, v) in [
            ("origin", "0 0 0"),
            ("angles", "0 0 0"),
            ("targetname", "lamp"),
            ("model", "models/props_c17/oildrum001.mdl"),
            ("skin", "1"),
            ("solid", "6"),
            ("fademindist", "-1"),
            ("spawnflags", "0"),
            ("hp_note", "x"),
        ] {
            e.set(k, v);
        }
        e
    }

    #[test]
    fn unused_keys_are_the_ones_the_class_does_not_define() {
        let keys = unused_keys(&prop_entity(), &class());
        assert_eq!(keys, vec!["model", "skin", "solid", "fademindist"]);
    }

    #[test]
    fn spawnflags_go_when_the_class_has_none() {
        let fgd =
            hp_fgd::Fgd::parse(r#"@PointClass = info_target [ targetname(target_source) : "Name" ]"#, None).resolve();
        let keys = unused_keys(&prop_entity(), fgd.class("info_target").expect("class"));
        assert!(keys.iter().any(|k| k == "spawnflags"));
        assert!(!keys.iter().any(|k| k == "targetname" || k == "origin" || k == "angles"));
    }

    #[test]
    fn default_resets_list_changed_defaults_only() {
        let mut e = prop_entity();
        e.set("_light", "1 2 3 4");
        let resets = default_resets(&e, &class());
        assert_eq!(resets, vec![("_light".to_owned(), "255 255 255 200".to_owned())]);
        e.set("_light", "255 255 255 200");
        assert!(default_resets(&e, &class()).iter().all(|(k, _)| k != "_light"));
    }
}
