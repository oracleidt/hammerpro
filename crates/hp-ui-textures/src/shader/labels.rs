use hp_app::i18n::tr;

pub fn node_title(id: &str) -> &'static str {
    tr(&format!("shader.node.{id}"))
}

pub fn node_tip(id: &str) -> &'static str {
    tr(&format!("shader.node.{id}.tip"))
}

pub fn category(id: &str) -> &'static str {
    tr(&format!("shader.cat.{id}"))
}

pub fn pin(name: &str) -> &'static str {
    tr(&format!("shader.pin.{name}"))
}

pub fn param(name: &str) -> &'static str {
    tr(&format!("shader.param.{name}"))
}

pub fn option(name: &str) -> &'static str {
    tr(&format!("shader.opt.{name}"))
}

pub fn preset(id: &str) -> &'static str {
    tr(&format!("shader.preset.{id}"))
}

pub fn preset_tip(id: &str) -> &'static str {
    tr(&format!("shader.preset.{id}.tip"))
}

pub fn diagnostic(d: &hp_shader::Diagnostic) -> String {
    let key = format!("shader.diag.{}", d.code);
    if hp_app::i18n::has_key(&key) { hp_app::i18n::tf(&key, &[&d.detail]) } else { d.message() }
}

#[cfg(test)]
mod tests {
    use hp_app::i18n::has_key;
    use hp_shader::catalog::{CATEGORIES, NODES, ParamKind};

    #[test]
    fn every_label_is_translated() {
        crate::register_translations();
        let mut missing: Vec<String> = Vec::new();
        let mut need = |key: String| {
            if !has_key(&key) {
                missing.push(key);
            }
        };
        for c in CATEGORIES {
            need(format!("shader.cat.{c}"));
        }
        for n in NODES {
            need(format!("shader.node.{}", n.id));
            need(format!("shader.node.{}.tip", n.id));
            for p in n.inputs.iter().chain(n.outputs) {
                need(format!("shader.pin.{}", p.name));
            }
            for p in n.params {
                need(format!("shader.param.{}", p.name));
                if let ParamKind::Choice { options, .. } = p.kind {
                    for o in options {
                        need(format!("shader.opt.{o}"));
                    }
                }
            }
        }
        for p in hp_shader::PRESETS {
            need(format!("shader.preset.{}", p.id));
            need(format!("shader.preset.{}.tip", p.id));
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "{missing:#?}");
    }
}
