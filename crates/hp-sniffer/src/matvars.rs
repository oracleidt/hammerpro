use std::collections::HashMap;

use crate::matmem::{read_cstring, read_symbol};
use crate::procmem::MemSource;

pub const VAR_NAMES_RVA: u64 = 0x20f_49f0;
pub const MATERIAL_SHADER_AT: usize = 0x20;
pub const MATERIAL_VAR_COUNT_AT: usize = 0x32;
pub const MATERIAL_VARS_AT: usize = 0x38;
const VAR_SIZE: usize = 0x48;
const VAR_STRING_AT: usize = 8;
const VAR_INT_AT: usize = 0x10;
const VAR_VECTOR_AT: usize = 0x14;
const VAR_TYPE_AT: usize = 0x24;
const VAR_SYMBOL_AT: usize = 0x26;
const VAR_OBJECT_AT: usize = 0x39;
const TEXTURE_NAME_AT: usize = 0x14;
const MATRIX_BYTES: usize = 0x44;
const TYPE_FLOAT: u8 = 0;
const TYPE_STRING: u8 = 1;
const TYPE_VECTOR: u8 = 2;
const TYPE_TEXTURE: u8 = 3;
const TYPE_INT: u8 = 4;
const TYPE_FOURCC: u8 = 5;
const TYPE_MATRIX: u8 = 7;
const TYPE_MATERIAL: u8 = 8;
const SHADER_PARAM_RECORD: usize = 40;
const DERIVED_MIN_MATERIALS: usize = 6;
const EOL: &str = "\r\n";
const SHADER_PARAM_TYPE_AT: usize = 16;
const SHADER_PARAM_DEFAULT_AT: usize = 24;

fn u64_at(d: &[u8], at: usize) -> u64 {
    u64::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3], d[at + 4], d[at + 5], d[at + 6], d[at + 7]])
}

fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn i32_at(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn user_pointer(p: u64) -> bool {
    (0x1_0000..0x7fff_ffff_ffff).contains(&p)
}

#[derive(Clone, Debug, PartialEq)]
pub enum VarValue {
    Float(f32),
    Text(String),
    Vector(Vec<f32>),
    Texture(String),
    Int(i32),
    FourCc(i32),
    Matrix([f32; 16]),
    Material(String),
    Undefined,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatVar {
    pub name: String,
    pub value: VarValue,
}

pub struct SymbolTables {
    pub var_names: u64,
    pub object_names: u64,
}

pub fn float_text(v: f32) -> String {
    format!("{v:.6}")
}

pub fn matrix_text(m: &[f32; 16]) -> String {
    let (a, b, c, d, tx, ty) = (m[0], m[1], m[4], m[5], m[3], m[7]);
    let sx = (a * a + c * c).sqrt();
    let sy = (b * b + d * d).sqrt();
    let angle = c.atan2(a).to_degrees();
    let (sin, cos) = angle.to_radians().sin_cos();
    let (cx, cy) = (0.5f32, 0.5f32);
    let rx = cos * sx * (-cx) - sin * sy * (-cy);
    let ry = sin * sx * (-cx) + cos * sy * (-cy);
    let ox = tx - (cx + rx);
    let oy = ty - (cy + ry);
    let t = |v: f32| {
        let s = format!("{v:.4}");
        let s = s.trim_end_matches('0').trim_end_matches('.').to_owned();
        if s.is_empty() || s == "-0" { "0".to_owned() } else { s }
    };
    format!("center .5 .5 scale {} {} rotate {} translate {} {}", t(sx), t(sy), t(angle), t(ox), t(oy))
}

pub fn value_text(v: &VarValue) -> Option<String> {
    Some(match v {
        VarValue::Float(f) => float_text(*f),
        VarValue::Text(s) | VarValue::Texture(s) | VarValue::Material(s) => s.clone(),
        VarValue::Vector(c) => {
            let parts: Vec<String> = c.iter().map(|x| float_text(*x)).collect();
            format!("[{}]", parts.join(" "))
        }
        VarValue::Int(i) | VarValue::FourCc(i) => i.to_string(),
        VarValue::Matrix(m) => matrix_text(m),
        VarValue::Undefined => return None,
    })
}

pub fn shader_name<S: MemSource + ?Sized>(src: &S, shader: u64) -> Option<String> {
    if !user_pointer(shader) {
        return None;
    }
    let vtable = crate::matmem::read_u64(src, shader);
    let first = crate::matmem::read_u64(src, vtable);
    let code = src.read_vec(first, 8);
    if code.len() != 8 || code[0] != 0x48 || code[2] != 0x05 {
        return None;
    }
    let disp = i64::from(i32::from_le_bytes([code[3], code[4], code[5], code[6]]));
    let target = (first as i64 + 7 + disp) as u64;
    let text = match code[1] {
        0x8d => target,
        0x8b => crate::matmem::read_u64(src, target),
        _ => return None,
    };
    read_cstring(src, text, 64).filter(|n| !n.is_empty() && n.bytes().all(|b| (0x20..0x7f).contains(&b)))
}

pub fn read_var<S: MemSource + ?Sized>(src: &S, var: u64, tables: &SymbolTables) -> Option<MatVar> {
    let b = src.read_vec(var, VAR_SIZE);
    if b.len() != VAR_SIZE {
        return None;
    }
    let ty = b[VAR_TYPE_AT] & 0xf;
    let comps = usize::from(b[VAR_TYPE_AT] >> 4).clamp(1, 4);
    let name = read_symbol(src, tables.var_names, u16_at(&b, VAR_SYMBOL_AT))?;
    let object = u64_at(&b, VAR_OBJECT_AT);
    let value = match ty {
        TYPE_FLOAT => VarValue::Float(f32_at(&b, VAR_VECTOR_AT)),
        TYPE_STRING => {
            let p = u64_at(&b, VAR_STRING_AT);
            VarValue::Text(if user_pointer(p) { read_cstring(src, p, 1024)? } else { String::new() })
        }
        TYPE_VECTOR => VarValue::Vector((0..comps).map(|k| f32_at(&b, VAR_VECTOR_AT + k * 4)).collect()),
        TYPE_TEXTURE => {
            if !user_pointer(object) {
                VarValue::Undefined
            } else {
                let t = src.read_vec(object, 0x20);
                if t.len() != 0x20 {
                    return None;
                }
                VarValue::Texture(read_symbol(src, tables.object_names, u16_at(&t, TEXTURE_NAME_AT))?)
            }
        }
        TYPE_INT => VarValue::Int(i32_at(&b, VAR_INT_AT)),
        TYPE_FOURCC => VarValue::FourCc(i32_at(&b, VAR_INT_AT)),
        TYPE_MATRIX => {
            if !user_pointer(object) {
                VarValue::Undefined
            } else {
                let m = src.read_vec(object, MATRIX_BYTES);
                if m.len() != MATRIX_BYTES {
                    return None;
                }
                let mut out = [0f32; 16];
                for (k, v) in out.iter_mut().enumerate() {
                    *v = f32_at(&m, k * 4);
                }
                VarValue::Matrix(out)
            }
        }
        TYPE_MATERIAL => {
            if !user_pointer(object) {
                VarValue::Undefined
            } else {
                let t = src.read_vec(object, 0x30);
                if t.len() != 0x30 {
                    return None;
                }
                VarValue::Material(read_symbol(src, tables.object_names, u16_at(&t, 0x28))?)
            }
        }
        _ => VarValue::Undefined,
    };
    Some(MatVar { name, value })
}

pub fn read_vars<S: MemSource + ?Sized>(src: &S, material: &[u8], tables: &SymbolTables) -> Vec<MatVar> {
    let count = usize::from(material[MATERIAL_VAR_COUNT_AT]);
    let array = u64_at(material, MATERIAL_VARS_AT);
    if count == 0 || !user_pointer(array) {
        return Vec::new();
    }
    let ptrs = src.read_vec(array, count * 8);
    if ptrs.len() != count * 8 {
        return Vec::new();
    }
    ptrs.chunks_exact(8).filter_map(|c| read_var(src, u64_at(c, 0), tables)).collect()
}

#[derive(Clone, Debug, Default)]
pub struct ShaderDefaults {
    pub by_name: HashMap<String, Vec<String>>,
}

impl ShaderDefaults {
    pub fn is_default(&self, name: &str, text: &str) -> bool {
        self.by_name.get(&name.to_ascii_lowercase()).is_some_and(|list| list.iter().any(|d| same_value(d, text)))
    }
}

fn same_value(default: &str, text: &str) -> bool {
    if default.eq_ignore_ascii_case(text) {
        return true;
    }
    let nums = |s: &str| -> Option<Vec<f32>> {
        s.trim_matches(|c| c == '[' || c == ']' || c == '{' || c == '}')
            .split_whitespace()
            .map(|t| t.parse::<f32>().ok())
            .collect()
    };
    match (nums(default), nums(text)) {
        (Some(a), Some(b)) if !a.is_empty() => {
            a.len() == b.len() && a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1.0e-5)
        }
        _ => default.is_empty() && text.is_empty(),
    }
}

pub fn parse_param_records(data: &[u8], base: u64, lo: u64, hi: u64) -> Vec<(String, String)> {
    let in_range = |p: u64| p >= lo && p < hi;
    let cstr = |p: u64, max: usize| -> Option<String> {
        let off = usize::try_from(p.checked_sub(base)?).ok()?;
        let tail = data.get(off..)?;
        let end = tail.iter().take(max).position(|b| *b == 0)?;
        let s = &tail[..end];
        s.iter().all(|b| (0x20..0x7f).contains(b)).then(|| String::from_utf8_lossy(s).into_owned())
    };
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + SHADER_PARAM_RECORD <= data.len() {
        let name_ptr = u64_at(data, at);
        if in_range(name_ptr)
            && let Some(name) = cstr(name_ptr, 64).filter(|n| n.len() > 2 && n.starts_with('$'))
        {
            let ty = i32_at(data, at + SHADER_PARAM_TYPE_AT);
            let def_ptr = u64_at(data, at + SHADER_PARAM_DEFAULT_AT);
            if (0..=10).contains(&ty)
                && in_range(def_ptr)
                && let Some(def) = cstr(def_ptr, 96)
            {
                out.push((name.to_ascii_lowercase(), def));
            }
        }
        at += 8;
    }
    out
}

pub fn read_shader_defaults<S: MemSource + ?Sized>(src: &S, modules: &[&str]) -> ShaderDefaults {
    let mut d = ShaderDefaults::default();
    for m in modules {
        let Some(base) = src.module_base(m) else { continue };
        let regions: Vec<_> =
            src.regions().into_iter().filter(|r| r.image && r.base >= base && r.base < base + 0x40_0000).collect();
        let hi = regions.iter().map(|r| r.base + r.size).max().unwrap_or(base);
        let mut image = vec![0u8; (hi - base) as usize];
        for r in &regions {
            let data = src.read_vec(r.base, r.size as usize);
            let at = (r.base - base) as usize;
            image[at..at + data.len()].copy_from_slice(&data);
        }
        for (name, def) in parse_param_records(&image, base, base, hi) {
            let list = d.by_name.entry(name).or_default();
            if !list.contains(&def) {
                list.push(def);
            }
        }
    }
    d
}

pub const FLAG_NAMES: [&str; 32] = [
    "$debug",
    "$no_fullbright",
    "$no_draw",
    "$use_in_fillrate_mode",
    "$vertexcolor",
    "$vertexalpha",
    "$selfillum",
    "$additive",
    "$alphatest",
    "$multipass",
    "$znearer",
    "$model",
    "$flat",
    "$nocull",
    "$nofog",
    "$ignorez",
    "$decal",
    "$envmapsphere",
    "$noalphamod",
    "$envmapcameraspace",
    "$basealphaenvmapmask",
    "$translucent",
    "$normalmapalphaenvmapmask",
    "$softwareskin",
    "$opaquetexture",
    "$envmapmode",
    "$nodecal",
    "$halflambert",
    "$wireframe",
    "$allowalphatocoverage",
    "$alphamodifiedbyproxy",
    "$vertexfog",
];

fn int_of(vars: &[MatVar], name: &str) -> Option<i32> {
    vars.iter().find(|v| v.name.eq_ignore_ascii_case(name)).and_then(|v| match v.value {
        VarValue::Int(i) | VarValue::FourCc(i) => Some(i),
        _ => None,
    })
}

pub fn derived_values(all: &[(String, Vec<MatVar>)]) -> std::collections::HashSet<(String, String, String)> {
    let mut totals: HashMap<&str, usize> = HashMap::new();
    let mut counts: HashMap<(&str, String, String), usize> = HashMap::new();
    for (shader, vars) in all {
        *totals.entry(shader.as_str()).or_default() += 1;
        for v in vars {
            if let Some(t) = value_text(&v.value) {
                *counts.entry((shader.as_str(), v.name.to_ascii_lowercase(), t.to_ascii_lowercase())).or_default() += 1;
            }
        }
    }
    counts
        .into_iter()
        .filter(|((shader, _, _), n)| {
            let total = totals.get(shader).copied().unwrap_or(0);
            total >= DERIVED_MIN_MATERIALS && *n * 10 >= total * 9
        })
        .map(|((shader, name, text), _)| (shader.to_owned(), name, text))
        .collect()
}

const SPRITE_ORIENTATIONS: [&str; 5] =
    ["parallel_upright", "facing_upright", "vp_parallel", "oriented", "vp_parallel_oriented"];

pub fn sprite_orientation(text: &str) -> Option<String> {
    let index = text.trim().parse::<f32>().ok()?;
    SPRITE_ORIENTATIONS.get(index as usize).filter(|_| index >= 0.0 && index.fract() == 0.0).map(|s| (*s).to_owned())
}

pub fn rebuild_vmt(
    shader: &str,
    vars: &[MatVar],
    defaults: &ShaderDefaults,
    derived: &std::collections::HashSet<(String, String, String)>,
) -> String {
    let mut out = format!("\"{shader}\"{EOL}{{{EOL}");
    if let (Some(flags), Some(defined)) = (int_of(vars, "$flags"), int_of(vars, "$flags_defined")) {
        for (bit, name) in FLAG_NAMES.iter().enumerate() {
            if (defined as u32) >> bit & 1 != 0 {
                out.push_str(&format!("\t\"{name}\"\t\t\"{}\"{EOL}", (flags as u32) >> bit & 1));
            }
        }
    }
    for v in vars {
        let lname = v.name.to_ascii_lowercase();
        if matches!(lname.as_str(), "$flags" | "$flags_defined" | "$flags2" | "$flags_defined2") {
            continue;
        }
        let Some(text) = value_text(&v.value) else { continue };
        if matches!(v.value, VarValue::Text(ref t) if t.is_empty()) {
            continue;
        }
        if matches!(v.value, VarValue::Texture(ref t) | VarValue::Material(ref t) if t.is_empty()) {
            continue;
        }
        if defaults.is_default(&lname, &text)
            || derived.contains(&(shader.to_owned(), lname.clone(), text.to_ascii_lowercase()))
        {
            continue;
        }
        let text = if lname == "$spriteorientation" { sprite_orientation(&text).unwrap_or(text) } else { text };
        out.push_str(&format!("\t\"{}\"\t\t\"{}\"{EOL}", v.name, text));
    }
    out.push_str(&format!("}}{EOL}"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::VecMem;

    #[test]
    fn a_texture_transform_matrix_is_turned_back_into_the_vmt_words() {
        let angle = 30f32.to_radians();
        let (s, c) = angle.sin_cos();
        let (sx, sy) = (1.2f32, 1.2f32);
        let cx = 0.5f32;
        let m = [
            c * sx,
            -s * sy,
            0.0,
            cx - (c * sx * cx - s * sy * cx),
            s * sx,
            c * sy,
            0.0,
            cx - (s * sx * cx + c * sy * cx),
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ];
        let text = matrix_text(&m);
        assert!(text.starts_with("center .5 .5 scale 1.2 1.2 rotate 30 translate "), "{text}");
    }

    #[test]
    fn values_are_written_the_way_the_compiler_writes_them() {
        assert_eq!(value_text(&VarValue::Float(0.12)).as_deref(), Some("0.120000"));
        assert_eq!(value_text(&VarValue::Int(7)).as_deref(), Some("7"));
        assert_eq!(value_text(&VarValue::Vector(vec![1.0, 0.5, 0.0])).as_deref(), Some("[1.000000 0.500000 0.000000]"));
        assert_eq!(value_text(&VarValue::Texture("a/b".to_owned())).as_deref(), Some("a/b"));
        assert!(value_text(&VarValue::Undefined).is_none());
    }

    #[test]
    fn variables_equal_to_the_shader_default_are_left_out() {
        let mut d = ShaderDefaults::default();
        d.by_name.insert("$alpha".to_owned(), vec!["1.0".to_owned()]);
        d.by_name.insert("$color".to_owned(), vec!["[1 1 1]".to_owned()]);
        let vars = vec![
            MatVar { name: "$alpha".to_owned(), value: VarValue::Float(1.0) },
            MatVar { name: "$color".to_owned(), value: VarValue::Vector(vec![1.0, 1.0, 1.0]) },
            MatVar { name: "$basetexture".to_owned(), value: VarValue::Texture("dev/grid".to_owned()) },
            MatVar { name: "$detailscale".to_owned(), value: VarValue::Float(0.12) },
            MatVar { name: "$envmap".to_owned(), value: VarValue::Undefined },
        ];
        let text = rebuild_vmt("LightmappedGeneric", &vars, &d, &Default::default());
        assert_eq!(
            text,
            "\"LightmappedGeneric\"\r\n{\r\n\t\"$basetexture\"\t\t\"dev/grid\"\r\n\t\"$detailscale\"\t\t\"0.120000\"\r\n}\r\n"
        );
    }

    #[test]
    fn shader_parameter_records_are_read_from_static_data() {
        let base = 0x1000_0000u64;
        let mut data = vec![0u8; 0x400];
        data[0x200..0x200 + 7].copy_from_slice(b"$ALPHA\0");
        data[0x210..0x210 + 4].copy_from_slice(b"1.0\0");
        data[0x220..0x220 + 5].copy_from_slice(b"help\0");
        let rec = 0x40usize;
        data[rec..rec + 8].copy_from_slice(&(base + 0x200).to_le_bytes());
        data[rec + 8..rec + 16].copy_from_slice(&(base + 0x220).to_le_bytes());
        data[rec + 16..rec + 20].copy_from_slice(&0i32.to_le_bytes());
        data[rec + 24..rec + 32].copy_from_slice(&(base + 0x210).to_le_bytes());
        let found = parse_param_records(&data, base, base, base + 0x400);
        assert_eq!(found, vec![("$alpha".to_owned(), "1.0".to_owned())]);
    }

    fn put(m: &mut VecMem, at: u64, bytes: Vec<u8>) {
        m.add(at, bytes);
    }

    #[test]
    fn a_variable_is_decoded_by_its_type() {
        let mut m = VecMem::default();
        let table = 0x1000_0000u64;
        let mut t = vec![0u8; 0x80];
        t[8..16].copy_from_slice(&0x1000_1000u64.to_le_bytes());
        t[0x38..0x40].copy_from_slice(&0x1000_2000u64.to_le_bytes());
        put(&mut m, table, t);
        let mut elements = vec![0u8; 48];
        for (k, off) in [0u16, 16, 32].iter().enumerate() {
            let e = k * 12;
            elements[e + 8..e + 10].copy_from_slice(&0u16.to_le_bytes());
            elements[e + 10..e + 12].copy_from_slice(&off.to_le_bytes());
        }
        put(&mut m, 0x1000_1000, elements);
        put(&mut m, 0x1000_2000, 0x1000_3000u64.to_le_bytes().to_vec());
        let mut pool = vec![0u8; 128];
        pool[10..10 + 11].copy_from_slice(b"$detailscal");
        pool[26..26 + 8].copy_from_slice(b"$color\0\0");
        pool[42..42 + 8].copy_from_slice(b"a/tex\0\0\0");
        put(&mut m, 0x1000_3000, pool);
        let tables = SymbolTables { var_names: table, object_names: table };
        let mut var = vec![0u8; VAR_SIZE];
        var[VAR_TYPE_AT] = TYPE_VECTOR | (3 << 4);
        var[VAR_SYMBOL_AT..VAR_SYMBOL_AT + 2].copy_from_slice(&1u16.to_le_bytes());
        for (k, v) in [0.25f32, 0.5, 1.0].iter().enumerate() {
            var[VAR_VECTOR_AT + k * 4..VAR_VECTOR_AT + k * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        put(&mut m, 0x2000_0000, var);
        let got = read_var(&m, 0x2000_0000, &tables).expect("var");
        assert_eq!(got.name, "$color");
        assert_eq!(got.value, VarValue::Vector(vec![0.25, 0.5, 1.0]));
        let mut tex_var = vec![0u8; VAR_SIZE];
        tex_var[VAR_TYPE_AT] = TYPE_TEXTURE | (4 << 4);
        tex_var[VAR_SYMBOL_AT..VAR_SYMBOL_AT + 2].copy_from_slice(&1u16.to_le_bytes());
        tex_var[VAR_OBJECT_AT..VAR_OBJECT_AT + 8].copy_from_slice(&0x2100_0000u64.to_le_bytes());
        put(&mut m, 0x2000_1000, tex_var);
        let mut texture = vec![0u8; 0x40];
        texture[TEXTURE_NAME_AT..TEXTURE_NAME_AT + 2].copy_from_slice(&2u16.to_le_bytes());
        put(&mut m, 0x2100_0000, texture);
        let got = read_var(&m, 0x2000_1000, &tables).expect("texture var");
        assert_eq!(got.value, VarValue::Texture("a/tex".to_owned()));
    }

    #[test]
    fn a_shader_name_is_read_from_the_first_virtual_method() {
        for (opcode, indirect) in [(0x8du8, false), (0x8bu8, true)] {
            let mut m = VecMem::default();
            m.add(0x10_0000, 0x20_0000u64.to_le_bytes().to_vec());
            m.add(0x20_0000, 0x30_0000u64.to_le_bytes().to_vec());
            let target = 0x40_0000u64;
            let disp = (target as i64 - (0x30_0000 + 7)) as i32;
            let mut code = vec![0x48, opcode, 0x05];
            code.extend_from_slice(&disp.to_le_bytes());
            code.push(0xc3);
            m.add(0x30_0000, code);
            if indirect {
                m.add(target, 0x50_0000u64.to_le_bytes().to_vec());
                m.add(0x50_0000, b"VertexLitGeneric\0".to_vec());
            } else {
                m.add(target, b"VertexLitGeneric\0".to_vec());
            }
            assert_eq!(shader_name(&m, 0x10_0000).as_deref(), Some("VertexLitGeneric"), "{opcode:x}");
        }
        assert!(shader_name(&VecMem::default(), 0x10_0000).is_none());
    }
}
