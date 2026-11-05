use crate::PcfError;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Element(Option<usize>),
    Int(i32),
    Float(f32),
    Bool(bool),
    Str(String),
    Color([u8; 4]),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Elements(Vec<Option<usize>>),
    Ints(Vec<i32>),
    Floats(Vec<f32>),
    Vec3s(Vec<[f32; 3]>),
    Strs(Vec<String>),
    External(String),
    References(Vec<Reference>),
    Raw { kind: u8, data: Vec<u8> },
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Reference {
    Element(Option<usize>),
    External(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    pub kind: String,
    pub name: String,
    pub attrs: Vec<(String, Value)>,
}

impl Element {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.attrs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v)
    }

    pub fn float(&self, name: &str) -> Option<f32> {
        match self.get(name)? {
            Value::Float(v) => Some(*v),
            Value::Int(v) => Some(*v as f32),
            Value::Bool(b) => Some(f32::from(u8::from(*b))),
            _ => None,
        }
        .filter(|v| v.is_finite())
    }

    pub fn int(&self, name: &str) -> Option<i32> {
        match self.get(name)? {
            Value::Int(v) => Some(*v),
            Value::Float(v) if v.is_finite() => Some(*v as i32),
            Value::Bool(b) => Some(i32::from(*b)),
            _ => None,
        }
    }

    pub fn bool(&self, name: &str) -> Option<bool> {
        match self.get(name)? {
            Value::Bool(b) => Some(*b),
            Value::Int(v) => Some(*v != 0),
            Value::Float(v) => Some(*v != 0.0),
            _ => None,
        }
    }

    pub fn vec3(&self, name: &str) -> Option<[f32; 3]> {
        match self.get(name)? {
            Value::Vec3(v) if v.iter().all(|c| c.is_finite()) => Some(*v),
            Value::Raw { kind: TYPE_QANGLE, data } if data.len() == 12 => {
                let v =
                    std::array::from_fn(|i| f32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().expect("four bytes")));
                v.iter().all(|c| c.is_finite()).then_some(v)
            }
            _ => None,
        }
    }

    pub fn color(&self, name: &str) -> Option<[u8; 4]> {
        match self.get(name)? {
            Value::Color(c) => Some(*c),
            _ => None,
        }
    }

    pub fn str(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn elements(&self, name: &str) -> Vec<usize> {
        match self.get(name) {
            Some(Value::Elements(v)) => v.iter().flatten().copied().collect(),
            Some(Value::Element(Some(i))) => vec![*i],
            Some(Value::References(refs)) => {
                refs.iter().filter_map(|r| if let Reference::Element(id) = r { *id } else { None }).collect()
            }
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dmx {
    pub version: u32,
    pub elements: Vec<Element>,
    pub ids: Vec<[u8; 16]>,
}

#[derive(Clone, Copy, Debug)]
struct Layout {
    count_i32: bool,
    index_i32: bool,
    names_indexed: bool,
    values_indexed: bool,
    time_is_int: bool,
}

struct Cur<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Cur<'a> {
    fn left(&self) -> usize {
        self.d.len().saturating_sub(self.p)
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.d.get(self.p..self.p.checked_add(n)?)?;
        self.p += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    fn i32(&mut self) -> Option<i32> {
        self.take(4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f32(&mut self) -> Option<f32> {
        self.take(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn count(&mut self, item_min: usize) -> Option<usize> {
        let n = usize::try_from(self.i32()?).ok()?;
        (n.saturating_mul(item_min.max(1)) <= self.left()).then_some(n)
    }
    fn cstr(&mut self) -> Option<&'a [u8]> {
        let rest = self.d.get(self.p..)?;
        let n = rest.iter().position(|&b| b == 0)?;
        self.p += n + 1;
        Some(&rest[..n])
    }
}

const TYPE_ELEMENT: u8 = 1;
const TYPE_INT: u8 = 2;
const TYPE_FLOAT: u8 = 3;
const TYPE_BOOL: u8 = 4;
const TYPE_STRING: u8 = 5;
const TYPE_VOID: u8 = 6;
const TYPE_TIME: u8 = 7;
const TYPE_COLOR: u8 = 8;
const TYPE_VEC2: u8 = 9;
const TYPE_VEC3: u8 = 10;
const TYPE_VEC4: u8 = 11;
const TYPE_QANGLE: u8 = 12;
const TYPE_QUAT: u8 = 13;
const TYPE_MATRIX: u8 = 14;
const ARRAY_BASE: u8 = 14;

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn fixed_size(t: u8, layout: Layout) -> Option<usize> {
    Some(match t {
        TYPE_ELEMENT | TYPE_INT | TYPE_FLOAT | TYPE_COLOR => 4,
        TYPE_BOOL => 1,
        TYPE_TIME => {
            if layout.time_is_int {
                4
            } else {
                16
            }
        }
        TYPE_VEC2 => 8,
        TYPE_VEC3 | TYPE_QANGLE => 12,
        TYPE_VEC4 | TYPE_QUAT => 16,
        TYPE_MATRIX => 64,
        _ => return None,
    })
}

fn element_ref(c: &mut Cur<'_>, count: usize) -> Option<Reference> {
    let i = c.i32()?;
    if i == -2 {
        return Some(Reference::External(text(c.cstr()?)));
    }
    if i == -1 {
        return Some(Reference::Element(None));
    }
    let i = usize::try_from(i).ok()?;
    (i < count).then_some(Reference::Element(Some(i)))
}

fn read_value(c: &mut Cur<'_>, t: u8, layout: Layout, table: &[&[u8]], count: usize) -> Option<Value> {
    let index = |c: &mut Cur<'_>| -> Option<usize> {
        if layout.index_i32 { usize::try_from(c.i32()?).ok() } else { c.u16().map(usize::from) }
    };
    Some(match t {
        TYPE_ELEMENT => match element_ref(c, count)? {
            Reference::Element(i) => Value::Element(i),
            Reference::External(s) => Value::External(s),
        },
        TYPE_INT => Value::Int(c.i32()?),
        TYPE_FLOAT => Value::Float(c.f32()?),
        TYPE_BOOL => Value::Bool(c.u8()? != 0),
        TYPE_STRING => {
            let s = if layout.values_indexed { *table.get(index(c)?)? } else { c.cstr()? };
            Value::Str(text(s))
        }
        TYPE_VOID => {
            let start = c.p;
            let n = c.count(1)?;
            c.take(n)?;
            Value::Raw { kind: t, data: c.d[start..c.p].to_vec() }
        }
        TYPE_COLOR => {
            let b = c.take(4)?;
            Value::Color([b[0], b[1], b[2], b[3]])
        }
        TYPE_VEC2 => Value::Vec2([c.f32()?, c.f32()?]),
        TYPE_VEC3 => Value::Vec3([c.f32()?, c.f32()?, c.f32()?]),
        TYPE_VEC4 => Value::Vec4([c.f32()?, c.f32()?, c.f32()?, c.f32()?]),
        t if t > ARRAY_BASE => {
            let base = t - ARRAY_BASE;
            match base {
                TYPE_ELEMENT => {
                    let n = c.count(4)?;
                    let mut v = Vec::with_capacity(n);
                    for _ in 0..n {
                        v.push(element_ref(c, count)?);
                    }
                    if v.iter().all(|r| matches!(r, Reference::Element(_))) {
                        Value::Elements(
                            v.into_iter()
                                .map(|r| match r {
                                    Reference::Element(i) => i,
                                    Reference::External(_) => unreachable!(),
                                })
                                .collect(),
                        )
                    } else {
                        Value::References(v)
                    }
                }
                TYPE_INT => {
                    let n = c.count(4)?;
                    let mut v = Vec::with_capacity(n);
                    for _ in 0..n {
                        v.push(c.i32()?);
                    }
                    Value::Ints(v)
                }
                TYPE_FLOAT => {
                    let n = c.count(4)?;
                    let mut v = Vec::with_capacity(n);
                    for _ in 0..n {
                        v.push(c.f32()?);
                    }
                    Value::Floats(v)
                }
                TYPE_VEC3 => {
                    let n = c.count(12)?;
                    let mut v = Vec::with_capacity(n);
                    for _ in 0..n {
                        v.push([c.f32()?, c.f32()?, c.f32()?]);
                    }
                    Value::Vec3s(v)
                }
                TYPE_STRING => {
                    let n = c.count(1)?;
                    let mut v = Vec::with_capacity(n);
                    for _ in 0..n {
                        v.push(text(c.cstr()?));
                    }
                    Value::Strs(v)
                }
                TYPE_VOID => {
                    let start = c.p;
                    let n = c.count(4)?;
                    for _ in 0..n {
                        let k = c.count(1)?;
                        c.take(k)?;
                    }
                    Value::Raw { kind: t, data: c.d[start..c.p].to_vec() }
                }
                b => {
                    let start = c.p;
                    let size = fixed_size(b, layout)?;
                    let n = c.count(size)?;
                    c.take(n.checked_mul(size)?)?;
                    Value::Raw { kind: t, data: c.d[start..c.p].to_vec() }
                }
            }
        }
        t => {
            let start = c.p;
            c.take(fixed_size(t, layout)?)?;
            Value::Raw { kind: t, data: c.d[start..c.p].to_vec() }
        }
    })
}

fn parse_with(data: &[u8], body: usize, version: u32, layout: Layout) -> Option<Dmx> {
    let mut c = Cur { d: data, p: body };
    let mut table: Vec<&[u8]> = Vec::new();
    if version >= 2 {
        let n = if layout.count_i32 { c.count(1)? } else { usize::from(c.u16()?) };
        if n > c.left() {
            return None;
        }
        table.reserve(n);
        for _ in 0..n {
            table.push(c.cstr()?);
        }
    }
    let index = |c: &mut Cur<'_>| -> Option<usize> {
        if layout.index_i32 { usize::try_from(c.i32()?).ok() } else { c.u16().map(usize::from) }
    };
    let count = c.count(17)?;
    let mut elements: Vec<Element> = Vec::with_capacity(count);
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = if version >= 2 { *table.get(index(&mut c)?)? } else { c.cstr()? };
        let name = if layout.names_indexed { *table.get(index(&mut c)?)? } else { c.cstr()? };
        ids.push(c.take(16)?.try_into().ok()?);
        elements.push(Element { kind: text(kind), name: text(name), attrs: Vec::new() });
    }
    for e in elements.iter_mut() {
        let n = c.count(2)?;
        e.attrs.reserve(n);
        for _ in 0..n {
            let name = if version >= 2 { *table.get(index(&mut c)?)? } else { c.cstr()? };
            let t = c.u8()?;
            let v = read_value(&mut c, t, layout, &table, count)?;
            e.attrs.push((text(name), v));
        }
    }
    (c.p == data.len()).then_some(Dmx { version, elements, ids })
}

pub fn parse(data: &[u8]) -> Result<Dmx, PcfError> {
    if data.len() > crate::MAX_FILE_SIZE {
        return Err(PcfError::TooLarge);
    }
    let head_end = data.iter().take(256).position(|&b| b == 0).ok_or(PcfError::NotDmx)?;
    let head = std::str::from_utf8(&data[..head_end]).map_err(|_| PcfError::NotDmx)?;
    let mut words = head.split_whitespace();
    let ok = words.next() == Some("<!--")
        && words.next() == Some("dmx")
        && words.next() == Some("encoding")
        && words.next() == Some("binary");
    if !ok {
        return Err(PcfError::NotDmx);
    }
    let version: u32 = words.next().and_then(|w| w.parse().ok()).ok_or(PcfError::NotDmx)?;
    if !(1..=5).contains(&version) {
        return Err(PcfError::Unsupported(version));
    }
    let body = head_end + 1;
    let l = |count_i32, index_i32, names_indexed, values_indexed, time_is_int| Layout {
        count_i32,
        index_i32,
        names_indexed,
        values_indexed,
        time_is_int,
    };
    let candidates: Vec<Layout> = match version {
        1 | 2 => vec![l(false, false, false, false, false)],
        3 => vec![
            l(false, false, false, false, true),
            l(true, false, false, false, true),
            l(false, false, false, false, false),
            l(true, false, false, false, false),
        ],
        4 => vec![
            l(true, false, true, true, true),
            l(false, false, true, true, true),
            l(true, false, true, true, false),
            l(false, false, true, true, false),
        ],
        _ => vec![
            l(true, true, true, true, true),
            l(true, false, true, true, true),
            l(true, true, true, true, false),
            l(true, false, true, true, false),
        ],
    };
    candidates.into_iter().find_map(|layout| parse_with(data, body, version, layout)).ok_or(PcfError::Corrupt)
}

pub fn write(dmx: &Dmx) -> Result<Vec<u8>, PcfError> {
    use std::collections::HashMap;
    let mut strings = Vec::<&str>::new();
    let mut table = HashMap::<&str, i32>::new();
    fn add<'a>(s: &'a str, strings: &mut Vec<&'a str>, table: &mut HashMap<&'a str, i32>) -> Result<(), PcfError> {
        if s.contains('\0') {
            return Err(PcfError::Corrupt);
        }
        if !table.contains_key(s) {
            let index = i32::try_from(strings.len()).map_err(|_| PcfError::TooLarge)?;
            strings.push(s);
            table.insert(s, index);
        }
        Ok(())
    }
    for e in &dmx.elements {
        add(&e.kind, &mut strings, &mut table)?;
        add(&e.name, &mut strings, &mut table)?;
        for (key, value) in &e.attrs {
            add(key, &mut strings, &mut table)?;
            if let Value::Str(s) = value {
                add(s, &mut strings, &mut table)?;
            }
        }
    }
    let mut out = b"<!-- dmx encoding binary 5 format pcf 2 -->\n\0".to_vec();
    let count = |n: usize| i32::try_from(n).map(i32::to_le_bytes).map_err(|_| PcfError::TooLarge);
    out.extend(count(strings.len())?);
    for s in strings {
        out.extend(s.as_bytes());
        out.push(0);
    }
    out.extend(count(dmx.elements.len())?);
    for (index, e) in dmx.elements.iter().enumerate() {
        out.extend(table[&e.kind.as_str()].to_le_bytes());
        out.extend(table[&e.name.as_str()].to_le_bytes());
        out.extend(dmx.ids.get(index).copied().unwrap_or_else(|| generated_id(index)));
    }
    fn reference(out: &mut Vec<u8>, r: &Reference, count: usize) -> Result<(), PcfError> {
        match r {
            Reference::Element(Some(i)) if *i < count => out.extend((*i as i32).to_le_bytes()),
            Reference::Element(None) => out.extend((-1i32).to_le_bytes()),
            Reference::External(s) if !s.contains('\0') => {
                out.extend((-2i32).to_le_bytes());
                out.extend(s.as_bytes());
                out.push(0);
            }
            _ => return Err(PcfError::Corrupt),
        }
        Ok(())
    }
    for e in &dmx.elements {
        out.extend(count(e.attrs.len())?);
        for (key, value) in &e.attrs {
            out.extend(table[&key.as_str()].to_le_bytes());
            let kind = match value {
                Value::Element(_) | Value::External(_) => 1,
                Value::Int(_) => 2,
                Value::Float(_) => 3,
                Value::Bool(_) => 4,
                Value::Str(_) => 5,
                Value::Color(_) => 8,
                Value::Vec2(_) => 9,
                Value::Vec3(_) => 10,
                Value::Vec4(_) => 11,
                Value::Elements(_) | Value::References(_) => 15,
                Value::Ints(_) => 16,
                Value::Floats(_) => 17,
                Value::Strs(_) => 19,
                Value::Vec3s(_) => 24,
                Value::Raw { kind, .. } => *kind,
                Value::Other => return Err(PcfError::Corrupt),
            };
            out.push(kind);
            match value {
                Value::Element(i) => reference(&mut out, &Reference::Element(*i), dmx.elements.len())?,
                Value::External(s) => reference(&mut out, &Reference::External(s.clone()), dmx.elements.len())?,
                Value::Int(v) => out.extend(v.to_le_bytes()),
                Value::Float(v) => out.extend(v.to_le_bytes()),
                Value::Bool(v) => out.push(u8::from(*v)),
                Value::Str(s) => out.extend(table[&s.as_str()].to_le_bytes()),
                Value::Color(v) => out.extend(v),
                Value::Vec2(v) => v.iter().for_each(|f| out.extend(f.to_le_bytes())),
                Value::Vec3(v) => v.iter().for_each(|f| out.extend(f.to_le_bytes())),
                Value::Vec4(v) => v.iter().for_each(|f| out.extend(f.to_le_bytes())),
                Value::Elements(v) => {
                    out.extend(count(v.len())?);
                    for r in v {
                        reference(&mut out, &Reference::Element(*r), dmx.elements.len())?;
                    }
                }
                Value::References(v) => {
                    out.extend(count(v.len())?);
                    for r in v {
                        reference(&mut out, r, dmx.elements.len())?;
                    }
                }
                Value::Ints(v) => {
                    out.extend(count(v.len())?);
                    v.iter().for_each(|i| out.extend(i.to_le_bytes()));
                }
                Value::Floats(v) => {
                    out.extend(count(v.len())?);
                    v.iter().for_each(|f| out.extend(f.to_le_bytes()));
                }
                Value::Vec3s(v) => {
                    out.extend(count(v.len())?);
                    v.iter().flatten().for_each(|f| out.extend(f.to_le_bytes()));
                }
                Value::Strs(v) => {
                    out.extend(count(v.len())?);
                    for s in v {
                        if s.contains('\0') {
                            return Err(PcfError::Corrupt);
                        }
                        out.extend(s.as_bytes());
                        out.push(0);
                    }
                }
                Value::Raw { data, .. } => out.extend(data),
                Value::Other => return Err(PcfError::Corrupt),
            }
            if out.len() > crate::MAX_FILE_SIZE {
                return Err(PcfError::TooLarge);
            }
        }
    }
    parse(&out)?;
    Ok(out)
}

pub(crate) fn generated_id(index: usize) -> [u8; 16] {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut id = [0; 16];
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    id[..8].copy_from_slice(&(nanos ^ index as u64).to_le_bytes());
    id[8..].copy_from_slice(&NEXT.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    id
}
