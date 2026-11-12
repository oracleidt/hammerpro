use glam::DVec3;

use crate::{Document, EditError, EditResult, Id, ObjRef, SelectOp};

pub const COMBINED_VISGROUP: &str = "Combined props (originals)";

pub const SHARED_KEYS: [&str; 8] = [
    "disableshadows",
    "disablevertexlighting",
    "disableselfshadowing",
    "ignorenormals",
    "disableflashlight",
    "drawinfastreflection",
    "disableshadowdepth",
    "lightmapresolutionx",
];

pub const FADE_KEYS: [&str; 4] = ["fademindist", "fademaxdist", "fadescale", "screenspacefade"];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergedKeys {
    pub keys: Vec<(String, String)>,
    pub conflicts: Vec<String>,
    pub dropped: Vec<String>,
    pub solid: bool,
}

fn value(props: &[(String, String)], key: &str) -> String {
    props.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.trim().to_owned()).unwrap_or_default()
}

pub fn merge_prop_keys(props: &[&[(String, String)]]) -> MergedKeys {
    let mut out = MergedKeys::default();
    let normalized = |key: &str, v: String| -> String {
        if v.is_empty() && SHARED_KEYS.contains(&key) && key != "lightmapresolutionx" { "0".to_owned() } else { v }
    };
    for key in SHARED_KEYS {
        let values: Vec<String> = props.iter().map(|p| normalized(key, value(p, key))).collect();
        let first = values.first().cloned().unwrap_or_default();
        if values.iter().any(|v| *v != first) {
            out.conflicts.push(key.to_owned());
        } else if !first.is_empty() && first != "0" {
            out.keys.push((key.to_owned(), first));
        }
    }
    for key in FADE_KEYS {
        let values: Vec<String> = props.iter().map(|p| value(p, key)).collect();
        let first = values.first().cloned().unwrap_or_default();
        if values.iter().any(|v| *v != first) {
            out.dropped.push(key.to_owned());
        } else if !first.is_empty() {
            out.keys.push((key.to_owned(), first));
        }
    }
    out.solid = props.iter().any(|p| value(p, "solid") != "0");
    out.keys.push(("solid".to_owned(), if out.solid { "6" } else { "0" }.to_owned()));
    out
}

impl Document {
    pub fn combine_props(
        &mut self,
        props: &[Id],
        model: &str,
        origin: DVec3,
        keys: &[(String, String)],
        keep_originals: bool,
    ) -> EditResult<Id> {
        let existing: Vec<Id> = props.iter().copied().filter(|id| self.scene.entities.contains_key(id)).collect();
        if existing.len() < 2 {
            return Err(EditError::Invalid("select at least two props to combine".to_owned()));
        }
        let model = model.trim().replace('\\', "/");
        if model.is_empty() {
            return Err(EditError::Invalid("empty model path".to_owned()));
        }
        let model = if model.starts_with("models/") { model } else { format!("models/{model}") };
        let model = if model.ends_with(".mdl") { model } else { format!("{model}.mdl") };
        let mut kv: Vec<(String, String)> = vec![("model".to_owned(), model)];
        kv.extend(keys.iter().filter(|(k, _)| !k.eq_ignore_ascii_case("model")).cloned());
        let refs: Vec<ObjRef> = existing.iter().map(|id| ObjRef::Entity(*id)).collect();
        self.tx("Combine props", |doc| {
            let origin = DVec3::new(origin.x.round(), origin.y.round(), origin.z.round());
            let id = doc.create_point_entity("prop_static", origin, &kv)?;
            if keep_originals {
                let group = doc.scene.visgroups().iter().find(|g| g.name == COMBINED_VISGROUP).map(|g| g.id);
                let group = match group {
                    Some(g) => {
                        doc.add_to_visgroup(g, &refs)?;
                        g
                    }
                    None => doc.create_visgroup(COMBINED_VISGROUP, None, &refs)?,
                };
                doc.set_visgroup_visible(group, false)?;
            } else {
                doc.delete(&refs)?;
            }
            doc.select(&[ObjRef::Entity(id)], SelectOp::Replace);
            Ok(id)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
    }

    #[test]
    fn shared_lighting_flags_are_kept_and_conflicts_reported() {
        let a = kv(&[("disableshadows", "1"), ("fademaxdist", "800"), ("solid", "6")]);
        let b = kv(&[("disableshadows", "1"), ("fademaxdist", "800"), ("solid", "0")]);
        let m = merge_prop_keys(&[&a, &b]);
        assert!(m.conflicts.is_empty());
        assert!(m.keys.contains(&("disableshadows".to_owned(), "1".to_owned())));
        assert!(m.keys.contains(&("fademaxdist".to_owned(), "800".to_owned())));
        assert!(m.solid);
        let c = kv(&[("disableshadows", "0"), ("solid", "0")]);
        let d = kv(&[("solid", "0"), ("fademaxdist", "500")]);
        let m = merge_prop_keys(&[&c, &d]);
        assert!(m.conflicts.is_empty(), "a missing flag is the same as 0");
        assert_eq!(m.dropped, ["fademaxdist"]);
        assert!(!m.solid);
        let m = merge_prop_keys(&[&a, &c]);
        assert_eq!(m.conflicts, ["disableshadows"]);
    }

    #[test]
    fn combining_replaces_the_props_in_one_undo_step() {
        let mut doc = Document::new_empty();
        let a = doc.create_point_entity("prop_static", DVec3::ZERO, &kv(&[("model", "models/a.mdl")])).unwrap();
        let b = doc.create_point_entity("prop_static", DVec3::X * 64.0, &kv(&[("model", "models/b.mdl")])).unwrap();
        let before = hp_vmf::write(&doc.to_map());
        let id = doc.combine_props(&[a, b], "hammerpro/combined/x", DVec3::new(32.2, 0.0, 0.0), &[], true).unwrap();
        let e = &doc.scene().entity(id).unwrap().entity;
        assert_eq!(e.get("model"), Some("models/hammerpro/combined/x.mdl"));
        assert_eq!(e.origin(), Some(DVec3::new(32.0, 0.0, 0.0)));
        assert!(doc.scene().entity(a).is_some(), "the originals are kept");
        let group = doc.scene().visgroups().iter().find(|g| g.name == COMBINED_VISGROUP).unwrap().id;
        assert!(!doc.is_visgroup_visible(group));
        assert!(doc.undo());
        assert_eq!(hp_vmf::write(&doc.to_map()), before);
        doc.combine_props(&[a, b], "x", DVec3::ZERO, &[], false).unwrap();
        assert!(doc.scene().entity(a).is_none() && doc.scene().entity(b).is_none());
        assert!(doc.combine_props(&[a], "x", DVec3::ZERO, &[], false).is_err());
    }
}
