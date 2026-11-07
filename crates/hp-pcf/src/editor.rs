use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::dmx::{self, Dmx, Element, Reference, Value};
use crate::{PcfError, SystemDef};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Emitters,
    Initializers,
    Operators,
    Forces,
    Constraints,
    Renderers,
}

impl Category {
    pub const ALL: [Self; 6] =
        [Self::Emitters, Self::Initializers, Self::Operators, Self::Forces, Self::Constraints, Self::Renderers];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Emitters => "emitters",
            Self::Initializers => "initializers",
            Self::Operators => "operators",
            Self::Forces => "forces",
            Self::Constraints => "constraints",
            Self::Renderers => "renderers",
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str().eq_ignore_ascii_case(name))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Fire,
    Smoke,
    Sparks,
    Rain,
    Explosion,
}

impl Preset {
    pub const ALL: [Self; 5] = [Self::Fire, Self::Smoke, Self::Sparks, Self::Rain, Self::Explosion];
    pub fn name(self) -> &'static str {
        match self {
            Self::Fire => "fire",
            Self::Smoke => "smoke",
            Self::Sparks => "sparks",
            Self::Rain => "rain",
            Self::Explosion => "explosion",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error("element {0} does not exist")]
    MissingElement(usize),
    #[error("element {0} is not a particle system")]
    NotSystem(usize),
    #[error("the name must be nonempty and contain no NUL characters")]
    InvalidName,
    #[error("a particle system named {0} already exists")]
    DuplicateName(String),
    #[error("operator index is out of range")]
    InvalidIndex,
    #[error("the operator clipboard is empty")]
    EmptyClipboard,
    #[error("child particle systems cannot contain a cycle")]
    ChildCycle,
    #[error("the value is invalid or has an invalid element reference")]
    InvalidValue,
    #[error("{function}: parameter {parameter} requires DMX type {expected}")]
    ParameterType { function: String, parameter: String, expected: &'static str },
    #[error("the file name must be a simple .pcf file name")]
    InvalidFileName,
    #[error("the document is too large to retain an undoable edit")]
    TooLarge,
    #[error(transparent)]
    Pcf(#[from] PcfError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub element: usize,
    pub attribute: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct ExportResult {
    pub pcf: PathBuf,
    pub manifest: PathBuf,
    pub lua: String,
}

#[derive(Clone, Debug)]
struct Snapshot {
    data: Dmx,
    label: String,
    bytes: usize,
}

#[derive(Clone, Debug)]
pub struct Document {
    data: Dmx,
    saved: Dmx,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    clipboard: Option<(Dmx, usize)>,
    revision: u64,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    pub fn new() -> Self {
        Self::from_dmx(Dmx {
            version: 5,
            elements: vec![Element {
                kind: "DmElement".into(),
                name: "particleSystemDefinitions".into(),
                attrs: vec![("particleSystemDefinitions".into(), Value::Elements(Vec::new()))],
            }],
            ids: vec![dmx::generated_id(0)],
        })
    }
    pub fn from_dmx(data: Dmx) -> Self {
        Self { saved: data.clone(), data, undo: Vec::new(), redo: Vec::new(), clipboard: None, revision: 0 }
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, PcfError> {
        Ok(Self::from_dmx(dmx::parse(bytes)?))
    }
    pub fn dmx(&self) -> &Dmx {
        &self.data
    }
    pub fn element(&self, id: usize) -> Option<&Element> {
        self.data.elements.get(id)
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn is_modified(&self) -> bool {
        self.data != self.saved
    }
    pub fn mark_saved(&mut self) {
        self.saved = self.data.clone();
    }
    pub fn discard_changes(&mut self) -> bool {
        if !self.is_modified() {
            return false;
        }
        self.data.clone_from(&self.saved);
        self.undo.clear();
        self.redo.clear();
        self.clipboard = None;
        self.revision = self.revision.wrapping_add(1);
        true
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, PcfError> {
        dmx::write(&self.data)
    }
    pub fn definitions(&self) -> Vec<SystemDef> {
        crate::model::systems(&self.data)
    }
    pub fn systems(&self) -> Vec<usize> {
        self.data
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == "DmeParticleSystemDefinition")
            .map(|(i, _)| i)
            .collect()
    }
    pub fn system_name(&self, id: usize) -> Option<&str> {
        self.data.elements.get(id).filter(|e| e.kind == "DmeParticleSystemDefinition").map(|e| e.name.as_str())
    }
    pub fn find_system(&self, name: &str) -> Option<usize> {
        self.systems().into_iter().find(|&i| self.data.elements[i].name.eq_ignore_ascii_case(name))
    }
    fn require_system(&self, id: usize) -> Result<(), EditError> {
        self.system_name(id).map(|_| ()).ok_or(EditError::NotSystem(id))
    }
    fn check_name(&self, name: &str, except: Option<usize>) -> Result<(), EditError> {
        if name.trim().is_empty() || name.contains('\0') {
            return Err(EditError::InvalidName);
        }
        if self
            .systems()
            .into_iter()
            .any(|i| Some(i) != except && self.data.elements[i].name.eq_ignore_ascii_case(name))
        {
            return Err(EditError::DuplicateName(name.to_owned()));
        }
        Ok(())
    }
    fn push_element(&mut self, element: Element) -> usize {
        let index = self.data.elements.len();
        self.data.elements.push(element);
        self.data.ids.push(dmx::generated_id(index));
        index
    }
    fn checkpoint(&mut self, label: &str) -> Result<(), EditError> {
        let bytes = estimate(&self.data);
        if bytes > crate::MAX_FILE_SIZE * 2 {
            return Err(EditError::TooLarge);
        }
        while !self.undo.is_empty()
            && (self.undo.len() >= 128 || self.undo.iter().map(|s| s.bytes).sum::<usize>() + bytes > 32 << 20)
        {
            self.undo.remove(0);
        }
        self.undo.push(Snapshot { data: self.data.clone(), label: label.into(), bytes });
        self.redo.clear();
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|s| s.label.as_str())
    }
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.label.as_str())
    }
    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.undo.pop() else {
            return false;
        };
        let bytes = estimate(&self.data);
        self.redo.push(Snapshot {
            data: std::mem::replace(&mut self.data, snapshot.data),
            label: snapshot.label,
            bytes,
        });
        self.revision = self.revision.wrapping_add(1);
        true
    }
    pub fn redo(&mut self) -> bool {
        let Some(snapshot) = self.redo.pop() else {
            return false;
        };
        let bytes = estimate(&self.data);
        self.undo.push(Snapshot {
            data: std::mem::replace(&mut self.data, snapshot.data),
            label: snapshot.label,
            bytes,
        });
        self.revision = self.revision.wrapping_add(1);
        true
    }
    pub fn set_attr(&mut self, element: usize, key: &str, value: Value) -> Result<(), EditError> {
        let current = self.data.elements.get(element).ok_or(EditError::MissingElement(element))?;
        if key.is_empty() || key.contains('\0') || !valid_value(&value, self.data.elements.len()) {
            return Err(EditError::InvalidValue);
        }
        let value = normalize_parameter(current.str("functionName"), key, value)?;
        if current.get(key) == Some(&value) {
            return Ok(());
        }
        self.checkpoint("Change parameter")?;
        put(&mut self.data.elements[element], key, value);
        Ok(())
    }
    pub fn remove_attr(&mut self, element: usize, key: &str) -> Result<(), EditError> {
        let current = self.data.elements.get(element).ok_or(EditError::MissingElement(element))?;
        if current.get(key).is_none() {
            return Ok(());
        }
        self.checkpoint("Remove parameter")?;
        self.data.elements[element].attrs.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
        Ok(())
    }
    pub fn set_attrs(&mut self, element: usize, values: Vec<(String, Value)>) -> Result<(), EditError> {
        let current = self.data.elements.get(element).ok_or(EditError::MissingElement(element))?;
        let mut keys = HashSet::new();
        if values.iter().any(|(key, value)| {
            key.is_empty()
                || key.contains('\0')
                || !keys.insert(key.to_ascii_lowercase())
                || !valid_value(value, self.data.elements.len())
        }) {
            return Err(EditError::InvalidValue);
        }
        let function = values
            .iter()
            .find_map(|(key, value)| {
                if key.eq_ignore_ascii_case("functionName")
                    && let Value::Str(function) = value
                {
                    return Some(function.as_str());
                }
                None
            })
            .or_else(|| current.str("functionName"));
        let values = values
            .iter()
            .map(|(key, value)| normalize_parameter(function, key, value.clone()).map(|value| (key.clone(), value)))
            .collect::<Result<Vec<_>, _>>()?;
        if values.iter().all(|(key, value)| current.get(key) == Some(value)) {
            return Ok(());
        }
        self.checkpoint("Change parameters")?;
        for (key, value) in values {
            put(&mut self.data.elements[element], &key, value);
        }
        Ok(())
    }
    fn attach_system(&mut self, id: usize) {
        let root =
            self.data.elements.iter().position(|e| e.get("particleSystemDefinitions").is_some()).unwrap_or_else(|| {
                self.push_element(Element {
                    kind: "DmElement".into(),
                    name: "particleSystemDefinitions".into(),
                    attrs: Vec::new(),
                })
            });
        let mut definitions = self.data.elements[root].elements("particleSystemDefinitions");
        if !definitions.contains(&id) {
            definitions.push(id);
        }
        put(
            &mut self.data.elements[root],
            "particleSystemDefinitions",
            Value::Elements(definitions.into_iter().map(Some).collect()),
        );
    }
    pub fn create_system(&mut self, name: &str) -> Result<usize, EditError> {
        self.check_name(name, None)?;
        self.checkpoint("Create system")?;
        let mut attrs = vec![
            ("max_particles".into(), Value::Int(1000)),
            ("initial_particles".into(), Value::Int(0)),
            ("material".into(), Value::Str("particle/particle_smokegrenade".into())),
            ("radius".into(), Value::Float(5.0)),
            ("color".into(), Value::Color([255; 4])),
            ("bounding_box_min".into(), Value::Vec3([-64.0; 3])),
            ("bounding_box_max".into(), Value::Vec3([64.0; 3])),
        ];
        attrs.extend(Category::ALL.into_iter().map(|c| (c.as_str().into(), Value::Elements(Vec::new()))));
        attrs.push(("children".into(), Value::Elements(Vec::new())));
        let id = self.push_element(Element { kind: "DmeParticleSystemDefinition".into(), name: name.into(), attrs });
        self.attach_system(id);
        Ok(id)
    }
    pub fn rename_system(&mut self, id: usize, name: &str) -> Result<(), EditError> {
        self.require_system(id)?;
        self.check_name(name, Some(id))?;
        if self.data.elements[id].name == name {
            return Ok(());
        }
        self.checkpoint("Rename system")?;
        self.data.elements[id].name = name.into();
        Ok(())
    }
    pub fn duplicate_system(&mut self, id: usize, name: &str) -> Result<usize, EditError> {
        self.require_system(id)?;
        self.check_name(name, None)?;
        self.checkpoint("Duplicate system")?;
        let source = self.data.clone();
        let duplicate = self.clone_graph(&source, id, &mut HashMap::new(), true);
        self.data.elements[duplicate].name = name.into();
        self.attach_system(duplicate);
        Ok(duplicate)
    }
    pub fn delete_system(&mut self, id: usize) -> Result<(), EditError> {
        self.require_system(id)?;
        self.checkpoint("Delete system")?;
        let wrappers: HashSet<usize> = self
            .data
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == "DmeParticleChild" && e.elements("child").contains(&id))
            .map(|(i, _)| i)
            .collect();
        for e in &mut self.data.elements {
            for (_, v) in &mut e.attrs {
                remove_reference(v, id);
                for &wrapper in &wrappers {
                    remove_reference(v, wrapper);
                }
            }
        }
        self.data.elements[id] = Element { kind: "DmElement".into(), name: String::new(), attrs: Vec::new() };
        for wrapper in wrappers {
            self.data.elements[wrapper] = Element { kind: "DmElement".into(), name: String::new(), attrs: Vec::new() };
        }
        Ok(())
    }
    pub fn operators(&self, system: usize, category: Category) -> Vec<usize> {
        self.data
            .elements
            .get(system)
            .filter(|e| e.kind == "DmeParticleSystemDefinition")
            .map_or_else(Vec::new, |e| e.elements(category.as_str()))
    }
    pub fn add_operator(&mut self, system: usize, category: Category, function: &str) -> Result<usize, EditError> {
        self.add_operator_with_params(system, category, function, Vec::new())
    }
    pub fn add_operator_with_params(
        &mut self,
        system: usize,
        category: Category,
        function: &str,
        attrs: Vec<(String, Value)>,
    ) -> Result<usize, EditError> {
        self.require_system(system)?;
        if function.trim().is_empty() || function.contains('\0') {
            return Err(EditError::InvalidName);
        }
        if attrs.iter().any(|(key, value)| {
            key.is_empty()
                || key.contains('\0')
                || key.eq_ignore_ascii_case("functionName")
                || !valid_value(value, self.data.elements.len())
        }) {
            return Err(EditError::InvalidValue);
        }
        let mut keys = HashSet::new();
        if attrs.iter().any(|(key, _)| !keys.insert(key.to_ascii_lowercase())) {
            return Err(EditError::InvalidValue);
        }
        let attrs = attrs
            .into_iter()
            .map(|(key, value)| normalize_parameter(Some(function), &key, value).map(|value| (key, value)))
            .collect::<Result<Vec<_>, _>>()?;
        self.checkpoint("Add operator")?;
        let mut params = vec![("functionName".into(), Value::Str(function.into()))];
        params.extend(attrs);
        let id =
            self.push_element(Element { kind: "DmeParticleOperator".into(), name: function.into(), attrs: params });
        let mut ops = self.operators(system, category);
        ops.push(id);
        self.set_ops(system, category, ops);
        Ok(id)
    }
    fn set_ops(&mut self, system: usize, category: Category, ops: Vec<usize>) {
        put(&mut self.data.elements[system], category.as_str(), Value::Elements(ops.into_iter().map(Some).collect()));
    }
    pub fn remove_operator(&mut self, system: usize, category: Category, index: usize) -> Result<(), EditError> {
        self.require_system(system)?;
        let mut ops = self.operators(system, category);
        if index >= ops.len() {
            return Err(EditError::InvalidIndex);
        }
        self.checkpoint("Remove operator")?;
        ops.remove(index);
        self.set_ops(system, category, ops);
        Ok(())
    }
    pub fn move_operator(
        &mut self,
        system: usize,
        category: Category,
        from: usize,
        to: usize,
    ) -> Result<(), EditError> {
        self.require_system(system)?;
        let mut ops = self.operators(system, category);
        if from >= ops.len() || to >= ops.len() {
            return Err(EditError::InvalidIndex);
        }
        if from == to {
            return Ok(());
        }
        self.checkpoint("Move operator")?;
        let id = ops.remove(from);
        ops.insert(to, id);
        self.set_ops(system, category, ops);
        Ok(())
    }
    pub fn copy_operator(&mut self, id: usize) -> Result<(), EditError> {
        self.data.elements.get(id).ok_or(EditError::MissingElement(id))?;
        self.clipboard = Some((self.data.clone(), id));
        Ok(())
    }
    pub fn has_clipboard(&self) -> bool {
        self.clipboard.is_some()
    }
    pub fn paste_operator(&mut self, system: usize, category: Category) -> Result<usize, EditError> {
        self.require_system(system)?;
        let (source, id) = self.clipboard.clone().ok_or(EditError::EmptyClipboard)?;
        self.checkpoint("Paste operator")?;
        let pasted = self.clone_graph(&source, id, &mut HashMap::new(), true);
        let mut ops = self.operators(system, category);
        ops.push(pasted);
        self.set_ops(system, category, ops);
        Ok(pasted)
    }
    fn clone_graph(&mut self, source: &Dmx, id: usize, map: &mut HashMap<usize, usize>, force: bool) -> usize {
        if let Some(&cloned) = map.get(&id) {
            return cloned;
        }
        let mut queue = vec![(id, force)];
        let mut pending = Vec::new();
        while let Some((next, force)) = queue.pop() {
            if map.contains_key(&next) {
                continue;
            }
            let element = &source.elements[next];
            if !force
                && element.kind == "DmeParticleSystemDefinition"
                && let Some(existing) = self.find_system(&element.name)
            {
                map.insert(next, existing);
                continue;
            }
            let cloned = self.push_element(element.clone());
            map.insert(next, cloned);
            pending.push((next, cloned));
            for (_, value) in &element.attrs {
                queue.extend(references(value).into_iter().map(|r| (r, false)));
            }
        }
        for (original, cloned) in pending {
            let mut attrs = source.elements[original].attrs.clone();
            for (_, value) in &mut attrs {
                visit_references(value, |reference| *reference = map[reference]);
            }
            self.data.elements[cloned].attrs = attrs;
        }
        map[&id]
    }
    pub fn import(&mut self, source: &Document) -> Result<Vec<usize>, EditError> {
        self.checkpoint("Import systems")?;
        let offset = self.data.elements.len();
        let mut imported = source.data.elements.clone();
        let mut names: HashSet<String> =
            self.systems().into_iter().map(|i| self.data.elements[i].name.to_ascii_lowercase()).collect();
        for element in &mut imported {
            for (_, value) in &mut element.attrs {
                visit_references(value, |reference| *reference += offset);
            }
            if element.kind == "DmeParticleSystemDefinition" {
                let base = element.name.clone();
                let mut suffix = 2;
                while names.contains(&element.name.to_ascii_lowercase()) {
                    element.name = format!("{base}_{suffix}");
                    suffix += 1;
                }
                names.insert(element.name.to_ascii_lowercase());
            }
        }
        let ids: Vec<usize> = imported
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == "DmeParticleSystemDefinition")
            .map(|(i, _)| offset + i)
            .collect();
        for e in imported {
            self.push_element(e);
        }
        for &id in &ids {
            self.attach_system(id);
        }
        Ok(ids)
    }
    pub fn children(&self, system: usize) -> Vec<usize> {
        self.data.elements.get(system).map_or_else(Vec::new, |e| e.elements("children"))
    }
    fn reaches(&self, from: usize, target: usize, seen: &mut HashSet<usize>) -> bool {
        let mut queue = vec![from];
        while let Some(id) = queue.pop() {
            if id == target {
                return true;
            }
            if !seen.insert(id) {
                continue;
            }
            for child in self.children(id) {
                if let Some(e) = self.data.elements.get(child) {
                    queue.extend(e.elements("child"));
                }
            }
        }
        false
    }
    pub fn add_child(&mut self, system: usize, child: usize, delay: f32) -> Result<usize, EditError> {
        self.require_system(system)?;
        self.require_system(child)?;
        if !delay.is_finite() || delay < 0.0 {
            return Err(EditError::InvalidValue);
        }
        if self.reaches(child, system, &mut HashSet::new()) {
            return Err(EditError::ChildCycle);
        }
        self.checkpoint("Add child")?;
        let id = self.push_element(Element {
            kind: "DmeParticleChild".into(),
            name: self.data.elements[child].name.clone(),
            attrs: vec![
                ("child".into(), Value::Element(Some(child))),
                ("delay".into(), Value::Float(delay)),
                ("end cap".into(), Value::Bool(false)),
            ],
        });
        let mut children = self.children(system);
        children.push(id);
        put(&mut self.data.elements[system], "children", Value::Elements(children.into_iter().map(Some).collect()));
        Ok(id)
    }
    pub fn remove_child(&mut self, system: usize, index: usize) -> Result<(), EditError> {
        self.require_system(system)?;
        let mut children = self.children(system);
        if index >= children.len() {
            return Err(EditError::InvalidIndex);
        }
        self.checkpoint("Remove child")?;
        children.remove(index);
        put(&mut self.data.elements[system], "children", Value::Elements(children.into_iter().map(Some).collect()));
        Ok(())
    }
    pub fn validate(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let mut issue = |severity, element, attribute: Option<&str>, message: String| {
            issues.push(ValidationIssue { severity, element, attribute: attribute.map(str::to_owned), message });
        };
        let mut names = HashSet::new();
        for (id, e) in self.data.elements.iter().enumerate() {
            if e.kind == "DmeParticleSystemDefinition" {
                if e.name.trim().is_empty() || !names.insert(e.name.to_ascii_lowercase()) {
                    issue(Severity::Error, id, None, "Empty or duplicate particle system name".into());
                }
                if e.int("max_particles").is_some_and(|n| !(1..=100_000).contains(&n)) {
                    issue(
                        Severity::Warning,
                        id,
                        Some("max_particles"),
                        "Particle count is outside the preview's supported range (1 to 100000)".into(),
                    );
                }
                if e.str("material").is_none_or(str::is_empty) {
                    issue(Severity::Warning, id, Some("material"), "No particle material is assigned".into());
                }
                for category in Category::ALL {
                    for op in e.elements(category.as_str()) {
                        if let Some(operator) = self.data.elements.get(op) {
                            if operator.str("functionName").is_none_or(str::is_empty) {
                                issue(
                                    Severity::Error,
                                    op,
                                    Some("functionName"),
                                    "Operator has no function name".into(),
                                );
                            }
                        } else {
                            issue(Severity::Error, id, Some(category.as_str()), "Operator reference is invalid".into());
                        }
                    }
                }
                for child in self.children(id) {
                    if let Some(child) = self.data.elements.get(child) {
                        for descendant in child.elements("child") {
                            if self.reaches(descendant, id, &mut HashSet::new()) {
                                issue(
                                    Severity::Error,
                                    id,
                                    Some("children"),
                                    "Child particle systems contain a cycle".into(),
                                );
                            }
                        }
                    }
                }
            }
            let mut keys = HashSet::new();
            for (key, value) in &e.attrs {
                if !keys.insert(key.to_ascii_lowercase()) {
                    issue(Severity::Error, id, Some(key), "Duplicate parameter name".into());
                }
                if !valid_value(value, self.data.elements.len()) {
                    issue(Severity::Error, id, Some(key), "Invalid parameter value or element reference".into());
                }
                if let Some(function) = e.str("functionName")
                    && let Some(expected) = crate::parameters::expected(function, key)
                    && !expected.accepts(value)
                {
                    issue(
                        Severity::Warning,
                        id,
                        Some(key),
                        format!(
                            "Source expects DMX type {} for this parameter; the game may ignore the value",
                            expected.name()
                        ),
                    );
                }
                if (key.to_ascii_lowercase().contains("control point")
                    || key.to_ascii_lowercase().contains("control_point"))
                    && let Value::Int(cp) = value
                    && !(0..=63).contains(cp)
                {
                    issue(
                        Severity::Warning,
                        id,
                        Some(key),
                        "Control point index is outside Source's range (0 to 63)".into(),
                    );
                }
                let counterpart = key
                    .strip_suffix("_min")
                    .map(|s| format!("{s}_max"))
                    .or_else(|| key.strip_suffix(" min").map(|s| format!("{s} max")));
                if let Some(max) = counterpart
                    && let (Some(min), Some(max)) = (e.float(key), e.float(&max))
                    && min > max
                {
                    issue(Severity::Warning, id, Some(key), "Minimum value exceeds maximum value".into());
                }
            }
        }
        for definition in self.definitions() {
            let plan = crate::Plan::new(&definition);
            for function in plan.unsupported {
                let element = self.find_system(&definition.name).unwrap_or(0);
                issue(
                    Severity::Warning,
                    element,
                    None,
                    format!("Operator is preserved for Source but has no preview implementation: {function}"),
                );
            }
        }
        issues
    }
    pub fn validate_materials(&self, exists: impl Fn(&str) -> bool) -> Vec<ValidationIssue> {
        self.systems()
            .into_iter()
            .filter_map(|element| {
                let material = self.data.elements[element].str("material")?;
                (!material.is_empty() && !exists(material)).then(|| ValidationIssue {
                    severity: Severity::Warning,
                    element,
                    attribute: Some("material".into()),
                    message: format!("Material is not mounted: {material}"),
                })
            })
            .collect()
    }
    pub fn export_addon(&self, root: &Path, file_name: &str) -> Result<ExportResult, EditError> {
        if file_name.is_empty()
            || !file_name.to_ascii_lowercase().ends_with(".pcf")
            || file_name.contains(['/', '\\', '\0', '"', '\n', '\r', ':'])
            || file_name == ".pcf"
        {
            return Err(EditError::InvalidFileName);
        }
        let bytes = self.to_bytes()?;
        let directory = root.join("particles");
        std::fs::create_dir_all(&directory)?;
        let pcf = directory.join(file_name);
        let manifest = directory.join("particles_manifest.txt");
        let old = match std::fs::read_to_string(&manifest) {
            Ok(old) => old,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.into()),
        };
        let entry = format!("particles/{file_name}");
        let next = update_manifest(&old, &entry)?;
        atomic_write(&pcf, &bytes)?;
        atomic_write(&manifest, next.as_bytes())?;
        let mut lua = format!("game.AddParticles(\"particles/{file_name}\")");
        for id in self.systems() {
            let escaped = self.data.elements[id]
                .name
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r");
            lua.push_str(&format!("\nPrecacheParticleSystem(\"{escaped}\")"));
        }
        Ok(ExportResult { pcf, manifest, lua })
    }
    pub fn preset(preset: Preset) -> Self {
        let mut doc = Self::new();
        let system = doc.create_system(preset.name()).expect("valid preset name");
        let (life, radius, rate, velocity, color, material) = match preset {
            Preset::Fire => (0.8, 10.0, 60.0, [0.0, 0.0, 65.0], [255, 135, 25, 255], "sprites/flamelet1"),
            Preset::Smoke => {
                (3.0, 16.0, 24.0, [0.0, 0.0, 28.0], [140, 140, 140, 180], "particle/particle_smokegrenade")
            }
            Preset::Sparks => (0.7, 1.5, 40.0, [0.0, 0.0, 120.0], [255, 210, 100, 255], "effects/spark"),
            Preset::Rain => (2.0, 1.0, 250.0, [0.0, 0.0, -500.0], [180, 200, 220, 180], "effects/spark"),
            Preset::Explosion => {
                (1.2, 24.0, 100.0, [0.0, 0.0, 45.0], [255, 180, 75, 255], "particle/particle_smokegrenade")
            }
        };
        let set = |doc: &mut Self, id, key: &str, value| put(&mut doc.data.elements[id], key, value);
        set(&mut doc, system, "material", Value::Str(material.into()));
        let emitter = doc
            .add_operator(
                system,
                Category::Emitters,
                if preset == Preset::Explosion { "emit_instantaneously" } else { "emit_continuously" },
            )
            .expect("preset emitter");
        set(
            &mut doc,
            emitter,
            if preset == Preset::Explosion { "num_to_emit" } else { "emission_rate" },
            if preset == Preset::Explosion { Value::Int(rate as i32) } else { Value::Float(rate) },
        );
        let lifetime = doc.add_operator(system, Category::Initializers, "Lifetime Random").expect("preset initializer");
        set(&mut doc, lifetime, "lifetime_min", Value::Float(life * 0.75));
        set(&mut doc, lifetime, "lifetime_max", Value::Float(life));
        let size = doc.add_operator(system, Category::Initializers, "Radius Random").expect("preset initializer");
        set(&mut doc, size, "radius_min", Value::Float(radius * 0.5));
        set(&mut doc, size, "radius_max", Value::Float(radius));
        let tint = doc.add_operator(system, Category::Initializers, "Color Random").expect("preset initializer");
        set(&mut doc, tint, "color1", Value::Color(color));
        set(&mut doc, tint, "color2", Value::Color(color));
        let position = doc
            .add_operator(system, Category::Initializers, "Position Within Sphere Random")
            .expect("preset initializer");
        set(&mut doc, position, "distance_max", Value::Float(if preset == Preset::Rain { 240.0 } else { 8.0 }));
        set(&mut doc, position, "speed_in_local_coordinate_system_min", Value::Vec3(velocity));
        set(&mut doc, position, "speed_in_local_coordinate_system_max", Value::Vec3(velocity));
        if preset == Preset::Explosion || preset == Preset::Sparks {
            set(&mut doc, position, "speed_min", Value::Float(40.0));
            set(&mut doc, position, "speed_max", Value::Float(160.0));
        }
        let movement = doc.add_operator(system, Category::Operators, "Movement Basic").expect("preset operator");
        set(
            &mut doc,
            movement,
            "gravity",
            Value::Vec3(if preset == Preset::Sparks { [0.0, 0.0, -250.0] } else { [0.0; 3] }),
        );
        doc.add_operator(system, Category::Operators, "Alpha Fade and Decay").expect("preset operator");
        doc.add_operator(
            system,
            Category::Renderers,
            if preset == Preset::Sparks || preset == Preset::Rain {
                "render_sprite_trail"
            } else {
                "render_animated_sprites"
            },
        )
        .expect("preset renderer");
        doc.undo.clear();
        doc.redo.clear();
        doc.revision = 1;
        doc
    }
}

fn normalize_parameter(function: Option<&str>, key: &str, value: Value) -> Result<Value, EditError> {
    let Some(function) = function else {
        return Ok(value);
    };
    if function.eq_ignore_ascii_case("Render models")
        && key.eq_ignore_ascii_case("sequence 0 model")
        && let Value::Str(path) = &value
    {
        let path = path.trim().replace('\\', "/");
        let path = if path.get(..7).is_some_and(|prefix| prefix.eq_ignore_ascii_case("models/")) {
            path[7..].to_owned()
        } else {
            path
        };
        return Ok(Value::Str(path));
    }
    let Some(expected) = crate::parameters::expected(function, key) else {
        return Ok(value);
    };
    expected.normalize(value).map_err(|kind| EditError::ParameterType {
        function: function.into(),
        parameter: key.into(),
        expected: kind.name(),
    })
}

fn put(element: &mut Element, key: &str, value: Value) {
    if let Some((_, current)) = element.attrs.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
        *current = value;
    } else {
        element.attrs.push((key.into(), value));
    }
}

fn valid_value(value: &Value, count: usize) -> bool {
    let reference = |r: &Option<usize>| r.is_none_or(|id| id < count);
    match value {
        Value::Element(r) => reference(r),
        Value::Elements(v) => v.iter().all(reference),
        Value::External(s) | Value::Str(s) => !s.contains('\0'),
        Value::References(v) => v.iter().all(|r| match r {
            Reference::Element(r) => reference(r),
            Reference::External(s) => !s.contains('\0'),
        }),
        Value::Float(f) => f.is_finite(),
        Value::Vec2(v) => v.iter().all(|f| f.is_finite()),
        Value::Vec3(v) => v.iter().all(|f| f.is_finite()),
        Value::Vec4(v) => v.iter().all(|f| f.is_finite()),
        Value::Floats(v) => v.iter().all(|f| f.is_finite()),
        Value::Vec3s(v) => v.iter().flatten().all(|f| f.is_finite()),
        Value::Strs(v) => v.iter().all(|s| !s.contains('\0')),
        Value::Other => false,
        Value::Raw { kind, data } => valid_raw(*kind, data),
        _ => true,
    }
}

fn valid_raw(kind: u8, data: &[u8]) -> bool {
    let first = || data.get(..4).and_then(|b| <[u8; 4]>::try_from(b).ok()).map(i32::from_le_bytes);
    match kind {
        6 => first().is_some_and(|n| n >= 0 && n as usize == data.len() - 4),
        7 => data.len() == 4 || data.len() == 16,
        12 => data.len() == 12,
        13 => data.len() == 16,
        14 => data.len() == 64,
        18 | 21..=23 | 25..=28 => {
            let Some(n) = first() else { return false };
            let size = match kind {
                18 => 1,
                21 | 22 => 4,
                23 => 8,
                25 | 27 => 16,
                26 => 12,
                28 => 64,
                _ => return false,
            };
            n >= 0 && (n as usize).checked_mul(size).and_then(|n| n.checked_add(4)) == Some(data.len())
        }
        20 => {
            let Some(n) = first() else { return false };
            if n < 0 {
                return false;
            }
            let mut p = 4usize;
            for _ in 0..n {
                let Some(len) =
                    data.get(p..p.saturating_add(4)).and_then(|b| <[u8; 4]>::try_from(b).ok()).map(i32::from_le_bytes)
                else {
                    return false;
                };
                if len < 0 {
                    return false;
                }
                let Some(end) = p.checked_add(4).and_then(|p| p.checked_add(len as usize)) else {
                    return false;
                };
                if end > data.len() {
                    return false;
                }
                p = end;
            }
            p == data.len()
        }
        _ => false,
    }
}

fn visit_references(value: &mut Value, mut visit: impl FnMut(&mut usize)) {
    match value {
        Value::Element(Some(id)) => visit(id),
        Value::Elements(ids) => ids.iter_mut().flatten().for_each(visit),
        Value::References(ids) => ids.iter_mut().for_each(|r| {
            if let Reference::Element(Some(id)) = r {
                visit(id)
            }
        }),
        _ => {}
    }
}

fn references(value: &Value) -> Vec<usize> {
    match value {
        Value::Element(Some(id)) => vec![*id],
        Value::Elements(ids) => ids.iter().flatten().copied().collect(),
        Value::References(ids) => {
            ids.iter().filter_map(|r| if let Reference::Element(id) = r { *id } else { None }).collect()
        }
        _ => Vec::new(),
    }
}

fn remove_reference(value: &mut Value, target: usize) {
    match value {
        Value::Element(id) if *id == Some(target) => *id = None,
        Value::Elements(ids) => ids.retain(|id| *id != Some(target)),
        Value::References(ids) => ids.retain(|r| !matches!(r, Reference::Element(Some(id)) if *id == target)),
        _ => {}
    }
}

fn estimate(data: &Dmx) -> usize {
    data.elements
        .iter()
        .map(|e| {
            e.kind.len()
                + e.name.len()
                + e.attrs
                    .iter()
                    .map(|(k, v)| {
                        k.len()
                            + match v {
                                Value::Str(s) | Value::External(s) => s.len(),
                                Value::Strs(v) => v.iter().map(String::len).sum(),
                                Value::Elements(v) => v.len() * 16,
                                Value::References(v) => v.len() * 32,
                                Value::Ints(v) => v.len() * 4,
                                Value::Floats(v) => v.len() * 4,
                                Value::Vec3s(v) => v.len() * 12,
                                Value::Raw { data, .. } => data.len(),
                                _ => 32,
                            }
                            + 64
                    })
                    .sum::<usize>()
                + 128
        })
        .sum()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    use std::io::Write;
    let temp = path.with_extension(format!(
        "hp-{}-{}.tmp",
        std::process::id(),
        dmx::generated_id(0).iter().map(|b| format!("{b:02x}")).collect::<String>()
    ));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn update_manifest(old: &str, entry: &str) -> Result<String, EditError> {
    if old.trim().is_empty() {
        return Ok(format!("\"particles_manifest\"\n{{\n\t\"file\"\t\"{entry}\"\n}}\n"));
    }
    let quoted = format!("\"{entry}\"");
    if old.to_ascii_lowercase().contains(&quoted.to_ascii_lowercase()) {
        return Ok(old.into());
    }
    let end = old.rfind('}').ok_or(EditError::InvalidValue)?;
    let mut next = old[..end].to_owned();
    if !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(&format!("\t\"file\"\t\"{entry}\"\n"));
    next.push_str(&old[end..]);
    Ok(next)
}

#[cfg(test)]
mod tests;
