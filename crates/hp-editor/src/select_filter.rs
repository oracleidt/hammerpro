use crate::glob::{glob_match, material_match};
use crate::scene::{Owner, Scene};
use crate::selection::ObjRef;

pub const MAX_FILTER_LEN: usize = 512;
const MAX_TERMS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilterKind {
    Brushes,
    Entities,
    Props,
    Lights,
}

impl FilterKind {
    pub const ALL: [FilterKind; 4] = [FilterKind::Brushes, FilterKind::Entities, FilterKind::Props, FilterKind::Lights];

    pub fn id(self) -> &'static str {
        match self {
            FilterKind::Brushes => "brushes",
            FilterKind::Entities => "entities",
            FilterKind::Props => "props",
            FilterKind::Lights => "lights",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id().eq_ignore_ascii_case(id.trim()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Class,
    Name,
    Material,
    Model,
    Any,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Term {
    field: Field,
    patterns: Vec<String>,
    negate: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectFilter {
    text: String,
    terms: Vec<Term>,
    kinds: [bool; 4],
}

impl Default for SelectFilter {
    fn default() -> Self {
        Self { text: String::new(), terms: Vec::new(), kinds: [true; 4] }
    }
}

fn field_of(key: &str) -> Option<Field> {
    match key.to_ascii_lowercase().as_str() {
        "class" | "c" | "classname" => Some(Field::Class),
        "name" | "n" | "targetname" => Some(Field::Name),
        "material" | "mat" | "m" | "texture" | "tex" => Some(Field::Material),
        "model" | "mdl" => Some(Field::Model),
        _ => None,
    }
}

fn parse_terms(text: &str) -> Vec<Term> {
    let mut out = Vec::new();
    for raw in text.split_whitespace().take(MAX_TERMS) {
        let (negate, body) = match raw.strip_prefix(['-', '!']) {
            Some(rest) if !rest.is_empty() => (true, rest),
            _ => (false, raw),
        };
        let (field, value) = match body.split_once(':') {
            Some((k, v)) if field_of(k).is_some() => (field_of(k).unwrap_or(Field::Any), v),
            _ => (Field::Any, body),
        };
        let patterns: Vec<String> =
            value.split(',').map(|p| p.trim().trim_matches('"').replace('\\', "/")).filter(|p| !p.is_empty()).collect();
        if !patterns.is_empty() {
            out.push(Term { field, patterns, negate });
        }
    }
    out
}

fn is_light_class(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    c.starts_with("light") || c == "point_spotlight" || c == "env_projectedtexture"
}

fn is_prop_class(class: &str) -> bool {
    class.to_ascii_lowercase().starts_with("prop_")
}

fn contains_ci(text: &str, needle: &str) -> bool {
    text.to_ascii_lowercase().contains(&needle.to_ascii_lowercase())
}

struct Facts<'a> {
    class: &'a str,
    name: Option<&'a str>,
    model: Option<&'a str>,
    materials: Vec<&'a str>,
    kind: FilterKind,
}

impl SelectFilter {
    pub fn new(text: &str) -> Self {
        let mut f = Self::default();
        f.set_text(text);
        f
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: &str) {
        let clean: String = text.chars().filter(|c| !c.is_control()).take(MAX_FILTER_LEN).collect();
        self.terms = parse_terms(&clean);
        self.text = clean;
    }

    pub fn kind_enabled(&self, kind: FilterKind) -> bool {
        self.kinds[kind as usize]
    }

    pub fn set_kind(&mut self, kind: FilterKind, on: bool) {
        self.kinds[kind as usize] = on;
    }

    pub fn set_kinds(&mut self, kinds: &[FilterKind]) {
        self.kinds = [false; 4];
        for k in kinds {
            self.kinds[*k as usize] = true;
        }
    }

    pub fn enabled_kinds(&self) -> Vec<FilterKind> {
        FilterKind::ALL.into_iter().filter(|k| self.kind_enabled(*k)).collect()
    }

    pub fn has_terms(&self) -> bool {
        !self.terms.is_empty()
    }

    pub fn all_kinds(&self) -> bool {
        self.kinds.iter().all(|k| *k)
    }

    pub fn is_active(&self) -> bool {
        self.has_terms() || !self.all_kinds()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.has_terms() {
            parts.push(self.text.trim().to_owned());
        }
        if !self.all_kinds() {
            let kinds: Vec<&str> = self.enabled_kinds().into_iter().map(FilterKind::id).collect();
            parts.push(if kinds.is_empty() { "-".to_owned() } else { kinds.join("+") });
        }
        parts.join("  ")
    }

    fn facts<'a>(&self, scene: &'a Scene, obj: ObjRef) -> Option<Facts<'a>> {
        match obj {
            ObjRef::Solid(id) => {
                let node = scene.solid(id)?;
                let materials: Vec<&str> = node.solid.sides.iter().map(|s| s.material.as_str()).collect();
                match node.owner {
                    Owner::World => Some(Facts {
                        class: "worldspawn",
                        name: None,
                        model: None,
                        materials,
                        kind: FilterKind::Brushes,
                    }),
                    Owner::Entity(e) => {
                        let ent = &scene.entity(e)?.entity;
                        Some(Facts {
                            class: &ent.classname,
                            name: ent.get("targetname"),
                            model: ent.get("model"),
                            materials,
                            kind: Self::entity_kind(&ent.classname),
                        })
                    }
                }
            }
            ObjRef::Entity(id) => {
                let ent = &scene.entity(id)?.entity;
                let mut materials = Vec::new();
                for sid in scene.entity_solid_ids(id) {
                    if let Some(n) = scene.solid(sid) {
                        materials.extend(n.solid.sides.iter().map(|s| s.material.as_str()));
                    }
                }
                Some(Facts {
                    class: &ent.classname,
                    name: ent.get("targetname"),
                    model: ent.get("model"),
                    materials,
                    kind: Self::entity_kind(&ent.classname),
                })
            }
        }
    }

    fn entity_kind(class: &str) -> FilterKind {
        if is_light_class(class) {
            FilterKind::Lights
        } else if is_prop_class(class) {
            FilterKind::Props
        } else {
            FilterKind::Entities
        }
    }

    fn term_hits(term: &Term, f: &Facts<'_>) -> bool {
        term.patterns.iter().any(|p| match term.field {
            Field::Class => glob_match(p, f.class),
            Field::Name => f.name.is_some_and(|n| glob_match(p, n)),
            Field::Material => f.materials.iter().any(|m| material_match(p, m)),
            Field::Model => f.model.is_some_and(|m| {
                let m = m.replace('\\', "/");
                glob_match(p, &m) || (!p.contains(['*', '?']) && contains_ci(&m, p))
            }),
            Field::Any => {
                if p.contains(['*', '?']) {
                    glob_match(p, f.class) || f.name.is_some_and(|n| glob_match(p, n))
                } else {
                    contains_ci(f.class, p) || f.name.is_some_and(|n| contains_ci(n, p))
                }
            }
        })
    }

    pub fn matches(&self, scene: &Scene, obj: ObjRef) -> bool {
        if !self.is_active() {
            return true;
        }
        let Some(f) = self.facts(scene, obj) else { return false };
        if !self.kind_enabled(f.kind) {
            return false;
        }
        for field in [Field::Class, Field::Name, Field::Material, Field::Model, Field::Any] {
            let mut positives = self.terms.iter().filter(|t| t.field == field && !t.negate).peekable();
            if positives.peek().is_some() && !positives.any(|t| Self::term_hits(t, &f)) {
                return false;
            }
        }
        !self.terms.iter().filter(|t| t.negate).any(|t| Self::term_hits(t, &f))
    }

    pub fn retain(&self, scene: &Scene, objs: &mut Vec<ObjRef>) {
        if self.is_active() {
            objs.retain(|o| self.matches(scene, *o));
        }
    }
}
