use bytemuck::{Pod, Zeroable};
use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TexId(pub(crate) u32);

impl TexId {
    pub const MISSING: TexId = TexId(0);
    pub const LOADING: TexId = TexId(1);
    pub const WHITE: TexId = TexId(2);
    pub(crate) const FLAT_NORMAL: TexId = TexId(u32::MAX);

    pub const fn raw(self) -> u32 {
        self.0
    }

    pub const fn is_builtin(self) -> bool {
        self.0 <= 2 || self.0 == u32::MAX
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TexFormat {
    Rgba8Srgb,
    Rgba16Float,
    Bc1Srgb,
    Bc2Srgb,
    Bc3Srgb,
    Rgba8Linear,
    Bc1Linear,
    Bc2Linear,
    Bc3Linear,
}

impl TexFormat {
    pub const fn block_bytes(self) -> Option<usize> {
        match self {
            TexFormat::Rgba8Srgb | TexFormat::Rgba8Linear | TexFormat::Rgba16Float => None,
            TexFormat::Bc1Srgb | TexFormat::Bc1Linear => Some(8),
            TexFormat::Bc2Srgb | TexFormat::Bc3Srgb | TexFormat::Bc2Linear | TexFormat::Bc3Linear => Some(16),
        }
    }

    pub const fn is_linear(self) -> bool {
        matches!(
            self,
            TexFormat::Rgba16Float
                | TexFormat::Rgba8Linear
                | TexFormat::Bc1Linear
                | TexFormat::Bc2Linear
                | TexFormat::Bc3Linear
        )
    }

    pub const fn to_linear(self) -> Self {
        match self {
            TexFormat::Rgba16Float => TexFormat::Rgba16Float,
            TexFormat::Rgba8Srgb | TexFormat::Rgba8Linear => TexFormat::Rgba8Linear,
            TexFormat::Bc1Srgb | TexFormat::Bc1Linear => TexFormat::Bc1Linear,
            TexFormat::Bc2Srgb | TexFormat::Bc2Linear => TexFormat::Bc2Linear,
            TexFormat::Bc3Srgb | TexFormat::Bc3Linear => TexFormat::Bc3Linear,
        }
    }

    pub fn level_size(self, width: u32, height: u32) -> usize {
        let (w, h) = (width.max(1) as usize, height.max(1) as usize);
        match self.block_bytes() {
            None => w.saturating_mul(h).saturating_mul(if self == Self::Rgba16Float { 8 } else { 4 }),
            Some(b) => w.div_ceil(4).saturating_mul(h.div_ceil(4)).saturating_mul(b),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureUpload {
    pub width: u32,
    pub height: u32,
    pub format: TexFormat,
    pub levels: Vec<Vec<u8>>,
}

impl TextureUpload {
    pub fn rgba16_float(width: u32, height: u32, pixels: &[[f32; 4]]) -> Result<Self, RenderError> {
        if width == 0 || height == 0 || u64::from(width) * u64::from(height) != pixels.len() as u64 {
            return Err(RenderError::InvalidTexture("float texture size does not match its texels".into()));
        }
        let data = pixels
            .iter()
            .flat_map(|p| {
                p.iter().flat_map(|v| {
                    let clean = if v.is_finite() { v.clamp(-65504.0, 65504.0) } else { 0.0 };
                    crate::compiled_light::f16_bits(clean).to_le_bytes()
                })
            })
            .collect();
        Ok(Self { width, height, format: TexFormat::Rgba16Float, levels: vec![data] })
    }
}

macro_rules! bitflags_u32 {
    ($(#[$meta:meta])* $name:ident { $($(#[$fmeta:meta])* $flag:ident = $val:expr;)* }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name(pub u32);

        impl $name {
            $($(#[$fmeta])* pub const $flag: $name = $name($val);)*

            pub const fn empty() -> Self {
                Self(0)
            }

            pub const fn bits(self) -> u32 {
                self.0
            }

            pub const fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }

            pub const fn intersects(self, other: Self) -> bool {
                self.0 & other.0 != 0
            }

            pub const fn is_empty(self) -> bool {
                self.0 == 0
            }

            pub fn set(&mut self, other: Self, on: bool) {
                if on {
                    self.0 |= other.0;
                } else {
                    self.0 &= !other.0;
                }
            }
        }

        impl BitOr for $name {
            type Output = Self;
            fn bitor(self, rhs: Self) -> Self {
                Self(self.0 | rhs.0)
            }
        }

        impl BitOrAssign for $name {
            fn bitor_assign(&mut self, rhs: Self) {
                self.0 |= rhs.0;
            }
        }

        impl BitAnd for $name {
            type Output = Self;
            fn bitand(self, rhs: Self) -> Self {
                Self(self.0 & rhs.0)
            }
        }

        impl BitAndAssign for $name {
            fn bitand_assign(&mut self, rhs: Self) {
                self.0 &= rhs.0;
            }
        }

        impl Not for $name {
            type Output = Self;
            fn not(self) -> Self {
                Self(!self.0)
            }
        }
    };
}

bitflags_u32! {
    MaterialFlags {
        TRANSLUCENT = 1;
        ALPHA_TEST = 1 << 1;
        ADDITIVE = 1 << 2;
        TOOL = 1 << 3;
        UNLIT = 1 << 4;
        TWO_SIDED = 1 << 5;
        NODRAW_IN_3D = 1 << 6;
        SKY = 1 << 7;
        OPAQUE_TEXTURE = 1 << 8;
        MODULATE = 1 << 9;
        MODULATE_2X = 1 << 10;
        DECAL_MODULATE = 1 << 11;
        SELF_ILLUM = 1 << 12;
        LIGHT_BLOCKER = 1 << 13;
        SPRITE_CARD = 1 << 14;
        PARTICLE_TINT_GAMMA22 = 1 << 15;
        PARTICLE_CLAMP_U = 1 << 16;
        PARTICLE_CLAMP_V = 1 << 17;
    }
}

bitflags_u32! {
    ObjectFlags {
        SELECTED = 1;
        HIDDEN = 1 << 1;
        HOVERED = 1 << 2;
        NO_3D = 1 << 3;
        NO_2D = 1 << 4;
        ALWAYS_EDGES = 1 << 5;
        INVALID = 1 << 6;
        DIMMED = 1 << 7;
        INSTANCE = 1 << 8;
        SKY_3D = 1 << 9;
        ROPE = 1 << 10;
        GAME_HIDDEN = 1 << 11;
        DISPLACEMENT = 1 << 12;
        NO_GAME_WIRE = 1 << 13;
        BRUSH_ENTITY = 1 << 14;
        COMPILED_LIGHT = 1 << 15;
        NO_PROBE = 1 << 16;
        NO_SHADOW = 1 << 17;
        HULL_SHADOW = 1 << 18;
    }
}

pub type MaterialId = u32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialDesc {
    pub texture: Option<TexId>,
    pub tex_size: [f32; 2],
    pub flags: MaterialFlags,
    pub tint: [f32; 4],
    pub bump: Option<BumpDesc>,
    pub texture2: Option<TexId>,
    pub texture3: Option<TexId>,
    pub texture4: Option<TexId>,
    pub scroll: [f32; 2],
    pub base_uv: [f32; 4],
    pub base_uv_cross: [f32; 2],
    pub alpha_ref: f32,
    pub detail: Option<DetailDesc>,
    pub water: Option<WaterDesc>,
    pub envmap: Option<EnvmapDesc>,
    pub self_illum: Option<SelfIllumDesc>,
    pub four_way: Option<FourWayDesc>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FourWayDesc {
    pub lum: [[f32; 2]; 4],
    pub blend: [[f32; 2]; 4],
    pub lum_blend_factor: [f32; 4],
    pub uv_scale: [[f32; 2]; 4],
    pub modulate_2x: [bool; 4],
}

impl Default for FourWayDesc {
    fn default() -> Self {
        Self {
            lum: [[0.0, 1.0]; 4],
            blend: [[0.0, 1.0]; 4],
            lum_blend_factor: [0.0; 4],
            uv_scale: [[1.0, 1.0]; 4],
            modulate_2x: [false; 4],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelfIllumDesc {
    pub tint: [f32; 3],
    pub mask: Option<TexId>,
    pub mask_alpha: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CubemapId(pub(crate) u16);

impl CubemapId {
    pub const SKY: CubemapId = CubemapId(0);

    pub const fn raw(self) -> u16 {
        self.0
    }
}

pub const NO_CUBEMAP: u16 = u16::MAX;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CubemapProbe {
    pub origin: [f32; 3],
    pub size: u32,
    pub sides: Vec<(ObjectId, [f32; 4])>,
}

#[derive(Clone, Copy, Debug)]
pub struct CubemapImage<'a> {
    pub size: u32,
    pub faces: [&'a [[f32; 3]]; 6],
    pub origin: Option<[f32; 3]>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CubemapStats {
    pub probes: u32,
    pub ready: u32,
    pub images: u32,
    pub textures: u32,
    pub pending_faces: u32,
    pub resolution: u32,
    pub bytes: u64,
    pub faces_rendered: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EnvmapAlphaMask {
    #[default]
    None,
    InverseBase,
    Base,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EnvmapTintSpace {
    #[default]
    Linear,
    Gamma22,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvmapDesc {
    pub cubemap: Option<CubemapId>,
    pub texture: Option<TexId>,
    pub tint: [f32; 3],
    pub tint_space: EnvmapTintSpace,
    pub contrast: f32,
    pub saturation: [f32; 3],
    pub mask: Option<TexId>,
    pub mask_alpha: bool,
    pub mask_uv: [f32; 4],
    pub mask_uv_cross: [f32; 2],
    pub mask_scroll: [f32; 2],
    pub alpha_mask: EnvmapAlphaMask,
    pub fresnel: f32,
    pub phong_fresnel: f32,
    pub fresnel_ranges: [f32; 3],
    pub light_scale: f32,
    pub no_envmap: [bool; 2],
    pub parallax: bool,
}

impl Default for EnvmapDesc {
    fn default() -> Self {
        Self {
            cubemap: None,
            texture: None,
            tint: [1.0; 3],
            tint_space: EnvmapTintSpace::Linear,
            contrast: 0.0,
            saturation: [1.0; 3],
            mask: None,
            mask_alpha: false,
            mask_uv: [1.0, 1.0, 0.0, 0.0],
            mask_uv_cross: [0.0; 2],
            mask_scroll: [0.0; 2],
            alpha_mask: EnvmapAlphaMask::None,
            fresnel: 1.0,
            phong_fresnel: 0.0,
            fresnel_ranges: [0.0, 0.5, 1.0],
            light_scale: 0.0,
            no_envmap: [false; 2],
            parallax: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WaterDetail {
    Simple,
    ReflectWorld,
    #[default]
    ReflectAll,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BumpDesc {
    pub texture: TexId,
    pub texture2: Option<TexId>,
    pub blend4: Option<[f32; 4]>,
    pub ssbump: bool,
    pub diffuse: bool,
    pub uv: [f32; 4],
    pub uv_cross: [f32; 2],
    pub scroll: [f32; 2],
    pub uv2: Option<[f32; 4]>,
    pub uv2_cross: [f32; 2],
    pub scroll2: Option<[f32; 2]>,
}

impl BumpDesc {
    pub fn new(texture: TexId) -> Self {
        Self {
            texture,
            texture2: None,
            blend4: None,
            ssbump: false,
            diffuse: true,
            uv: [1.0, 1.0, 0.0, 0.0],
            uv_cross: [0.0; 2],
            scroll: [0.0; 2],
            uv2: None,
            uv2_cross: [0.0; 2],
            scroll2: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterDesc {
    pub normal_map: Option<TexId>,
    pub frames: [u32; 3],
    pub frame_rate: f32,
    pub bump_uv: [f32; 4],
    pub bump_uv_cross: [f32; 2],
    pub bump_scroll: [f32; 2],
    pub scroll1: [f32; 2],
    pub scroll2: [f32; 2],
    pub refract: bool,
    pub refract_tint: [f32; 3],
    pub refract_amount: f32,
    pub reflect: bool,
    pub reflect_tint: [f32; 3],
    pub reflect_amount: f32,
    pub fog_color: [f32; 3],
    pub fog_range: [f32; 2],
    pub cheap: bool,
    pub cheap_blend: bool,
    pub cheap_range: [f32; 2],
    pub fresnel: bool,
    pub reflect_blend: f32,
    pub base_texture: Option<TexId>,
    pub bottom: Option<WaterBottom>,
    pub envmap: bool,
    pub above_water: bool,
    pub reflect_entities: bool,
    pub reflect_skybox_only: bool,
    pub reflect_3d_sky: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterBottom {
    pub refract_tint: [f32; 3],
    pub refract_amount: f32,
    pub fog_color: [f32; 3],
    pub fog_range: [f32; 2],
}

impl Default for WaterDesc {
    fn default() -> Self {
        Self {
            normal_map: None,
            frames: [1, 1, 1],
            frame_rate: 0.0,
            bump_uv: [1.0, 1.0, 0.0, 0.0],
            bump_uv_cross: [0.0; 2],
            bump_scroll: [0.0; 2],
            scroll1: [0.0; 2],
            scroll2: [0.0; 2],
            refract: true,
            refract_tint: [1.0; 3],
            refract_amount: 0.0,
            reflect: true,
            reflect_tint: [1.0; 3],
            reflect_amount: 0.8,
            fog_color: [1.0, 0.0, 0.0],
            fog_range: [0.0, 0.0],
            cheap: false,
            cheap_blend: true,
            cheap_range: [500.0, 1000.0],
            fresnel: true,
            reflect_blend: 1.0,
            base_texture: None,
            bottom: None,
            envmap: false,
            above_water: true,
            reflect_entities: false,
            reflect_skybox_only: false,
            reflect_3d_sky: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailDesc {
    pub texture: TexId,
    pub mode: u32,
    pub raw: bool,
    pub sets_alpha: bool,
    pub uv: [f32; 4],
    pub uv_cross: [f32; 2],
    pub scroll: [f32; 2],
    pub tint_blend: [f32; 4],
}

impl DetailDesc {
    pub const MODE_MOD2X: u32 = 0;
    pub const MODE_ADDITIVE: u32 = 1;
    pub const MODE_DETAIL_OVER_BASE: u32 = 2;
    pub const MODE_FADE: u32 = 3;
    pub const MODE_BASE_OVER_DETAIL: u32 = 4;
    pub const MODE_ADDITIVE_SELFILLUM: u32 = 5;
    pub const MODE_ADDITIVE_SELFILLUM_THRESHOLD_FADE: u32 = 6;
    pub const MODE_MOD2X_SELECT_TWO_PATTERNS: u32 = 7;
    pub const MODE_MULTIPLY: u32 = 8;
    pub const MODE_MASK_BASE_BY_DETAIL_ALPHA: u32 = 9;
    pub const MODE_SSBUMP_BUMP: u32 = 10;
    pub const MODE_SSBUMP_NOBUMP: u32 = 11;
    pub const MODE_TWO_TEXTURE_LERP: u32 = 12;
    pub const MODE_TWO_TEXTURE_MASK: u32 = 13;
}

impl Default for MaterialDesc {
    fn default() -> Self {
        Self {
            texture: None,
            tex_size: [512.0, 512.0],
            flags: MaterialFlags::empty(),
            tint: [1.0; 4],
            bump: None,
            texture2: None,
            texture3: None,
            texture4: None,
            scroll: [0.0; 2],
            base_uv: [1.0, 1.0, 0.0, 0.0],
            base_uv_cross: [0.0; 2],
            alpha_ref: 0.5,
            detail: None,
            water: None,
            envmap: None,
            self_illum: None,
            four_way: None,
        }
    }
}

pub type ObjectId = u32;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub alpha: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshBatch {
    pub material: MaterialId,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub blend: Vec<BlendVertex>,
    pub cubemaps: Vec<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendVertex {
    pub weights: [f32; 4],
    pub painted: bool,
}

impl BlendVertex {
    pub fn painted(weights: [f32; 4]) -> Self {
        Self { weights, painted: true }
    }
}

impl Default for BlendVertex {
    fn default() -> Self {
        Self { weights: [1.0, 0.0, 0.0, 0.0], painted: false }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectMesh {
    pub batches: Vec<MeshBatch>,
    pub edges: Vec<[f32; 3]>,
    pub detail_edges: Vec<[f32; 3]>,
    pub source_edges: Vec<[f32; 3]>,
    pub marks: Vec<MarkTri>,
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
    pub wire_color: [u8; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WalkClass {
    #[default]
    Buildable = 0,
    Walkable = 1,
    Steep = 2,
    Lime = 3,
    Yellow = 4,
    Orange = 5,
    Ember = 6,
    Overhang = 7,
}

impl WalkClass {
    pub const ALL: [WalkClass; 3] = [WalkClass::Buildable, WalkClass::Walkable, WalkClass::Steep];

    pub const GRADIENT: [WalkClass; 8] = [
        WalkClass::Buildable,
        WalkClass::Lime,
        WalkClass::Yellow,
        WalkClass::Walkable,
        WalkClass::Orange,
        WalkClass::Ember,
        WalkClass::Steep,
        WalkClass::Overhang,
    ];

    pub const fn srgb(self) -> [u8; 3] {
        match self {
            WalkClass::Buildable => [60, 200, 90],
            WalkClass::Walkable => [250, 190, 40],
            WalkClass::Steep => [235, 55, 70],
            WalkClass::Lime => [150, 210, 60],
            WalkClass::Yellow => [235, 225, 50],
            WalkClass::Orange => [245, 140, 45],
            WalkClass::Ember => [240, 95, 55],
            WalkClass::Overhang => [150, 25, 110],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MarkTri {
    pub p: [[f32; 3]; 3],
    pub class: WalkClass,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Detail2d {
    #[default]
    All,
    Selected,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelId(pub(crate) u32);

impl ModelId {
    pub const fn raw(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelPart {
    pub material: MaterialId,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub tangents: Vec<[f32; 4]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelLod {
    pub switch_point: f32,
    pub parts: Vec<ModelPart>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DetailSpriteOrientation {
    #[default]
    Fixed,
    ScreenAligned,
    Vertical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DetailSprite {
    pub origin: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub ul: [f32; 2],
    pub lr: [f32; 2],
    pub tex: [f32; 4],
    pub orientation: DetailSpriteOrientation,
    pub sway: u8,
    pub color: [u8; 4],
    pub object: ObjectId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelInstance {
    pub model: ModelId,
    pub transform: [[f32; 4]; 4],
    pub object: ObjectId,
    pub tint: [f32; 4],
    pub fade: [f32; 2],
}

impl ModelInstance {
    pub fn detail_fade(dist: f32, fade: f32) -> [f32; 2] {
        let dist = if dist.is_finite() { dist.max(1.0) } else { 1.0 };
        let fade = if fade.is_finite() { fade.max(0.0) } else { 0.0 };
        [-(dist - fade).max(0.0), -dist]
    }
}

impl Default for ModelInstance {
    fn default() -> Self {
        Self {
            model: ModelId(0),
            transform: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
            object: 0,
            tint: [1.0; 4],
            fade: [0.0; 2],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunGlowSprite {
    pub material: MaterialId,
    pub size: f32,
    pub color: [f32; 3],
    pub modulate: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunGlow {
    pub direction: [f32; 3],
    pub core: SunGlowSprite,
    pub overlay: SunGlowSprite,
    pub hdr_scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteInstance {
    pub pos: [f32; 3],
    pub size: f32,
    pub material: MaterialId,
    pub color: [f32; 4],
    pub object: ObjectId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LightKind {
    Point,
    Spot { dir: [f32; 3], inner_deg: f32, outer_deg: f32 },
    Directional { dir: [f32; 3] },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightFalloff {
    pub constant: f32,
    pub linear: f32,
    pub quadratic: f32,
    pub cap: f32,
    pub cutoff: f32,
    pub fade: [f32; 2],
    pub exponent: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewLight {
    pub kind: LightKind,
    pub pos: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    pub falloff: Option<LightFalloff>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SunParams {
    pub spread_deg: f32,
    pub sky_ambient: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quality {
    pub sun_shadow_size: u32,
    pub local_shadow_size: u32,
}

impl Default for Quality {
    fn default() -> Self {
        Self { sun_shadow_size: u32::MAX, local_shadow_size: u32::MAX }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightStats {
    pub lights: u32,
    pub directional: u32,
    pub left_out: u32,
    pub crowded_clusters: u32,
    pub sky_occlusion: bool,
    pub sky_layers: [u32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OrthoAxis {
    Top,
    Front,
    Side,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewKind {
    Perspective { eye: [f32; 3], forward: [f32; 3], up: [f32; 3], fov_y_deg: f32, near: f32, far: f32 },
    Ortho { axis: OrthoAxis, center: [f32; 3], pixels_per_unit: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderMode {
    Textured,
    Flat,
    Wireframe,
    TexturedWire,
    Lit,
    Unshaded,
    SmoothingGroups,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewCounts {
    pub draw_calls: u32,
    pub triangles: u64,
    pub world_draws: u32,
    pub world_triangles: u64,
    pub model_draws: u32,
    pub model_triangles: u64,
    pub model_instances: u32,
    pub sprites: u32,
    pub material_changes: u32,
    pub chunks_in_frustum: u32,
    pub chunks_occluded: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridParams {
    pub size: f32,
    pub major_every: u32,
    pub show_1024: bool,
    pub cells: bool,
    pub opacity: f32,
}

impl Default for GridParams {
    fn default() -> Self {
        Self { size: 16.0, major_every: 8, show_1024: true, cells: true, opacity: 1.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewParams {
    pub size: [u32; 2],
    pub kind: ViewKind,
    pub mode: RenderMode,
    pub grid: Option<GridParams>,
    pub background: [f32; 3],
    pub selection_color: [f32; 4],
    pub show_sprites: bool,
    pub show_models: bool,
    pub msaa: bool,
    pub game_pixel_grid: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Models2d {
    Boxes,
    Wireframe,
    #[default]
    SelectedOnly,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid3d {
    pub size: f32,
    pub major_every: u32,
    pub fade_cells: f32,
    pub opacity: f32,
}

impl Default for Grid3d {
    fn default() -> Self {
        Self { size: 16.0, major_every: 8, fade_cells: 64.0, opacity: 0.45 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyView {
    pub faces: [Option<TexId>; 6],
    pub uv: [[f32; 4]; 6],
    pub yaw_deg: f32,
}

impl SkyView {
    pub const IDENTITY_UV: [f32; 4] = [1.0, 1.0, 0.0, 0.0];

    pub fn new(faces: [Option<TexId>; 6], yaw_deg: f32) -> Self {
        Self { faces, uv: [Self::IDENTITY_UV; 6], yaw_deg }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyPaint {
    pub top: [f32; 3],
    pub bottom: [f32; 3],
    pub fade_bias: f32,
    pub hdr_scale: f32,
    pub sun_size: f32,
    pub sun_color: [f32; 3],
    pub sun_dir: [f32; 3],
    pub dusk_scale: f32,
    pub dusk_intensity: f32,
    pub dusk_color: [f32; 3],
    pub star_texture: Option<TexId>,
    pub draw_stars: bool,
    pub star_layers: u32,
    pub star_scale: f32,
    pub star_fade: f32,
    pub star_speed: f32,
}

impl SkyPaint {
    pub const BLACK: SkyPaint = SkyPaint {
        top: [0.0; 3],
        bottom: [0.0; 3],
        fade_bias: 1.0,
        hdr_scale: 1.0,
        sun_size: 0.0,
        sun_color: [0.0; 3],
        sun_dir: [0.4, 0.0, 0.01],
        dusk_scale: 1.0,
        dusk_intensity: 0.0,
        dusk_color: [1.0, 0.2, 0.0],
        star_texture: None,
        draw_stars: false,
        star_layers: 1,
        star_scale: 0.5,
        star_fade: 1.0,
        star_speed: 0.01,
    };
}

impl Default for SkyPaint {
    fn default() -> Self {
        SkyPaint {
            top: [0.2, 0.5, 1.0],
            bottom: [0.8, 1.0, 1.0],
            hdr_scale: 0.66,
            sun_size: 2.0,
            sun_color: [0.2, 0.1, 0.0],
            dusk_intensity: 1.0,
            ..SkyPaint::BLACK
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky3d {
    pub origin: [f32; 3],
    pub scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fog {
    pub color: [f32; 3],
    pub color2: [f32; 3],
    pub dir: [f32; 3],
    pub blend: bool,
    pub start: f32,
    pub end: f32,
    pub max_density: f32,
    pub far_z: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogVolume {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub fog: Option<Fog>,
    pub preview: Fog,
    pub active: bool,
}

impl FogVolume {
    pub fn contains(&self, p: [f32; 3]) -> bool {
        (0..3).all(|i| p[i] >= self.min[i] && p[i] <= self.max[i])
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowControl {
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub distance: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewOptions {
    pub shadow_control: Option<ShadowControl>,
    pub screen_fade: [f32; 4],
    pub fade_modulate: bool,
    pub grid_3d: Option<Grid3d>,
    pub models_2d: Models2d,
    pub model_distance: f32,
    pub fill_selected_2d: f32,
    pub vertex_dots_min_zoom: f32,
    pub sprites_2d_px: f32,
    pub show_sky: bool,
    pub show_sky_3d: bool,
    pub sky_see_through: bool,
    pub water_preview: bool,
    pub water_detail: WaterDetail,
    pub show_fog: bool,
    pub fog_volumes: bool,
    pub fog_volume_inside: bool,
    pub occlusion_culling: bool,
    pub force_opaque: bool,
    pub walkable: bool,
    pub detail_2d: Detail2d,
    pub walk_marks: bool,
    pub cull_radius: f32,
    pub time: f32,
    pub model_fade: bool,
    pub collision_models: bool,
    pub collision_only: bool,
    pub selection_outline: bool,
    pub grid_dotted: bool,
    pub grid_highlight_64: bool,
    pub grid_min_px: f32,
    pub source_brush_2d: bool,
    pub game_wireframe: u8,
    pub compiled_lighting: bool,
    pub normal_maps: bool,
    pub cubemaps: bool,
    pub light_shadows: u32,
    pub indirect_light: bool,
    pub classic_colors: bool,
    pub model_lods: bool,
    pub sun_glow: bool,
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            shadow_control: None,
            screen_fade: [0.0; 4],
            fade_modulate: false,
            grid_3d: None,
            models_2d: Models2d::default(),
            model_distance: 0.0,
            fill_selected_2d: 0.0,
            vertex_dots_min_zoom: 0.0,
            sprites_2d_px: 0.0,
            show_sky: false,
            show_sky_3d: false,
            sky_see_through: false,
            water_preview: false,
            water_detail: WaterDetail::ReflectAll,
            show_fog: false,
            fog_volumes: false,
            fog_volume_inside: false,
            occlusion_culling: false,
            force_opaque: false,
            walkable: false,
            detail_2d: Detail2d::All,
            walk_marks: false,
            cull_radius: 0.0,
            time: 0.0,
            model_fade: false,
            collision_models: false,
            collision_only: false,
            selection_outline: false,
            grid_dotted: false,
            grid_highlight_64: false,
            grid_min_px: 4.0,
            source_brush_2d: false,
            game_wireframe: 0,
            compiled_lighting: false,
            normal_maps: false,
            cubemaps: false,
            light_shadows: 0,
            indirect_light: false,
            classic_colors: false,
            model_lods: true,
            sun_glow: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Environment {
    pub sky: Option<SkyView>,
    pub sky_paint: Option<SkyPaint>,
    pub sky_3d: Option<Sky3d>,
    pub fog: Option<Fog>,
    pub sky_fog: Option<Fog>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackgroundImage {
    pub texture: TexId,
    pub axis: OrthoAxis,
    pub corners: [[f32; 3]; 4],
    pub opacity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StaticLineStyle {
    pub width_px: f32,
    pub on_top: bool,
    pub in_2d: bool,
    pub in_3d: bool,
    pub visible: bool,
}

impl Default for StaticLineStyle {
    fn default() -> Self {
        Self { width_px: 1.0, on_top: false, in_2d: true, in_3d: true, visible: true }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayLine {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayTri {
    pub p: [[f32; 3]; 3],
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayPoint {
    pub pos: [f32; 3],
    pub color: [f32; 4],
    pub size_px: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    pub lines: Vec<OverlayLine>,
    pub lines_on_top: Vec<OverlayLine>,
    pub triangles: Vec<OverlayTri>,
    pub triangles_on_top: Vec<OverlayTri>,
    pub points: Vec<OverlayPoint>,
}

impl Overlay {
    pub fn clear(&mut self) {
        self.lines.clear();
        self.lines_on_top.clear();
        self.triangles.clear();
        self.triangles_on_top.clear();
        self.points.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
            && self.lines_on_top.is_empty()
            && self.triangles.is_empty()
            && self.triangles_on_top.is_empty()
            && self.points.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub draw_calls: u32,
    pub triangles: u64,
    pub objects: u32,
    pub chunks: u32,
    pub textures: u32,
    pub approx_gpu_bytes: u64,
    pub retained_uploads: u64,
    pub retained_overlay_bytes: u64,
    pub chunks_in_frustum: u32,
    pub chunks_occluded: u32,
    pub static_lines: u64,
    pub content_generation: u64,
    pub shadowed_lights: u32,
    pub preview_lights: u32,
    pub lights_left_out: u32,
    pub crowded_clusters: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryReport {
    pub geometry: u64,
    pub textures: u64,
    pub models: u64,
    pub lightmaps: u64,
    pub compiled: u64,
    pub other: u64,
    pub total: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("invalid texture: {0}")]
    InvalidTexture(String),
    #[error("unknown view {0}")]
    UnknownView(u32),
    #[error("GPU readback failed: {0}")]
    Readback(String),
    #[error("the graphics card is out of memory")]
    OutOfMemory,
}
