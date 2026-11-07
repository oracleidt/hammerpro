use std::fmt;
use std::ops::{BitAnd, BitOr, BitOrAssign};

use crate::Material;
use crate::value::{normalize_material_name, normalize_texture_path, parse_bool, parse_vec3};

pub const DEFAULT_ALPHA_TEST_REFERENCE: f32 = 0.7;

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct CompileFlags(u32);

impl CompileFlags {
    pub const NODRAW: Self = Self(1 << 0);
    pub const CLIP: Self = Self(1 << 1);
    pub const PLAYER_CLIP: Self = Self(1 << 2);
    pub const NPC_CLIP: Self = Self(1 << 3);
    pub const SKY: Self = Self(1 << 4);
    pub const SKY_2D: Self = Self(1 << 5);
    pub const TRIGGER: Self = Self(1 << 6);
    pub const HINT: Self = Self(1 << 7);
    pub const SKIP: Self = Self(1 << 8);
    pub const AREA_PORTAL: Self = Self(1 << 9);
    pub const OCCLUDER: Self = Self(1 << 10);
    pub const WATER: Self = Self(1 << 11);
    pub const SLIME: Self = Self(1 << 12);
    pub const LADDER: Self = Self(1 << 13);
    pub const NO_LIGHT: Self = Self(1 << 14);
    pub const NO_CHOP: Self = Self(1 << 15);
    pub const NON_SOLID: Self = Self(1 << 16);
    pub const DETAIL: Self = Self(1 << 17);
    pub const ORIGIN: Self = Self(1 << 18);
    pub const FOG: Self = Self(1 << 19);
    pub const GRATE: Self = Self(1 << 20);
    pub const BLOCK_LOS: Self = Self(1 << 21);
    pub const PASS_BULLETS: Self = Self(1 << 22);
    pub const INVISIBLE: Self = Self(1 << 23);
    pub const NO_PORTAL: Self = Self(1 << 24);

    pub const TOOL_MASK: Self = Self(
        Self::NODRAW.0
            | Self::CLIP.0
            | Self::PLAYER_CLIP.0
            | Self::NPC_CLIP.0
            | Self::SKY.0
            | Self::SKY_2D.0
            | Self::TRIGGER.0
            | Self::HINT.0
            | Self::SKIP.0
            | Self::AREA_PORTAL.0
            | Self::OCCLUDER.0
            | Self::ORIGIN.0
            | Self::FOG.0
            | Self::BLOCK_LOS.0
            | Self::INVISIBLE.0,
    );

    const TABLE: &'static [(&'static str, CompileFlags)] = &[
        ("%compilenodraw", Self::NODRAW),
        ("%compileclip", Self::CLIP),
        ("%compileplayerclip", Self::PLAYER_CLIP),
        ("%playerclip", Self::PLAYER_CLIP),
        ("%compilenpcclip", Self::NPC_CLIP),
        ("%compilemonsterclip", Self::NPC_CLIP),
        ("%compilesky", Self::SKY),
        ("%compile2dsky", Self::SKY_2D),
        ("%compiletrigger", Self::TRIGGER),
        ("%compilehint", Self::HINT),
        ("%compileskip", Self::SKIP),
        ("%compileareaportal", Self::AREA_PORTAL),
        ("%compileoccluder", Self::OCCLUDER),
        ("%compilewater", Self::WATER),
        ("%compileslime", Self::SLIME),
        ("%compileladder", Self::LADDER),
        ("%compilenolight", Self::NO_LIGHT),
        ("%compilenochop", Self::NO_CHOP),
        ("%compilenonsolid", Self::NON_SOLID),
        ("%compiledetail", Self::DETAIL),
        ("%compileorigin", Self::ORIGIN),
        ("%compilefog", Self::FOG),
        ("%compilegrate", Self::GRATE),
        ("%compileblocklos", Self::BLOCK_LOS),
        ("%compilepassbullets", Self::PASS_BULLETS),
        ("%compileinvisible", Self::INVISIBLE),
        ("%noportal", Self::NO_PORTAL),
    ];

    const NAMES: &'static [(&'static str, CompileFlags)] = &[
        ("nodraw", Self::NODRAW),
        ("clip", Self::CLIP),
        ("playerclip", Self::PLAYER_CLIP),
        ("npcclip", Self::NPC_CLIP),
        ("sky", Self::SKY),
        ("2dsky", Self::SKY_2D),
        ("trigger", Self::TRIGGER),
        ("hint", Self::HINT),
        ("skip", Self::SKIP),
        ("areaportal", Self::AREA_PORTAL),
        ("occluder", Self::OCCLUDER),
        ("water", Self::WATER),
        ("slime", Self::SLIME),
        ("ladder", Self::LADDER),
        ("nolight", Self::NO_LIGHT),
        ("nochop", Self::NO_CHOP),
        ("nonsolid", Self::NON_SOLID),
        ("detail", Self::DETAIL),
        ("origin", Self::ORIGIN),
        ("fog", Self::FOG),
        ("grate", Self::GRATE),
        ("blocklos", Self::BLOCK_LOS),
        ("passbullets", Self::PASS_BULLETS),
        ("invisible", Self::INVISIBLE),
        ("noportal", Self::NO_PORTAL),
    ];

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub fn from_param(key: &str) -> Option<Self> {
        Self::TABLE.iter().find(|(name, _)| name.eq_ignore_ascii_case(key)).map(|&(_, flag)| flag)
    }

    pub fn names(self) -> impl Iterator<Item = &'static str> {
        Self::NAMES.iter().filter(move |(_, flag)| self.contains(*flag)).map(|&(name, _)| name)
    }
}

impl BitOr for CompileFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for CompileFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for CompileFlags {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl fmt::Debug for CompileFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CompileFlags(")?;
        for (i, name) in self.names().enumerate() {
            if i > 0 {
                f.write_str(" | ")?;
            }
            f.write_str(name)?;
        }
        f.write_str(")")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ToolKind {
    Nodraw,
    Clip,
    PlayerClip,
    NpcClip,
    Trigger,
    Skybox,
    Skybox2d,
    Hint,
    Skip,
    AreaPortal,
    Occluder,
    Invisible,
    BlockLight,
    BlockBullets,
    Fog,
    Ladder,
    Origin,
    Other,
}

impl ToolKind {
    pub const ALL: [ToolKind; 18] = [
        ToolKind::Nodraw,
        ToolKind::Clip,
        ToolKind::PlayerClip,
        ToolKind::NpcClip,
        ToolKind::Trigger,
        ToolKind::Skybox,
        ToolKind::Skybox2d,
        ToolKind::Hint,
        ToolKind::Skip,
        ToolKind::AreaPortal,
        ToolKind::Occluder,
        ToolKind::Invisible,
        ToolKind::BlockLight,
        ToolKind::BlockBullets,
        ToolKind::Fog,
        ToolKind::Ladder,
        ToolKind::Origin,
        ToolKind::Other,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ToolKind::Nodraw => "nodraw",
            ToolKind::Clip => "clip",
            ToolKind::PlayerClip => "playerclip",
            ToolKind::NpcClip => "npcclip",
            ToolKind::Trigger => "trigger",
            ToolKind::Skybox => "skybox",
            ToolKind::Skybox2d => "skybox2d",
            ToolKind::Hint => "hint",
            ToolKind::Skip => "skip",
            ToolKind::AreaPortal => "areaportal",
            ToolKind::Occluder => "occluder",
            ToolKind::Invisible => "invisible",
            ToolKind::BlockLight => "blocklight",
            ToolKind::BlockBullets => "blockbullets",
            ToolKind::Fog => "fog",
            ToolKind::Ladder => "ladder",
            ToolKind::Origin => "origin",
            ToolKind::Other => "other",
        }
    }

    const NAMES: &'static [(&'static str, ToolKind)] = &[
        ("tools/toolsnodraw", ToolKind::Nodraw),
        ("tools/toolsclip", ToolKind::Clip),
        ("tools/toolsplayerclip", ToolKind::PlayerClip),
        ("tools/toolsnpcclip", ToolKind::NpcClip),
        ("tools/toolstrigger", ToolKind::Trigger),
        ("tools/toolsskybox", ToolKind::Skybox),
        ("tools/toolsskybox2d", ToolKind::Skybox2d),
        ("tools/toolshint", ToolKind::Hint),
        ("tools/toolsskip", ToolKind::Skip),
        ("tools/toolsareaportal", ToolKind::AreaPortal),
        ("tools/toolsoccluder", ToolKind::Occluder),
        ("tools/toolsinvisible", ToolKind::Invisible),
        ("tools/toolsinvisibleladder", ToolKind::Ladder),
        ("tools/toolsladder", ToolKind::Ladder),
        ("tools/toolsblocklight", ToolKind::BlockLight),
        ("tools/toolsblockbullets", ToolKind::BlockBullets),
        ("tools/toolsfog", ToolKind::Fog),
        ("tools/toolsorigin", ToolKind::Origin),
    ];

    const PREFIXES: &'static [(&'static str, ToolKind)] = &[
        ("tools/toolsclip_", ToolKind::Clip),
        ("tools/toolsplayerclip_", ToolKind::PlayerClip),
        ("tools/toolsnpcclip_", ToolKind::NpcClip),
        ("tools/toolsnodraw_", ToolKind::Nodraw),
        ("tools/toolstrigger_", ToolKind::Trigger),
        ("tools/toolsblockbullets", ToolKind::BlockBullets),
        ("tools/toolsinvisible_", ToolKind::Invisible),
    ];

    fn from_known_name(name: &str) -> Option<Self> {
        if let Some(&(_, kind)) = Self::NAMES.iter().find(|(n, _)| *n == name) {
            return Some(kind);
        }
        Self::PREFIXES.iter().find(|(p, _)| name.starts_with(p)).map(|&(_, kind)| kind)
    }

    fn from_flags(f: CompileFlags) -> Option<Self> {
        use CompileFlags as F;
        let kind = if f.contains(F::SKY_2D) {
            ToolKind::Skybox2d
        } else if f.contains(F::SKY) {
            ToolKind::Skybox
        } else if f.contains(F::AREA_PORTAL) {
            ToolKind::AreaPortal
        } else if f.contains(F::OCCLUDER) {
            ToolKind::Occluder
        } else if f.contains(F::HINT) {
            ToolKind::Hint
        } else if f.contains(F::SKIP) {
            ToolKind::Skip
        } else if f.contains(F::ORIGIN) {
            ToolKind::Origin
        } else if f.contains(F::TRIGGER) {
            ToolKind::Trigger
        } else if f.contains(F::FOG) {
            ToolKind::Fog
        } else if f.contains(F::LADDER) && f.contains(F::NODRAW) {
            ToolKind::Ladder
        } else if f.contains(F::CLIP) || f.contains(F::PLAYER_CLIP | F::NPC_CLIP) {
            ToolKind::Clip
        } else if f.contains(F::PLAYER_CLIP) {
            ToolKind::PlayerClip
        } else if f.contains(F::NPC_CLIP) {
            ToolKind::NpcClip
        } else if f.contains(F::BLOCK_LOS) {
            ToolKind::Other
        } else if f.contains(F::INVISIBLE) {
            ToolKind::Invisible
        } else if f.contains(F::NODRAW) {
            if f.contains(F::NON_SOLID) {
                ToolKind::BlockLight
            } else if f.contains(F::PASS_BULLETS) {
                ToolKind::Invisible
            } else {
                ToolKind::Nodraw
            }
        } else {
            return None;
        };
        Some(kind)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ShaderKind {
    LightmappedGeneric,
    WorldVertexTransition,
    Lightmapped4WayBlend,
    WorldTwoTextureBlend,
    VertexLitGeneric,
    UnlitGeneric,
    UnlitTwoTexture,
    Water,
    Refract,
    Sky,
    DecalModulate,
    Modulate,
    Cable,
    Sprite,
    SpriteCard,
    Eyes,
    Teeth,
    MonitorScreen,
    Patch,
    Other,
}

impl ShaderKind {
    pub fn from_shader_name(name: &str) -> Self {
        match shader_base_name(name).as_str() {
            "lightmappedgeneric" => Self::LightmappedGeneric,
            "worldvertextransition" => Self::WorldVertexTransition,
            "lightmapped_4wayblend" => Self::Lightmapped4WayBlend,
            "worldtwotextureblend" => Self::WorldTwoTextureBlend,
            "vertexlitgeneric" => Self::VertexLitGeneric,
            "unlitgeneric" => Self::UnlitGeneric,
            "unlittwotexture" => Self::UnlitTwoTexture,
            "water" => Self::Water,
            "refract" => Self::Refract,
            "sky" => Self::Sky,
            "decalmodulate" => Self::DecalModulate,
            "modulate" => Self::Modulate,
            "cable" | "splinerope" => Self::Cable,
            "sprite" => Self::Sprite,
            "spritecard" => Self::SpriteCard,
            "eyes" | "eyerefract" => Self::Eyes,
            "teeth" => Self::Teeth,
            "monitorscreen" => Self::MonitorScreen,
            "patch" => Self::Patch,
            _ => Self::Other,
        }
    }

    pub fn is_lightmapped(self) -> bool {
        matches!(
            self,
            Self::LightmappedGeneric
                | Self::WorldVertexTransition
                | Self::Lightmapped4WayBlend
                | Self::WorldTwoTextureBlend
        )
    }

    pub fn is_blend(self) -> bool {
        matches!(self, Self::WorldVertexTransition | Self::Lightmapped4WayBlend)
    }

    pub fn is_model(self) -> bool {
        matches!(self, Self::VertexLitGeneric | Self::Eyes | Self::Teeth)
    }

    pub fn is_unlit(self) -> bool {
        matches!(
            self,
            Self::UnlitGeneric
                | Self::UnlitTwoTexture
                | Self::Sky
                | Self::Sprite
                | Self::SpriteCard
                | Self::MonitorScreen
        )
    }
}

pub fn shader_base_name(name: &str) -> String {
    const SUFFIXES: &[&str] =
        &["_hdr_dx9", "_dx9_hdr", "_dx90", "_dx95", "_dx9", "_dx81", "_dx80", "_dx8", "_dx70", "_dx7", "_dx60", "_dx6"];
    let lower = name.trim().to_ascii_lowercase();
    let mut base = lower.as_str();
    base = base.strip_prefix("sdk_").unwrap_or(base);
    for suffix in SUFFIXES {
        if let Some(stripped) = base.strip_suffix(suffix)
            && !stripped.is_empty()
        {
            base = stripped;
            break;
        }
    }
    base.to_owned()
}

const PREVIEW_KEYS: &[&str] = &[
    "$basetexture",
    "$basetexture2",
    "%tooltexture",
    "$hdrbasetexture",
    "$hdrcompressedtexture",
    "$basetexture3",
    "$basetexture4",
    "$iris",
    "$refracttinttexture",
    "$normalmap",
    "$bumpmap",
    "$dudvmap",
];

fn is_virtual_texture(name: &str) -> bool {
    name.starts_with("_rt_") || name == "env_cubemap"
}

impl Material {
    pub fn texture(&self, key: &str) -> Option<String> {
        self.get(key).map(normalize_texture_path).filter(|s| !s.is_empty())
    }

    pub fn base_texture(&self) -> Option<String> {
        self.texture("$basetexture")
    }

    pub fn base_texture2(&self) -> Option<String> {
        self.texture("$basetexture2")
    }

    pub fn bump_map(&self) -> Option<String> {
        self.texture("$bumpmap").or_else(|| self.texture("$normalmap"))
    }

    pub fn env_map(&self) -> Option<String> {
        self.texture("$envmap")
    }

    pub fn tool_texture(&self) -> Option<String> {
        self.texture("%tooltexture")
    }

    pub fn preview_texture(&self) -> Option<String> {
        if (self.is_tool() || self.is_water())
            && let Some(t) = self.tool_texture()
            && !is_virtual_texture(&t)
        {
            return Some(t);
        }
        PREVIEW_KEYS.iter().filter_map(|key| self.texture(key)).find(|t| !is_virtual_texture(t))
    }

    pub fn surface_prop(&self) -> Option<&str> {
        self.get("$surfaceprop").map(str::trim).filter(|s| !s.is_empty())
    }

    pub fn surface_prop2(&self) -> Option<&str> {
        self.get("$surfaceprop2").map(str::trim).filter(|s| !s.is_empty())
    }

    pub fn keywords(&self) -> impl Iterator<Item = &str> {
        self.get("%keywords").unwrap_or("").split(',').map(str::trim).filter(|s| !s.is_empty())
    }

    pub fn has_keyword(&self, keyword: &str) -> bool {
        self.keywords().any(|k| k.eq_ignore_ascii_case(keyword))
    }

    pub fn translucent(&self) -> bool {
        self.flag("$translucent")
    }

    pub fn alpha_test(&self) -> bool {
        self.flag("$alphatest")
    }

    pub fn alpha_test_reference(&self) -> f32 {
        match self.get_f32("$alphatestreference") {
            Some(v) if v > 0.0 => v.min(1.0),
            _ => DEFAULT_ALPHA_TEST_REFERENCE,
        }
    }

    pub fn additive(&self) -> bool {
        self.flag("$additive")
    }

    pub fn no_cull(&self) -> bool {
        self.flag("$nocull")
    }

    pub fn self_illum(&self) -> bool {
        self.flag("$selfillum")
    }

    pub fn vertex_alpha(&self) -> bool {
        self.flag("$vertexalpha")
    }

    pub fn vertex_color(&self) -> bool {
        self.flag("$vertexcolor")
    }

    pub fn decal(&self) -> bool {
        self.flag("$decal") || self.shader_kind() == ShaderKind::DecalModulate
    }

    pub fn model_flag(&self) -> bool {
        self.flag("$model")
    }

    pub fn ignore_z(&self) -> bool {
        self.flag("$ignorez")
    }

    pub fn no_tool_texture(&self) -> bool {
        self.flag("%notooltexture")
    }

    pub fn alpha(&self) -> f32 {
        self.get_f32("$alpha").map_or(1.0, |a| a.clamp(0.0, 1.0))
    }

    pub fn color_var(&self) -> Option<[f32; 3]> {
        self.get("$color").and_then(parse_vec3)
    }

    pub fn color2_var(&self) -> Option<[f32; 3]> {
        self.get("$color2").and_then(parse_vec3)
    }

    pub fn color(&self) -> [f32; 3] {
        let a = self.color_var().unwrap_or([1.0; 3]);
        let b = self.color2_var().unwrap_or([1.0; 3]);
        [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
    }

    pub fn compile_flags(&self) -> CompileFlags {
        let mut seen = CompileFlags::empty();
        let mut flags = CompileFlags::empty();
        for (key, value) in &self.params {
            if !key.starts_with('%') {
                continue;
            }
            let Some(flag) = CompileFlags::from_param(key) else {
                continue;
            };
            if seen.contains(flag) {
                continue;
            }
            seen |= flag;
            if parse_bool(value) == Some(true) {
                flags |= flag;
            }
        }
        flags
    }

    pub fn is_tool(&self) -> bool {
        self.compile_flags().intersects(CompileFlags::TOOL_MASK)
            || self.base_texture().is_some_and(|t| t.starts_with("tools/"))
    }

    pub fn tool_kind(&self, material_name: &str) -> Option<ToolKind> {
        let name = normalize_material_name(material_name);
        if let Some(kind) = ToolKind::from_known_name(&name) {
            return Some(kind);
        }
        if let Some(kind) = ToolKind::from_flags(self.compile_flags()) {
            return Some(kind);
        }
        if name.starts_with("tools/") {
            return Some(ToolKind::Other);
        }
        None
    }

    pub fn is_water(&self) -> bool {
        self.shader_kind() == ShaderKind::Water
            || self.compile_flags().intersects(CompileFlags::WATER | CompileFlags::SLIME)
    }

    pub fn shader_kind(&self) -> ShaderKind {
        ShaderKind::from_shader_name(&self.shader)
    }

    pub fn shader_base_name(&self) -> String {
        shader_base_name(&self.shader)
    }

    pub fn is_lightmapped(&self) -> bool {
        self.shader_kind().is_lightmapped()
    }

    pub fn is_blend(&self) -> bool {
        self.shader_kind().is_blend()
    }

    pub fn is_model(&self) -> bool {
        self.shader_kind().is_model() || self.model_flag()
    }

    pub fn is_unlit(&self) -> bool {
        self.shader_kind().is_unlit()
    }

    pub fn is_sky_shader(&self) -> bool {
        self.shader_kind() == ShaderKind::Sky
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn mat(src: &str) -> Material {
        parse(src).unwrap()
    }

    #[test]
    fn textures() {
        let m = mat(r#""WorldVertexTransition" {
                "$basetexture" "Materials\Nature\Grass01.vtf"
                "$basetexture2" "nature/DIRT01"
                "$bumpmap" ""
                "$normalmap" "nature/grass_normal"
                "$envmap" "env_cubemap"
                "%tooltexture" "nature/grass_tool"
                "$surfaceprop" " grass "
                "$surfaceprop2" "dirt"
            }"#);
        assert_eq!(m.base_texture().as_deref(), Some("nature/grass01"));
        assert_eq!(m.base_texture2().as_deref(), Some("nature/dirt01"));
        assert_eq!(m.bump_map().as_deref(), Some("nature/grass_normal"));
        assert_eq!(m.env_map().as_deref(), Some("env_cubemap"));
        assert_eq!(m.tool_texture().as_deref(), Some("nature/grass_tool"));
        assert_eq!(m.preview_texture().as_deref(), Some("nature/grass01"));
        assert_eq!(m.surface_prop(), Some("grass"));
        assert_eq!(m.surface_prop2(), Some("dirt"));
        assert!(m.is_blend() && m.is_lightmapped());
        assert_eq!(m.texture("$missing"), None);
    }

    #[test]
    fn preview_rules() {
        let water =
            mat(r#"Water { "%compilewater" 1 "%tooltexture" "dev/water_normal" "$normalmap" "dev/water_normal2" }"#);
        assert!(water.is_water());
        assert_eq!(water.preview_texture().as_deref(), Some("dev/water_normal"));

        let water_no_tool = mat(r#"Water { "$normalmap" "dev/water_normal2" "$bumpmap" "dev/dudv" }"#);
        assert_eq!(water_no_tool.preview_texture().as_deref(), Some("dev/water_normal2"));

        let tool =
            mat(r#"LightmappedGeneric { "$basetexture" "tools/x" "%compilenodraw" 1 "%tooltexture" "tools/x_tool" }"#);
        assert_eq!(tool.preview_texture().as_deref(), Some("tools/x_tool"));

        let regular = mat(r#"LightmappedGeneric { "$basetexture" "a/b" "%tooltexture" "a/b_tool" }"#);
        assert_eq!(regular.preview_texture().as_deref(), Some("a/b"));

        let rt = mat(r#"UnlitGeneric { "$basetexture" "_rt_FullFrameFB" "$normalmap" "x/n" }"#);
        assert_eq!(rt.preview_texture().as_deref(), Some("x/n"));

        let sky = mat(r#"Sky { "$hdrcompressedtexture" "skybox/sky_hdr" }"#);
        assert_eq!(sky.preview_texture().as_deref(), Some("skybox/sky_hdr"));
        assert!(sky.is_sky_shader() && sky.is_unlit());

        let nothing = mat("Sky { }");
        assert_eq!(nothing.preview_texture(), None);
    }

    #[test]
    fn render_flags() {
        let m = mat(r#"VertexLitGeneric {
                $translucent 1 $alphatest 1 $alphatestreference .3 $additive 0 $nocull 1
                $selfillum 1 $vertexalpha 1 $vertexcolor 1 $decal 1 $model 1 $ignorez 1
                $alpha 2 "%noToolTexture" 1
            }"#);
        assert!(m.translucent() && m.alpha_test() && !m.additive() && m.no_cull());
        assert!(m.self_illum() && m.vertex_alpha() && m.vertex_color() && m.decal());
        assert!(m.model_flag() && m.ignore_z() && m.no_tool_texture() && m.is_model());
        assert!((m.alpha_test_reference() - 0.3).abs() < 1e-6);
        assert_eq!(m.alpha(), 1.0);

        let plain = mat("LightmappedGeneric { $alphatestreference 0 }");
        assert_eq!(plain.alpha_test_reference(), DEFAULT_ALPHA_TEST_REFERENCE);
        assert!(!plain.decal());
        assert!(mat("DecalModulate { }").decal());
        assert!((mat("UnlitGeneric { $alpha .5 }").alpha() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn colors() {
        let m = mat(r#"VertexLitGeneric { "$color" "[1 0.5 0]" "$color2" "{255 255 51}" }"#);
        assert_eq!(m.color_var(), Some([1.0, 0.5, 0.0]));
        let c2 = m.color2_var().unwrap();
        assert!((c2[2] - 0.2).abs() < 1e-6);
        let c = m.color();
        assert!((c[0] - 1.0).abs() < 1e-6 && (c[1] - 0.5).abs() < 1e-6 && c[2] == 0.0);
        assert_eq!(mat("X { }").color(), [1.0; 3]);
        assert_eq!(mat("X { $color2 \"[.5 .5 .5]\" }").color(), [0.5; 3]);
    }

    #[test]
    fn keywords() {
        let m = mat(r#"X { "%keywords" " concrete, Floor ,,gm_construct " }"#);
        assert_eq!(m.keywords().collect::<Vec<_>>(), ["concrete", "Floor", "gm_construct"]);
        assert!(m.has_keyword("floor"));
        assert!(!m.has_keyword("wood"));
        assert_eq!(mat("X { }").keywords().count(), 0);
    }

    #[test]
    fn compile_flags() {
        let m = mat(r#"X {
                "%compileNoDraw" 1 "%compileClip" "0" "%playerclip" 1 "%compile2DSky" 1
                "%compilenodraw" 0 "%keywords" "x" "%noportal" "1"
            }"#);
        let f = m.compile_flags();
        assert!(f.contains(CompileFlags::NODRAW));
        assert!(!f.contains(CompileFlags::CLIP));
        assert!(f.contains(CompileFlags::PLAYER_CLIP | CompileFlags::SKY_2D | CompileFlags::NO_PORTAL));
        assert_eq!(f.names().collect::<Vec<_>>(), ["nodraw", "playerclip", "2dsky", "noportal"]);
        assert_eq!(format!("{f:?}"), "CompileFlags(nodraw | playerclip | 2dsky | noportal)");
        assert_eq!(CompileFlags::from_param("%COMPILEWATER"), Some(CompileFlags::WATER));
        assert_eq!(CompileFlags::from_param("$basetexture"), None);
        assert!(CompileFlags::empty().is_empty());
        assert_eq!(CompileFlags::from_bits(f.bits()), f);
        assert_eq!(f & CompileFlags::NODRAW, CompileFlags::NODRAW);
        assert!(m.is_tool());
        assert!(!mat("X { %compilewater 1 }").is_tool());
        assert!(mat("X { $basetexture Tools/ToolsBlack %compilenolight 1 }").is_tool());
    }

    #[test]
    fn tool_kinds_by_name() {
        let nolight = mat(r#"LightmappedGeneric { "$basetexture" "Tools/toolsareaportal" "%compileNoLight" 1 }"#);
        assert_eq!(nolight.tool_kind("tools/toolsareaportal"), Some(ToolKind::AreaPortal));
        assert_eq!(nolight.tool_kind("Materials/TOOLS/toolsoccluder.vmt"), Some(ToolKind::Occluder));
        let empty = mat("LightmappedGeneric { }");
        for (name, kind) in [
            ("tools/toolsnodraw", ToolKind::Nodraw),
            ("tools/toolsclip", ToolKind::Clip),
            ("tools/toolsclip_wood", ToolKind::Clip),
            ("tools/toolsplayerclip", ToolKind::PlayerClip),
            ("tools/toolsnpcclip", ToolKind::NpcClip),
            ("tools/toolstrigger", ToolKind::Trigger),
            ("tools/toolsskybox", ToolKind::Skybox),
            ("tools/toolsskybox2d", ToolKind::Skybox2d),
            ("tools/toolshint", ToolKind::Hint),
            ("tools/toolsskip", ToolKind::Skip),
            ("tools/toolsinvisible", ToolKind::Invisible),
            ("tools/toolsinvisibleladder", ToolKind::Ladder),
            ("tools/toolsblocklight", ToolKind::BlockLight),
            ("tools/toolsblockbullets", ToolKind::BlockBullets),
            ("tools/toolsblockbullets2", ToolKind::BlockBullets),
            ("tools/toolsfog", ToolKind::Fog),
            ("tools/toolsorigin", ToolKind::Origin),
            ("tools/toolsblack", ToolKind::Other),
            ("tools/toolsblock_los", ToolKind::Other),
        ] {
            assert_eq!(empty.tool_kind(name), Some(kind), "{name}");
        }
        assert_eq!(empty.tool_kind("concrete/floor01"), None);
    }

    #[test]
    fn tool_kinds_by_flags() {
        let cases = [
            ("%compile2dsky 1 %compilesky 1", ToolKind::Skybox2d),
            ("%compilesky 1", ToolKind::Skybox),
            ("%compileareaportal 1", ToolKind::AreaPortal),
            ("%compileoccluder 1", ToolKind::Occluder),
            ("%compilehint 1", ToolKind::Hint),
            ("%compileskip 1", ToolKind::Skip),
            ("%compileorigin 1", ToolKind::Origin),
            ("%compiletrigger 1", ToolKind::Trigger),
            ("%compilefog 1", ToolKind::Fog),
            ("%compilenodraw 1 %compilepassbullets 1 %compileladder 1", ToolKind::Ladder),
            ("%compileclip 1", ToolKind::Clip),
            ("%compileplayerclip 1 %compilenpcclip 1", ToolKind::Clip),
            ("%compilenodraw 1 %compilepassbullets 1 %playerclip 1", ToolKind::PlayerClip),
            ("%compilenpcclip 1", ToolKind::NpcClip),
            ("%compileblocklos 1 %compilenodraw 1", ToolKind::Other),
            ("%compileinvisible 1", ToolKind::Invisible),
            ("%compilenonsolid 1 %compilenodraw 1 %compiledetail 1", ToolKind::BlockLight),
            ("%compilenodraw 1 %compilepassbullets 1", ToolKind::Invisible),
            ("%compilenodraw 1", ToolKind::Nodraw),
        ];
        for (params, kind) in cases {
            let m = mat(&format!("LightmappedGeneric {{ {params} }}"));
            assert_eq!(m.tool_kind("custom/mytool"), Some(kind), "{params}");
        }
        assert_eq!(mat("X { %compilewater 1 }").tool_kind("nature/water"), None);
        assert_eq!(mat("X { %compileladder 1 }").tool_kind("metal/ladder"), None);
        assert_eq!(mat("X { %compilenodraw 0 }").tool_kind("a/b"), None);
    }

    #[test]
    fn tool_kind_strings() {
        assert_eq!(ToolKind::ALL.len(), 18);
        let mut names: Vec<_> = ToolKind::ALL.iter().map(|k| k.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 18);
    }

    #[test]
    fn shader_kinds() {
        for (name, kind) in [
            ("LightmappedGeneric", ShaderKind::LightmappedGeneric),
            ("SDK_LightmappedGeneric_DX9", ShaderKind::LightmappedGeneric),
            ("lightmappedgeneric_hdr_dx9", ShaderKind::LightmappedGeneric),
            ("WorldVertexTransition", ShaderKind::WorldVertexTransition),
            ("Lightmapped_4WayBlend", ShaderKind::Lightmapped4WayBlend),
            ("WorldTwoTextureBlend", ShaderKind::WorldTwoTextureBlend),
            ("VertexLitGeneric", ShaderKind::VertexLitGeneric),
            ("UnlitGeneric", ShaderKind::UnlitGeneric),
            ("UnlitTwoTexture", ShaderKind::UnlitTwoTexture),
            ("Water_DX90", ShaderKind::Water),
            ("Refract", ShaderKind::Refract),
            ("sky", ShaderKind::Sky),
            ("DecalModulate", ShaderKind::DecalModulate),
            ("Modulate", ShaderKind::Modulate),
            ("SplineRope", ShaderKind::Cable),
            ("Sprite", ShaderKind::Sprite),
            ("SpriteCard", ShaderKind::SpriteCard),
            ("EyeRefract", ShaderKind::Eyes),
            ("Teeth", ShaderKind::Teeth),
            ("MonitorScreen", ShaderKind::MonitorScreen),
            ("Patch", ShaderKind::Patch),
            ("screenspace_general", ShaderKind::Other),
            ("_dx9", ShaderKind::Other),
        ] {
            assert_eq!(ShaderKind::from_shader_name(name), kind, "{name}");
        }
        assert_eq!(shader_base_name(" SDK_Water_DX81 "), "water");
        assert!(ShaderKind::VertexLitGeneric.is_model());
        assert!(!ShaderKind::LightmappedGeneric.is_unlit());
        let m = mat("WorldVertexTransition { }");
        assert_eq!(m.shader_base_name(), "worldvertextransition");
        assert!(!m.is_model() && !m.is_unlit());
    }
}
