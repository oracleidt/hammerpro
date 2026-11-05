use glam::Vec3;

use crate::emit::Buf;
use crate::materials::MaterialSource;
use crate::props::ModelSource;

pub const DPRP: u32 = u32::from_be_bytes(*b"dprp");
pub const DPLT: u32 = u32::from_be_bytes(*b"dplt");
pub const DPLH: u32 = u32::from_be_bytes(*b"dplh");
pub const DPRP_VERSION: u16 = 4;
pub const DPL_VERSION: u16 = 0;
pub const OBJECT_SIZE: usize = 52;
pub const SPRITE_SIZE: usize = 32;
pub const NAME_LENGTH: usize = 128;
pub const STYLE_SIZE: usize = 5;
pub const MAX_OBJECTS: usize = 65535;

pub mod kind {
    pub const MODEL: u8 = 0;
    pub const SPRITE: u8 = 1;
    pub const SHAPE_CROSS: u8 = 2;
    pub const SHAPE_TRI: u8 = 3;
}

#[derive(Clone, Copy, Debug)]
pub struct CrtRand(u32);

impl CrtRand {
    pub fn new(seed: u32) -> Self {
        Self(seed)
    }

    pub fn next_int(&mut self) -> i32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.0 >> 16) & 0x7fff) as i32
    }

    pub fn unit(&mut self) -> f32 {
        self.next_int() as f32 / 32767.0
    }
}

#[derive(Clone, Debug)]
struct Uniform {
    idum: i32,
    iy: i32,
    iv: [i32; 32],
}

impl Uniform {
    fn new(seed: i32) -> Self {
        Self { idum: if seed < 0 { seed } else { seed.wrapping_neg() }, iy: 0, iv: [0; 32] }
    }

    fn step(idum: i32) -> i32 {
        const IA: i32 = 16807;
        const IM: i32 = 2_147_483_647;
        const IQ: i32 = 127_773;
        const IR: i32 = 2836;
        let k = idum / IQ;
        let mut v = IA.wrapping_mul(idum - k * IQ).wrapping_sub(IR.wrapping_mul(k));
        if v < 0 {
            v = v.wrapping_add(IM);
        }
        v
    }

    fn generate(&mut self) -> i32 {
        const NTAB: usize = 32;
        const NDIV: i32 = 1 + (2_147_483_647 - 1) / NTAB as i32;
        if self.idum <= 0 || self.iy == 0 {
            self.idum = if self.idum.wrapping_neg() < 1 { 1 } else { self.idum.wrapping_neg() };
            for j in (0..NTAB + 8).rev() {
                self.idum = Self::step(self.idum);
                if j < NTAB {
                    self.iv[j] = self.idum;
                }
            }
            self.iy = self.iv[0];
        }
        self.idum = Self::step(self.idum);
        let j = (self.iy / NDIV) as usize & (NTAB - 1);
        self.iy = self.iv[j];
        self.iv[j] = self.idum;
        self.iy
    }

    fn float(&mut self) -> f32 {
        const RNMX: f64 = 1.0 - 1.2e-7;
        let fl = ((1.0 / 2_147_483_647.0f64) * f64::from(self.generate())) as f32;
        if f64::from(fl) > RNMX { RNMX as f32 } else { fl }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Gaussian {
    have: bool,
    value: f32,
}

impl Gaussian {
    fn sample(&mut self, u: &mut Uniform, mean: f32, std_dev: f32) -> f32 {
        if self.have {
            self.have = false;
            return std_dev * self.value + mean;
        }
        let (v1, v2, rsq) = loop {
            let v1 = 2.0 * u.float() - 1.0;
            let v2 = 2.0 * u.float() - 1.0;
            let rsq = v1 * v1 + v2 * v2;
            if rsq <= 1.0 && rsq != 0.0 {
                break (v1, v2, rsq);
            }
        };
        let fac = ((-2.0 * f64::from(rsq).ln() / f64::from(rsq)) as f32).sqrt();
        self.value = v1 * fac;
        self.have = true;
        std_dev * (v2 * fac) + mean
    }
}

fn leading_number(s: &str) -> Option<(f64, usize)> {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let digits = |i: &mut usize| {
        let at = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - at
    };
    let mut n = digits(&mut i);
    if i < b.len() && b[i] == b'.' {
        i += 1;
        n += digits(&mut i);
    }
    if n == 0 {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        if digits(&mut j) > 0 {
            i = j;
        }
    }
    s[..i].parse::<f64>().ok().map(|v| (v, i))
}

fn atof(s: &str) -> f64 {
    leading_number(s.trim_start()).map_or(0.0, |(v, _)| v)
}

fn atoi(s: &str) -> i32 {
    let t = s.trim_start();
    let b = t.as_bytes();
    let mut i = 0;
    let neg = !b.is_empty() && b[0] == b'-';
    if !b.is_empty() && (b[0] == b'-' || b[0] == b'+') {
        i = 1;
    }
    let mut v: i64 = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        v = (v * 10 + i64::from(b[i] - b'0')).min(i64::from(i32::MAX) + 1);
        i += 1;
    }
    let v = if neg { -v } else { v };
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn scan_floats(s: &str, max: usize) -> Vec<f32> {
    let mut out = Vec::new();
    let mut rest = s;
    while out.len() < max {
        let t = rest.trim_start();
        let Some((v, n)) = leading_number(t) else { break };
        out.push(v as f32);
        rest = &t[n..];
    }
    out
}

fn kv_float(b: &hp_kv::Block, key: &str, default: f32) -> f32 {
    match b.get(key) {
        None => default,
        Some(v) => v.trim().parse::<f64>().map_or(atof(v) as f32, |x| x as f32),
    }
}

fn kv_int(b: &hp_kv::Block, key: &str, default: i32) -> i32 {
    match b.get(key) {
        None => default,
        Some(v) => match v.trim().parse::<i64>() {
            Ok(x) => x as i32,
            Err(_) => v.trim().parse::<f64>().map_or(atoi(v), |x| x as i32),
        },
    }
}

fn has_subkeys(b: &hp_kv::Block) -> bool {
    !b.props.is_empty() || !b.children.is_empty()
}

#[derive(Clone, Debug, PartialEq)]
pub struct DetailModel {
    pub model: Option<String>,
    pub kind: Option<u8>,
    pub pos: [[f32; 2]; 2],
    pub tex: [[f32; 2]; 2],
    pub scale_std_dev: f32,
    pub shape_size: u8,
    pub shape_angle: u8,
    pub sway: u8,
    pub amount: f32,
    pub min_cos: f32,
    pub max_cos: f32,
    pub upright: bool,
    pub orientation: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DetailGroup {
    pub alpha: f32,
    pub models: Vec<DetailModel>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DetailType {
    pub name: String,
    pub density: f32,
    pub groups: Vec<DetailGroup>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DetailDict {
    pub types: Vec<DetailType>,
}

impl DetailDict {
    pub fn find(&self, name: &str) -> Option<&DetailType> {
        self.types
            .iter()
            .find(|t| t.name == name)
            .or_else(|| self.types.iter().find(|t| t.name.eq_ignore_ascii_case(name)))
    }
}

fn parse_model(b: &hp_kv::Block, total: f32) -> Result<DetailModel, String> {
    let mut m = DetailModel {
        model: b.get("model").map(str::to_owned),
        kind: None,
        pos: [[0.0; 2]; 2],
        tex: [[0.0; 2]; 2],
        scale_std_dev: 0.0,
        shape_size: 0,
        shape_angle: 0,
        sway: 0,
        amount: 0.0,
        min_cos: 0.0,
        max_cos: 0.0,
        upright: false,
        orientation: 0,
    };
    if m.model.is_some() {
        m.kind = Some(kind::MODEL);
    } else if let Some(sprite) = b.get("sprite") {
        m.kind = Some(match b.get("sprite_shape") {
            Some(s) if s.eq_ignore_ascii_case("cross") => kind::SHAPE_CROSS,
            Some(s) if s.eq_ignore_ascii_case("tri") => kind::SHAPE_TRI,
            _ => kind::SPRITE,
        });
        let mut v = [0.0f32, 0.0, 64.0, 64.0, 512.0];
        let read = scan_floats(sprite, 5);
        v[..read.len()].copy_from_slice(&read);
        if read.len() != 5 || v[4] == 0.0 {
            return Err(format!("Invalid arguments to \"sprite\" in detail.vbsp ({} \"{sprite}\")", b.name));
        }
        let [x, y, w, h, size] = v;
        m.tex = [[(x + 0.5) / size, (y + 0.5) / size], [(x + w - 0.5) / size, (y + h - 0.5) / size]];
        m.pos = [[-10.0, 20.0], [10.0, 0.0]];
        if let Some(ss) = b.get("spritesize") {
            let mut s = [x, y, w, h];
            let read = scan_floats(ss, 4);
            s[..read.len()].copy_from_slice(&read);
            let [x, y, w, h] = s;
            let (ox, oy) = (w * x, h * y);
            m.pos = [[-ox, h - oy], [w - ox, -oy]];
        }
        m.scale_std_dev = kv_float(b, "spriterandomscale", 0.0);
        m.sway = (255.0 * f64::from(kv_float(b, "sway", 0.0)).clamp(0.0, 1.0)) as u8;
        m.shape_angle = kv_int(b, "shape_angle", 0) as u8;
        m.shape_size = (255.0 * f64::from(kv_float(b, "shape_size", 0.0)).clamp(0.0, 1.0)) as u8;
    }
    m.amount = kv_float(b, "amount", 1.0) + total;
    m.upright = kv_int(b, "upright", 0) != 0;
    let cos = |deg: f32| (f64::from(deg) * std::f64::consts::PI / 180.0).cos() as f32;
    m.min_cos = cos(kv_float(b, "minAngle", 180.0));
    m.max_cos = cos(kv_float(b, "maxAngle", 180.0));
    m.orientation = kv_int(b, "detailOrientation", 0);
    if m.min_cos < m.max_cos {
        m.min_cos = m.max_cos;
    }
    Ok(m)
}

pub fn parse_detail_vbsp(text: &str) -> Result<DetailDict, String> {
    let root = hp_kv::parse(text).map_err(|e| format!("detail.vbsp: {e}"))?;
    let mut dict = DetailDict::default();
    let Some(top) = root.children.first() else { return Ok(dict) };
    for t in top.children.iter().filter(|b| has_subkeys(b)) {
        let mut ty = DetailType { name: t.name.clone(), density: kv_float(t, "density", 0.0), groups: Vec::new() };
        for g in t.children.iter().filter(|b| has_subkeys(b)) {
            let alpha = kv_float(g, "alpha", 1.0);
            let at = ty.groups.iter().rposition(|x| alpha > x.alpha).map_or(0, |i| i + 1);
            let mut group = DetailGroup { alpha, models: Vec::new() };
            let mut total = 0.0f32;
            for mb in g.children.iter().filter(|b| has_subkeys(b)) {
                let m = parse_model(mb, total)?;
                total = m.amount;
                group.models.push(m);
            }
            if total > 1.0 {
                for m in &mut group.models {
                    m.amount /= total;
                }
            }
            ty.groups.insert(at, group);
        }
        dict.types.push(ty);
    }
    Ok(dict)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailObject {
    pub origin: Vec3,
    pub angles: Vec3,
    pub dict: u16,
    pub leaf: u16,
    pub lighting: [u8; 4],
    pub light_styles: u32,
    pub light_style_count: u8,
    pub sway: u8,
    pub shape_angle: u8,
    pub shape_size: u8,
    pub orientation: u8,
    pub kind: u8,
    pub scale: f32,
}

impl DetailObject {
    fn write(&self, b: &mut Buf) {
        for v in [self.origin, self.angles] {
            b.f32(v.x);
            b.f32(v.y);
            b.f32(v.z);
        }
        b.u16(self.dict);
        b.u16(self.leaf);
        b.0.extend_from_slice(&self.lighting);
        b.u32(self.light_styles);
        b.0.extend_from_slice(&[
            self.light_style_count,
            self.sway,
            self.shape_angle,
            self.shape_size,
            self.orientation,
            0,
            0,
            0,
            self.kind,
            0,
            0,
            0,
        ]);
        b.f32(self.scale);
    }

    fn read(r: &[u8]) -> DetailObject {
        let f = |o: usize| f32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
        DetailObject {
            origin: Vec3::new(f(0), f(4), f(8)),
            angles: Vec3::new(f(12), f(16), f(20)),
            dict: u16::from_le_bytes([r[24], r[25]]),
            leaf: u16::from_le_bytes([r[26], r[27]]),
            lighting: [r[28], r[29], r[30], r[31]],
            light_styles: u32::from_le_bytes([r[32], r[33], r[34], r[35]]),
            light_style_count: r[36],
            sway: r[37],
            shape_angle: r[38],
            shape_size: r[39],
            orientation: r[40],
            kind: r[44],
            scale: f(48),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DetailLump {
    pub models: Vec<String>,
    pub sprites: Vec<[f32; 8]>,
    pub objects: Vec<DetailObject>,
}

impl DetailLump {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = Buf::default();
        b.i32(self.models.len() as i32);
        for n in &self.models {
            let mut raw = [0u8; NAME_LENGTH];
            let bytes = n.as_bytes();
            let len = bytes.len().min(NAME_LENGTH);
            raw[..len].copy_from_slice(&bytes[..len]);
            b.0.extend_from_slice(&raw);
        }
        b.i32(self.sprites.len() as i32);
        for s in &self.sprites {
            for v in s {
                b.f32(*v);
            }
        }
        b.i32(self.objects.len() as i32);
        for o in &self.objects {
            o.write(&mut b);
        }
        b.0
    }

    pub fn parse(d: &[u8]) -> Result<DetailLump, String> {
        let int = |o: usize| -> Result<usize, String> {
            let b = d.get(o..o + 4).ok_or_else(|| format!("detail props: cut at byte {o} of {}", d.len()))?;
            let v = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            usize::try_from(v).map_err(|_| format!("detail props: negative count {v}"))
        };
        let mut out = DetailLump::default();
        let models = int(0)?;
        let mut at = 4usize;
        let names =
            d.get(at..at.saturating_add(models.saturating_mul(NAME_LENGTH))).ok_or("detail props: model names cut")?;
        for raw in names.chunks_exact(NAME_LENGTH) {
            let end = raw.iter().position(|b| *b == 0).unwrap_or(NAME_LENGTH);
            out.models.push(String::from_utf8_lossy(&raw[..end]).into_owned());
        }
        at += models * NAME_LENGTH;
        let sprites = int(at)?;
        at += 4;
        let rects =
            d.get(at..at.saturating_add(sprites.saturating_mul(SPRITE_SIZE))).ok_or("detail props: sprites cut")?;
        for r in rects.chunks_exact(SPRITE_SIZE) {
            let mut s = [0.0f32; 8];
            for (k, v) in s.iter_mut().enumerate() {
                *v = f32::from_le_bytes([r[k * 4], r[k * 4 + 1], r[k * 4 + 2], r[k * 4 + 3]]);
            }
            out.sprites.push(s);
        }
        at += sprites * SPRITE_SIZE;
        let count = int(at)?;
        at += 4;
        let records = d.len() - at.min(d.len());
        if records != count.saturating_mul(OBJECT_SIZE) {
            return Err(format!("detail props: {records} bytes for {count} objects of {OBJECT_SIZE} bytes"));
        }
        out.objects = d[at..].chunks_exact(OBJECT_SIZE).map(DetailObject::read).collect();
        Ok(out)
    }
}

pub fn parse_styles(d: &[u8]) -> Result<Vec<([u8; 4], u8)>, String> {
    let n = d.get(0..4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or("detail light styles: no count")?;
    let n = usize::try_from(n).map_err(|_| format!("detail light styles: negative count {n}"))?;
    if d.len() != 4 + n.saturating_mul(STYLE_SIZE) {
        return Err(format!("detail light styles: {} bytes for {n} records of {STYLE_SIZE} bytes", d.len() - 4));
    }
    Ok(d[4..].chunks_exact(STYLE_SIZE).map(|r| ([r[0], r[1], r[2], r[3]], r[4])).collect())
}

pub fn styles_to_bytes(styles: &[([u8; 4], u8)]) -> Vec<u8> {
    let mut b = Vec::with_capacity(4 + styles.len() * STYLE_SIZE);
    b.extend_from_slice(&(styles.len() as i32).to_le_bytes());
    for (c, s) in styles {
        b.extend_from_slice(c);
        b.push(*s);
    }
    b
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityDetail {
    pub origin: Vec3,
    pub angles: Vec3,
    pub orientation: i32,
    pub model: Option<String>,
    pub pos: [[f32; 2]; 2],
    pub tex: [[f32; 2]; 2],
}

impl EntityDetail {
    pub fn is_detail_class(class: &str) -> bool {
        ["detail_prop", "prop_detail", "prop_detail_sprite"].iter().any(|c| class.eq_ignore_ascii_case(c))
    }

    pub fn from_entity(e: &hp_vmf::Entity) -> Option<EntityDetail> {
        if !Self::is_detail_class(&e.classname) {
            return None;
        }
        let floats = |k: &str, n: usize| {
            let mut v = scan_floats(e.get(k).unwrap_or(""), n);
            v.resize(n, 0.0);
            v
        };
        let o = floats("origin", 3);
        let a = floats("angles", 3);
        let mut d = EntityDetail {
            origin: Vec3::new(o[0], o[1], o[2]),
            angles: Vec3::new(a[0], a[1], a[2]),
            orientation: atoi(e.get("detailOrientation").unwrap_or("0")),
            model: None,
            pos: [[0.0; 2]; 2],
            tex: [[0.0; 2]; 2],
        };
        if e.classname.eq_ignore_ascii_case("prop_detail_sprite") {
            let (ul, lr) = (floats("position_ul", 2), floats("position_lr", 2));
            let (tul, tsize) = (floats("tex_ul", 2), floats("tex_size", 2));
            let total = atof(e.get("tex_total_size").unwrap_or("0")) as f32;
            d.pos = [[ul[0], ul[1]], [lr[0], lr[1]]];
            let t1 = [tsize[0] + tul[0] - 0.5, tsize[1] + tul[1] - 0.5];
            let t0 = [tul[0] + 0.5, tul[1] + 0.5];
            d.tex = [[t0[0] / total, t0[1] / total], [t1[0] / total, t1[1] / total]];
        } else {
            d.model = Some(e.get("model").unwrap_or("").to_owned());
        }
        Some(d)
    }
}

#[derive(Clone, Debug, Default)]
pub struct BspLumps {
    pub planes: Vec<u8>,
    pub nodes: Vec<u8>,
    pub faces: Vec<u8>,
    pub faceids: Vec<u8>,
    pub vertexes: Vec<u8>,
    pub edges: Vec<u8>,
    pub surfedges: Vec<u8>,
    pub dispinfo: Vec<u8>,
    pub dispverts: Vec<u8>,
    pub models: Vec<u8>,
    pub texinfo: Vec<u8>,
}

fn f32_at(d: &[u8], o: usize) -> f32 {
    d.get(o..o + 4).map_or(0.0, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn i32_at(d: &[u8], o: usize) -> i32 {
    d.get(o..o + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn u16_at(d: &[u8], o: usize) -> u16 {
    d.get(o..o + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
}

fn vec_at(d: &[u8], o: usize) -> Vec3 {
    Vec3::new(f32_at(d, o), f32_at(d, o + 4), f32_at(d, o + 8))
}

impl BspLumps {
    pub fn from_fn(get: impl Fn(usize) -> Vec<u8>) -> BspLumps {
        use crate::emit::lump;
        BspLumps {
            planes: get(lump::PLANES),
            nodes: get(lump::NODES),
            faces: get(lump::FACES),
            faceids: get(lump::FACEIDS),
            vertexes: get(lump::VERTEXES),
            edges: get(lump::EDGES),
            surfedges: get(lump::SURFEDGES),
            dispinfo: get(lump::DISPINFO),
            dispverts: get(lump::DISP_VERTS),
            models: get(lump::MODELS),
            texinfo: get(lump::TEXINFO),
        }
    }

    pub fn of_bsp(bsp: &hp_bsp::Bsp) -> BspLumps {
        Self::from_fn(|i| bsp.lump(i).map(|c| c.into_owned()).unwrap_or_default())
    }

    fn face_count(&self) -> usize {
        self.faces.len() / 56
    }

    pub fn face_texinfo(&self, f: usize) -> i32 {
        i32::from(u16_at(&self.faces, f * 56 + 10) as i16)
    }

    pub fn texinfo_texdata(&self, ti: usize) -> i32 {
        i32_at(&self.texinfo, ti * 72 + 68)
    }

    pub fn face_points(&self, f: usize) -> Vec<Vec3> {
        let o = f * 56;
        let first = i32_at(&self.faces, o + 4);
        let n = u16_at(&self.faces, o + 8) as i16;
        let mut out = Vec::with_capacity(n.max(0) as usize);
        for k in 0..n.max(0) as usize {
            let Ok(at) = usize::try_from(first) else { break };
            let se = i32_at(&self.surfedges, (at + k) * 4);
            let e = se.unsigned_abs() as usize;
            let v = u16_at(&self.edges, e * 4 + if se < 0 { 2 } else { 0 });
            out.push(vec_at(&self.vertexes, v as usize * 12));
        }
        out
    }

    pub fn leaf_at(&self, p: Vec3) -> u16 {
        let nn = self.nodes.len() / 32;
        let mut node = 0i32;
        let mut steps = 0;
        while node >= 0 && (node as usize) < nn && steps <= nn {
            steps += 1;
            let o = node as usize * 32;
            let plane = i32_at(&self.nodes, o).max(0) as usize * 20;
            let (n, d) = (vec_at(&self.planes, plane), f32_at(&self.planes, plane + 12));
            node = if p.dot(n) < d { i32_at(&self.nodes, o + 8) } else { i32_at(&self.nodes, o + 4) };
        }
        if node < 0 { (-1 - node).clamp(0, 65535) as u16 } else { 0 }
    }

    fn face_origins(&self) -> Vec<Vec3> {
        let mut out = vec![Vec3::ZERO; self.face_count()];
        for m in 0..self.models.len() / 48 {
            let o = m * 48;
            let origin = vec_at(&self.models, o + 24);
            let (first, count) =
                (i32_at(&self.models, o + 40).max(0) as usize, i32_at(&self.models, o + 44).max(0) as usize);
            for f in out.iter_mut().skip(first).take(count) {
                *f = origin;
            }
        }
        out
    }
}

fn normalize(v: Vec3) -> Vec3 {
    v * (1.0 / (v.length() + 1e-10))
}

struct CoreDisp {
    width: usize,
    points: [Vec3; 4],
    flat: Vec<Vec3>,
    verts: Vec<Vec3>,
    alphas: Vec<f32>,
}

fn barycentric(v0: Vec3, v1: Vec3, v2: Vec3, pt: Vec3) -> Option<[f32; 3]> {
    let total = (v1 - v0).cross(v2 - v0).length() * 0.5;
    let oo = if total != 0.0 { 1.0 / total } else { 0.0 };
    let c0 = (v1 - pt).cross(v2 - pt).length() * 0.5 * oo;
    let c1 = (v2 - pt).cross(v0 - pt).length() * 0.5 * oo;
    let c2 = (v0 - pt).cross(v1 - pt).length() * 0.5 * oo;
    ((1.0 - (c0 + c1 + c2)).abs() < 1e-3).then_some([c0, c1, c2])
}

impl CoreDisp {
    fn build(corners: &[Vec3], start: Vec3, power: i32, verts: &[u8]) -> Option<CoreDisp> {
        if corners.len() != 4 || !(1..=4).contains(&power) {
            return None;
        }
        let mut best = (999_999_999.0f32, None);
        for (i, c) in corners.iter().enumerate() {
            let d = (start - *c).length_squared();
            if d < best.0 {
                best = (d, Some(i));
            }
        }
        let s = best.1?;
        let points = [corners[s], corners[(s + 1) % 4], corners[(s + 2) % 4], corners[(s + 3) % 4]];
        let width = (1usize << power) + 1;
        if verts.len() < width * width * 20 {
            return None;
        }
        let oo = 1.0 / (width - 1) as f32;
        let edge0 = (points[1] - points[0]) * oo;
        let edge1 = (points[2] - points[3]) * oo;
        let mut flat = Vec::with_capacity(width * width);
        let mut out = Vec::with_capacity(width * width);
        let mut alphas = Vec::with_capacity(width * width);
        for i in 0..width {
            let e0 = edge0 * i as f32 + points[0];
            let e1 = edge1 * i as f32 + points[3];
            let seg = (e1 - e0) * oo;
            for j in 0..width {
                let k = (i * width + j) * 20;
                let f = e0 + seg * j as f32;
                flat.push(f);
                out.push(f + vec_at(verts, k) * f32_at(verts, k + 12));
                alphas.push(f32_at(verts, k + 16));
            }
        }
        Some(CoreDisp { width, points, flat, verts: out, alphas })
    }

    fn triangle(&self, idx: [(usize, usize); 3], normal: [usize; 3], pt: Vec3) -> Option<(Vec3, Vec3, f32)> {
        let k = idx.map(|(v, u)| v * self.width + u);
        let flat = k.map(|i| self.flat[i]);
        let verts = k.map(|i| self.verts[i]);
        let c = barycentric(flat[0], flat[1], flat[2], pt)?;
        let p = verts[0] * c[0] + verts[1] * c[1] + verts[2] * c[2];
        let alpha = self.alphas[k[0]] * c[0] + self.alphas[k[1]] * c[1] + self.alphas[k[2]] * c[2];
        let [a, b, cc] = normal;
        let n = normalize((verts[a] - verts[b]).cross(verts[cc] - verts[b]));
        Some((p, n, alpha))
    }

    fn position(&self, u: f32, v: f32) -> Option<(Vec3, Vec3, f32)> {
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return None;
        }
        let p = &self.points;
        let a = p[0] + (p[1] - p[0]) * v;
        let b = p[3] + (p[2] - p[3]) * v;
        let pt = a + (b - a) * u;
        let w = self.width;
        let fu = u * (w as f32 - 1.000_001);
        let fv = v * (w as f32 - 1.000_001);
        let (su, sv) = (fu as usize, fv as usize);
        let (nu, nv) = ((su + 1).min(w - 1), (sv + 1).min(w - 1));
        let (frac_u, frac_v) = (fu - su as f32, fv - sv as f32);
        let odd = (sv * w + su) % 2 == 1;
        let tl_br_1 = ([(nv, su), (nv, nu), (sv, nu)], [0, 1, 2]);
        let tl_br_2 = ([(sv, su), (nv, su), (sv, nu)], [2, 0, 1]);
        let bl_tr_1 = ([(sv, su), (nv, su), (nv, nu)], [0, 1, 2]);
        let bl_tr_2 = ([(sv, su), (nv, nu), (sv, nu)], [1, 2, 0]);
        let (first, backup) = if odd {
            if frac_u + frac_v >= 1.0 + 0.000_01 { (tl_br_1, tl_br_2) } else { (tl_br_2, tl_br_1) }
        } else if frac_u < frac_v {
            (bl_tr_1, bl_tr_2)
        } else {
            (bl_tr_2, bl_tr_1)
        };
        self.triangle(first.0, first.1, pt).or_else(|| self.triangle(backup.0, backup.1, pt))
    }
}

fn matrix_angles(forward: Vec3, left: Vec3, up_z: f32) -> Vec3 {
    let xy = (forward.x * forward.x + forward.y * forward.y).sqrt();
    if xy > 0.001 {
        Vec3::new(
            (-forward.z).atan2(xy).to_degrees(),
            forward.y.atan2(forward.x).to_degrees(),
            left.z.atan2(up_z).to_degrees(),
        )
    } else {
        Vec3::new((-forward.z).atan2(xy).to_degrees(), (-left.x).atan2(left.y).to_degrees(), 0.0)
    }
}

struct Builder {
    lump: DetailLump,
    overflow: usize,
    cap: usize,
}

impl Default for Builder {
    fn default() -> Self {
        Builder { lump: DetailLump::default(), overflow: 0, cap: MAX_OBJECTS }
    }
}

impl Builder {
    fn full(&mut self) -> bool {
        if self.lump.objects.len() >= self.cap {
            self.overflow += 1;
            return true;
        }
        false
    }

    fn add_model(&mut self, name: &str, origin: Vec3, angles: Vec3, orientation: i32) {
        if self.full() {
            return;
        }
        let key = &name.as_bytes()[..name.len().min(NAME_LENGTH)];
        let dict = match self.lump.models.iter().rposition(|m| m.as_bytes() == key) {
            Some(i) => i,
            None => {
                self.lump.models.push(String::from_utf8_lossy(key).into_owned());
                self.lump.models.len() - 1
            }
        };
        self.lump.objects.push(DetailObject {
            origin,
            angles,
            dict: dict as u16,
            leaf: 0,
            lighting: [255, 255, 255, 0],
            light_styles: 0,
            light_style_count: 0,
            sway: 0,
            shape_angle: 0,
            shape_size: 0,
            orientation: orientation as u8,
            kind: kind::MODEL,
            scale: 1.0,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn add_sprite(
        &mut self,
        origin: Vec3,
        angles: Vec3,
        orientation: i32,
        pos: [[f32; 2]; 2],
        tex: [[f32; 2]; 2],
        scale: f32,
        ty: u8,
        shape: (u8, u8, u8),
    ) {
        if self.full() {
            return;
        }
        let rect = [pos[0][0], pos[0][1], pos[1][0], pos[1][1], tex[0][0], tex[0][1], tex[1][0], tex[1][1]];
        let same = |s: &[f32; 8]| s.iter().zip(&rect).all(|(a, b)| a.to_bits() == b.to_bits());
        let dict = match self.lump.sprites.iter().rposition(same) {
            Some(i) => i,
            None => {
                self.lump.sprites.push(rect);
                self.lump.sprites.len() - 1
            }
        };
        self.lump.objects.push(DetailObject {
            origin,
            angles,
            dict: dict as u16,
            leaf: 0,
            lighting: [255, 255, 255, 0],
            light_styles: 0,
            light_style_count: 0,
            sway: shape.2,
            shape_angle: shape.0,
            shape_size: shape.1,
            orientation: orientation as u8,
            kind: ty,
            scale,
        });
    }
}

struct Placer<'a> {
    rand: CrtRand,
    uniform: Uniform,
    gauss: Gaussian,
    out: Builder,
    model_ok: &'a dyn Fn(&str) -> bool,
    blockers: &'a [DetailBlocker],
}

#[derive(Clone, Debug, Default)]
pub struct DetailBlocker {
    pub mins: Vec3,
    pub maxs: Vec3,
    pub planes: Vec<(Vec3, f32)>,
}

impl DetailBlocker {
    pub fn contains(&self, p: Vec3) -> bool {
        if (0..3).any(|i| p[i] > self.maxs[i] || self.mins[i] > p[i]) {
            return false;
        }
        self.planes.iter().all(|(n, d)| (p.x - d * n.x) * n.x + (p.y - d * n.y) * n.y + (p.z - d * n.z) * n.z <= 0.0)
    }
}

impl Placer<'_> {
    fn blocked(&self, p: Vec3) -> bool {
        self.blockers.iter().rev().any(|b| b.contains(p))
    }

    fn select_group(&mut self, t: &DetailType, alpha: f32) -> usize {
        let g = &t.groups;
        let mut start = 0;
        while start + 1 < g.len() {
            if alpha < g[start + 1].alpha {
                break;
            }
            start += 1;
        }
        let end = (start + 1).min(g.len() - 1);
        if start == end {
            return start;
        }
        let d = g[end].alpha - g[start].alpha;
        let dist = if d != 0.0 { (alpha - g[start].alpha) / d } else { 0.0 };
        if self.rand.unit() > dist { start } else { end }
    }

    fn select_detail(&mut self, g: &DetailGroup) -> Option<usize> {
        let r = self.rand.unit();
        g.models.iter().position(|m| r <= m.amount)
    }

    fn place(&mut self, m: &DetailModel, pt: Vec3, normal: Vec3) {
        let cos = normal.z;
        if cos < m.max_cos {
            return;
        }
        if cos < m.min_cos {
            let p = (cos - m.max_cos) / (m.min_cos - m.max_cos);
            if self.rand.unit() > p {
                return;
            }
        }
        let angles = if m.upright {
            Vec3::new(0.0, 360.0 * self.rand.next_int() as f32 / 32767.0, 0.0)
        } else {
            let z = normalize(normal);
            let mut x = Vec3::X;
            if f64::from(x.dot(z).abs()) - 1.0 > -1e-3 {
                x = Vec3::Y;
            }
            let y = normalize(z.cross(x));
            let x = normalize(y.cross(z));
            let rot = 360.0 * self.rand.next_int() as f32 / 32767.0;
            let rad = (f64::from(rot) * (std::f64::consts::PI / 180.0)) as f32;
            let (s, c) = (f64::from(rad).sin() as f32, f64::from(rad).cos() as f32);
            matrix_angles(x * c + y * s, x * -s + y * c, z.z)
        };
        match m.kind {
            Some(kind::MODEL) => {
                let name = m.model.as_deref().unwrap_or("");
                if (self.model_ok)(name) {
                    self.out.add_model(name, pt, angles, m.orientation);
                }
            }
            Some(ty) => {
                let scale = if m.scale_std_dev != 0.0 {
                    self.gauss.sample(&mut self.uniform, 1.0, m.scale_std_dev).abs()
                } else {
                    1.0
                };
                self.out.add_sprite(
                    pt,
                    angles,
                    m.orientation,
                    m.pos,
                    m.tex,
                    scale,
                    ty,
                    (m.shape_angle, m.shape_size, m.sway),
                );
            }
            None => {}
        }
    }

    fn on_face(&mut self, points: &[Vec3], t: &DetailType, offset: Vec3) {
        if points.len() < 3 {
            return;
        }
        let v0 = points[0];
        for i in 1..points.len() - 1 {
            let e1 = points[i] - v0;
            let e2 = points[i + 1] - v0;
            let area_vec = e1.cross(e2);
            let len = area_vec.length();
            let area = 0.5 * len;
            let samples = (f64::from(area * t.density) * 0.000_001) as i32;
            for _ in 0..samples {
                let mut u = self.rand.unit();
                let mut v = self.rand.unit();
                if v > 1.0 - u {
                    u = 1.0 - u;
                    v = 1.0 - v;
                }
                let g = self.select_group(t, 1.0);
                let Some(m) = self.select_detail(&t.groups[g]) else { continue };
                let pt = v0 + e1 * u + e2 * v;
                let normal = area_vec * (1.0 / -len);
                if self.blocked(pt + offset) {
                    continue;
                }
                self.place(&t.groups[g].models[m], pt + offset, normal);
            }
        }
    }

    fn on_disp(&mut self, points: &[Vec3], disp: &CoreDisp, t: &DetailType, offset: Vec3) {
        let v0 = points[0];
        let mut area = 0.0f32;
        for i in 1..=2 {
            area += 0.5 * (points[i] - v0).cross(points[i + 1] - v0).length();
        }
        let samples = (f64::from(area * t.density) * 0.000_001) as i32;
        let (mut pt, mut normal, mut alpha) = (Vec3::ZERO, Vec3::Z, 0.0f32);
        for _ in 0..samples {
            let u = self.rand.unit();
            let v = self.rand.unit();
            if let Some((p, n, a)) = disp.position(u, v) {
                (pt, normal, alpha) = (p, n, a);
            }
            alpha /= 255.0;
            if self.blocked(pt + offset) {
                continue;
            }
            let g = self.select_group(t, alpha);
            let Some(m) = self.select_detail(&t.groups[g]) else { continue };
            self.place(&t.groups[g].models[m], pt + offset, normal);
        }
    }
}

pub fn sample_points(lumps: &BspLumps, face: usize, count: usize) -> Vec<(usize, Vec3)> {
    let points = lumps.face_points(face);
    if points.len() < 3 {
        return Vec::new();
    }
    let mut rand = CrtRand::new(u32::from(u16_at(&lumps.faceids, face * 2)));
    let r: Vec<f32> = (0..count).map(|_| rand.unit()).collect();
    let offset = lumps.face_origins().get(face).copied().unwrap_or(Vec3::ZERO);
    let disp = u16_at(&lumps.faces, face * 56 + 12) as i16;
    let mut out = Vec::new();
    if disp >= 0 {
        let o = disp as usize * 176;
        let first = i32_at(&lumps.dispinfo, o + 12).max(0) as usize;
        let core = lumps
            .dispverts
            .get(first * 20..)
            .and_then(|v| CoreDisp::build(&points, vec_at(&lumps.dispinfo, o), i32_at(&lumps.dispinfo, o + 20), v));
        let Some(core) = core else { return out };
        for i in 0..count.saturating_sub(1) {
            if let Some((p, _, _)) = core.position(r[i], r[i + 1]) {
                out.push((i, p + offset));
            }
        }
        return out;
    }
    let v0 = points[0];
    for t in 1..points.len() - 1 {
        let (e1, e2) = (points[t] - v0, points[t + 1] - v0);
        for i in 0..count.saturating_sub(1) {
            let (mut u, mut v) = (r[i], r[i + 1]);
            if v > 1.0 - u {
                u = 1.0 - u;
                v = 1.0 - v;
            }
            out.push((i, v0 + e1 * u + e2 * v + offset));
        }
    }
    out
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlaceReport {
    pub faces: usize,
    pub displacements: usize,
    pub overflow: usize,
    pub unknown: Vec<(String, String)>,
    pub types: Vec<(String, usize, usize)>,
}

pub fn place(
    lumps: &BspLumps,
    dict: &DetailDict,
    type_of: &dyn Fn(usize) -> Option<(String, String)>,
    entities: &[EntityDetail],
    model_ok: &dyn Fn(&str) -> bool,
    blockers: &[DetailBlocker],
) -> (DetailLump, PlaceReport) {
    let mut p = Placer {
        rand: CrtRand::new(0),
        uniform: Uniform::new(0),
        gauss: Gaussian::default(),
        out: Builder::default(),
        model_ok,
        blockers,
    };
    let mut report = PlaceReport::default();
    let origins = lumps.face_origins();
    for (j, &origin) in origins.iter().enumerate() {
        let Ok(ti) = usize::try_from(lumps.face_texinfo(j)) else { continue };
        let Some((material, name)) = type_of(ti) else { continue };
        let Some(t) = dict.find(&name) else {
            if !report.unknown.iter().any(|(m, _)| m.eq_ignore_ascii_case(&material)) {
                report.unknown.push((material, name));
            }
            continue;
        };
        if t.groups.is_empty() {
            continue;
        }
        let points = lumps.face_points(j);
        if points.len() < 3 {
            continue;
        }
        let seed = u16_at(&lumps.faceids, j * 2);
        p.rand = CrtRand::new(u32::from(seed));
        p.uniform = Uniform::new(i32::from(seed));
        let disp = u16_at(&lumps.faces, j * 56 + 12) as i16;
        report.faces += 1;
        let before = p.out.lump.objects.len();
        let ti = match report.types.iter().position(|x| x.0 == t.name) {
            Some(i) => i,
            None => {
                report.types.push((t.name.clone(), 0, 0));
                report.types.len() - 1
            }
        };
        report.types[ti].1 += 1;
        if disp < 0 {
            p.on_face(&points, t, origin);
            report.types[ti].2 += p.out.lump.objects.len() - before;
            continue;
        }
        let o = disp as usize * 176;
        if lumps.dispinfo.len() < o + 176 {
            continue;
        }
        let start = vec_at(&lumps.dispinfo, o);
        let first = i32_at(&lumps.dispinfo, o + 12).max(0) as usize;
        let power = i32_at(&lumps.dispinfo, o + 20);
        let Some(verts) = lumps.dispverts.get(first * 20..) else { continue };
        let Some(core) = CoreDisp::build(&points, start, power, verts) else { continue };
        report.displacements += 1;
        p.on_disp(&points, &core, t, origin);
        report.types[ti].2 += p.out.lump.objects.len() - before;
    }
    for e in entities {
        match &e.model {
            Some(m) => {
                if model_ok(m) {
                    p.out.add_model(m, e.origin, e.angles, e.orientation);
                }
            }
            None => p.out.add_sprite(e.origin, e.angles, e.orientation, e.pos, e.tex, 1.0, kind::SPRITE, (0, 0, 0)),
        }
    }
    let mut lump = p.out.lump;
    report.overflow = p.out.overflow;
    for o in &mut lump.objects {
        o.leaf = lumps.leaf_at(o.origin);
    }
    lump.objects.sort_by_key(|o| o.leaf);
    (lump, report)
}

#[derive(Clone, Debug)]
pub enum ScatterSurface {
    Face { points: Vec<Vec3>, normal: Vec3 },
    Disp { power: u32, base: Vec<Vec3>, verts: Vec<Vec3>, alphas: Vec<f32> },
}

pub fn scatter(
    surface: &ScatterSurface,
    t: &DetailType,
    seed: u32,
    cap: usize,
    model_ok: &dyn Fn(&str) -> bool,
) -> DetailLump {
    let mut p = Placer {
        rand: CrtRand::new(seed),
        uniform: Uniform::new(seed as i32),
        gauss: Gaussian::default(),
        out: Builder { cap: cap.min(MAX_OBJECTS), ..Builder::default() },
        model_ok,
        blockers: &[],
    };
    if t.groups.is_empty() {
        return p.out.lump;
    }
    match surface {
        ScatterSurface::Face { points, normal } => {
            if points.len() < 3 {
                return p.out.lump;
            }
            let mut pts = points.clone();
            let cross = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
            if cross.dot(*normal) > 0.0 {
                pts.reverse();
            }
            p.on_face(&pts, t, Vec3::ZERO);
        }
        ScatterSurface::Disp { power, base, verts, alphas } => {
            let w = (1usize << (*power).clamp(1, 4)) + 1;
            if !(1..=4).contains(power) || base.len() != w * w || verts.len() != w * w || alphas.len() != w * w {
                return p.out.lump;
            }
            let points = [base[0], base[(w - 1) * w], base[w * w - 1], base[w - 1]];
            let core = CoreDisp {
                width: w,
                points,
                flat: base.clone(),
                verts: verts.clone(),
                alphas: alphas.iter().map(|a| a * 255.0).collect(),
            };
            p.on_disp(&points, &core, t, Vec3::ZERO);
        }
    }
    p.out.lump
}

pub fn compile_lump(
    entities: &[&hp_vmf::Entity],
    get: &dyn Fn(usize) -> Vec<u8>,
    materials: &dyn MaterialSource,
    models: &dyn ModelSource,
    blockers: &[DetailBlocker],
    log: &mut dyn FnMut(&str),
) -> Vec<u8> {
    let lumps = BspLumps::from_fn(get);
    lump_with_names(entities, &lumps, get, materials, models, blockers, log)
}

pub fn original_material(name: &str) -> String {
    let n = name.replace('\\', "/");
    let Some(rest) =
        n.get(..5).filter(|p| p.eq_ignore_ascii_case("maps/")).and_then(|_| n[5..].split_once('/')).map(|(_, r)| r)
    else {
        return n;
    };
    if let Some(base) = rest.strip_suffix("_wvt_patch").or_else(|| rest.strip_suffix("_WVT_PATCH")) {
        return base.to_owned();
    }
    let parts: Vec<&str> = rest.rsplitn(4, '_').collect();
    if parts.len() == 4
        && parts[..3].iter().all(|p| !p.is_empty() && p.trim_start_matches('-').bytes().all(|b| b.is_ascii_digit()))
    {
        return parts[3].to_owned();
    }
    rest.to_owned()
}

pub fn bsp_lump(
    entities: &[&hp_vmf::Entity],
    bsp: &hp_bsp::Bsp,
    materials: &dyn MaterialSource,
    models: &dyn ModelSource,
    blockers: &[DetailBlocker],
    log: &mut dyn FnMut(&str),
) -> Vec<u8> {
    let lumps = BspLumps::of_bsp(bsp);
    let get = |i: usize| bsp.lump(i).map(|c| c.into_owned()).unwrap_or_default();
    lump_with_names(entities, &lumps, &get, materials, models, blockers, log)
}

fn lump_with_names(
    entities: &[&hp_vmf::Entity],
    lumps: &BspLumps,
    get: &dyn Fn(usize) -> Vec<u8>,
    materials: &dyn MaterialSource,
    models: &dyn ModelSource,
    blockers: &[DetailBlocker],
    log: &mut dyn FnMut(&str),
) -> Vec<u8> {
    use crate::emit::lump;
    let (texdata, table, strings) =
        (get(lump::TEXDATA), get(lump::TEXDATA_STRING_TABLE), get(lump::TEXDATA_STRING_DATA));
    let name_of = |ti: usize| -> Option<(usize, String)> {
        let td = usize::try_from(lumps.texinfo_texdata(ti)).ok()?;
        let id = usize::try_from(i32_at(&texdata, td * 32 + 12)).ok()?;
        let at = usize::try_from(i32_at(&table, id * 4)).ok()?;
        let raw = strings.get(at..)?;
        let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
        Some((td, original_material(&String::from_utf8_lossy(&raw[..end]))))
    };
    lump_for(entities, lumps, &name_of, materials, models, blockers, log)
}

fn lump_for(
    entities: &[&hp_vmf::Entity],
    lumps: &BspLumps,
    name_of: &dyn Fn(usize) -> Option<(usize, String)>,
    materials: &dyn MaterialSource,
    models: &dyn ModelSource,
    blockers: &[DetailBlocker],
    log: &mut dyn FnMut(&str),
) -> Vec<u8> {
    let mut types: std::collections::HashMap<usize, (String, Option<String>)> = std::collections::HashMap::new();
    for f in 0..lumps.face_count() {
        let Ok(ti) = usize::try_from(lumps.face_texinfo(f)) else { continue };
        let Some((td, name)) = name_of(ti) else { continue };
        types.entry(td).or_insert_with(|| {
            let t = materials.facts(&name).detail_type;
            (name, t)
        });
    }
    let used = types.values().any(|t| t.1.is_some());
    let explicit: Vec<EntityDetail> = entities.iter().filter_map(|e| EntityDetail::from_entity(e)).collect();
    let mut dict = DetailDict::default();
    if used {
        let world = entities.first();
        let name =
            world.and_then(|w| w.get("detailvbsp")).map(str::trim).filter(|s| !s.is_empty()).unwrap_or("detail.vbsp");
        match materials.read_file(name) {
            Some(bytes) => match parse_detail_vbsp(&hp_kv::decode(&bytes).0) {
                Ok(d) => dict = d,
                Err(e) => log(&format!("Error: {e}: no detail props")),
            },
            None => log(&format!("Warning: {name} not found: no detail props on the %detailtype materials")),
        }
    }
    let type_of = |ti: usize| -> Option<(String, String)> {
        let (td, _) = name_of(ti)?;
        let (name, t) = types.get(&td)?;
        Some((name.clone(), t.clone()?))
    };
    let warned = std::sync::Mutex::new(Vec::<String>::new());
    let model_ok = |m: &str| -> bool {
        let ok = models.is_static_prop(m).unwrap_or(true);
        if !ok
            && let Ok(mut w) = warned.lock()
            && !w.iter().any(|x| x == m)
        {
            w.push(m.to_owned());
        }
        ok
    };
    let (lump, report) = place(lumps, &dict, &type_of, &explicit, &model_ok, blockers);
    for (name, faces, objects) in &report.types {
        log(&format!("detail type {name}: {objects} objects on {faces} faces"));
    }
    for (m, t) in &report.unknown {
        log(&format!("Warning: material {m} uses unknown detail object type {t}"));
    }
    for m in warned.into_inner().unwrap_or_default() {
        log(&format!("Warning: detail model {m} is missing or not compiled with $staticprop: not placed"));
    }
    if report.overflow > 0 {
        log(&format!("Error! Too many detail props on this map: {} were not emitted (65535 at most)", report.overflow));
    }
    let models_n = lump.objects.iter().filter(|o| o.kind == kind::MODEL).count();
    log(&format!(
        "{} detail props ({} models, {} sprites) on {} faces ({} displacements)",
        lump.objects.len(),
        models_n,
        lump.objects.len() - models_n,
        report.faces,
        report.displacements
    ));
    lump.to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crt_rand_like_msvc() {
        let mut r = CrtRand::new(1);
        assert_eq!([r.next_int(), r.next_int(), r.next_int(), r.next_int()], [41, 18467, 6334, 26500]);
        let mut u = Uniform::new(7);
        let a: Vec<f32> = (0..100).map(|_| u.float()).collect();
        assert!(a.iter().all(|v| (0.0..1.0).contains(v)));
        let mean = a.iter().sum::<f32>() / 100.0;
        assert!((mean - 0.5).abs() < 0.1, "{mean}");
        let mut u2 = Uniform::new(7);
        assert_eq!(u2.float(), a[0]);
        let mut g = Gaussian::default();
        let s: Vec<f32> = (0..2000).map(|_| g.sample(&mut u, 1.0, 0.5)).collect();
        let m = s.iter().sum::<f32>() / s.len() as f32;
        let sd = (s.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / s.len() as f32).sqrt();
        assert!((m - 1.0).abs() < 0.05 && (sd - 0.5).abs() < 0.05, "{m} {sd}");
    }

    #[test]
    fn numbers_like_the_c_library() {
        assert_eq!(atof(" 12.5abc"), 12.5);
        assert_eq!(atof("x"), 0.0);
        assert_eq!(atoi(" -42.9"), -42);
        assert_eq!(scan_floats("1 2.5 x 4", 5), vec![1.0, 2.5]);
        assert_eq!(scan_floats("1e2 .5", 5), vec![100.0, 0.5]);
    }

    const VBSP: &str = r#"
detail
{
    grass
    {
        "density" "1000000.0"
        Group2 { "alpha" "1"
            A { "sprite" "0 0 64 32 512" "spritesize" "0.5 0 64 32" "amount" "0.5" "sway" "0.5" "upright" "1" }
            B { "sprite" "64 0 32 32 512" "sprite_shape" "cross" "shape_size" "0.25" "shape_angle" "30" "amount" "1.0" "maxAngle" "30" "minAngle" "10" }
        }
        Group1 { "alpha" "0"
            C { "model" "models/a.mdl" "amount" "0.3" }
        }
    }
    empty { }
}
"#;

    #[test]
    fn reads_detail_vbsp_like_vbsp() {
        let d = parse_detail_vbsp(VBSP).unwrap();
        assert_eq!(d.types.len(), 1);
        let t = d.find("GRASS").unwrap();
        assert_eq!(t.density, 1_000_000.0);
        assert_eq!(t.groups.iter().map(|g| g.alpha).collect::<Vec<_>>(), vec![0.0, 1.0]);
        let g = &t.groups[1];
        assert!((g.models[0].amount - 0.5 / 1.5).abs() < 1e-6 && (g.models[1].amount - 1.0).abs() < 1e-6);
        let a = &g.models[0];
        assert_eq!(a.kind, Some(kind::SPRITE));
        assert_eq!(a.pos, [[-32.0, 32.0], [32.0, -0.0]]);
        assert_eq!(a.tex, [[0.5 / 512.0, 0.5 / 512.0], [63.5 / 512.0, 31.5 / 512.0]]);
        assert_eq!(a.sway, 127);
        assert!(a.upright);
        assert_eq!((a.min_cos, a.max_cos), (-1.0, -1.0));
        let b = &g.models[1];
        assert_eq!((b.kind, b.shape_size, b.shape_angle), (Some(kind::SHAPE_CROSS), 63, 30));
        assert!(
            (b.max_cos - 30f32.to_radians().cos()).abs() < 1e-6 && (b.min_cos - 10f32.to_radians().cos()).abs() < 1e-6
        );
        assert_eq!(t.groups[0].models[0].kind, Some(kind::MODEL));
        assert!(parse_detail_vbsp("detail { t { g { m { \"sprite\" \"1 2\" } } } }").is_err());
    }

    fn floor_bsp(disp: bool) -> BspLumps {
        let mut l = BspLumps::default();
        let mut b = Buf::default();
        b.vec3(glam::DVec3::Z);
        b.f32(0.0);
        b.i32(2);
        l.planes = b.0;
        let mut b = Buf::default();
        b.i32(0);
        b.i32(-1);
        b.i32(-2);
        b.0.extend_from_slice(&[0u8; 20]);
        l.nodes = b.0;
        let pts = [(0.0, 0.0), (0.0, 256.0), (256.0, 256.0), (256.0, 0.0)];
        let mut b = Buf::default();
        for (x, y) in pts {
            b.vec3(glam::DVec3::new(x, y, 0.0));
        }
        l.vertexes = b.0;
        let mut b = Buf::default();
        for (s, e) in [(0u16, 0u16), (0, 1), (1, 2), (2, 3), (3, 0)] {
            b.u16(s);
            b.u16(e);
        }
        l.edges = b.0;
        let mut b = Buf::default();
        for e in 1..=4 {
            b.i32(e);
        }
        l.surfedges = b.0;
        let mut b = Buf::default();
        b.u16(0);
        b.u8(0);
        b.u8(1);
        b.i32(0);
        b.i16(4);
        b.i16(0);
        b.i16(if disp { 0 } else { -1 });
        b.0.resize(56, 0);
        l.faces = b.0;
        l.faceids = 1234u16.to_le_bytes().to_vec();
        if disp {
            let mut b = Buf::default();
            b.vec3(glam::DVec3::new(0.0, 0.0, 0.0));
            b.i32(0);
            b.i32(0);
            b.i32(2);
            b.0.resize(176, 0);
            l.dispinfo = b.0;
            let mut v = Buf::default();
            for i in 0..5 {
                for j in 0..5 {
                    v.vec3(glam::DVec3::Z);
                    v.f32(if i == 2 && j == 2 { 16.0 } else { 0.0 });
                    v.f32(if j >= 2 { 255.0 } else { 0.0 });
                }
            }
            l.dispverts = v.0;
        }
        l
    }

    #[test]
    fn places_on_faces_and_displacements() {
        let dict = parse_detail_vbsp(VBSP).unwrap();
        let type_of = |_: usize| Some(("grass/floor".to_owned(), "grass".to_owned()));
        let all = |_: &str| true;
        let (lump, report) = place(&floor_bsp(false), &dict, &type_of, &[], &all, &[]);
        assert_eq!(report.faces, 1);
        assert!(lump.objects.len() > 30_000 && lump.objects.len() <= 65_536, "{}", lump.objects.len());
        assert!(lump.models.is_empty() && lump.sprites.len() == 2);
        for o in &lump.objects {
            assert!(
                o.origin.x >= -0.01
                    && o.origin.x <= 256.01
                    && o.origin.y >= -0.01
                    && o.origin.y <= 256.01
                    && o.origin.z == 0.0
            );
            assert_eq!(o.leaf, 0);
            if o.kind == kind::SPRITE {
                assert!(o.angles.x == 0.0 && o.angles.z == 0.0);
            } else {
                assert_eq!(o.kind, kind::SHAPE_CROSS);
                assert!(o.angles.x.abs() < 1e-3 && o.angles.z.abs() < 1e-3, "{:?}", o.angles);
                assert_eq!((o.shape_size, o.shape_angle), (63, 30));
            }
        }
        let again = place(&floor_bsp(false), &dict, &type_of, &[], &all, &[]).0;
        assert_eq!(again, lump);
        assert_eq!(DetailLump::parse(&lump.to_bytes()).unwrap(), lump);

        let (lump, report) = place(&floor_bsp(true), &dict, &type_of, &[], &|m| m != "models/b.mdl", &[]);
        assert_eq!(report.displacements, 1);
        assert!(!lump.models.is_empty() && !lump.sprites.is_empty());
        let mut odd = 0;
        for o in &lump.objects {
            let model = o.kind == kind::MODEL;
            if (model && o.origin.x > 128.01) || (!model && o.origin.x < 63.99) {
                odd += 1;
            }
            if (o.origin.x - 128.0).abs() < 8.0 && (o.origin.y - 128.0).abs() < 8.0 {
                assert!(o.origin.z > 8.0, "{:?}", o.origin);
            }
            assert!(o.origin.z >= 0.0 && o.origin.z <= 16.01);
        }
        assert!(odd * 1000 < lump.objects.len(), "{odd}");
        let core = CoreDisp::build(&floor_bsp(true).face_points(0), Vec3::ZERO, 2, &floor_bsp(true).dispverts).unwrap();
        let (p, n, a) = core.position(0.5, 0.5).unwrap();
        assert!(
            (p - Vec3::new(128.0, 128.0, 16.0)).length() < 1e-3 && n.z > 0.0 && (a - 255.0).abs() < 1e-3,
            "{p} {n} {a}"
        );
        let (p, n, _) = core.position(0.375, 0.375).unwrap();
        assert!((p.z - 8.0).abs() < 1e-3 && n.z > 0.5 && n.z < 0.99, "{p} {n}");
    }

    #[test]
    fn original_of_patched_materials() {
        assert_eq!(original_material("maps/gm_x/nature/blendgrass01_-128_256_64"), "nature/blendgrass01");
        assert_eq!(original_material("maps/gm_x/nature/blendgrass01_wvt_patch"), "nature/blendgrass01");
        assert_eq!(original_material("nature/grass_01_02"), "nature/grass_01_02");
        assert_eq!(original_material("MAPS/gm_x/brick/wall"), "brick/wall");
    }

    #[test]
    fn entities_and_limits() {
        let mut e = hp_vmf::Entity { classname: "prop_detail_sprite".to_owned(), ..Default::default() };
        e.set("origin", "1 2 3");
        e.set("angles", "0 90 0");
        e.set("position_ul", "-8 16");
        e.set("position_lr", "8 0");
        e.set("tex_ul", "0 0");
        e.set("tex_size", "64 64");
        e.set("tex_total_size", "512");
        let d = EntityDetail::from_entity(&e).unwrap();
        assert_eq!(d.tex, [[0.5 / 512.0, 0.5 / 512.0], [63.5 / 512.0, 63.5 / 512.0]]);
        let l = floor_bsp(false);
        let (lump, _) = place(&l, &DetailDict::default(), &|_| None, &[d.clone(), d], &|_| true, &[]);
        assert_eq!((lump.objects.len(), lump.sprites.len()), (2, 1));
        assert_eq!(lump.objects[0].origin, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(DetailLump::default().to_bytes(), vec![0u8; 12]);
        assert!(DetailLump::parse(&[0u8; 11]).is_err());
        assert_eq!(parse_styles(&styles_to_bytes(&[([1, 2, 3, 4], 5)])).unwrap(), vec![([1, 2, 3, 4], 5)]);
    }

    fn scatter_dict() -> DetailDict {
        parse_detail_vbsp(
            "detail { grass { \"density\" \"100000\" Group { \"alpha\" \"1\" A { \"sprite\" \"0 0 64 32 512\" \"amount\" \"1\" \"sway\" \"0.5\" } } } }",
        )
        .unwrap()
    }

    #[test]
    fn scatter_covers_a_face_deterministically() {
        let dict = scatter_dict();
        let t = dict.find("grass").unwrap();
        let quad = vec![
            Vec3::new(0.0, 0.0, 10.0),
            Vec3::new(0.0, 256.0, 10.0),
            Vec3::new(256.0, 256.0, 10.0),
            Vec3::new(256.0, 0.0, 10.0),
        ];
        let s = ScatterSurface::Face { points: quad.clone(), normal: Vec3::Z };
        let a = scatter(&s, t, 7, 10_000, &|_| true);
        let b = scatter(&s, t, 7, 10_000, &|_| true);
        assert_eq!(a, b);
        assert!(a.objects.len() > 400, "{}", a.objects.len());
        assert!(a.objects.iter().all(|o| (0.0..=256.0).contains(&o.origin.x) && o.origin.z == 10.0));
        let reversed = ScatterSurface::Face { points: quad.into_iter().rev().collect(), normal: Vec3::Z };
        assert_eq!(scatter(&reversed, t, 7, 10_000, &|_| true).objects.len(), a.objects.len());
        assert_eq!(scatter(&s, t, 7, 25, &|_| true).objects.len(), 25);
        assert_ne!(scatter(&s, t, 8, 10_000, &|_| true), a);
        assert!(a.objects.iter().all(|o| o.sway == 127 && o.kind == kind::SPRITE));
    }

    #[test]
    fn scatter_follows_a_displacement_and_its_alpha() {
        let dict = parse_detail_vbsp(
            "detail { grass { \"density\" \"100000\" Dirt { \"alpha\" \"0\" A { \"sprite\" \"0 0 64 32 512\" } } Lush { \"alpha\" \"1\" B { \"sprite\" \"64 0 64 32 512\" } } } }",
        )
        .unwrap();
        let t = dict.find("grass").unwrap();
        let w = 5;
        let base: Vec<Vec3> =
            (0..w * w).map(|k| Vec3::new((k % w) as f32 * 64.0, (k / w) as f32 * 64.0, 0.0)).collect();
        let verts: Vec<Vec3> = base.iter().map(|b| *b + Vec3::Z * 8.0).collect();
        let surface = |a: f32| ScatterSurface::Disp {
            power: 2,
            base: base.clone(),
            verts: verts.clone(),
            alphas: vec![a; w * w],
        };
        let lush = scatter(&surface(1.0), t, 3, 100_000, &|_| true);
        let dirt = scatter(&surface(0.0), t, 3, 100_000, &|_| true);
        assert!(lush.objects.len() > 300 && dirt.objects.len() > 300);
        assert!(lush.objects.iter().all(|o| (o.origin.z - 8.0).abs() < 0.01));
        assert!(lush.sprites.iter().all(|s| s[4] > 0.12), "alpha 1 picks the second group");
        assert!(dirt.sprites.iter().all(|s| s[4] < 0.12), "alpha 0 picks the first group");
        let bad = ScatterSurface::Disp { power: 2, base: base.clone(), verts: Vec::new(), alphas: Vec::new() };
        assert!(scatter(&bad, t, 3, 100, &|_| true).objects.is_empty());
    }
}
