use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use glam::{DMat4, DVec2, DVec3};

use crate::{
    Alpha, Material, MeshObject, Model, NormalKind, Result, TextureSource, Triangle, UpAxis, base64, find_file,
    invalid, push_polygon, smooth_normals,
};

const MAX_ARRAY: usize = 200_000_000;
const CREASE_DEG: f64 = 60.0;
const MAGIC: &[u8] = b"Kaydara FBX Binary  \0";

#[derive(Clone, Debug, PartialEq)]
enum Prop {
    Int(i64),
    Float(f64),
    Str(String),
    Ints(Vec<i64>),
    Floats(Vec<f64>),
    Bytes(Vec<u8>),
}

impl Prop {
    fn int(&self) -> Option<i64> {
        match self {
            Prop::Int(v) => Some(*v),
            Prop::Float(f) => Some(*f as i64),
            _ => None,
        }
    }
    fn float(&self) -> Option<f64> {
        match self {
            Prop::Float(v) => Some(*v),
            Prop::Int(v) => Some(*v as f64),
            _ => None,
        }
    }
    fn str(&self) -> Option<&str> {
        match self {
            Prop::Str(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Node {
    name: String,
    props: Vec<Prop>,
    children: Vec<Node>,
}

impl Node {
    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
    fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
    fn p70(&self, key: &str) -> Option<&[Prop]> {
        self.child("Properties70")?
            .children_named("P")
            .find(|p| p.props.first().and_then(Prop::str) == Some(key))
            .map(|p| p.props.get(4..).unwrap_or(&[]))
    }
    fn p70_vec(&self, key: &str) -> Option<DVec3> {
        let v = self.p70(key)?;
        Some(DVec3::new(v.first()?.float()?, v.get(1)?.float()?, v.get(2)?.float()?))
    }
    fn p70_int(&self, key: &str) -> Option<i64> {
        self.p70(key)?.first()?.int()
    }
    fn p70_float(&self, key: &str) -> Option<f64> {
        self.p70(key)?.first()?.float()
    }
    fn array_f(&self, child: &str) -> Option<Cow<'_, [f64]>> {
        match self.child(child)?.props.first()? {
            Prop::Floats(v) => Some(Cow::Borrowed(v)),
            Prop::Ints(v) => Some(Cow::Owned(v.iter().map(|&i| i as f64).collect())),
            _ => None,
        }
    }
    fn array_i(&self, child: &str) -> Option<Cow<'_, [i64]>> {
        match self.child(child)?.props.first()? {
            Prop::Ints(v) => Some(Cow::Borrowed(v)),
            Prop::Floats(v) => Some(Cow::Owned(v.iter().map(|&f| f as i64).collect())),
            Prop::Int(v) => Some(Cow::Owned(vec![*v])),
            _ => None,
        }
    }
    fn text(&self, child: &str) -> Option<&str> {
        self.child(child)?.props.first()?.str()
    }
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
    wide: bool,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end =
            self.pos.checked_add(n).filter(|e| *e <= self.b.len()).ok_or_else(|| invalid("FBX: truncated file"))?;
        let s = &self.b[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn u64(&mut self) -> Result<u64> {
        let s = self.take(8)?;
        Ok(u64::from_le_bytes(s.try_into().map_err(|_| invalid("FBX"))?))
    }
    fn offset(&mut self) -> Result<u64> {
        if self.wide { self.u64() } else { self.u32().map(u64::from) }
    }

    fn array(&mut self, elem: usize) -> Result<Vec<u8>> {
        let count = self.u32()? as usize;
        let encoding = self.u32()?;
        let len = self.u32()? as usize;
        let data = self.take(len)?;
        let want =
            count.checked_mul(elem).filter(|n| *n <= MAX_ARRAY).ok_or_else(|| invalid("FBX: array too large"))?;
        let raw = if encoding == 1 {
            let mut out = Vec::with_capacity(want);
            flate2::read::ZlibDecoder::new(data)
                .take(want as u64)
                .read_to_end(&mut out)
                .map_err(|e| invalid(format!("FBX: {e}")))?;
            out
        } else {
            data.to_vec()
        };
        if raw.len() < want {
            return Err(invalid("FBX: short array"));
        }
        Ok(raw)
    }

    fn prop(&mut self) -> Result<Prop> {
        let t = self.u8()?;
        Ok(match t {
            b'Y' => {
                let s = self.take(2)?;
                Prop::Int(i64::from(i16::from_le_bytes([s[0], s[1]])))
            }
            b'C' => Prop::Int(i64::from(self.u8()?)),
            b'I' => Prop::Int(i64::from(self.u32()? as i32)),
            b'L' => Prop::Int(self.u64()? as i64),
            b'F' => {
                let s = self.take(4)?;
                Prop::Float(f64::from(f32::from_le_bytes([s[0], s[1], s[2], s[3]])))
            }
            b'D' => Prop::Float(f64::from_bits(self.u64()?)),
            b'S' => {
                let n = self.u32()? as usize;
                let s = String::from_utf8_lossy(self.take(n)?).into_owned();
                Prop::Str(s.split("\u{0}\u{1}").next().unwrap_or("").to_owned())
            }
            b'R' => {
                let n = self.u32()? as usize;
                Prop::Bytes(self.take(n)?.to_vec())
            }
            b'f' => Prop::Floats(
                self.array(4)?
                    .chunks_exact(4)
                    .map(|c| f64::from(f32::from_le_bytes([c[0], c[1], c[2], c[3]])))
                    .collect(),
            ),
            b'd' => Prop::Floats(
                self.array(8)?
                    .chunks_exact(8)
                    .map(|c| f64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]))
                    .collect(),
            ),
            b'i' => Prop::Ints(
                self.array(4)?
                    .chunks_exact(4)
                    .map(|c| i64::from(i32::from_le_bytes([c[0], c[1], c[2], c[3]])))
                    .collect(),
            ),
            b'l' => Prop::Ints(
                self.array(8)?
                    .chunks_exact(8)
                    .map(|c| i64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]))
                    .collect(),
            ),
            b'b' | b'c' => Prop::Ints(self.array(1)?.into_iter().map(i64::from).collect()),
            other => return Err(invalid(format!("FBX: unknown property type {}", other as char))),
        })
    }

    fn node(&mut self, depth: usize) -> Result<Option<Node>> {
        if depth > 64 {
            return Err(invalid("FBX: nodes nested too deep"));
        }
        let end = self.offset()? as usize;
        let count = self.offset()?;
        let _len = self.offset()?;
        let name_len = self.u8()? as usize;
        if end == 0 && count == 0 && name_len == 0 {
            return Ok(None);
        }
        let name = String::from_utf8_lossy(self.take(name_len)?).into_owned();
        let mut node = Node { name, ..Node::default() };
        for _ in 0..count.min(1 << 20) {
            node.props.push(self.prop()?);
        }
        if end > self.b.len() {
            return Err(invalid("FBX: bad node size"));
        }
        while self.pos < end {
            match self.node(depth + 1)? {
                Some(c) => node.children.push(c),
                None => break,
            }
        }
        self.pos = end.max(self.pos);
        Ok(Some(node))
    }
}

fn parse_binary(bytes: &[u8]) -> Result<(Node, u32)> {
    let version = u32::from_le_bytes(bytes.get(23..27).ok_or_else(|| invalid("FBX"))?.try_into().unwrap_or([0; 4]));
    let mut r = Reader { b: bytes, pos: 27, wide: version >= 7500 };
    let mut root = Node::default();
    while r.pos < bytes.len() {
        match r.node(0)? {
            Some(n) => root.children.push(n),
            None => break,
        }
    }
    Ok((root, version))
}

struct Ascii<'a> {
    b: &'a [u8],
    pos: usize,
}

impl Ascii<'_> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }
    fn skip(&mut self, lines: bool) {
        while let Some(c) = self.peek() {
            match c {
                b' ' | b'\t' | b'\r' => self.pos += 1,
                b'\n' if lines => self.pos += 1,
                b';' if lines => {
                    while self.peek().is_some_and(|c| c != b'\n') {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }
    fn word(&mut self) -> &str {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'|' | b'-' | b'+' | b'.')) {
            self.pos += 1;
        }
        std::str::from_utf8(&self.b[start..self.pos]).unwrap_or("")
    }
    fn string(&mut self) -> String {
        self.pos += 1;
        let start = self.pos;
        while self.peek().is_some_and(|c| c != b'"') {
            self.pos += 1;
        }
        let s = String::from_utf8_lossy(&self.b[start..self.pos]).into_owned();
        self.pos += 1;
        match s.split_once("::") {
            Some((class, name)) if !class.is_empty() && class.bytes().all(|c| c.is_ascii_alphabetic()) => {
                name.to_owned()
            }
            _ => s,
        }
    }
    fn value(word: &str) -> Prop {
        if let Ok(i) = word.parse::<i64>() {
            Prop::Int(i)
        } else if let Ok(f) = word.parse::<f64>() {
            Prop::Float(f)
        } else {
            Prop::Str(word.to_owned())
        }
    }
    fn array(&mut self) -> Result<Prop> {
        let _count = self.word().to_owned();
        self.skip(true);
        if self.peek() != Some(b'{') {
            return Err(invalid("FBX: bad array"));
        }
        self.pos += 1;
        self.skip(true);
        let key = self.word().to_owned();
        if key != "a" {
            return Err(invalid("FBX: bad array"));
        }
        self.skip(false);
        if self.peek() == Some(b':') {
            self.pos += 1;
        }
        let (mut ints, mut floats, mut is_float) = (Vec::new(), Vec::new(), false);
        loop {
            self.skip(true);
            match self.peek() {
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                Some(b',') => self.pos += 1,
                None => return Err(invalid("FBX: truncated array")),
                _ => {
                    let w = self.word();
                    if w.is_empty() {
                        return Err(invalid("FBX: bad array value"));
                    }
                    if !is_float && let Ok(i) = w.parse::<i64>() {
                        ints.push(i);
                    } else {
                        let f = w.parse::<f64>().map_err(|_| invalid("FBX: bad array value"))?;
                        if !is_float {
                            is_float = true;
                            floats = ints.iter().map(|&i| i as f64).collect();
                        }
                        floats.push(f);
                    }
                    if ints.len() + floats.len() > MAX_ARRAY / 8 {
                        return Err(invalid("FBX: array too large"));
                    }
                }
            }
        }
        Ok(if is_float { Prop::Floats(floats) } else { Prop::Ints(ints) })
    }
    fn nodes(&mut self, depth: usize) -> Result<Vec<Node>> {
        if depth > 64 {
            return Err(invalid("FBX: nodes nested too deep"));
        }
        let mut out = Vec::new();
        loop {
            self.skip(true);
            match self.peek() {
                None => return Ok(out),
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(out);
                }
                _ => {}
            }
            let name = self.word().to_owned();
            if name.is_empty() || self.peek() != Some(b':') {
                return Err(invalid("FBX: not an FBX file"));
            }
            self.pos += 1;
            let mut node = Node { name, ..Node::default() };
            loop {
                self.skip(false);
                match self.peek() {
                    Some(b'"') => node.props.push(Prop::Str(self.string())),
                    Some(b'*') => {
                        self.pos += 1;
                        node.props.push(self.array()?);
                    }
                    Some(b',') => {
                        self.pos += 1;
                        self.skip(true);
                    }
                    Some(b'{') => {
                        self.pos += 1;
                        node.children = self.nodes(depth + 1)?;
                        break;
                    }
                    Some(b'\n') | Some(b'}') | Some(b';') | None => break,
                    _ => {
                        let w = self.word().to_owned();
                        if w.is_empty() {
                            self.pos += 1;
                        } else {
                            node.props.push(Self::value(&w));
                        }
                    }
                }
            }
            if node.name == "Content" {
                let text: String = node.props.iter().filter_map(Prop::str).collect();
                if let Some(bytes) = base64(&text) {
                    node.props = vec![Prop::Bytes(bytes)];
                }
            }
            out.push(node);
        }
    }
}

fn parse_ascii(bytes: &[u8]) -> Result<(Node, u32)> {
    let mut a = Ascii { b: bytes, pos: 0 };
    let children = a.nodes(0)?;
    let root = Node { children, ..Node::default() };
    let version = root
        .child("FBXHeaderExtension")
        .and_then(|h| h.child("FBXVersion"))
        .and_then(|v| v.props.first()?.int())
        .unwrap_or(0) as u32;
    Ok((root, version))
}

fn apply_templates(root: &mut Node) {
    let mut templates: HashMap<String, Vec<Node>> = HashMap::new();
    if let Some(defs) = root.child("Definitions") {
        for t in defs.children_named("ObjectType") {
            let Some(kind) = t.props.first().and_then(Prop::str) else { continue };
            let ps: Vec<Node> = t
                .children_named("PropertyTemplate")
                .filter_map(|pt| pt.child("Properties70"))
                .flat_map(|p| p.children_named("P").cloned())
                .collect();
            if !ps.is_empty() {
                templates.insert(kind.to_owned(), ps);
            }
        }
    }
    let Some(objects) = root.children.iter_mut().find(|c| c.name == "Objects") else { return };
    for o in &mut objects.children {
        let Some(tpl) = templates.get(&o.name) else { continue };
        let props = match o.children.iter().position(|c| c.name == "Properties70") {
            Some(i) => &mut o.children[i],
            None => {
                o.children.push(Node { name: "Properties70".to_owned(), ..Node::default() });
                o.children.last_mut().expect("pushed")
            }
        };
        let own: std::collections::HashSet<String> =
            props.children.iter().filter_map(|p| p.props.first().and_then(Prop::str).map(str::to_owned)).collect();
        for p in tpl {
            if p.props.first().and_then(Prop::str).is_some_and(|k| !own.contains(k)) {
                props.children.push(p.clone());
            }
        }
    }
}

fn parse(bytes: &[u8]) -> Result<Node> {
    let (root, version) = if bytes.starts_with(MAGIC) {
        parse_binary(bytes)?
    } else if bytes.windows(9).take(8192).any(|w| w == b"FBXHeader") {
        parse_ascii(bytes)?
    } else {
        return Err(invalid("not an FBX file"));
    };
    if version != 0 && version < 7000 {
        return Err(invalid(format!(
            "FBX version {}.{} is too old: export it as FBX 2011 (7.x) or later",
            version / 1000,
            version % 1000 / 100
        )));
    }
    let mut root = root;
    apply_templates(&mut root);
    Ok(root)
}

fn euler(v: DVec3, order: i64) -> DMat4 {
    let (x, y, z) = (
        DMat4::from_rotation_x(v.x.to_radians()),
        DMat4::from_rotation_y(v.y.to_radians()),
        DMat4::from_rotation_z(v.z.to_radians()),
    );
    match order {
        1 => y * z * x,
        2 => x * z * y,
        3 => z * x * y,
        4 => y * x * z,
        5 => x * y * z,
        _ => z * y * x,
    }
}

fn local_matrix(m: &Node) -> DMat4 {
    let v = |key: &str, default: DVec3| m.p70_vec(key).filter(|v| v.is_finite()).unwrap_or(default);
    let t = DMat4::from_translation(v("Lcl Translation", DVec3::ZERO));
    let order = m.p70_int("RotationOrder").unwrap_or(0);
    let r = euler(v("Lcl Rotation", DVec3::ZERO), order);
    let pre = euler(v("PreRotation", DVec3::ZERO), 0);
    let post = euler(v("PostRotation", DVec3::ZERO), 0);
    let s = DMat4::from_scale(v("Lcl Scaling", DVec3::ONE));
    let roff = DMat4::from_translation(v("RotationOffset", DVec3::ZERO));
    let rp = DMat4::from_translation(v("RotationPivot", DVec3::ZERO));
    let soff = DMat4::from_translation(v("ScalingOffset", DVec3::ZERO));
    let sp = DMat4::from_translation(v("ScalingPivot", DVec3::ZERO));
    t * roff * rp * pre * r * post.inverse() * rp.inverse() * soff * sp * s * sp.inverse()
}

fn geometric_matrix(m: &Node) -> DMat4 {
    let v = |key: &str, default: DVec3| m.p70_vec(key).filter(|v| v.is_finite()).unwrap_or(default);
    DMat4::from_translation(v("GeometricTranslation", DVec3::ZERO))
        * euler(v("GeometricRotation", DVec3::ZERO), 0)
        * DMat4::from_scale(v("GeometricScaling", DVec3::ONE))
}

fn axis_matrix(settings: Option<&Node>) -> DMat4 {
    let get = |key: &str, default: i64| settings.and_then(|s| s.p70_int(key)).unwrap_or(default);
    let axis = |i: i64, sign: i64| {
        let mut v = DVec3::ZERO;
        v[i.clamp(0, 2) as usize] = if sign < 0 { -1.0 } else { 1.0 };
        v
    };
    let up = axis(get("UpAxis", 1), get("UpAxisSign", 1));
    let front = axis(get("FrontAxis", 2), get("FrontAxisSign", 1));
    let coord = axis(get("CoordAxis", 0), get("CoordAxisSign", 1));
    if up.cross(front) == DVec3::ZERO || up.cross(coord) == DVec3::ZERO || front.cross(coord) == DVec3::ZERO {
        return DMat4::IDENTITY;
    }
    DMat4::from_cols(
        glam::DVec4::new(coord.x, up.x, front.x, 0.0),
        glam::DVec4::new(coord.y, up.y, front.y, 0.0),
        glam::DVec4::new(coord.z, up.z, front.z, 0.0),
        glam::DVec4::W,
    )
}

#[allow(clippy::too_many_arguments)]
fn layer_value(
    values: &[f64],
    indices: Option<&[i64]>,
    mapping: &str,
    reference: &str,
    n: usize,
    corner: usize,
    vertex: usize,
    polygon: usize,
) -> Option<Vec<f64>> {
    let mut i = match mapping {
        "ByPolygonVertex" => corner,
        "ByVertex" | "ByVertice" | "ByControlPoint" => vertex,
        "ByPolygon" => polygon,
        "AllSame" => 0,
        _ => return None,
    };
    if reference == "IndexToDirect" || reference == "Index" {
        i = usize::try_from(*indices?.get(i)?).ok()?;
    }
    values.get(i * n..i * n + n).map(<[f64]>::to_vec)
}

struct Layer<'a> {
    values: Cow<'a, [f64]>,
    indices: Option<Cow<'a, [i64]>>,
    mapping: &'a str,
    reference: &'a str,
}

impl<'a> Layer<'a> {
    fn new(node: Option<&'a Node>, data: &str, index: &str) -> Option<Self> {
        let node = node?;
        Some(Self {
            values: node.array_f(data)?,
            indices: node.array_i(index),
            mapping: node.text("MappingInformationType").unwrap_or("ByPolygonVertex"),
            reference: node.text("ReferenceInformationType").unwrap_or("Direct"),
        })
    }
    fn get(&self, n: usize, corner: usize, vertex: usize, polygon: usize) -> Option<Vec<f64>> {
        layer_value(&self.values, self.indices.as_deref(), self.mapping, self.reference, n, corner, vertex, polygon)
    }
}

fn texture_source(tex: &Node, video: Option<&Node>, dir: &Path) -> Option<TextureSource> {
    let names: Vec<&str> = [tex.text("RelativeFilename"), tex.text("FileName")]
        .into_iter()
        .chain([video.and_then(|v| v.text("RelativeFilename")), video.and_then(|v| v.text("Filename"))])
        .flatten()
        .collect();
    if let Some(path) = names.iter().find_map(|n| find_file(dir, n)) {
        return Some(TextureSource { path: Some(path), embedded: None });
    }
    let Some(Prop::Bytes(data)) = video.and_then(|v| v.child("Content")).and_then(|c| c.props.first()) else {
        return None;
    };
    if data.is_empty() {
        return None;
    }
    let ext = names
        .iter()
        .find_map(|n| Path::new(&n.replace('\\', "/")).extension().map(|e| e.to_string_lossy().to_lowercase()))
        .unwrap_or_else(|| "png".to_owned());
    Some(TextureSource { path: None, embedded: Some((data.clone(), ext)) })
}

fn texture_uv(tex: &Node) -> ([f64; 2], [f64; 2]) {
    let pair = |key: &str, default: f64| {
        tex.p70_vec(key).filter(|v| v.x.is_finite() && v.y.is_finite()).map_or([default; 2], |v| [v.x, v.y])
    };
    let scale = pair("Scaling", 1.0);
    if scale.iter().any(|s| s.abs() < 1e-9) { ([1.0; 2], [0.0; 2]) } else { (scale, pair("Translation", 0.0)) }
}

fn read_material_extras(o: &Node, m: &mut Material) {
    let e = &mut m.extras;
    let color = |a: &str, b: &str| o.p70_vec(a).or_else(|| o.p70_vec(b));
    if let Some(c) = color("EmissiveColor", "Emissive") {
        let factor = o.p70_float("EmissiveFactor").unwrap_or(1.0);
        let c = [c.x * factor, c.y * factor, c.z * factor];
        if c.iter().any(|v| *v > 0.01) {
            e.emissive = Some(c);
        }
    }
    if let Some(c) = color("SpecularColor", "Specular") {
        let factor = o.p70_float("SpecularFactor").unwrap_or(1.0);
        e.specular = Some(((c.x + c.y + c.z) / 3.0 * factor).clamp(0.0, 1.0));
    }
    e.shininess = o.p70_float("ShininessExponent").or_else(|| o.p70_float("Shininess"));
}

fn is_color_slot(prop: &str) -> bool {
    let p = prop.to_ascii_lowercase();
    p.is_empty() || p.contains("diffuse") || p.contains("base") || p.contains("albedo")
}

pub(crate) fn read(bytes: &[u8], dir: &Path) -> Result<Model> {
    let root = parse(bytes)?;
    let settings = root.child("GlobalSettings");
    let unit_cm = settings.and_then(|s| s.p70_float("UnitScaleFactor")).unwrap_or(1.0);
    let axes = axis_matrix(settings);
    let objects = root.child("Objects").ok_or_else(|| invalid("FBX: no objects"))?;
    let id = |n: &Node| n.props.first().and_then(Prop::int);

    let mut geometries: HashMap<i64, &Node> = HashMap::new();
    let mut models: HashMap<i64, &Node> = HashMap::new();
    let mut materials_by_id: HashMap<i64, usize> = HashMap::new();
    let mut materials: Vec<Material> = Vec::new();
    let mut textures: HashMap<i64, &Node> = HashMap::new();
    let mut videos: HashMap<i64, &Node> = HashMap::new();
    for o in &objects.children {
        let Some(oid) = id(o) else { continue };
        match o.name.as_str() {
            "Geometry" if o.props.get(2).and_then(Prop::str) == Some("Mesh") => {
                geometries.insert(oid, o);
            }
            "Model" => {
                models.insert(oid, o);
            }
            "Material" => {
                let name = o.props.get(1).and_then(Prop::str).filter(|n| !n.is_empty()).unwrap_or("material");
                let mut m = Material::named(name);
                if let Some(c) = o.p70_vec("DiffuseColor").or_else(|| o.p70_vec("Diffuse")) {
                    m.color = [c.x.clamp(0.0, 1.0), c.y.clamp(0.0, 1.0), c.z.clamp(0.0, 1.0), 1.0];
                }
                if let Some(a) = o.p70_float("Opacity").filter(|a| *a < 0.999 && *a >= 0.0) {
                    m.color[3] = a;
                    m.alpha = Alpha::Blend;
                }
                read_material_extras(o, &mut m);
                materials_by_id.insert(oid, materials.len());
                materials.push(m);
            }
            "Texture" => {
                textures.insert(oid, o);
            }
            "Video" => {
                videos.insert(oid, o);
            }
            _ => {}
        }
    }
    let mut parent_of: HashMap<i64, i64> = HashMap::new();
    let mut geometry_model: HashMap<i64, i64> = HashMap::new();
    let mut model_materials: HashMap<i64, Vec<usize>> = HashMap::new();
    let mut material_textures: Vec<(usize, i64)> = Vec::new();
    let mut other_textures: Vec<(usize, i64, String)> = Vec::new();
    let mut texture_video: HashMap<i64, i64> = HashMap::new();
    if let Some(conns) = root.child("Connections") {
        for c in conns.children_named("C") {
            let (Some(child), Some(parent)) = (c.props.get(1).and_then(Prop::int), c.props.get(2).and_then(Prop::int))
            else {
                continue;
            };
            if geometries.contains_key(&child) && models.contains_key(&parent) {
                geometry_model.insert(child, parent);
            } else if models.contains_key(&child) && models.contains_key(&parent) {
                parent_of.insert(child, parent);
            } else if let Some(&mi) = materials_by_id.get(&child) {
                if models.contains_key(&parent) {
                    model_materials.entry(parent).or_default().push(mi);
                }
            } else if textures.contains_key(&child)
                && let Some(&mi) = materials_by_id.get(&parent)
            {
                let slot = c.props.get(3).and_then(Prop::str).unwrap_or("");
                if is_color_slot(slot) {
                    material_textures.push((mi, child));
                } else {
                    other_textures.push((mi, child, slot.to_ascii_lowercase()));
                }
            } else if videos.contains_key(&child) && textures.contains_key(&parent) {
                texture_video.insert(parent, child);
            }
        }
    }
    for (mi, tid) in material_textures {
        let m = &mut materials[mi];
        if m.texture.is_some() || m.embedded.is_some() {
            continue;
        }
        let tex = textures[&tid];
        let video = texture_video.get(&tid).and_then(|v| videos.get(v));
        let names: Vec<&str> = [tex.text("RelativeFilename"), tex.text("FileName")]
            .into_iter()
            .chain([video.and_then(|v| v.text("RelativeFilename")), video.and_then(|v| v.text("Filename"))])
            .flatten()
            .collect();
        m.texture = names.iter().find_map(|n| find_file(dir, n));
        if m.texture.is_none()
            && let Some(Prop::Bytes(data)) = video.and_then(|v| v.child("Content")).and_then(|c| c.props.first())
            && !data.is_empty()
        {
            let ext = names
                .iter()
                .find_map(|n| Path::new(&n.replace('\\', "/")).extension().map(|e| e.to_string_lossy().to_lowercase()))
                .unwrap_or_else(|| "png".to_owned());
            m.embedded = Some((data.clone(), ext));
        }
        if m.texture.is_some() || m.embedded.is_some() {
            m.color[..3].copy_from_slice(&[1.0; 3]);
            let (scale, offset) = texture_uv(tex);
            m.extras.set_flipped_v_transform(scale, offset);
        }
    }
    for (mi, tid, slot) in other_textures {
        let tex = textures[&tid];
        let video = texture_video.get(&tid).and_then(|v| videos.get(v));
        let src = texture_source(tex, video.copied(), dir);
        let e = &mut materials[mi].extras;
        match slot.as_str() {
            s if s.contains("normal") || s.contains("bump") => {
                let kind = if s.contains("normal") { NormalKind::DirectX } else { NormalKind::Bump };
                match src {
                    Some(src) if e.normal.is_none() => e.normal = Some((src, kind)),
                    Some(_) => {}
                    None => e.ignore("normal_missing"),
                }
            }
            s if s.contains("emissive") => match src {
                Some(src) => {
                    e.emissive_texture = Some(src);
                    e.emissive.get_or_insert([1.0; 3]);
                }
                None => e.ignore("emissive_missing"),
            },
            s if s.contains("specular") || s.contains("shininess") || s.contains("reflection") => {
                e.ignore("specular_texture");
            }
            s if s.contains("displacement") => e.ignore("height_texture"),
            _ => {}
        }
    }
    let world = |model: i64| {
        let mut m = DMat4::IDENTITY;
        let mut cur = Some(model);
        let mut depth = 0;
        while let Some(id) = cur {
            let Some(node) = models.get(&id) else { break };
            m = local_matrix(node) * m;
            cur = parent_of.get(&id).copied();
            depth += 1;
            if depth > 64 {
                break;
            }
        }
        m
    };

    let default_material = materials.len();
    materials.push(Material::named("default"));
    let mut triangles: Vec<Triangle> = Vec::new();
    let mut gids: Vec<&i64> = geometries.keys().collect();
    gids.sort_unstable();
    let mut objects: Vec<MeshObject> = Vec::new();
    for gid in gids {
        let first = triangles.len();
        let g = geometries[gid];
        let (Some(verts), Some(poly)) = (g.array_f("Vertices"), g.array_i("PolygonVertexIndex")) else { continue };
        let model = geometry_model.get(gid).copied();
        let xf = axes
            * model.map_or(DMat4::IDENTITY, |m| {
                world(m) * models.get(&m).map_or(DMat4::IDENTITY, |n| geometric_matrix(n))
            });
        let mats = model.and_then(|m| model_materials.get(&m)).cloned().unwrap_or_default();
        let normals = Layer::new(g.child("LayerElementNormal"), "Normals", "NormalsIndex");
        let uvs = Layer::new(g.child("LayerElementUV"), "UV", "UVIndex");
        let mat_layer = g.child("LayerElementMaterial");
        let mat_ids = mat_layer.and_then(|l| l.array_i("Materials"));
        let mat_mapping = mat_layer.and_then(|l| l.text("MappingInformationType")).unwrap_or("AllSame");
        let positions: Vec<DVec3> = verts.chunks_exact(3).map(|p| DVec3::new(p[0], p[1], p[2])).collect();
        let mut polygons: Vec<(usize, Vec<usize>)> = Vec::new();
        let mut start = 0;
        let mut current = Vec::new();
        for (corner, &raw) in poly.iter().enumerate() {
            let last = raw < 0;
            let vi = if last { (-raw - 1) as usize } else { raw as usize };
            if vi >= positions.len() {
                return Err(invalid("FBX: polygon index out of range"));
            }
            current.push(vi);
            if last {
                polygons.push((start, std::mem::take(&mut current)));
                start = corner + 1;
            }
        }
        let smooth = if normals.is_none() {
            let polys: Vec<(Vec<usize>, u32)> = polygons.iter().map(|(_, p)| (p.clone(), 1)).collect();
            Some(smooth_normals(&positions, &polys, CREASE_DEG))
        } else {
            None
        };
        let mut corners: Vec<(DVec3, Option<DVec3>, DVec2)> = Vec::new();
        for (polygon, (first, idx)) in polygons.iter().enumerate() {
            corners.clear();
            for (k, &vi) in idx.iter().enumerate() {
                let corner = first + k;
                let n = match &smooth {
                    Some(s) => s.get(polygon).and_then(|p| p.get(k)).copied(),
                    None => normals
                        .as_ref()
                        .and_then(|l| l.get(3, corner, vi, polygon))
                        .map(|v| DVec3::new(v[0], v[1], v[2])),
                };
                let uv = uvs
                    .as_ref()
                    .and_then(|l| l.get(2, corner, vi, polygon))
                    .map_or(DVec2::ZERO, |v| DVec2::new(v[0], 1.0 - v[1]));
                corners.push((positions[vi], n, uv));
            }
            let local = mat_ids
                .as_ref()
                .and_then(|ids| ids.get(if mat_mapping == "AllSame" { 0 } else { polygon }).copied())
                .unwrap_or(0);
            let material = usize::try_from(local).ok().and_then(|i| mats.get(i)).copied().unwrap_or(default_material);
            push_polygon(&mut triangles, &corners, material, &xf);
        }
        if triangles.len() > first {
            let raw =
                model.and_then(|m| models.get(&m)).and_then(|n| n.props.get(1)).and_then(Prop::str).unwrap_or_default();
            let name = raw.split('\0').next().unwrap_or(raw);
            let name = name.strip_prefix("Model::").unwrap_or(name);
            objects.push(MeshObject { name: name.to_owned(), extras: None, first, count: triangles.len() - first });
        }
    }
    let unit_cm = if unit_cm.is_finite() && unit_cm > 0.0 { unit_cm } else { 1.0 };
    Ok(Model { triangles, materials, up: UpAxis::Y, meters_per_unit: unit_cm / 100.0, objects })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct W(Vec<u8>);

    enum P<'a> {
        I(i32),
        L(i64),
        D(f64),
        S(&'a str),
        Dd(&'a [f64]),
        Ii(&'a [i32]),
        R(&'a [u8]),
    }

    impl W {
        fn node(&mut self, name: &str, props: &[P], children: impl FnOnce(&mut W)) {
            let start = self.0.len();
            self.0.extend_from_slice(&[0u8; 12]);
            self.0.push(name.len() as u8);
            self.0.extend_from_slice(name.as_bytes());
            let props_start = self.0.len();
            for p in props {
                match p {
                    P::I(v) => {
                        self.0.push(b'I');
                        self.0.extend_from_slice(&v.to_le_bytes());
                    }
                    P::L(v) => {
                        self.0.push(b'L');
                        self.0.extend_from_slice(&v.to_le_bytes());
                    }
                    P::D(v) => {
                        self.0.push(b'D');
                        self.0.extend_from_slice(&v.to_le_bytes());
                    }
                    P::S(s) => {
                        self.0.push(b'S');
                        self.0.extend_from_slice(&(s.len() as u32).to_le_bytes());
                        self.0.extend_from_slice(s.as_bytes());
                    }
                    P::R(b) => {
                        self.0.push(b'R');
                        self.0.extend_from_slice(&(b.len() as u32).to_le_bytes());
                        self.0.extend_from_slice(b);
                    }
                    P::Dd(a) => {
                        self.0.push(b'd');
                        self.0.extend_from_slice(&(a.len() as u32).to_le_bytes());
                        self.0.extend_from_slice(&0u32.to_le_bytes());
                        self.0.extend_from_slice(&((a.len() * 8) as u32).to_le_bytes());
                        for v in *a {
                            self.0.extend_from_slice(&v.to_le_bytes());
                        }
                    }
                    P::Ii(a) => {
                        let mut raw = Vec::new();
                        for v in *a {
                            raw.extend_from_slice(&v.to_le_bytes());
                        }
                        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                        std::io::Write::write_all(&mut enc, &raw).unwrap();
                        let z = enc.finish().unwrap();
                        self.0.push(b'i');
                        self.0.extend_from_slice(&(a.len() as u32).to_le_bytes());
                        self.0.extend_from_slice(&1u32.to_le_bytes());
                        self.0.extend_from_slice(&(z.len() as u32).to_le_bytes());
                        self.0.extend_from_slice(&z);
                    }
                }
            }
            let props_len = self.0.len() - props_start;
            let before = self.0.len();
            children(self);
            if self.0.len() != before {
                self.0.extend_from_slice(&[0u8; 13]);
            }
            let end = self.0.len() as u32;
            self.0[start..start + 4].copy_from_slice(&end.to_le_bytes());
            self.0[start + 4..start + 8].copy_from_slice(&(props.len() as u32).to_le_bytes());
            self.0[start + 8..start + 12].copy_from_slice(&(props_len as u32).to_le_bytes());
        }

        fn p(&mut self, key: &str, values: &[f64]) {
            let mut props = vec![P::S(key), P::S(""), P::S(""), P::S("A")];
            props.extend(values.iter().map(|v| P::D(*v)));
            self.node("P", &props, |_| {});
        }
    }

    #[test]
    fn a_quad_placed_by_its_model() {
        let mut w = W(Vec::new());
        w.0.extend_from_slice(b"Kaydara FBX Binary  \0\x1a\0");
        w.0.extend_from_slice(&7400u32.to_le_bytes());
        w.node("GlobalSettings", &[], |w| {
            w.node("Properties70", &[], |w| {
                w.node("P", &[P::S("UpAxis"), P::S("int"), P::S("Integer"), P::S(""), P::I(1)], |_| {});
                w.node("P", &[P::S("UnitScaleFactor"), P::S("double"), P::S("Number"), P::S(""), P::D(100.0)], |_| {});
            });
        });
        w.node("Objects", &[], |w| {
            w.node("Geometry", &[P::L(10), P::S("quad\0\x01Geometry"), P::S("Mesh")], |w| {
                w.node("Vertices", &[P::Dd(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0])], |_| {});
                w.node("PolygonVertexIndex", &[P::Ii(&[0, 1, 2, -4])], |_| {});
            });
            w.node("Model", &[P::L(20), P::S("quad\0\x01Model"), P::S("Mesh")], |w| {
                w.node("Properties70", &[], |w| w.p("Lcl Translation", &[5.0, 0.0, 0.0]));
            });
            w.node("Material", &[P::L(30), P::S("steel\0\x01Material"), P::S("")], |w| {
                w.node("Properties70", &[], |w| w.p("DiffuseColor", &[0.5, 0.5, 0.25]));
            });
            w.node("Texture", &[P::L(40), P::S("steel\0\x01Texture"), P::S("")], |w| {
                w.node("RelativeFilename", &[P::S("C:\\nowhere\\steel.png")], |_| {});
            });
            w.node("Video", &[P::L(50), P::S("steel\0\x01Video"), P::S("Clip")], |w| {
                w.node("Content", &[P::R(b"\x89PNG data")], |_| {});
            });
        });
        w.node("Connections", &[], |w| {
            w.node("C", &[P::S("OO"), P::L(10), P::L(20)], |_| {});
            w.node("C", &[P::S("OO"), P::L(20), P::L(0)], |_| {});
            w.node("C", &[P::S("OO"), P::L(30), P::L(20)], |_| {});
            w.node("C", &[P::S("OP"), P::L(40), P::L(30), P::S("DiffuseColor")], |_| {});
            w.node("C", &[P::S("OO"), P::L(50), P::L(40)], |_| {});
        });
        w.0.extend_from_slice(&[0u8; 13]);
        let m = read(&w.0, Path::new(".")).expect("read");
        assert_eq!(m.triangles.len(), 2);
        assert_eq!(m.triangles[0].pos[1], DVec3::new(6.0, 0.0, 0.0), "model translation applied");
        let mat = &m.materials[m.triangles[0].material];
        assert_eq!(mat.name, "steel");
        assert_eq!(mat.color, [1.0, 1.0, 1.0, 1.0], "the texture replaces the color");
        assert_eq!(mat.embedded, Some((b"\x89PNG data".to_vec(), "png".to_owned())), "embedded media");
        assert_eq!(m.up, UpAxis::Y);
        assert!((m.meters_per_unit - 1.0).abs() < 1e-12, "a unit of 100 cm is a meter");
        assert!(read(b"; FBX 7.4.0 project file\nFBXHeaderExtension:  {", Path::new(".")).is_err());
        assert!(read(b"Kaydara FBX Binary  \0\x1a\0\x00", Path::new(".")).is_err());
    }

    #[test]
    fn ascii_with_axes_pivots_and_mirroring() {
        let text = r#"; FBX 7.5.0 project file
FBXHeaderExtension:  {
	FBXVersion: 7500
}
GlobalSettings:  {
	Properties70:  {
		P: "UpAxis", "int", "Integer", "",2
		P: "UpAxisSign", "int", "Integer", "",1
		P: "FrontAxis", "int", "Integer", "",1
		P: "FrontAxisSign", "int", "Integer", "",-1
		P: "CoordAxis", "int", "Integer", "",0
		P: "CoordAxisSign", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",2.54
	}
}
Objects:  {
	Geometry: 10, "Geometry::quad", "Mesh" {
		Vertices: *12 {
			a: 0,0,0,1,0,0,1,1,0,
			0,1,0
		}
		PolygonVertexIndex: *4 {
			a: 0,1,2,-4
		}
	}
	Model: 20, "Model::quad", "Mesh" {
		Properties70:  {
			P: "RotationPivot", "Vector3D", "Vector", "",1,0,0
			P: "Lcl Rotation", "Lcl Rotation", "", "A",0,0,90
			P: "Lcl Scaling", "Lcl Scaling", "", "A",1,1,-1
		}
	}
	Material: 30, "Material::paint red", "" {
		Properties70:  {
			P: "DiffuseColor", "Color", "", "A",1,0,0
		}
	}
}
Connections:  {
	C: "OO",10,20
	C: "OO",20,0
	C: "OO",30,20
}
"#;
        let m = read(text.as_bytes(), Path::new(".")).expect("read");
        assert_eq!(m.triangles.len(), 2);
        assert_eq!(m.materials[m.triangles[0].material].name, "paint red");
        assert!((m.meters_per_unit - 0.0254).abs() < 1e-12);
        let p = m.triangles.iter().flat_map(|t| t.pos).find(|p| (p.x - 1.0).abs() < 1e-9 && p.z.abs() > 0.5);
        assert!(p.is_some_and(|p| (p.z - 1.0).abs() < 1e-9 && p.y.abs() < 1e-9), "{:?}", m.triangles[0].pos);
        for t in &m.triangles {
            let face = (t.pos[1] - t.pos[0]).cross(t.pos[2] - t.pos[0]);
            assert!(face.dot(t.normal[0]) > 0.0, "winding agrees with the normal");
        }
        assert!(
            read(b"; FBX 6.1.0 project file\nFBXHeaderExtension:  {\n\tFBXVersion: 6100\n}\n", Path::new(".")).is_err()
        );
    }
}
