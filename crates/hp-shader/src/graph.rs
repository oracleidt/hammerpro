use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::catalog::{NodeDef, ParamDef, ParamKind, node_def};

pub const FORMAT: &str = "hpshader";
pub const VERSION: u32 = 1;
pub const MAX_NODES: usize = 512;
pub const MAX_LINKS: usize = 2048;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_TEXT: usize = 1024;
pub const MAX_STOPS: usize = 16;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: u32,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub pos: [f32; 2],
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, Vec<f32>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub from: u32,
    pub out: String,
    pub to: u32,
    #[serde(rename = "in")]
    pub input: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub links: Vec<Link>,
}

fn default_format() -> String {
    FORMAT.to_owned()
}

fn default_version() -> u32 {
    VERSION
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub node: Option<u32>,
    pub code: &'static str,
    pub detail: String,
}

impl Diagnostic {
    pub fn new(severity: Severity, node: Option<u32>, code: &'static str, detail: impl Into<String>) -> Self {
        Self { severity, node, code, detail: detail.into() }
    }

    pub fn message(&self) -> String {
        let d = &self.detail;
        match self.code {
            "cycle" => "this link makes a loop; it is ignored".to_owned(),
            "unknown_node" => format!("unknown node type {d}"),
            "bad_link" => format!("broken link: {d}"),
            "no_output" => "the graph has no output node".to_owned(),
            "no_source" => "this texture node has no image".to_owned(),
            "missing_texture" => format!("cannot load the image: {d}"),
            "output_unconnected" => "this output has nothing connected".to_owned(),
            "duplicate_output" => format!("two outputs write {d}"),
            "no_base" => "no base texture output: no material file is written".to_owned(),
            "bad_param" => format!("bad value: {d}"),
            "too_big" => format!("size reduced: {d}"),
            _ => d.clone(),
        }
    }
}

impl Graph {
    pub fn new() -> Self {
        Self {
            format: FORMAT.to_owned(),
            version: VERSION,
            settings: Settings::default(),
            nodes: Vec::new(),
            links: Vec::new(),
        }
    }

    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: u32) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn def_of(&self, id: u32) -> Option<&'static NodeDef> {
        self.node(id).and_then(|n| node_def(&n.kind))
    }

    pub fn next_id(&self) -> u32 {
        self.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1
    }

    pub fn add_node(&mut self, kind: &str, pos: [f32; 2]) -> Result<u32, String> {
        if node_def(kind).is_none() {
            return Err(format!("unknown node type \"{kind}\""));
        }
        if self.nodes.len() >= MAX_NODES {
            return Err(format!("a graph has at most {MAX_NODES} nodes"));
        }
        let id = self.next_id();
        let pos = [finite(pos[0]), finite(pos[1])];
        self.nodes.push(Node { id, kind: kind.to_owned(), pos, params: Map::new(), inputs: BTreeMap::new() });
        Ok(id)
    }

    pub fn add_with(&mut self, kind: &str, pos: [f32; 2], params: &[(&str, Value)]) -> Result<u32, String> {
        let id = self.add_node(kind, pos)?;
        for (name, value) in params {
            if let Err(e) = self.set_param(id, name, value.clone()) {
                self.remove_node(id);
                return Err(e);
            }
        }
        Ok(id)
    }

    pub fn remove_node(&mut self, id: u32) -> bool {
        let before = self.nodes.len();
        self.nodes.retain(|n| n.id != id);
        self.links.retain(|l| l.from != id && l.to != id);
        self.nodes.len() != before
    }

    pub fn link_to(&self, to: u32, input: &str) -> Option<&Link> {
        self.links.iter().find(|l| l.to == to && l.input == input)
    }

    pub fn links_from(&self, from: u32, out: &str) -> impl Iterator<Item = &Link> {
        let out = out.to_owned();
        self.links.iter().filter(move |l| l.from == from && l.out == out)
    }

    pub fn disconnect(&mut self, to: u32, input: &str) -> bool {
        let before = self.links.len();
        self.links.retain(|l| !(l.to == to && l.input == input));
        self.links.len() != before
    }

    pub fn depends_on(&self, node: u32, ancestor: u32) -> bool {
        let mut seen = HashSet::new();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if n == ancestor {
                return true;
            }
            if !seen.insert(n) {
                continue;
            }
            stack.extend(self.links.iter().filter(|l| l.to == n).map(|l| l.from));
        }
        false
    }

    pub fn can_connect(&self, from: u32, out: &str, to: u32, input: &str) -> Result<(), String> {
        let fd = self.def_of(from).ok_or_else(|| format!("no node {from}"))?;
        let td = self.def_of(to).ok_or_else(|| format!("no node {to}"))?;
        if fd.output(out).is_none() {
            return Err(format!("node {from} ({}) has no output \"{out}\"", fd.id));
        }
        if td.input(input).is_none() {
            return Err(format!("node {to} ({}) has no input \"{input}\"", td.id));
        }
        if from == to || self.depends_on(from, to) {
            return Err("this link would make a loop".to_owned());
        }
        Ok(())
    }

    pub fn connect(&mut self, from: u32, out: &str, to: u32, input: &str) -> Result<(), String> {
        self.can_connect(from, out, to, input)?;
        if self.link_to(to, input).is_none() && self.links.len() >= MAX_LINKS {
            return Err(format!("a graph has at most {MAX_LINKS} links"));
        }
        self.disconnect(to, input);
        self.links.push(Link { from, out: out.to_owned(), to, input: input.to_owned() });
        Ok(())
    }

    pub fn set_param(&mut self, id: u32, name: &str, value: Value) -> Result<(), String> {
        let def = self.def_of(id).ok_or_else(|| format!("no node {id}"))?;
        let p = def.param(name).ok_or_else(|| format!("{} has no parameter \"{name}\"", def.id))?;
        let value = normalize_param(p, &value).map_err(|e| format!("{}.{name}: {e}", def.id))?;
        if let Some(n) = self.node_mut(id) {
            n.params.insert(name.to_owned(), value);
        }
        Ok(())
    }

    pub fn param_value(&self, id: u32, name: &str) -> Option<Value> {
        let def = self.def_of(id)?;
        let p = def.param(name)?;
        let stored = self.node(id)?.params.get(name).and_then(|v| normalize_param(p, v).ok());
        Some(stored.unwrap_or_else(|| default_param(p)))
    }

    pub fn set_input_default(&mut self, id: u32, pin: &str, value: &[f32]) -> Result<(), String> {
        let def = self.def_of(id).ok_or_else(|| format!("no node {id}"))?;
        if def.input(pin).is_none() {
            return Err(format!("{} has no input \"{pin}\"", def.id));
        }
        if !(value.len() == 1 || value.len() == 3 || value.len() == 4) || value.iter().any(|v| !v.is_finite()) {
            return Err("an input default is 1, 3 or 4 numbers".to_owned());
        }
        let v: Vec<f32> = value.iter().map(|v| v.clamp(-1000.0, 1000.0)).collect();
        if let Some(n) = self.node_mut(id) {
            n.inputs.insert(pin.to_owned(), v);
        }
        Ok(())
    }

    pub fn clear_input_default(&mut self, id: u32, pin: &str) {
        if let Some(n) = self.node_mut(id) {
            n.inputs.remove(pin);
        }
    }

    pub fn input_default(&self, id: u32, pin: &str) -> Option<[f32; 4]> {
        let def = self.def_of(id)?.input(pin)?;
        let node = self.node(id)?;
        Some(match node.inputs.get(pin).map(Vec::as_slice) {
            Some([v]) => [*v, *v, *v, 1.0],
            Some([r, g, b]) => [*r, *g, *b, 1.0],
            Some([r, g, b, a]) => [*r, *g, *b, *a],
            _ => def.default,
        })
    }

    pub fn output_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.nodes.iter().filter(|n| n.kind == "output").map(|n| n.id).collect();
        ids.sort_unstable();
        ids
    }

    pub fn order_for(&self, targets: &[u32]) -> (Vec<u32>, Vec<Link>) {
        let mut order = Vec::new();
        let mut state: HashMap<u32, u8> = HashMap::new();
        let mut broken = Vec::new();
        let mut incoming: HashMap<u32, Vec<&Link>> = HashMap::new();
        for l in &self.links {
            incoming.entry(l.to).or_default().push(l);
        }
        for &t in targets {
            if self.node(t).is_none() || state.contains_key(&t) {
                continue;
            }
            let mut stack: Vec<(u32, usize)> = vec![(t, 0)];
            state.insert(t, 1);
            while let Some(top) = stack.last_mut() {
                let (n, idx) = *top;
                let ins = incoming.get(&n).map(Vec::as_slice).unwrap_or(&[]);
                if let Some(l) = ins.get(idx) {
                    top.1 += 1;
                    if self.node(l.from).is_none() {
                        continue;
                    }
                    match state.get(&l.from) {
                        Some(1) => broken.push((*l).clone()),
                        Some(_) => {}
                        None => {
                            state.insert(l.from, 1);
                            stack.push((l.from, 0));
                        }
                    }
                } else {
                    state.insert(n, 2);
                    order.push(n);
                    stack.pop();
                }
            }
        }
        (order, broken)
    }

    pub fn validate(&self) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        let mut ids = HashSet::new();
        for n in &self.nodes {
            if !ids.insert(n.id) {
                out.push(Diagnostic::new(
                    Severity::Error,
                    Some(n.id),
                    "bad_link",
                    format!("duplicate node id {}", n.id),
                ));
            }
            if node_def(&n.kind).is_none() {
                out.push(Diagnostic::new(Severity::Error, Some(n.id), "unknown_node", n.kind.clone()));
            }
        }
        for l in &self.links {
            if let Err(e) = self.can_connect_existing(l) {
                out.push(Diagnostic::new(Severity::Error, Some(l.to), "bad_link", e));
            }
        }
        let all: Vec<u32> = self.nodes.iter().map(|n| n.id).collect();
        let (_, broken) = self.order_for(&all);
        for l in broken {
            out.push(Diagnostic::new(Severity::Error, Some(l.to), "cycle", format!("{} -> {}", l.from, l.to)));
        }
        let outputs = self.output_ids();
        if outputs.is_empty() {
            out.push(Diagnostic::new(Severity::Warning, None, "no_output", ""));
        }
        let mut names: HashMap<String, u32> = HashMap::new();
        for id in outputs {
            if self.link_to(id, "color").is_none() {
                out.push(Diagnostic::new(Severity::Warning, Some(id), "output_unconnected", ""));
            }
            let name = self.output_suffix(id);
            if let Some(prev) = names.insert(name.clone(), id) {
                out.push(Diagnostic::new(
                    Severity::Error,
                    Some(id),
                    "duplicate_output",
                    format!("\"{name}\" (nodes {prev} and {id})"),
                ));
            }
        }
        out
    }

    fn can_connect_existing(&self, l: &Link) -> Result<(), String> {
        let fd = self.def_of(l.from).ok_or_else(|| format!("no node {}", l.from))?;
        let td = self.def_of(l.to).ok_or_else(|| format!("no node {}", l.to))?;
        if fd.output(&l.out).is_none() {
            return Err(format!("node {} has no output \"{}\"", l.from, l.out));
        }
        if td.input(&l.input).is_none() {
            return Err(format!("node {} has no input \"{}\"", l.to, l.input));
        }
        Ok(())
    }

    pub fn output_target(&self, id: u32) -> String {
        match self.param_value(id, "target") {
            Some(Value::String(s)) => s,
            _ => "basetexture".to_owned(),
        }
    }

    pub fn output_suffix(&self, id: u32) -> String {
        let custom = match self.param_value(id, "suffix") {
            Some(Value::String(s)) => sanitize_suffix(&s),
            _ => String::new(),
        };
        if custom.is_empty() { crate::catalog::default_suffix(&self.output_target(id)).to_owned() } else { custom }
    }

    pub fn sanitize(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        let mut seen = HashSet::new();
        self.nodes.retain(|n| {
            if node_def(&n.kind).is_none() {
                warnings.push(format!("node {}: unknown type \"{}\" removed", n.id, n.kind));
                return false;
            }
            if !seen.insert(n.id) {
                warnings.push(format!("node {}: duplicate id removed", n.id));
                return false;
            }
            true
        });
        if self.nodes.len() > MAX_NODES {
            self.nodes.truncate(MAX_NODES);
            warnings.push(format!("more than {MAX_NODES} nodes: the rest was dropped"));
        }
        for n in &mut self.nodes {
            n.pos = [finite(n.pos[0]), finite(n.pos[1])];
            let Some(def) = node_def(&n.kind) else { continue };
            let id = n.id;
            n.params.retain(|k, v| match def.param(k) {
                None => {
                    warnings.push(format!("node {id}: unknown parameter \"{k}\" removed"));
                    false
                }
                Some(p) => match normalize_param(p, v) {
                    Ok(nv) => {
                        *v = nv;
                        true
                    }
                    Err(e) => {
                        warnings.push(format!("node {id}: {k}: {e}; default used"));
                        false
                    }
                },
            });
            n.inputs.retain(|k, v| {
                let ok = def.input(k).is_some() && matches!(v.len(), 1 | 3 | 4) && v.iter().all(|x| x.is_finite());
                if !ok {
                    warnings.push(format!("node {id}: bad input default \"{k}\" removed"));
                }
                ok
            });
        }
        let links = std::mem::take(&mut self.links);
        let mut kept: Vec<Link> = Vec::new();
        for l in links {
            if self.can_connect_existing(&l).is_err() {
                warnings.push(format!("link {}.{} -> {}.{} removed", l.from, l.out, l.to, l.input));
                continue;
            }
            if let Some(i) = kept.iter().position(|k| k.to == l.to && k.input == l.input) {
                warnings.push(format!("input {}.{} had two links: the last one is kept", l.to, l.input));
                kept.remove(i);
            }
            if kept.len() >= MAX_LINKS {
                warnings.push(format!("more than {MAX_LINKS} links: the rest was dropped"));
                break;
            }
            kept.push(l);
        }
        self.links = Vec::new();
        for l in kept {
            if l.from == l.to || self.depends_on(l.from, l.to) {
                warnings.push(format!("link {} -> {} makes a loop and was removed", l.from, l.to));
                continue;
            }
            self.links.push(l);
        }
        self.settings.width = self.settings.width.min(crate::MAX_BAKE_SIZE);
        self.settings.height = self.settings.height.min(crate::MAX_BAKE_SIZE);
        warnings
    }

    pub fn parse(text: &str) -> Result<(Graph, Vec<String>), String> {
        if text.len() > MAX_BYTES {
            return Err("the shader file is too big".to_owned());
        }
        let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
        if value.get("format").and_then(Value::as_str).is_some_and(|f| f != FORMAT) {
            return Err("this is not a Hammer Pro shader graph".to_owned());
        }
        if value.get("version").and_then(Value::as_u64).is_some_and(|v| v > u64::from(VERSION)) {
            return Err("this shader graph was made by a newer Hammer Pro".to_owned());
        }
        let mut graph: Graph = serde_json::from_value(value).map_err(|e| e.to_string())?;
        graph.format = FORMAT.to_owned();
        graph.version = VERSION;
        let warnings = graph.sanitize();
        Ok((graph, warnings))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_owned())
    }

    pub fn map_sources(&mut self, mut f: impl FnMut(&str) -> String) {
        for n in &mut self.nodes {
            if n.kind != "texture" {
                continue;
            }
            if let Some(Value::String(s)) = n.params.get_mut("source") {
                let mapped = f(s);
                *s = mapped;
            }
        }
    }

    pub fn sources(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for n in self.nodes.iter().filter(|n| n.kind == "texture") {
            if let Some(Value::String(s)) = n.params.get("source")
                && !s.trim().is_empty()
                && !out.contains(s)
            {
                out.push(s.clone());
            }
        }
        out
    }
}

fn finite(v: f32) -> f32 {
    if v.is_finite() { v.clamp(-100_000.0, 100_000.0) } else { 0.0 }
}

pub fn sanitize_suffix(s: &str) -> String {
    let cleaned: String = s
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c.to_ascii_lowercase() } else { '_' })
        .collect();
    cleaned.chars().take(32).collect()
}

fn number(v: &Value) -> Result<f64, String> {
    v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| "a number is expected".to_owned())
}

pub fn default_param(p: &ParamDef) -> Value {
    match p.kind {
        ParamKind::Float { default, .. } => json!(default),
        ParamKind::Int { default, .. } => json!(default),
        ParamKind::Bool { default } => json!(default),
        ParamKind::Color { default } => json!(default),
        ParamKind::Choice { options, default } => json!(options[default]),
        ParamKind::Text { default } => json!(default),
        ParamKind::Vec2 { default, .. } => json!(default),
        ParamKind::Gradient => json!([
            { "pos": 0.0, "color": [0.0, 0.0, 0.0, 1.0] },
            { "pos": 1.0, "color": [1.0, 1.0, 1.0, 1.0] }
        ]),
    }
}

pub fn normalize_param(p: &ParamDef, v: &Value) -> Result<Value, String> {
    match p.kind {
        ParamKind::Float { min, max, .. } => {
            let x = number(v)? as f32;
            Ok(json!(x.clamp(min, max)))
        }
        ParamKind::Int { min, max, .. } => {
            let x = number(v)?.round().clamp(f64::from(min), f64::from(max)) as i32;
            Ok(json!(x))
        }
        ParamKind::Bool { .. } => v.as_bool().map(Value::Bool).ok_or_else(|| "true or false is expected".to_owned()),
        ParamKind::Color { .. } => {
            let a = v.as_array().ok_or_else(|| "[r, g, b, a] is expected".to_owned())?;
            if a.len() != 3 && a.len() != 4 {
                return Err("[r, g, b, a] is expected".to_owned());
            }
            let mut c = [0.0f32, 0.0, 0.0, 1.0];
            for (i, x) in a.iter().enumerate() {
                c[i] = (number(x)? as f32).clamp(0.0, 1.0);
            }
            Ok(json!(c))
        }
        ParamKind::Choice { options, .. } => {
            let s = v.as_str().ok_or_else(|| "a text is expected".to_owned())?.trim().to_ascii_lowercase();
            options
                .iter()
                .find(|o| **o == s)
                .map(|o| json!(o))
                .ok_or_else(|| format!("\"{s}\" is not one of {}", options.join(", ")))
        }
        ParamKind::Text { .. } => {
            let s = v.as_str().ok_or_else(|| "a text is expected".to_owned())?;
            if s.chars().count() > MAX_TEXT {
                return Err(format!("at most {MAX_TEXT} characters"));
            }
            Ok(json!(s))
        }
        ParamKind::Vec2 { min, max, .. } => {
            let a = v.as_array().filter(|a| a.len() == 2).ok_or_else(|| "[x, y] is expected".to_owned())?;
            let x = (number(&a[0])? as f32).clamp(min, max);
            let y = (number(&a[1])? as f32).clamp(min, max);
            Ok(json!([x, y]))
        }
        ParamKind::Gradient => {
            let a = v.as_array().ok_or_else(|| "a list of stops is expected".to_owned())?;
            if a.len() < 2 || a.len() > MAX_STOPS {
                return Err(format!("2 to {MAX_STOPS} stops are expected"));
            }
            let mut stops: Vec<(f32, [f32; 4])> = Vec::new();
            for s in a {
                let pos = (number(s.get("pos").ok_or_else(|| "a stop needs pos".to_owned())?)? as f32).clamp(0.0, 1.0);
                let c = s.get("color").and_then(Value::as_array).ok_or_else(|| "a stop needs color".to_owned())?;
                if c.len() != 3 && c.len() != 4 {
                    return Err("a stop color is [r, g, b, a]".to_owned());
                }
                let mut col = [0.0f32, 0.0, 0.0, 1.0];
                for (i, x) in c.iter().enumerate() {
                    col[i] = (number(x)? as f32).clamp(0.0, 1.0);
                }
                stops.push((pos, col));
            }
            stops.sort_by(|a, b| a.0.total_cmp(&b.0));
            Ok(Value::Array(stops.into_iter().map(|(pos, color)| json!({ "pos": pos, "color": color })).collect()))
        }
    }
}

pub struct Params<'a> {
    node: &'a Node,
    def: &'a NodeDef,
}

impl<'a> Params<'a> {
    pub fn new(node: &'a Node, def: &'a NodeDef) -> Self {
        Self { node, def }
    }

    fn value(&self, name: &str) -> Value {
        let Some(p) = self.def.param(name) else { return Value::Null };
        self.node.params.get(name).and_then(|v| normalize_param(p, v).ok()).unwrap_or_else(|| default_param(p))
    }

    pub fn f(&self, name: &str) -> f32 {
        self.value(name).as_f64().unwrap_or(0.0) as f32
    }

    pub fn i(&self, name: &str) -> i32 {
        self.value(name).as_i64().unwrap_or(0) as i32
    }

    pub fn b(&self, name: &str) -> bool {
        self.value(name).as_bool().unwrap_or(false)
    }

    pub fn s(&self, name: &str) -> String {
        self.value(name).as_str().unwrap_or("").to_owned()
    }

    pub fn color(&self, name: &str) -> [f32; 4] {
        let v = self.value(name);
        let mut c = [0.0, 0.0, 0.0, 1.0];
        if let Some(a) = v.as_array() {
            for (i, x) in a.iter().take(4).enumerate() {
                c[i] = x.as_f64().unwrap_or(0.0) as f32;
            }
        }
        c
    }

    pub fn v2(&self, name: &str) -> [f32; 2] {
        let v = self.value(name);
        let mut c = [0.0, 0.0];
        if let Some(a) = v.as_array() {
            for (i, x) in a.iter().take(2).enumerate() {
                c[i] = x.as_f64().unwrap_or(0.0) as f32;
            }
        }
        c
    }

    pub fn stops(&self, name: &str) -> Vec<(f32, [f32; 4])> {
        let v = self.value(name);
        let mut out = Vec::new();
        for s in v.as_array().map(Vec::as_slice).unwrap_or(&[]) {
            let pos = s.get("pos").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            let mut col = [0.0, 0.0, 0.0, 1.0];
            if let Some(a) = s.get("color").and_then(Value::as_array) {
                for (i, x) in a.iter().take(4).enumerate() {
                    col[i] = x.as_f64().unwrap_or(0.0) as f32;
                }
            }
            out.push((pos, col));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tex_out() -> (Graph, u32, u32) {
        let mut g = Graph::new();
        let t = g.add_node("texture", [0.0, 0.0]).unwrap();
        let o = g.add_node("output", [200.0, 0.0]).unwrap();
        g.connect(t, "rgba", o, "color").unwrap();
        (g, t, o)
    }

    #[test]
    fn connect_replaces_and_rejects_loops() {
        let (mut g, t, o) = tex_out();
        let b = g.add_node("blur", [100.0, 0.0]).unwrap();
        g.connect(t, "rgba", b, "color").unwrap();
        g.connect(b, "color", o, "color").unwrap();
        assert_eq!(g.links.len(), 2);
        assert_eq!(g.link_to(o, "color").map(|l| l.from), Some(b));
        assert!(g.connect(o, "color", b, "color").is_err(), "an output has no pins");
        let c = g.add_node("blur", [0.0, 0.0]).unwrap();
        g.connect(b, "color", c, "color").unwrap();
        assert!(g.connect(c, "color", b, "color").is_err());
        assert!(g.connect(b, "color", b, "color").is_err());
        assert!(g.connect(t, "nope", b, "color").is_err());
        assert!(g.connect(t, "rgba", b, "nope").is_err());
        assert!(g.validate().iter().all(|d| d.severity != Severity::Error));
    }

    #[test]
    fn params_are_validated() {
        let (mut g, t, _) = tex_out();
        g.set_param(t, "wrap", json!("Clamp")).unwrap();
        assert_eq!(g.param_value(t, "wrap"), Some(json!("clamp")));
        assert!(g.set_param(t, "wrap", json!("spiral")).is_err());
        assert!(g.set_param(t, "nope", json!(1)).is_err());
        let b = g.add_node("blur", [0.0, 0.0]).unwrap();
        g.set_param(b, "radius", json!(1e9)).unwrap();
        assert_eq!(g.param_value(b, "radius"), Some(json!(64.0)));
        assert!(g.set_param(b, "radius", json!("x")).is_err());
        let gm = g.add_node("gradient_map", [0.0, 0.0]).unwrap();
        assert!(g.set_param(gm, "gradient", json!([{ "pos": 0.0, "color": [1, 0, 0] }])).is_err());
        g.set_param(
            gm,
            "gradient",
            json!([{ "pos": 1.0, "color": [1, 0, 0] }, { "pos": 0.0, "color": [0, 0, 1, 0.5] }]),
        )
        .unwrap();
        let stops = Params::new(g.node(gm).unwrap(), g.def_of(gm).unwrap()).stops("gradient");
        assert_eq!(stops[0].0, 0.0);
        assert_eq!(stops[1].1, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn parse_repairs_broken_files() {
        let text = r#"{
            "format": "hpshader", "version": 1,
            "nodes": [
                {"id": 1, "type": "texture", "params": {"wrap": "zigzag", "ghost": 1}},
                {"id": 2, "type": "warp_drive"},
                {"id": 3, "type": "output"},
                {"id": 3, "type": "output"},
                {"id": 4, "type": "blur"},
                {"id": 5, "type": "blur"}
            ],
            "links": [
                {"from": 1, "out": "rgba", "to": 3, "in": "color"},
                {"from": 2, "out": "x", "to": 3, "in": "alpha"},
                {"from": 4, "out": "color", "to": 5, "in": "color"},
                {"from": 5, "out": "color", "to": 4, "in": "color"}
            ]
        }"#;
        let (g, warnings) = Graph::parse(text).unwrap();
        assert!(warnings.len() >= 4, "{warnings:?}");
        assert_eq!(g.nodes.len(), 4);
        assert_eq!(g.links.len(), 2);
        assert!(g.validate().iter().all(|d| d.severity != Severity::Error), "{:?}", g.validate());
        assert!(Graph::parse("not json").is_err());
        assert!(Graph::parse(r#"{"format":"other"}"#).is_err());
        assert!(Graph::parse(r#"{"version":99}"#).is_err());
    }

    #[test]
    fn round_trip() {
        let (mut g, t, _) = tex_out();
        g.set_param(t, "source", json!("brick.png")).unwrap();
        g.set_input_default(t, "uv", &[0.1]).unwrap();
        let (back, warnings) = Graph::parse(&g.to_json()).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(back, g);
    }

    #[test]
    fn order_reports_cycles() {
        let mut g = Graph::new();
        let a = g.add_node("blur", [0.0, 0.0]).unwrap();
        let b = g.add_node("blur", [0.0, 0.0]).unwrap();
        g.links.push(Link { from: a, out: "color".into(), to: b, input: "color".into() });
        g.links.push(Link { from: b, out: "color".into(), to: a, input: "color".into() });
        let (order, broken) = g.order_for(&[a, b]);
        assert_eq!(order.len(), 2);
        assert_eq!(broken.len(), 1);
        assert!(g.validate().iter().any(|d| d.code == "cycle"));
    }

    #[test]
    fn duplicate_outputs_are_reported() {
        let (mut g, _, _) = tex_out();
        let o2 = g.add_node("output", [0.0, 0.0]).unwrap();
        assert!(g.validate().iter().any(|d| d.code == "duplicate_output" && d.node == Some(o2)));
        g.set_param(o2, "target", json!("normalmap")).unwrap();
        assert!(g.validate().iter().all(|d| d.code != "duplicate_output"));
        assert_eq!(g.output_suffix(o2), "_normal");
        g.set_param(o2, "suffix", json!("_N Map")).unwrap();
        assert_eq!(g.output_suffix(o2), "_n_map");
    }
}
