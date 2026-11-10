use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use glam::{DMat3, DMat4, DVec3};
use hp_geom::{Aabb, angle_matrix, transform_angles_mat};
use hp_vmf::{Entity, Group, Map, Solid};

pub const INSTANCE_CLASS: &str = "func_instance";
pub const PARMS_CLASS: &str = "func_instance_parms";
pub const IO_PROXY_CLASS: &str = "func_instance_io_proxy";
pub const MAX_DEPTH: usize = 16;
pub const MAX_PIECES: usize = 20_000;
pub const MAX_VALUE_LEN: usize = 8192;
pub const REPLACE_PREFIX: &str = "replace";
pub const PARM_SLOTS: usize = 10;

pub fn is_instance(e: &Entity) -> bool {
    e.classname.trim().eq_ignore_ascii_case(INSTANCE_CLASS)
}

pub fn is_parms(e: &Entity) -> bool {
    e.classname.trim().eq_ignore_ascii_case(PARMS_CLASS)
}

pub fn is_hidden_object(hidden: bool, editor: &hp_vmf::EditorInfo) -> bool {
    hidden || !editor.visgroup_shown || !editor.visgroup_auto_shown
}

pub fn parse_vec_lenient(s: &str) -> DVec3 {
    let mut out = [0.0f64; 3];
    for (slot, tok) in out.iter_mut().zip(s.split_ascii_whitespace()) {
        let v = leading_float(tok);
        *slot = if v.is_finite() { v } else { 0.0 };
    }
    DVec3::from_array(out)
}

fn leading_float(tok: &str) -> f64 {
    let bytes = tok.as_bytes();
    let mut end = 0;
    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut seen_exp = false;
    while end < bytes.len() {
        let c = bytes[end];
        let ok = match c {
            b'0'..=b'9' => {
                seen_digit = true;
                true
            }
            b'+' | b'-' => end == 0 || matches!(bytes[end - 1], b'e' | b'E'),
            b'.' if !seen_dot && !seen_exp => {
                seen_dot = true;
                true
            }
            b'e' | b'E' if seen_digit && !seen_exp => {
                seen_exp = true;
                true
            }
            _ => false,
        };
        if !ok {
            break;
        }
        end += 1;
    }
    let mut text = &tok[..end];
    while let Some(last) = text.as_bytes().last()
        && matches!(last, b'e' | b'E' | b'+' | b'-')
    {
        text = &text[..text.len() - 1];
    }
    text.parse::<f64>().unwrap_or(0.0)
}

pub fn fmt_vec(v: DVec3) -> String {
    hp_vmf::fmt_vec3(crate::ops::clean_vec(v))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FixupStyle {
    #[default]
    Prefix,
    Postfix,
    None,
}

impl FixupStyle {
    pub fn from_value(v: Option<&str>) -> FixupStyle {
        match v.map_or(0.0, |s| leading_float(s.trim())) as i64 {
            0 => FixupStyle::Prefix,
            1 => FixupStyle::Postfix,
            _ => FixupStyle::None,
        }
    }

    pub fn value(self) -> &'static str {
        match self {
            FixupStyle::Prefix => "0",
            FixupStyle::Postfix => "1",
            FixupStyle::None => "2",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyFixup {
    Name,
    Origin,
    Angle,
    AngleNegativePitch,
    Axis,
    Keep,
}

pub type KeyFixupFn<'a> = &'a (dyn Fn(&str, &str) -> Option<KeyFixup> + Sync);

pub fn default_key_fixup(key: &str) -> KeyFixup {
    const NAME_KEYS: &[&str] =
        &["targetname", "target", "parentname", "filtername", "damagefilter", "lasertarget", "NextKey"];
    if NAME_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key)) { KeyFixup::Name } else { KeyFixup::Keep }
}

#[derive(Clone, Debug, PartialEq)]
pub struct InstanceSpec {
    pub file: String,
    pub origin: DVec3,
    pub angles: DVec3,
    pub name: String,
    pub style: FixupStyle,
    pub replaces: Vec<(String, String)>,
}

impl InstanceSpec {
    pub fn of(e: &Entity) -> Option<InstanceSpec> {
        if !is_instance(e) {
            return None;
        }
        let name = e
            .get("targetname")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| e.get("name").map(str::trim).filter(|s| !s.is_empty()))
            .unwrap_or("")
            .to_owned();
        Some(InstanceSpec {
            file: e.get("file").map(str::trim).unwrap_or("").to_owned(),
            origin: e.get("origin").map_or(DVec3::ZERO, parse_vec_lenient),
            angles: e.get("angles").map_or(DVec3::ZERO, parse_vec_lenient),
            name,
            style: FixupStyle::from_value(e.get("fixup_style")),
            replaces: replaces_of(e),
        })
    }

    pub fn matrix(&self) -> DMat4 {
        DMat4::from_mat3_translation(angle_matrix(self.angles), self.origin)
    }
}

pub fn replaces_of(e: &Entity) -> Vec<(String, String)> {
    e.props.iter().filter(|(k, _)| is_replace_key(k)).filter_map(|(_, v)| split_replace(v)).collect()
}

pub fn is_replace_key(key: &str) -> bool {
    key.len() >= REPLACE_PREFIX.len()
        && key.as_bytes()[..REPLACE_PREFIX.len()].eq_ignore_ascii_case(REPLACE_PREFIX.as_bytes())
}

pub fn split_replace(value: &str) -> Option<(String, String)> {
    let (var, rest) = value.split_once(' ')?;
    if var.is_empty() {
        return None;
    }
    Some((var.to_owned(), rest.to_owned()))
}

pub fn replace_key(n: usize) -> String {
    format!("{REPLACE_PREFIX}{n:02}")
}

pub fn next_free_replace_key(e: &Entity) -> String {
    (1..=999).map(replace_key).find(|k| e.get(k).is_none()).unwrap_or_else(|| replace_key(1000))
}

pub fn substitute(hay: &str, needle: &str, with: &str) -> Option<String> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    let h = hay.as_bytes();
    let n = needle.as_bytes();
    let mut out: Option<String> = None;
    let mut last = 0;
    let mut i = 0;
    while i + n.len() <= h.len() {
        if h[i..i + n.len()].eq_ignore_ascii_case(n) && hay.is_char_boundary(i) && hay.is_char_boundary(i + n.len()) {
            let o = out.get_or_insert_with(|| String::with_capacity(hay.len() + with.len()));
            o.push_str(&hay[last..i]);
            o.push_str(with);
            if o.len() > MAX_VALUE_LEN {
                return None;
            }
            i += n.len();
            last = i;
        } else {
            i += 1;
        }
    }
    let mut o = out?;
    o.push_str(&hay[last..]);
    (o.len() <= MAX_VALUE_LEN).then_some(o)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fixup {
    pub name: String,
    pub style: FixupStyle,
    pub replaces: Vec<(String, String)>,
    pub matrix: DMat4,
    rotation: DMat3,
    rotated: bool,
}

impl Fixup {
    pub fn new(spec: &InstanceSpec, auto_index: u32) -> Fixup {
        let name = if spec.name.is_empty() { format!("InstanceAuto{auto_index}") } else { spec.name.clone() };
        let rotation = angle_matrix(spec.angles);
        let matrix = DMat4::from_mat3_translation(rotation, spec.origin);
        let ok = matrix.is_finite();
        Fixup {
            name,
            style: spec.style,
            replaces: spec.replaces.clone(),
            matrix: if ok { matrix } else { DMat4::IDENTITY },
            rotation: if ok { rotation } else { DMat3::IDENTITY },
            rotated: ok && spec.angles != DVec3::ZERO,
        }
    }

    fn with_defaults(mut self, parms: &[InstanceParm]) -> Fixup {
        for p in parms {
            let Some(default) = &p.default else { continue };
            if !self.replaces.iter().any(|(var, _)| var.eq_ignore_ascii_case(&p.name)) {
                self.replaces.push((p.name.clone(), default.clone()));
            }
        }
        self
    }

    fn within(mut self, outer: &Fixup) -> Fixup {
        self.replaces.extend(outer.replaces.iter().cloned());
        self
    }

    pub fn translation(delta: DVec3) -> Fixup {
        let delta = if delta.is_finite() { delta } else { DVec3::ZERO };
        Fixup {
            name: String::new(),
            style: FixupStyle::None,
            replaces: Vec::new(),
            matrix: DMat4::from_translation(delta),
            rotation: DMat3::IDENTITY,
            rotated: false,
        }
    }

    pub fn is_identity(&self) -> bool {
        self.matrix == DMat4::IDENTITY
    }

    pub fn replace<'s>(&self, value: &'s str) -> Cow<'s, str> {
        let mut out = Cow::Borrowed(value);
        for (var, with) in &self.replaces {
            if let Some(s) = substitute(&out, var, with) {
                out = Cow::Owned(s);
            }
        }
        out
    }

    pub fn fix_name(&self, name: &str) -> String {
        if name.is_empty() || name.starts_with('@') || name.starts_with('!') {
            return name.to_owned();
        }
        match self.style {
            FixupStyle::Prefix => format!("{}-{}", self.name, name),
            FixupStyle::Postfix => format!("{}-{}", name, self.name),
            FixupStyle::None => name.to_owned(),
        }
    }

    pub fn point(&self, p: DVec3) -> DVec3 {
        self.matrix.transform_point3(p)
    }

    pub fn direction(&self, d: DVec3) -> DVec3 {
        self.rotation * d
    }

    pub fn angles(&self, a: DVec3) -> DVec3 {
        if self.rotated { transform_angles_mat(a, &self.rotation) } else { a }
    }

    pub fn fix_entity(&self, e: &mut Entity, kinds: Option<KeyFixupFn<'_>>) {
        if !self.replaces.is_empty() {
            if let Cow::Owned(c) = self.replace(&e.classname) {
                e.classname = c;
            }
            let instance = is_instance(e);
            for (k, v) in &mut e.props {
                if instance && is_replace_key(k) {
                    if let Some((var, value)) = v.split_once(' ')
                        && let Cow::Owned(s) = self.replace(value)
                    {
                        *v = format!("{var} {s}");
                    }
                    continue;
                }
                if let Cow::Owned(s) = self.replace(v) {
                    *v = s;
                }
            }
        }
        let class = e.classname.clone();
        let known = kinds.and_then(|f| f(&class, "")).is_some();
        let brush_entity = !e.solids.is_empty();
        for (k, v) in &mut e.props {
            let kind = if k.eq_ignore_ascii_case("origin") {
                KeyFixup::Origin
            } else if k.eq_ignore_ascii_case("angles") {
                if brush_entity { KeyFixup::Keep } else { KeyFixup::Angle }
            } else {
                match kinds.and_then(|f| f(&class, k)) {
                    Some(kind) => kind,
                    None if known => KeyFixup::Keep,
                    None => default_key_fixup(k),
                }
            };
            if let Some(new) = self.fix_value(kind, v) {
                *v = new;
            }
        }
        if is_overlay_class(&class) {
            self.fix_overlay(e);
        }
        for c in &mut e.connections {
            if !self.replaces.is_empty() {
                for field in [&mut c.target, &mut c.input, &mut c.param] {
                    if let Cow::Owned(s) = self.replace(field) {
                        *field = s;
                    }
                }
            }
            let fixed = self.fix_name(&c.target);
            if fixed != c.target {
                c.target = fixed;
            }
        }
    }

    pub fn fix_value(&self, kind: KeyFixup, v: &str) -> Option<String> {
        let out = match kind {
            KeyFixup::Keep => return None,
            KeyFixup::Name => self.fix_name(v),
            KeyFixup::Origin => {
                if v.trim().is_empty() || self.is_identity() {
                    return None;
                }
                fmt_vec(self.point(parse_vec_lenient(v)))
            }
            KeyFixup::Angle => {
                if !self.rotated || v.trim().is_empty() {
                    return None;
                }
                fmt_vec(self.angles(parse_vec_lenient(v)))
            }
            KeyFixup::AngleNegativePitch => {
                if !self.rotated || v.trim().is_empty() {
                    return None;
                }
                let pitch = parse_vec_lenient(v).x;
                let a = self.angles(DVec3::new(-pitch, 0.0, 0.0));
                fmt_num(-a.x)
            }
            KeyFixup::Axis => {
                if self.is_identity() {
                    return None;
                }
                let (a, b) = v.split_once(',')?;
                let (pa, pb) = (self.point(parse_vec_lenient(a)), self.point(parse_vec_lenient(b)));
                format!("{}, {}", fmt_vec(pa), fmt_vec(pb))
            }
        };
        (out != v).then_some(out)
    }

    fn fix_overlay(&self, e: &mut Entity) {
        if self.is_identity() {
            return;
        }
        if let Some(o) = e.get("BasisOrigin").map(parse_vec_lenient) {
            e.set("BasisOrigin", fmt_vec(self.point(o)));
        }
        for key in ["BasisU", "BasisV", "BasisNormal"] {
            if let Some(d) = e.get(key).map(parse_vec_lenient) {
                e.set(key, fmt_vec(self.direction(d)));
            }
        }
    }

    pub fn fix_solid(&self, s: &mut Solid) -> Result<(), hp_geom::GeomError> {
        if self.is_identity() {
            return Ok(());
        }
        hp_geom::transform_solid(s, self.matrix, true, true)
    }
}

fn fmt_num(v: f64) -> String {
    let v = crate::ops::clean_vec(DVec3::splat(v)).x;
    let s = hp_vmf::fmt_vec3(DVec3::splat(v));
    s.split(' ').next().unwrap_or("0").to_owned()
}

fn is_overlay_class(class: &str) -> bool {
    ["info_overlay", "info_overlay_transition", "info_overlay_accessor"].iter().any(|c| c.eq_ignore_ascii_case(class))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceParm {
    pub name: String,
    pub kind: String,
    pub default: Option<String>,
}

pub fn parse_parm(value: &str) -> Option<InstanceParm> {
    let value = value.trim();
    let (name, rest) = value.split_once(char::is_whitespace).unwrap_or((value, ""));
    if name.len() < 2 || !name.starts_with('$') {
        return None;
    }
    let rest = rest.trim_start();
    let (kind, default) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let kind = if kind.is_empty() { "string" } else { kind };
    let default = default.trim();
    Some(InstanceParm {
        name: name.to_owned(),
        kind: kind.to_ascii_lowercase(),
        default: (!default.is_empty()).then(|| default.to_owned()),
    })
}

pub fn map_parms(map: &Map) -> Vec<InstanceParm> {
    let mut out: Vec<InstanceParm> = Vec::new();
    for e in map.entities.iter().filter(|e| is_parms(e)) {
        let mut keyed: Vec<(u32, &str)> = e
            .props
            .iter()
            .filter_map(|(k, v)| {
                k.get(..4).filter(|p| p.eq_ignore_ascii_case("parm"))?;
                let idx: u32 = k.get(4..)?.trim().parse().ok()?;
                Some((idx, v.as_str()))
            })
            .collect();
        keyed.sort_by_key(|(i, _)| *i);
        for (_, v) in keyed {
            if let Some(p) = parse_parm(v)
                && !out.iter().any(|o| o.name.eq_ignore_ascii_case(&p.name))
            {
                out.push(p);
            }
        }
    }
    out
}

pub fn parm_value<'e>(e: &'e Entity, name: &str) -> Option<(&'e str, &'e str)> {
    e.props.iter().filter(|(k, _)| is_replace_key(k)).find_map(|(k, v)| {
        let (var, value) = v.split_once(' ')?;
        var.eq_ignore_ascii_case(name).then_some((k.as_str(), value))
    })
}

pub fn normalize_file(file: &str) -> Option<String> {
    let f = file.trim().replace('\\', "/");
    if f.is_empty() || f.contains('\0') || f.starts_with("//") {
        return None;
    }
    let path = Path::new(&f);
    let has_vmf = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("vmf"));
    if has_vmf {
        return Some(f);
    }
    let stem_end = match (f.rfind('.'), f.rfind('/')) {
        (Some(dot), Some(slash)) if dot > slash + 1 => dot,
        (Some(dot), None) if dot > 0 => dot,
        _ => f.len(),
    };
    Some(format!("{}.vmf", &f[..stem_end]))
}

pub fn maps_folder(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in path.components() {
        out.push(c.as_os_str());
        if let Component::Normal(name) = c
            && name.eq_ignore_ascii_case("maps")
        {
            return Some(out);
        }
    }
    None
}

pub fn candidate_paths(
    file: &str,
    main_map: Option<&Path>,
    containing: Option<&Path>,
    game_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let Some(f) = normalize_file(file) else { return Vec::new() };
    let rel = Path::new(&f);
    if rel.is_absolute() || rel.has_root() {
        return vec![rel.to_path_buf()];
    }
    let mut bases: Vec<PathBuf> = Vec::new();
    for p in [main_map, containing].into_iter().flatten() {
        if let Some(dir) = p.parent() {
            bases.push(dir.to_path_buf());
        }
        if let Some(maps) = maps_folder(p) {
            bases.push(maps);
        }
    }
    if let Some(g) = game_dir {
        bases.push(g.join("maps"));
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for b in bases {
        if b.as_os_str().is_empty() {
            continue;
        }
        let p = b.join(rel);
        if !out.iter().any(|o| path_key(o) == path_key(&p)) {
            out.push(p);
        }
    }
    out
}

pub fn content_path(file: &str) -> Option<String> {
    let f = normalize_file(file)?;
    let p = Path::new(&f);
    if p.is_absolute() || p.has_root() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(format!("maps/{}", f.to_ascii_lowercase()))
}

pub fn path_key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if cfg!(windows) { s.to_lowercase() } else { s }
}

#[derive(Clone, Copy, Debug)]
pub struct LoadRequest<'a> {
    pub file: &'a str,
    pub containing: Option<&'a Path>,
    pub depth: usize,
}

#[derive(Clone, Debug)]
pub enum Loaded {
    Ready { path: Arc<Path>, map: Arc<Map> },
    Pending,
    Missing,
    Failed(String),
}

pub trait InstanceSource {
    fn load(&mut self, req: &LoadRequest<'_>) -> Loaded;
}

impl<F: FnMut(&LoadRequest<'_>) -> Loaded> InstanceSource for F {
    fn load(&mut self, req: &LoadRequest<'_>) -> Loaded {
        self(req)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstanceIssue {
    NoFile,
    Missing { file: String, depth: usize },
    Failed { file: String, message: String },
    Pending,
    Cycle { file: String },
    TooDeep { file: String },
    TooLarge,
}

#[derive(Clone, Debug)]
pub struct Piece {
    pub path: Arc<Path>,
    pub map: Arc<Map>,
    pub fixup: Arc<Fixup>,
    pub depth: usize,
}

#[derive(Clone, Copy, Default)]
pub struct ExpandOptions<'a> {
    pub skip_hidden: bool,
    pub kinds: Option<KeyFixupFn<'a>>,
    pub main_map: Option<&'a Path>,
    pub first_auto_index: u32,
}

impl std::fmt::Debug for ExpandOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExpandOptions")
            .field("skip_hidden", &self.skip_hidden)
            .field("kinds", &self.kinds.is_some())
            .field("main_map", &self.main_map)
            .field("first_auto_index", &self.first_auto_index)
            .finish()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Expansion {
    pub pieces: Vec<Piece>,
    pub unresolved: Vec<Entity>,
    pub issues: Vec<InstanceIssue>,
}

impl Expansion {
    pub fn is_pending(&self) -> bool {
        self.issues.contains(&InstanceIssue::Pending)
    }

    pub fn is_placed(&self) -> bool {
        self.pieces.first().is_some_and(|p| p.depth == 0)
    }
}

struct Queued {
    root: usize,
    entity: Entity,
    depth: usize,
    containing: Option<Arc<Path>>,
    chain: Arc<Vec<String>>,
    outer: Option<Arc<Fixup>>,
}

pub fn expand(roots: &[&Entity], source: &mut dyn InstanceSource, opts: &ExpandOptions<'_>) -> Vec<Expansion> {
    let mut out: Vec<Expansion> = roots.iter().map(|_| Expansion::default()).collect();
    let main_chain: Arc<Vec<String>> = Arc::new(opts.main_map.map(path_key).into_iter().collect());
    let mut queue: std::collections::VecDeque<Queued> = roots
        .iter()
        .enumerate()
        .map(|(root, e)| Queued {
            root,
            entity: (*e).clone(),
            depth: 0,
            containing: None,
            chain: Arc::clone(&main_chain),
            outer: None,
        })
        .collect();
    let mut auto = opts.first_auto_index.max(1);
    let mut placed = 0usize;
    while let Some(q) = queue.pop_front() {
        let Some(exp) = out.get_mut(q.root) else { continue };
        let Some(spec) = InstanceSpec::of(&q.entity) else { continue };
        let keep = |exp: &mut Expansion, issue: InstanceIssue, e: Entity, depth: usize| {
            if depth > 0 {
                exp.unresolved.push(e);
            }
            if !exp.issues.contains(&issue) {
                exp.issues.push(issue);
            }
        };
        if spec.file.is_empty() {
            keep(exp, InstanceIssue::NoFile, q.entity, q.depth);
            continue;
        }
        if q.depth > MAX_DEPTH {
            keep(exp, InstanceIssue::TooDeep { file: spec.file.clone() }, q.entity, q.depth);
            continue;
        }
        if placed >= MAX_PIECES {
            keep(exp, InstanceIssue::TooLarge, q.entity, q.depth);
            continue;
        }
        let req = LoadRequest { file: &spec.file, containing: q.containing.as_deref(), depth: q.depth };
        let (path, map) = match source.load(&req) {
            Loaded::Ready { path, map } => (path, map),
            Loaded::Pending => {
                keep(exp, InstanceIssue::Pending, q.entity, q.depth);
                continue;
            }
            Loaded::Missing => {
                keep(exp, InstanceIssue::Missing { file: spec.file.clone(), depth: q.depth }, q.entity, q.depth);
                continue;
            }
            Loaded::Failed(message) => {
                keep(exp, InstanceIssue::Failed { file: spec.file.clone(), message }, q.entity, q.depth);
                continue;
            }
        };
        let key = path_key(&path);
        if q.chain.contains(&key) {
            keep(exp, InstanceIssue::Cycle { file: spec.file.clone() }, q.entity, q.depth);
            continue;
        }
        let mut own = Fixup::new(&spec, auto).with_defaults(&map_parms(&map));
        if let Some(outer) = &q.outer {
            own = own.within(outer);
        }
        let fixup = Arc::new(own);
        auto = auto.saturating_add(1);
        placed += 1;
        let chain = {
            let mut c = (*q.chain).clone();
            c.push(key);
            Arc::new(c)
        };
        for e in map.entities.iter().filter(|e| is_instance(e)) {
            if opts.skip_hidden && is_hidden_object(e.hidden, &e.editor) {
                continue;
            }
            let mut nested = e.clone();
            nested.solids.clear();
            fixup.fix_entity(&mut nested, opts.kinds);
            queue.push_back(Queued {
                root: q.root,
                entity: nested,
                depth: q.depth + 1,
                containing: Some(Arc::clone(&path)),
                chain: Arc::clone(&chain),
                outer: Some(Arc::clone(&fixup)),
            });
        }
        exp.pieces.push(Piece { path, map, fixup, depth: q.depth });
    }
    out
}

impl Piece {
    pub fn world_solids(&self, skip_hidden: bool) -> Vec<Solid> {
        self.map
            .world
            .solids
            .iter()
            .filter(|s| !(skip_hidden && is_hidden_object(s.hidden, &s.editor)))
            .filter_map(|s| {
                let mut s = s.clone();
                self.fixup.fix_solid(&mut s).ok().map(|()| s)
            })
            .collect()
    }

    pub fn entities(&self, kinds: Option<KeyFixupFn<'_>>, skip_hidden: bool) -> Vec<Entity> {
        self.map
            .entities
            .iter()
            .filter(|e| !is_instance(e) && !is_parms(e))
            .filter(|e| !(skip_hidden && is_hidden_object(e.hidden, &e.editor)))
            .filter_map(|e| {
                let mut e = e.clone();
                let had_solids = !e.solids.is_empty();
                let solids = std::mem::take(&mut e.solids);
                e.solids = solids
                    .into_iter()
                    .filter(|s| !(skip_hidden && is_hidden_object(s.hidden, &s.editor)))
                    .filter_map(|mut s| self.fixup.fix_solid(&mut s).ok().map(|()| s))
                    .collect();
                if had_solids && e.solids.is_empty() {
                    return None;
                }
                self.fixup.fix_entity(&mut e, kinds);
                Some(e)
            })
            .collect()
    }

    pub fn point_entities(&self, kinds: Option<KeyFixupFn<'_>>, skip_hidden: bool) -> Vec<Entity> {
        self.map
            .entities
            .iter()
            .filter(|e| e.solids.is_empty() && !is_instance(e) && !is_parms(e))
            .filter(|e| !(skip_hidden && is_hidden_object(e.hidden, &e.editor)))
            .map(|e| {
                let mut e = e.clone();
                self.fixup.fix_entity(&mut e, kinds);
                e
            })
            .collect()
    }

    pub fn groups(&self) -> &[Group] {
        &self.map.world.groups
    }
}

#[derive(Clone, Debug, Default)]
pub struct InstanceFragment {
    pub solids: Vec<Solid>,
    pub entities: Vec<Entity>,
    pub issues: Vec<InstanceIssue>,
}

pub fn instance_fragment(
    child: &Arc<Map>,
    child_path: &Arc<Path>,
    inst: &Entity,
    source: &mut dyn InstanceSource,
    opts: &ExpandOptions<'_>,
) -> InstanceFragment {
    let mut root_done = false;
    let mut wrapped = |req: &LoadRequest<'_>| -> Loaded {
        if req.depth == 0 && !root_done {
            root_done = true;
            return Loaded::Ready { path: Arc::clone(child_path), map: Arc::clone(child) };
        }
        source.load(req)
    };
    let exp = expand(&[inst], &mut wrapped, opts).pop().unwrap_or_default();
    fragment_of(&exp, opts)
}

pub fn fragment_of(exp: &Expansion, opts: &ExpandOptions<'_>) -> InstanceFragment {
    let mut frag = InstanceFragment { issues: exp.issues.clone(), ..InstanceFragment::default() };
    for p in &exp.pieces {
        frag.solids.extend(p.world_solids(opts.skip_hidden));
        frag.entities.extend(p.entities(opts.kinds, opts.skip_hidden));
    }
    frag.entities.extend(exp.unresolved.iter().cloned());
    frag
}

pub fn own_bounds(map: &Map, skip_hidden: bool) -> Aabb {
    let mut b = Aabb::EMPTY;
    let solids =
        map.world.solids.iter().chain(map.entities.iter().filter(|e| !is_instance(e)).flat_map(|e| e.solids.iter()));
    for s in solids {
        if skip_hidden && is_hidden_object(s.hidden, &s.editor) {
            continue;
        }
        let g = hp_geom::build_solid(&s.sides);
        if !g.aabb.is_empty() && g.aabb.min.is_finite() && g.aabb.max.is_finite() {
            b = b.union(&g.aabb);
        }
    }
    for e in &map.entities {
        if !e.solids.is_empty()
            || is_instance(e)
            || is_parms(e)
            || (skip_hidden && is_hidden_object(e.hidden, &e.editor))
        {
            continue;
        }
        if let Some(o) = e.origin().filter(|o| o.is_finite()) {
            b = b.union(&Aabb::new(o - DVec3::splat(8.0), o + DVec3::splat(8.0)));
        }
    }
    b
}

pub fn transformed_aabb(b: &Aabb, m: &DMat4) -> Aabb {
    if b.is_empty() {
        return Aabb::EMPTY;
    }
    let mut out = Aabb::EMPTY;
    for i in 0..8 {
        let c = DVec3::new(
            if i & 1 != 0 { b.max.x } else { b.min.x },
            if i & 2 != 0 { b.max.y } else { b.min.y },
            if i & 4 != 0 { b.max.z } else { b.min.z },
        );
        out.extend(m.transform_point3(c));
    }
    out
}

#[derive(Clone, Copy, Default)]
pub struct CollapseOptions<'a> {
    pub kinds: Option<KeyFixupFn<'a>>,
    pub skip_hidden: bool,
}

impl std::fmt::Debug for CollapseOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollapseOptions")
            .field("kinds", &self.kinds.is_some())
            .field("skip_hidden", &self.skip_hidden)
            .finish()
    }
}

#[derive(Clone, Debug, Default)]
pub struct CollapseReport {
    pub collapsed: Vec<hp_vmf::Id>,
    pub objects: Vec<crate::ObjRef>,
    pub skipped: Vec<(hp_vmf::Id, InstanceIssue)>,
    pub warnings: Vec<(hp_vmf::Id, InstanceIssue)>,
}

pub fn fixed_up_names(expansions: &[Expansion], kinds: Option<KeyFixupFn<'_>>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for exp in expansions {
        for p in &exp.pieces {
            for e in p.map.entities.iter().filter(|e| !is_instance(e) && !is_parms(e)) {
                let Some(raw) = e.get("targetname").filter(|s| !s.trim().is_empty()) else { continue };
                let name = match kinds.and_then(|f| f(&e.classname, "targetname")) {
                    Some(KeyFixup::Keep) => p.fixup.replace(raw).into_owned(),
                    _ => p.fixup.fix_name(&p.fixup.replace(raw)),
                };
                names.push(name.to_ascii_lowercase());
            }
        }
    }
    names.sort_unstable();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_vmf::Connection;

    fn ent(class: &str, props: &[(&str, &str)]) -> Entity {
        Entity {
            id: 1,
            classname: class.to_owned(),
            props: props.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
            ..Entity::default()
        }
    }

    fn cube(id: u32, min: DVec3, max: DVec3) -> Solid {
        let (x0, y0, z0) = (min.x, min.y, min.z);
        let (x1, y1, z1) = (max.x, max.y, max.z);
        let v = DVec3::new;
        let floor = (DVec3::X, DVec3::NEG_Y);
        let wall_y = (DVec3::X, DVec3::NEG_Z);
        let wall_x = (DVec3::Y, DVec3::NEG_Z);
        let faces = [
            ([v(x0, y1, z1), v(x1, y1, z1), v(x1, y0, z1)], floor),
            ([v(x0, y0, z0), v(x1, y0, z0), v(x1, y1, z0)], floor),
            ([v(x1, y1, z1), v(x0, y1, z1), v(x0, y1, z0)], wall_y),
            ([v(x1, y0, z0), v(x0, y0, z0), v(x0, y0, z1)], wall_y),
            ([v(x1, y1, z0), v(x1, y0, z0), v(x1, y0, z1)], wall_x),
            ([v(x0, y1, z1), v(x0, y0, z1), v(x0, y0, z0)], wall_x),
        ];
        let sides = faces
            .iter()
            .enumerate()
            .map(|(i, (plane, (u, w)))| hp_vmf::Side {
                id: id * 10 + i as u32,
                plane: *plane,
                material: "DEV/DEV_MEASUREGENERIC01B".to_owned(),
                uaxis: hp_vmf::TexAxis { axis: *u, shift: 3.0, scale: 0.25 },
                vaxis: hp_vmf::TexAxis { axis: *w, shift: 5.0, scale: 0.25 },
                ..Default::default()
            })
            .collect();
        Solid { id, sides, ..Solid::default() }
    }

    fn child_map() -> Map {
        let mut m = Map::new_empty();
        m.world.solids.push(cube(2, DVec3::ZERO, DVec3::splat(16.0)));
        let mut door = ent("func_door", &[("targetname", "door"), ("speed", "$speed")]);
        door.solids.push(cube(3, DVec3::new(32.0, 0.0, 0.0), DVec3::new(48.0, 16.0, 64.0)));
        door.connections.push(Connection {
            output: "OnOpen".into(),
            target: "relay".into(),
            input: "Trigger".into(),
            ..Connection::default()
        });
        door.connections.push(Connection {
            output: "OnClose".into(),
            target: "@global".into(),
            input: "$input".into(),
            ..Connection::default()
        });
        m.entities.push(door);
        m.entities.push(ent(
            "light",
            &[("origin", "8 0 0"), ("angles", "0 90 0"), ("targetname", "lamp"), ("_light", "$color 200")],
        ));
        m.entities
            .push(ent("func_instance_parms", &[("parm1", "$color color255 255 0 0"), ("parm2", "$speed integer")]));
        m
    }

    fn inst(props: &[(&str, &str)]) -> Entity {
        ent("func_instance", props)
    }

    #[test]
    fn spec_reads_keys_like_vbsp() {
        let e = inst(&[
            ("targetname", " "),
            ("name", "named"),
            ("file", " instances/door.vmf "),
            ("origin", "10 x 30"),
            ("angles", "0 90"),
            ("fixup_style", "1"),
            ("replace01", "$color 255 0 0"),
            ("Replace02", "novalue"),
            ("replace10", "$speed 100"),
        ]);
        let s = InstanceSpec::of(&e).unwrap();
        assert_eq!(s.name, "named");
        assert_eq!(s.file, "instances/door.vmf");
        assert_eq!(s.origin, DVec3::new(10.0, 0.0, 30.0));
        assert_eq!(s.angles, DVec3::new(0.0, 90.0, 0.0));
        assert_eq!(s.style, FixupStyle::Postfix);
        assert_eq!(s.replaces, vec![("$color".into(), "255 0 0".into()), ("$speed".into(), "100".into())]);
        assert!(InstanceSpec::of(&ent("light", &[])).is_none());
        assert_eq!(FixupStyle::from_value(Some("junk")), FixupStyle::Prefix);
        assert_eq!(FixupStyle::from_value(Some("2")), FixupStyle::None);
        assert_eq!(FixupStyle::from_value(Some("7")), FixupStyle::None);
        assert_eq!(leading_float("12.5abc"), 12.5);
        assert_eq!(leading_float("-3e2x"), -300.0);
        assert_eq!(leading_float("1e"), 1.0);
        assert_eq!(leading_float("nan"), 0.0);
        assert_eq!(parse_vec_lenient("1e999 2"), DVec3::new(0.0, 2.0, 0.0));
    }

    #[test]
    fn substitution_is_case_insensitive_and_bounded() {
        assert_eq!(substitute("a $Color b $color", "$COLOR", "red").as_deref(), Some("a red b red"));
        assert_eq!(substitute("nothing", "$x", "y"), None);
        assert_eq!(substitute("", "$x", "y"), None);
        assert_eq!(substitute("é$xé", "$x", "y").as_deref(), Some("éyé"));
        let huge = "x".repeat(MAX_VALUE_LEN);
        assert_eq!(substitute("$a$a", "$a", &huge), None, "too long: unchanged");
        let f = Fixup::new(
            &InstanceSpec {
                file: String::new(),
                origin: DVec3::ZERO,
                angles: DVec3::ZERO,
                name: "i".into(),
                style: FixupStyle::Prefix,
                replaces: vec![("$a".into(), "$b".into()), ("$b".into(), "done".into())],
            },
            1,
        );
        assert_eq!(f.replace("$a"), "done");
    }

    #[test]
    fn names_follow_the_style() {
        let mut spec = InstanceSpec::of(&inst(&[("targetname", "room")])).unwrap();
        let f = Fixup::new(&spec, 3);
        assert_eq!(f.fix_name("door"), "room-door");
        assert_eq!(f.fix_name("@global"), "@global");
        assert_eq!(f.fix_name("!activator"), "!activator");
        assert_eq!(f.fix_name(""), "");
        spec.style = FixupStyle::Postfix;
        assert_eq!(Fixup::new(&spec, 3).fix_name("door"), "door-room");
        spec.style = FixupStyle::None;
        assert_eq!(Fixup::new(&spec, 3).fix_name("door"), "door");
        spec.name.clear();
        spec.style = FixupStyle::Prefix;
        assert_eq!(Fixup::new(&spec, 3).fix_name("door"), "InstanceAuto3-door");
    }

    #[test]
    fn rope_chains_keep_their_links_without_entity_definitions() {
        let spec = InstanceSpec::of(&inst(&[("targetname", "room"), ("origin", "100 0 0")])).unwrap();
        let f = Fixup::new(&spec, 1);
        let mut start =
            ent("move_rope", &[("targetname", "a"), ("NextKey", "b"), ("Slack", "25"), ("origin", "0 0 64")]);
        let mut end = ent("keyframe_rope", &[("targetname", "b"), ("NextKey", ""), ("origin", "64 0 64")]);
        f.fix_entity(&mut start, None);
        f.fix_entity(&mut end, None);
        assert_eq!(start.get("targetname"), Some("room-a"));
        assert_eq!(start.get("NextKey"), Some("room-b"));
        assert_eq!(end.get("targetname"), start.get("NextKey"));
        assert_eq!(end.get("NextKey"), Some(""));
        assert_eq!(start.get("Slack"), Some("25"));
        assert_eq!(start.get("origin"), Some("100 0 64"));
    }

    #[test]
    fn brush_entities_are_turned_once() {
        let mut child = Map::new_empty();
        let mut wheel = ent("func_rotating", &[("angles", "0 30 0"), ("origin", "8 0 8")]);
        wheel.solids.push(cube(4, DVec3::ZERO, DVec3::splat(16.0)));
        child.entities.push(wheel);
        child.entities.push(ent("info_target", &[("angles", "0 30 0"), ("origin", "8 0 8")]));
        let root = inst(&[("file", "x.vmf"), ("origin", "100 0 0"), ("angles", "0 90 0")]);
        let path: Arc<Path> = Arc::from(Path::new("C:/maps/x.vmf"));
        let mut none = |_: &LoadRequest<'_>| Loaded::Missing;
        let frag = instance_fragment(&Arc::new(child), &path, &root, &mut none, &ExpandOptions::default());
        let wheel = &frag.entities[0];
        assert_eq!(wheel.get("angles"), Some("0 30 0"));
        assert_eq!(wheel.get("origin"), Some("100 8 8"));
        let b = hp_geom::build_solid(&wheel.solids[0].sides).aabb;
        assert!((b.min - DVec3::new(84.0, 0.0, 0.0)).length() < 1e-6, "{b:?}");
        assert_eq!(frag.entities[1].get("angles"), Some("0 120 0"));
    }

    #[test]
    fn entity_keys_outputs_and_solids_are_fixed_up() {
        let root = inst(&[
            ("targetname", "a"),
            ("file", "x.vmf"),
            ("origin", "100 0 0"),
            ("angles", "0 90 0"),
            ("replace01", "$color 10 20 30"),
            ("replace02", "$speed 250"),
            ("replace03", "$input Open"),
        ]);
        let child = Arc::new(child_map());
        let path: Arc<Path> = Arc::from(Path::new("C:/maps/x.vmf"));
        let mut none = |_: &LoadRequest<'_>| Loaded::Missing;
        let frag = instance_fragment(&child, &path, &root, &mut none, &ExpandOptions::default());
        assert!(frag.issues.is_empty(), "{:?}", frag.issues);
        assert_eq!(frag.solids.len(), 1);
        let b = hp_geom::build_solid(&frag.solids[0].sides).aabb;
        assert!((b.min - DVec3::new(84.0, 0.0, 0.0)).length() < 1e-6, "{b:?}");
        assert!((b.max - DVec3::new(100.0, 16.0, 16.0)).length() < 1e-6, "{b:?}");
        assert_eq!(frag.entities.len(), 2, "parms dropped");
        let door = &frag.entities[0];
        assert_eq!(door.get("targetname"), Some("a-door"));
        assert_eq!(door.get("speed"), Some("250"));
        assert_eq!(door.connections[0].target, "a-relay");
        assert_eq!(door.connections[1].target, "@global");
        assert_eq!(door.connections[1].input, "Open");
        let light = &frag.entities[1];
        assert_eq!(light.get("origin"), Some("100 8 0"));
        assert_eq!(light.get("angles"), Some("0 180 0"));
        assert_eq!(light.get("_light"), Some("10 20 30 200"));
        assert_eq!(light.get("targetname"), Some("a-lamp"));
        let m = InstanceSpec::of(&root).unwrap().matrix();
        for (src, dst) in child.world.solids[0].sides.iter().zip(&frag.solids[0].sides) {
            for p in src.plane {
                let (a, b) = (hp_geom::texel_uv(src, p), hp_geom::texel_uv(dst, m.transform_point3(p)));
                assert!((a - b).length() < 1e-6, "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn known_classes_use_the_fgd_types() {
        let root = inst(&[("targetname", "a"), ("file", "x.vmf"), ("angles", "0 90 0")]);
        let spec = InstanceSpec::of(&root).unwrap();
        let f = Fixup::new(&spec, 1);
        let kinds = |class: &str, key: &str| -> Option<KeyFixup> {
            (class == "light_spot").then_some(match key {
                "pitch" => KeyFixup::AngleNegativePitch,
                "mytarget" => KeyFixup::Name,
                _ => KeyFixup::Keep,
            })
        };
        let mut e = ent("light_spot", &[("pitch", "-45"), ("mytarget", "x"), ("target", "y"), ("angles", "0 0 0")]);
        f.fix_entity(&mut e, Some(&kinds));
        assert_eq!(e.get("mytarget"), Some("a-x"));
        assert_eq!(e.get("target"), Some("y"), "known class: only its typed keys");
        assert_eq!(e.get("angles"), Some("0 90 0"), "angles always rotate");
        assert_eq!(e.get("pitch"), Some("-45"), "a pure yaw keeps the pitch");
        let mut u = ent("my_custom", &[("target", "y"), ("origin", "1 0 0")]);
        f.fix_entity(&mut u, Some(&kinds));
        assert_eq!(u.get("target"), Some("a-y"), "unknown class: common keys");
        assert_eq!(u.get("origin"), Some("0 1 0"));
        assert_eq!(f.fix_value(KeyFixup::Axis, "1 0 0, 2 0 0").as_deref(), Some("0 1 0, 0 2 0"));
        let mut o = ent("info_overlay", &[("BasisOrigin", "1 0 0"), ("BasisNormal", "1 0 0"), ("uv0", "-8 -8 0")]);
        f.fix_entity(&mut o, None);
        assert_eq!(o.get("BasisOrigin"), Some("0 1 0"));
        assert_eq!(o.get("BasisNormal"), Some("0 1 0"));
        assert_eq!(o.get("uv0"), Some("-8 -8 0"));
    }

    #[test]
    fn nested_instances_follow_their_parent_and_cycles_stop() {
        let mut a = Map::new_empty();
        a.entities.push(inst(&[("targetname", "inner"), ("file", "$which"), ("origin", "10 0 0")]));
        let mut b = Map::new_empty();
        b.world.solids.push(cube(5, DVec3::ZERO, DVec3::splat(8.0)));
        b.entities.push(ent("info_target", &[("targetname", "t"), ("origin", "0 0 0")]));
        b.entities.push(inst(&[("file", "a.vmf")]));
        let (a, b) = (Arc::new(a), Arc::new(b));
        let mut asked: Vec<(String, usize)> = Vec::new();
        let mut source = |req: &LoadRequest<'_>| {
            asked.push((req.file.to_owned(), req.depth));
            match req.file {
                "a.vmf" => Loaded::Ready { path: Arc::from(Path::new("/m/a.vmf")), map: Arc::clone(&a) },
                "b.vmf" => Loaded::Ready { path: Arc::from(Path::new("/m/b.vmf")), map: Arc::clone(&b) },
                _ => Loaded::Missing,
            }
        };
        let root =
            inst(&[("targetname", "top"), ("file", "a.vmf"), ("origin", "0 100 0"), ("replace01", "$which b.vmf")]);
        let opts = ExpandOptions::default();
        let exp = expand(&[&root], &mut source, &opts).pop().unwrap();
        assert_eq!(exp.pieces.len(), 2);
        assert_eq!(exp.issues, vec![InstanceIssue::Cycle { file: "a.vmf".into() }]);
        assert_eq!(exp.unresolved.len(), 1, "the cyclic instance is kept as is");
        let frag = fragment_of(&exp, &opts);
        let t = frag.entities.iter().find(|e| e.classname == "info_target").unwrap();
        assert_eq!(t.get("targetname"), Some("top-inner-t"), "names fixed by both levels");
        assert_eq!(t.get("origin"), Some("10 100 0"));
        assert_eq!(frag.solids.len(), 1);
        assert_eq!(asked[1], ("b.vmf".to_owned(), 1));
        let mut missing = |_: &LoadRequest<'_>| Loaded::Missing;
        let e = expand(&[&root], &mut missing, &opts).pop().unwrap();
        assert!(!e.is_placed());
        assert_eq!(e.issues, vec![InstanceIssue::Missing { file: "a.vmf".into(), depth: 0 }]);
        let mut pending = |_: &LoadRequest<'_>| Loaded::Pending;
        assert!(expand(&[&root], &mut pending, &opts)[0].is_pending());
        let no_file = inst(&[]);
        assert_eq!(expand(&[&no_file], &mut pending, &opts)[0].issues, vec![InstanceIssue::NoFile]);
    }

    #[test]
    fn nested_parameters_pass_through() {
        let mut a = Map::new_empty();
        a.entities.push(inst(&[("targetname", "inner"), ("file", "b.vmf"), ("replace01", "$color $color")]));
        a.entities.push(ent("func_instance_parms", &[("parm1", "$color color255"), ("parm2", "$speed integer 75")]));
        let mut b = Map::new_empty();
        b.entities.push(ent(
            "prop_static",
            &[("targetname", "crate"), ("model", "models/props_junk/wood_crate001a.mdl"), ("skin", "$speed")],
        ));
        b.entities.push(ent("light", &[("targetname", "lamp"), ("_light", "$color 200")]));
        let (a, b) = (Arc::new(a), Arc::new(b));
        let mut source = |req: &LoadRequest<'_>| match req.file {
            "a.vmf" => Loaded::Ready { path: Arc::from(Path::new("/m/a.vmf")), map: Arc::clone(&a) },
            "b.vmf" => Loaded::Ready { path: Arc::from(Path::new("/m/b.vmf")), map: Arc::clone(&b) },
            _ => Loaded::Missing,
        };
        let opts = ExpandOptions::default();
        let cases = [
            ("0", "0 255 0", "ia-inner-lamp", "75"),
            ("1", "0 0 255", "inner-ia-lamp", "75"),
            ("2", "255 0 0", "inner-lamp", "75"),
        ];
        for (style, color, lamp_name, skin) in cases {
            let root = inst(&[
                ("targetname", "ia"),
                ("file", "a.vmf"),
                ("fixup_style", style),
                ("replace01", &format!("$color {color}")),
            ]);
            let exp = expand(&[&root], &mut source, &opts).pop().unwrap();
            let frag = fragment_of(&exp, &opts);
            let prop = frag.entities.iter().find(|e| e.classname == "prop_static").unwrap();
            assert_eq!(prop.get("model"), Some("models/props_junk/wood_crate001a.mdl"));
            assert_eq!(prop.get("skin"), Some(skin), "parameter default of the instance file");
            let light = frag.entities.iter().find(|e| e.classname == "light").unwrap();
            assert_eq!(light.get("_light"), Some(format!("{color} 200").as_str()));
            assert_eq!(light.get("targetname"), Some(lamp_name));
        }
        let root =
            inst(&[("targetname", "ib"), ("file", "a.vmf"), ("replace01", "$color 1 2 3"), ("replace02", "$speed 9")]);
        let exp = expand(&[&root], &mut source, &opts).pop().unwrap();
        let frag = fragment_of(&exp, &opts);
        let prop = frag.entities.iter().find(|e| e.classname == "prop_static").unwrap();
        assert_eq!(prop.get("skin"), Some("9"), "an outer variable reaches the nested file");
        assert_eq!(prop.get("targetname"), Some("ib-inner-crate"));
        let mut kept = inst(&[("targetname", "x"), ("file", "missing.vmf"), ("replace01", "$color $color")]);
        Fixup::new(&InstanceSpec::of(&root).unwrap(), 1).fix_entity(&mut kept, None);
        assert_eq!(kept.get("replace01"), Some("$color 1 2 3"), "only the value of a replace key is substituted");
    }

    #[test]
    fn exponential_trees_are_bounded() {
        let mut m = Map::new_empty();
        for i in 0..4 {
            m.entities.push(inst(&[("file", "self.vmf"), ("origin", &format!("{i} 0 0"))]));
        }
        let m = Arc::new(m);
        let mut n = 0u64;
        let mut source = |_: &LoadRequest<'_>| {
            n += 1;
            Loaded::Ready { path: Arc::from(PathBuf::from(format!("/m/{n}.vmf"))), map: Arc::clone(&m) }
        };
        let root = inst(&[("file", "self.vmf")]);
        let exp = expand(&[&root], &mut source, &ExpandOptions::default()).pop().unwrap();
        assert!(exp.pieces.len() <= MAX_PIECES);
        assert!(
            exp.issues.contains(&InstanceIssue::TooLarge)
                || exp.issues.iter().any(|i| matches!(i, InstanceIssue::TooDeep { .. }))
        );
    }

    #[test]
    fn auto_names_follow_the_merge_order() {
        let mut m = Map::new_empty();
        m.entities.push(ent("info_target", &[("targetname", "t")]));
        let m = Arc::new(m);
        let mut source = |req: &LoadRequest<'_>| Loaded::Ready {
            path: Arc::from(PathBuf::from(format!("/m/{}", req.file))),
            map: Arc::clone(&m),
        };
        let a = inst(&[("file", "a.vmf")]);
        let b = inst(&[("file", "b.vmf")]);
        let named = inst(&[("file", "c.vmf"), ("targetname", "n")]);
        let opts = ExpandOptions::default();
        let exps = expand(&[&a, &named, &b], &mut source, &opts);
        let names: Vec<String> = exps.iter().map(|e| e.pieces[0].fixup.fix_name("t")).collect();
        assert_eq!(names, vec!["InstanceAuto1-t", "n-t", "InstanceAuto3-t"]);
        let all = fixed_up_names(&exps, None);
        assert_eq!(all, vec!["instanceauto1-t", "instanceauto3-t", "n-t"]);
    }

    #[test]
    fn hidden_objects_are_skipped_on_request() {
        let mut m = child_map();
        m.world.solids[0].hidden = true;
        m.entities[1].editor.visgroup_shown = false;
        let m = Arc::new(m);
        let path: Arc<Path> = Arc::from(Path::new("/m/x.vmf"));
        let root = inst(&[("file", "x.vmf")]);
        let mut none = |_: &LoadRequest<'_>| Loaded::Missing;
        let opts = ExpandOptions { skip_hidden: true, ..ExpandOptions::default() };
        let frag = instance_fragment(&m, &path, &root, &mut none, &opts);
        assert!(frag.solids.is_empty());
        assert_eq!(frag.entities.len(), 1, "the hidden light is skipped");
        let all = instance_fragment(&m, &path, &root, &mut none, &ExpandOptions::default());
        assert_eq!((all.solids.len(), all.entities.len()), (1, 2));
        let b = own_bounds(&m, false);
        assert!(!b.is_empty());
        assert_eq!(b.max.z, 64.0);
    }

    #[test]
    fn parameters_and_slots() {
        let m = child_map();
        let parms = map_parms(&m);
        assert_eq!(
            parms,
            vec![
                InstanceParm { name: "$color".into(), kind: "color255".into(), default: Some("255 0 0".into()) },
                InstanceParm { name: "$speed".into(), kind: "integer".into(), default: None },
            ]
        );
        assert_eq!(parse_parm("$x"), Some(InstanceParm { name: "$x".into(), kind: "string".into(), default: None }));
        assert_eq!(parse_parm("x string"), None);
        assert_eq!(parse_parm(""), None);
        let mut e = inst(&[("replace01", "$color 1 2 3"), ("replace03", "$speed 9")]);
        assert_eq!(parm_value(&e, "$COLOR"), Some(("replace01", "1 2 3")));
        assert_eq!(next_free_replace_key(&e), "replace02");
        e.set("replace02", "$a b");
        assert_eq!(next_free_replace_key(&e), "replace04");
        assert!(is_replace_key("REPLACE99") && !is_replace_key("repl"));
    }

    #[test]
    fn file_lookup_candidates() {
        assert_eq!(normalize_file(" instances\\door "), Some("instances/door.vmf".into()));
        assert_eq!(normalize_file("door.txt"), Some("door.vmf".into()));
        assert_eq!(normalize_file("a.b/door"), Some("a.b/door.vmf".into()));
        assert_eq!(normalize_file("door.VMF"), Some("door.VMF".into()));
        assert_eq!(normalize_file("\\\\server\\share\\x.vmf"), None);
        assert_eq!(normalize_file(""), None);
        let main = Path::new("/sdk/Maps/sub/main.vmf");
        let nested = Path::new("/other/inst/room.vmf");
        let c = candidate_paths("instances/door", Some(main), Some(nested), Some(Path::new("/game")));
        let keys: Vec<String> = c.iter().map(|p| path_key(p)).collect();
        let k = |s: &str| path_key(Path::new(s));
        assert_eq!(
            keys,
            vec![
                k("/sdk/Maps/sub/instances/door.vmf"),
                k("/sdk/Maps/instances/door.vmf"),
                k("/other/inst/instances/door.vmf"),
                k("/game/maps/instances/door.vmf"),
            ]
        );
        assert_eq!(content_path("Instances/Door"), Some("maps/instances/door.vmf".into()));
        assert_eq!(content_path("../x"), None);
        assert_eq!(maps_folder(Path::new("/a/b.vmf")), None);
    }
}
