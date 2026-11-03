use crate::matvars::SymbolTables;
use crate::procmem::MemSource;

pub const MATERIAL_PROXY_COUNT_AT: usize = 0x33;
pub const MATERIAL_PROXIES_AT: usize = 0x40;
const MAX_PROXIES: usize = 64;
const RESULT_VAR_AT: usize = 8;
const RESULT_COMPONENT_AT: usize = 0x10;
const SOURCE_ONE_AT: usize = 0x18;
const SOURCE_TWO_AT: usize = 0x20;
const INPUT_VAR_AT: usize = 8;
const INPUT_COMPONENT_AT: usize = 0x10;
const OBJECT_READ: usize = 0xa0;
const LUA_NAME_AT: usize = 0x10;
const LUA_KEY_AT: usize = 0x30;
const STRING_LENGTH_OFFSET: usize = 0x10;
const STRING_CAPACITY_OFFSET: usize = 0x18;
const LUA_INLINE: u64 = 16;
const LUA_CLASS: &str = "MaterialProxy@GarrysMod";
const EOL: &str = "\r\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Var,
    Float,
    Input,
    Bool,
    NotBool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    None,
    Result,
    Function,
}

#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub key: &'static str,
    pub kind: Kind,
    pub offset: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct ProxyClass {
    pub rtti: &'static str,
    pub name: &'static str,
    pub base: Base,
    pub fields: &'static [Field],
}

const NO_FIELDS: &[Field] = &[];
const FIELDS_0: &[Field] = &[
    Field { key: "animatedTextureVar", kind: Kind::Var, offset: 8 },
    Field { key: "animatedTextureFrameNumVar", kind: Kind::Var, offset: 16 },
    Field { key: "animatedTextureFrameRate", kind: Kind::Float, offset: 24 },
    Field { key: "animationNoWrap", kind: Kind::NotBool, offset: 28 },
];
const FIELDS_1: &[Field] =
    &[Field { key: "min", kind: Kind::Input, offset: 40 }, Field { key: "max", kind: Kind::Input, offset: 64 }];
const FIELDS_2: &[Field] = &[Field { key: "textureScrollVar", kind: Kind::Var, offset: 8 }];
const FIELDS_3: &[Field] = &[Field { key: "scale", kind: Kind::Input, offset: 24 }];
const FIELDS_4: &[Field] = &[
    Field { key: "scale", kind: Kind::Input, offset: 40 },
    Field { key: "offset", kind: Kind::Input, offset: 64 },
    Field { key: "minVal", kind: Kind::Input, offset: 88 },
    Field { key: "maxVal", kind: Kind::Input, offset: 112 },
];
const FIELDS_5: &[Field] = &[
    Field { key: "mean", kind: Kind::Input, offset: 24 },
    Field { key: "halfwidth", kind: Kind::Input, offset: 48 },
    Field { key: "minVal", kind: Kind::Input, offset: 72 },
    Field { key: "maxVal", kind: Kind::Input, offset: 96 },
];
const FIELDS_6: &[Field] = &[Field { key: "$fadeout", kind: Kind::Bool, offset: 16 }];
const FIELDS_7: &[Field] = &[
    Field { key: "lessEqualVar", kind: Kind::Var, offset: 40 },
    Field { key: "greaterVar", kind: Kind::Var, offset: 48 },
];
const FIELDS_8: &[Field] = &[
    Field { key: "rate", kind: Kind::Input, offset: 24 },
    Field { key: "initialValue", kind: Kind::Input, offset: 48 },
];
const FIELDS_9: &[Field] =
    &[Field { key: "axisVar", kind: Kind::Var, offset: 48 }, Field { key: "angle", kind: Kind::Input, offset: 24 }];
const FIELDS_10: &[Field] = &[Field { key: "scale", kind: Kind::Float, offset: 24 }];
const FIELDS_11: &[Field] = &[
    Field { key: "TextureVar", kind: Kind::Var, offset: 8 },
    Field { key: "TextureFrameNumVar", kind: Kind::Var, offset: 16 },
    Field { key: "$lighting", kind: Kind::Input, offset: 32 },
    Field { key: "PupilOpenRate", kind: Kind::Input, offset: 56 },
];
const FIELDS_12: &[Field] = &[
    Field { key: "textureScrollVar", kind: Kind::Var, offset: 16 },
    Field { key: "textureScrollRate", kind: Kind::Float, offset: 24 },
    Field { key: "textureScrollAngle", kind: Kind::Float, offset: 28 },
];
const FIELDS_13: &[Field] = &[
    Field { key: "sinePeriod", kind: Kind::Input, offset: 24 },
    Field { key: "sineMax", kind: Kind::Input, offset: 48 },
    Field { key: "sineMin", kind: Kind::Input, offset: 72 },
    Field { key: "timeOffset", kind: Kind::Input, offset: 96 },
];
const FIELDS_14: &[Field] = &[
    Field { key: "textureScrollVar", kind: Kind::Var, offset: 8 },
    Field { key: "textureScrollRate", kind: Kind::Input, offset: 16 },
    Field { key: "textureScrollAngle", kind: Kind::Input, offset: 40 },
    Field { key: "textureScale", kind: Kind::Input, offset: 64 },
];
const FIELDS_15: &[Field] = &[
    Field { key: "centerVar", kind: Kind::Var, offset: 24 },
    Field { key: "scaleVar", kind: Kind::Var, offset: 32 },
    Field { key: "rotateVar", kind: Kind::Var, offset: 40 },
    Field { key: "translateVar", kind: Kind::Var, offset: 48 },
];
const FIELDS_16: &[Field] = &[
    Field { key: "toggleTextureVar", kind: Kind::Var, offset: 8 },
    Field { key: "toggleTextureFrameNumVar", kind: Kind::Var, offset: 16 },
    Field { key: "toggleShouldWrap", kind: Kind::Bool, offset: 24 },
];
const FIELDS_17: &[Field] =
    &[Field { key: "minVal", kind: Kind::Input, offset: 24 }, Field { key: "maxVal", kind: Kind::Input, offset: 48 }];
const FIELDS_18: &[Field] =
    &[Field { key: "minVal", kind: Kind::Input, offset: 40 }, Field { key: "maxVal", kind: Kind::Input, offset: 64 }];

pub const CLASSES: &[ProxyClass] = &[
    ProxyClass { rtti: "CAbsProxy", name: "Abs", base: Base::Function, fields: NO_FIELDS },
    ProxyClass { rtti: "CAddProxy", name: "Add", base: Base::Function, fields: NO_FIELDS },
    ProxyClass { rtti: "CAlphaMaterialProxy", name: "Alpha", base: Base::None, fields: NO_FIELDS },
    ProxyClass {
        rtti: "CAnimatedEntityTextureProxy",
        name: "AnimatedEntityTexture",
        base: Base::None,
        fields: FIELDS_0,
    },
    ProxyClass {
        rtti: "CAnimatedOffsetTextureProxy",
        name: "AnimatedOffsetTexture",
        base: Base::None,
        fields: FIELDS_0,
    },
    ProxyClass { rtti: "CAnimatedTextureProxy", name: "AnimatedTexture", base: Base::None, fields: FIELDS_0 },
    ProxyClass { rtti: "CBreakableSurfaceProxy", name: "BreakableSurface", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CProxyBurnLevel", name: "BurnLevel", base: Base::Result, fields: NO_FIELDS },
    ProxyClass { rtti: "CCamoMaterialProxy", name: "Camo", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CClampProxy", name: "Clamp", base: Base::Function, fields: FIELDS_1 },
    ProxyClass { rtti: "CConveyorMaterialProxy", name: "ConveyorScroll", base: Base::None, fields: FIELDS_2 },
    ProxyClass { rtti: "CTimeMaterialProxy", name: "CurrentTime", base: Base::Result, fields: NO_FIELDS },
    ProxyClass { rtti: "CDivideProxy", name: "Divide", base: Base::Function, fields: NO_FIELDS },
    ProxyClass { rtti: "CDummyMaterialProxy", name: "Dummy", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CEmptyProxy", name: "Empty", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CEnginePostMaterialProxy", name: "engine_post", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CEntityOriginMaterialProxy", name: "EntityOrigin", base: Base::None, fields: NO_FIELDS },
    ProxyClass {
        rtti: "CEntityOriginAlyxMaterialProxy",
        name: "EntityOriginAlyx",
        base: Base::None,
        fields: NO_FIELDS,
    },
    ProxyClass { rtti: "CEntityRandomProxy", name: "EntityRandom", base: Base::Result, fields: FIELDS_3 },
    ProxyClass { rtti: "CEntitySpeedProxy", name: "EntitySpeed", base: Base::Result, fields: NO_FIELDS },
    ProxyClass {
        rtti: "CEp1IntroVortRefractMaterialProxy",
        name: "Ep1IntroVortRefract",
        base: Base::None,
        fields: NO_FIELDS,
    },
    ProxyClass { rtti: "CEqualsProxy", name: "Equals", base: Base::Function, fields: NO_FIELDS },
    ProxyClass { rtti: "CExponentialProxy", name: "Exponential", base: Base::Function, fields: FIELDS_4 },
    ProxyClass { rtti: "CFleshInteriorMaterialProxy", name: "FleshInterior", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CFracProxy", name: "Frac", base: Base::Function, fields: NO_FIELDS },
    ProxyClass { rtti: "CGaussianNoiseProxy", name: "GaussianNoise", base: Base::Result, fields: FIELDS_5 },
    ProxyClass { rtti: "CProxyHealth", name: "Health", base: Base::Result, fields: FIELDS_3 },
    ProxyClass { rtti: "CHeliBladeMaterialProxy", name: "HeliBlade", base: Base::None, fields: FIELDS_6 },
    ProxyClass { rtti: "CIntProxy", name: "Int", base: Base::Function, fields: NO_FIELDS },
    ProxyClass { rtti: "CProxyInvulnLevel", name: "InvulnLevel", base: Base::Result, fields: NO_FIELDS },
    ProxyClass { rtti: "CProxyIsNPC", name: "IsNPC", base: Base::Result, fields: FIELDS_3 },
    ProxyClass { rtti: "CLampBeamProxy", name: "lampbeam", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CLampHaloProxy", name: "lamphalo", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CLessOrEqualProxy", name: "LessOrEqual", base: Base::Function, fields: FIELDS_7 },
    ProxyClass { rtti: "CLinearRampProxy", name: "LinearRamp", base: Base::Result, fields: FIELDS_8 },
    ProxyClass { rtti: "CMaterialModifyProxy", name: "MaterialModify", base: Base::None, fields: NO_FIELDS },
    ProxyClass {
        rtti: "CMaterialModifyAnimatedProxy",
        name: "MaterialModifyAnimated",
        base: Base::None,
        fields: NO_FIELDS,
    },
    ProxyClass { rtti: "CMatrixRotateProxy", name: "MatrixRotate", base: Base::Result, fields: FIELDS_9 },
    ProxyClass { rtti: "CMotionBlurMaterialProxy", name: "MotionBlur", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CMultiplyProxy", name: "Multiply", base: Base::Function, fields: NO_FIELDS },
    ProxyClass { rtti: "ParticleSphereProxy", name: "ParticleSphereProxy", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CPlayerLogoProxy", name: "PlayerLogo", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CPlayerPositionProxy", name: "PlayerPosition", base: Base::Result, fields: FIELDS_10 },
    ProxyClass { rtti: "CPlayerProximityProxy", name: "PlayerProximity", base: Base::Result, fields: FIELDS_10 },
    ProxyClass { rtti: "CPlayerSpeedProxy", name: "PlayerSpeed", base: Base::Result, fields: FIELDS_10 },
    ProxyClass { rtti: "CPlayerTeamMatchProxy", name: "PlayerTeamMatch", base: Base::Result, fields: NO_FIELDS },
    ProxyClass { rtti: "CPlayerViewProxy", name: "PlayerView", base: Base::Result, fields: FIELDS_10 },
    ProxyClass { rtti: "CPupilProxy", name: "Pupil", base: Base::None, fields: FIELDS_11 },
    ProxyClass {
        rtti: "CSelectFirstIfNonZeroProxy",
        name: "SelectFirstIfNonZero",
        base: Base::Function,
        fields: NO_FIELDS,
    },
    ProxyClass { rtti: "CShadowProxy", name: "Shadow", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CShadowModelProxy", name: "ShadowModel", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CShieldProxy", name: "Shield", base: Base::None, fields: FIELDS_12 },
    ProxyClass { rtti: "CSineProxy", name: "Sine", base: Base::Result, fields: FIELDS_13 },
    ProxyClass { rtti: "CSpyInvisProxy", name: "spy_invis", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CSubtractProxy", name: "Subtract", base: Base::Function, fields: NO_FIELDS },
    ProxyClass { rtti: "CTextureScrollMaterialProxy", name: "TextureScroll", base: Base::None, fields: FIELDS_14 },
    ProxyClass { rtti: "CTextureTransformProxy", name: "TextureTransform", base: Base::Result, fields: FIELDS_15 },
    ProxyClass { rtti: "CBaseToggleTextureProxy", name: "ToggleTexture", base: Base::None, fields: FIELDS_16 },
    ProxyClass { rtti: "CUniformNoiseProxy", name: "UniformNoise", base: Base::Result, fields: FIELDS_17 },
    ProxyClass { rtti: "CVortEmissiveProxy", name: "VortEmissive", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CWaterLODMaterialProxy", name: "WaterLOD", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CWorldDimsProxy", name: "WorldDims", base: Base::None, fields: NO_FIELDS },
    ProxyClass { rtti: "CWrapMinMaxProxy", name: "WrapMinMax", base: Base::Function, fields: FIELDS_18 },
    ProxyClass { rtti: "CProxyUrineLevel", name: "YellowLevel", base: Base::Result, fields: NO_FIELDS },
];

fn u64_at(d: &[u8], at: usize) -> u64 {
    u64::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3], d[at + 4], d[at + 5], d[at + 6], d[at + 7]])
}

fn i32_at(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn user_pointer(p: u64) -> bool {
    (0x1_0000..0x7fff_ffff_ffff).contains(&p)
}

pub fn class_of(rtti: &str) -> Option<&'static ProxyClass> {
    CLASSES.iter().find(|c| c.rtti == rtti)
}

pub fn rtti_name<S: MemSource + ?Sized>(src: &S, object: u64) -> Option<String> {
    let table = crate::matmem::read_u64(src, object);
    if !user_pointer(table) {
        return None;
    }
    let col = crate::matmem::read_u64(src, table.checked_sub(8)?);
    if !user_pointer(col) {
        return None;
    }
    let c = src.read_vec(col, 24);
    if c.len() != 24 || i32_at(&c, 0) != 1 {
        return None;
    }
    let type_descriptor = u64::from(u32::from_le_bytes([c[12], c[13], c[14], c[15]]));
    let self_rva = u64::from(u32::from_le_bytes([c[20], c[21], c[22], c[23]]));
    let base = col.checked_sub(self_rva)?;
    let raw = crate::matmem::read_cstring(src, base + type_descriptor + 16, 120)?;
    raw.strip_prefix(".?AV").and_then(|r| r.strip_suffix("@@")).map(str::to_owned)
}

fn var_name<S: MemSource + ?Sized>(src: &S, pointer: u64, tables: &SymbolTables) -> Option<String> {
    if !user_pointer(pointer) {
        return None;
    }
    crate::matvars::read_var(src, pointer, tables).map(|v| v.name).filter(|n| !n.is_empty())
}

fn with_component(name: String, component: i32) -> String {
    if (0..4).contains(&component) { format!("{name}[{component}]") } else { name }
}

pub fn float_text(v: f32) -> String {
    if v == v.trunc() && v.abs() < 1.0e9 { format!("{}", v as i64) } else { format!("{v}") }
}

pub fn std_string<S: MemSource + ?Sized>(src: &S, object: &[u8], at: usize) -> Option<String> {
    if object.len() < at + STRING_CAPACITY_OFFSET + 8 {
        return None;
    }
    let length = usize::try_from(u64_at(object, at + STRING_LENGTH_OFFSET)).ok().filter(|l| (1..=512).contains(l))?;
    let raw = if u64_at(object, at + STRING_CAPACITY_OFFSET) < LUA_INLINE {
        object[at..at + length.min(15)].to_vec()
    } else {
        let pointer = u64_at(object, at);
        if !user_pointer(pointer) {
            return None;
        }
        src.read_vec(pointer, length)
    };
    let text = String::from_utf8(raw).ok()?;
    (!text.is_empty() && text.bytes().all(|b| (0x20..0x7f).contains(&b))).then_some(text)
}

pub fn lua_name<S: MemSource + ?Sized>(src: &S, object: &[u8]) -> Option<String> {
    std_string(src, object, LUA_NAME_AT)
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProxyBlock {
    pub name: String,
    pub keys: Vec<(String, String)>,
    pub decoded: bool,
    pub lua_key: Option<String>,
}

pub fn read_proxy<S: MemSource + ?Sized>(src: &S, object: u64, tables: &SymbolTables) -> Option<ProxyBlock> {
    let rtti = rtti_name(src, object)?;
    let d = src.read_vec(object, OBJECT_READ);
    if d.len() != OBJECT_READ {
        return None;
    }
    if rtti == LUA_CLASS {
        let name = lua_name(src, &d)?;
        return Some(ProxyBlock { name, keys: Vec::new(), decoded: false, lua_key: std_string(src, &d, LUA_KEY_AT) });
    }
    let Some(class) = class_of(&rtti) else {
        let stem = rtti.strip_prefix('C').unwrap_or(&rtti);
        let name = stem.strip_suffix("MaterialProxy").or_else(|| stem.strip_suffix("Proxy")).unwrap_or(stem).to_owned();
        return Some(ProxyBlock { name, keys: Vec::new(), decoded: false, lua_key: None });
    };
    let mut keys: Vec<(String, String)> = Vec::new();
    if class.base != Base::None
        && let Some(name) = var_name(src, u64_at(&d, RESULT_VAR_AT), tables)
    {
        keys.push(("resultVar".to_owned(), with_component(name, i32_at(&d, RESULT_COMPONENT_AT))));
    }
    if class.base == Base::Function {
        if let Some(n) = var_name(src, u64_at(&d, SOURCE_ONE_AT), tables) {
            keys.push(("srcVar1".to_owned(), n));
        }
        if let Some(n) = var_name(src, u64_at(&d, SOURCE_TWO_AT), tables) {
            keys.push(("srcVar2".to_owned(), n));
        }
    }
    for f in class.fields {
        let value = match f.kind {
            Kind::Var => match var_name(src, u64_at(&d, f.offset), tables) {
                Some(n) => n,
                None => continue,
            },
            Kind::Float => float_text(f32_at(&d, f.offset)),
            Kind::Input => match var_name(src, u64_at(&d, f.offset + INPUT_VAR_AT), tables) {
                Some(n) => with_component(n, i32_at(&d, f.offset + INPUT_COMPONENT_AT)),
                None => float_text(f32_at(&d, f.offset)),
            },
            Kind::Bool => {
                if d[f.offset] == 0 {
                    continue;
                }
                "1".to_owned()
            }
            Kind::NotBool => {
                if d[f.offset] != 0 {
                    continue;
                }
                "1".to_owned()
            }
        };
        keys.push((f.key.to_owned(), value));
    }
    Some(ProxyBlock { name: class.name.to_owned(), keys, decoded: true, lua_key: None })
}

pub fn read_proxies<S: MemSource + ?Sized>(src: &S, material: &[u8], tables: &SymbolTables) -> Vec<ProxyBlock> {
    let Some(count) = material.get(MATERIAL_PROXY_COUNT_AT).map(|c| usize::from(*c)) else { return Vec::new() };
    if count == 0 || count > MAX_PROXIES || material.len() < MATERIAL_PROXIES_AT + 8 {
        return Vec::new();
    }
    let array = u64_at(material, MATERIAL_PROXIES_AT);
    if !user_pointer(array) {
        return Vec::new();
    }
    let raw = src.read_vec(array, count * 8);
    if raw.len() != count * 8 {
        return Vec::new();
    }
    raw.chunks_exact(8).filter_map(|c| read_proxy(src, u64_at(c, 0), tables)).collect()
}

pub fn proxies_text(blocks: &[ProxyBlock]) -> String {
    if blocks.is_empty() {
        return String::new();
    }
    let mut out = format!("\t\"Proxies\"{EOL}\t{{{EOL}");
    for b in blocks {
        out.push_str(&format!("\t\t\"{}\"{EOL}\t\t{{{EOL}", b.name));
        for (k, v) in &b.keys {
            out.push_str(&format!("\t\t\t\"{k}\"\t\t\"{v}\"{EOL}"));
        }
        out.push_str(&format!("\t\t}}{EOL}"));
    }
    out.push_str(&format!("\t}}{EOL}"));
    out
}

pub fn insert_proxies(vmt: &str, blocks: &[ProxyBlock]) -> String {
    let text = proxies_text(blocks);
    if text.is_empty() {
        return vmt.to_owned();
    }
    match vmt.rfind('}') {
        Some(at) => format!("{}{}{}", &vmt[..at], text, &vmt[at..]),
        None => vmt.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::VecMem;

    const NAMES_TABLE: u64 = 0x1000_0000;

    fn symbols(m: &mut VecMem, names: &[&str]) -> SymbolTables {
        let mut t = vec![0u8; 0x80];
        t[8..16].copy_from_slice(&0x1000_1000u64.to_le_bytes());
        t[0x38..0x40].copy_from_slice(&0x1000_2000u64.to_le_bytes());
        m.add(NAMES_TABLE, t);
        let mut elements = vec![0u8; names.len().max(1) * 12];
        let mut pool = vec![0u8; 16 + names.len() * 32];
        for (k, n) in names.iter().enumerate() {
            elements[k * 12 + 10..k * 12 + 12].copy_from_slice(&((k * 32) as u16).to_le_bytes());
            pool[k * 32 + 10..k * 32 + 10 + n.len()].copy_from_slice(n.as_bytes());
        }
        m.add(0x1000_1000, elements);
        m.add(0x1000_2000, 0x1000_3000u64.to_le_bytes().to_vec());
        m.add(0x1000_3000, pool);
        SymbolTables { var_names: NAMES_TABLE, object_names: NAMES_TABLE }
    }

    fn var(m: &mut VecMem, at: u64, symbol: u16) {
        let mut v = vec![0u8; 0x48];
        v[0x24] = 2 | (3 << 4);
        v[0x26..0x28].copy_from_slice(&symbol.to_le_bytes());
        m.add(at, v);
    }

    fn object(m: &mut VecMem, at: u64, class: &str, body: &[(usize, Vec<u8>)]) {
        let module = 0x4000_0000u64;
        let type_descriptor = 0x1000u64;
        let col = 0x4000_8000u64;
        let table = 0x4000_9008u64;
        let mut c = vec![0u8; 24];
        c[0..4].copy_from_slice(&1u32.to_le_bytes());
        c[12..16].copy_from_slice(&(type_descriptor as u32).to_le_bytes());
        c[20..24].copy_from_slice(&((col - module) as u32).to_le_bytes());
        m.add(col, c);
        m.add(table - 8, col.to_le_bytes().to_vec());
        let mut td = vec![0u8; 16];
        td.extend_from_slice(format!(".?AV{class}@@\0").as_bytes());
        m.add(module + type_descriptor, td);
        let mut o = vec![0u8; OBJECT_READ];
        o[0..8].copy_from_slice(&table.to_le_bytes());
        for (off, bytes) in body {
            o[*off..*off + bytes.len()].copy_from_slice(bytes);
        }
        m.add(at, o);
    }

    fn q(v: u64) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }

    fn f(v: f32) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }

    #[test]
    fn a_sine_proxy_gives_its_result_variable_and_float_inputs() {
        let mut m = VecMem::default();
        let tables = symbols(&mut m, &["$alpha"]);
        var(&mut m, 0x2000_0000, 0);
        object(
            &mut m,
            0x5000_0000,
            "CSineProxy",
            &[
                (8, q(0x2000_0000)),
                (0x10, (-1i32).to_le_bytes().to_vec()),
                (0x18, f(2.0)),
                (0x30, f(1.0)),
                (0x48, f(0.1)),
                (0x60, f(0.0)),
            ],
        );
        let block = read_proxy(&m, 0x5000_0000, &tables).expect("proxy");
        assert_eq!(block.name, "Sine");
        assert!(block.decoded);
        assert_eq!(
            block.keys,
            vec![
                ("resultVar".to_owned(), "$alpha".to_owned()),
                ("sinePeriod".to_owned(), "2".to_owned()),
                ("sineMax".to_owned(), "1".to_owned()),
                ("sineMin".to_owned(), "0.1".to_owned()),
                ("timeOffset".to_owned(), "0".to_owned()),
            ]
        );
    }

    #[test]
    fn a_float_input_that_names_a_variable_gives_the_variable() {
        let mut m = VecMem::default();
        let tables = symbols(&mut m, &["$alpha", "$tmp"]);
        var(&mut m, 0x2000_0000, 0);
        var(&mut m, 0x2000_1000, 1);
        object(
            &mut m,
            0x5000_0000,
            "CClampProxy",
            &[
                (8, q(0x2000_0000)),
                (0x10, 2i32.to_le_bytes().to_vec()),
                (0x18, q(0x2000_1000)),
                (0x28, f(0.25)),
                (0x48, q(0x2000_1000)),
                (0x50, 1i32.to_le_bytes().to_vec()),
            ],
        );
        let block = read_proxy(&m, 0x5000_0000, &tables).expect("proxy");
        assert_eq!(block.name, "Clamp");
        assert_eq!(block.keys[0], ("resultVar".to_owned(), "$alpha[2]".to_owned()));
        assert_eq!(block.keys[1], ("srcVar1".to_owned(), "$tmp".to_owned()));
        assert_eq!(block.keys[2], ("min".to_owned(), "0.25".to_owned()));
        assert_eq!(block.keys[3], ("max".to_owned(), "$tmp[1]".to_owned()));
    }

    #[test]
    fn the_inverted_wrap_flag_of_an_animated_texture_is_turned_back() {
        let build = |wrap: u8| {
            let mut m = VecMem::default();
            let tables = symbols(&mut m, &["$basetexture", "$frame"]);
            var(&mut m, 0x2000_0000, 0);
            var(&mut m, 0x2000_1000, 1);
            object(
                &mut m,
                0x5000_0000,
                "CAnimatedTextureProxy",
                &[(8, q(0x2000_0000)), (0x10, q(0x2000_1000)), (0x18, f(30.0)), (0x1c, vec![wrap])],
            );
            read_proxy(&m, 0x5000_0000, &tables).expect("proxy")
        };
        let wraps = build(1);
        assert_eq!(wraps.keys.len(), 3);
        assert_eq!(wraps.keys[2], ("animatedTextureFrameRate".to_owned(), "30".to_owned()));
        let once = build(0);
        assert_eq!(once.keys[3], ("animationNoWrap".to_owned(), "1".to_owned()));
    }

    #[test]
    fn a_lua_proxy_gives_only_its_name_and_a_long_name_is_read_from_its_buffer() {
        let mut m = VecMem::default();
        let tables = symbols(&mut m, &[]);
        let mut inline = b"PlayerColor".to_vec();
        inline.resize(16, 0);
        object(&mut m, 0x5000_0000, "MaterialProxy@GarrysMod", &[(0x10, inline), (0x20, q(11)), (0x28, q(15))]);
        let block = read_proxy(&m, 0x5000_0000, &tables).expect("proxy");
        assert_eq!((block.name.as_str(), block.decoded, block.keys.len()), ("PlayerColor", false, 0));
        m.add(0x6000_0000, b"PlayerWeaponColor\0".to_vec());
        object(&mut m, 0x5100_0000, "MaterialProxy@GarrysMod", &[(0x10, q(0x6000_0000)), (0x20, q(17)), (0x28, q(31))]);
        let long = read_proxy(&m, 0x5100_0000, &tables).expect("proxy");
        assert_eq!(long.name, "PlayerWeaponColor");
    }

    #[test]
    fn the_proxies_of_a_material_are_inserted_before_its_closing_brace() {
        let blocks = vec![ProxyBlock {
            name: "Sine".to_owned(),
            keys: vec![("sineMax".to_owned(), "1".to_owned())],
            decoded: true,
            lua_key: None,
        }];
        let vmt = insert_proxies("\"UnlitGeneric\"\r\n{\r\n\t\"$alpha\"\t\t\"1\"\r\n}\r\n", &blocks);
        assert_eq!(
            vmt,
            "\"UnlitGeneric\"\r\n{\r\n\t\"$alpha\"\t\t\"1\"\r\n\t\"Proxies\"\r\n\t{\r\n\t\t\"Sine\"\r\n\t\t{\r\n\t\t\t\"sineMax\"\t\t\"1\"\r\n\t\t}\r\n\t}\r\n}\r\n"
        );
        assert_eq!(insert_proxies("x", &[]), "x");
    }

    #[test]
    fn the_material_record_points_at_the_proxy_array() {
        let mut m = VecMem::default();
        let tables = symbols(&mut m, &["$alpha"]);
        var(&mut m, 0x2000_0000, 0);
        object(&mut m, 0x5000_0000, "CSineProxy", &[(8, q(0x2000_0000))]);
        m.add(0x7000_0000, q(0x5000_0000));
        let mut rec = vec![0u8; 0xc0];
        rec[MATERIAL_PROXY_COUNT_AT] = 1;
        rec[MATERIAL_PROXIES_AT..MATERIAL_PROXIES_AT + 8].copy_from_slice(&0x7000_0000u64.to_le_bytes());
        let blocks = read_proxies(&m, &rec, &tables);
        assert_eq!(blocks.len(), 1);
        rec[MATERIAL_PROXY_COUNT_AT] = 0;
        assert!(read_proxies(&m, &rec, &tables).is_empty());
    }

    #[test]
    fn every_class_of_the_table_has_a_distinct_name_and_known_offsets() {
        let mut names: Vec<&str> = CLASSES.iter().map(|c| c.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CLASSES.len());
        assert!(CLASSES.iter().all(|c| c.fields.iter().all(|f| f.offset + 24 <= OBJECT_READ)));
        assert!(class_of("CTextureScrollMaterialProxy").is_some());
        assert!(class_of("CNoSuchProxy").is_none());
    }
}
