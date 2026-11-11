use std::sync::Arc;

use hp_app::state::MaterialInfo;
use hp_render::{MaterialDesc, MaterialFlags, SelfIllumDesc, TexId};
use hp_vmt::{Material, ToolKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MatKind {
    World,
    Model,
    Sprite,
    SpriteAdditive,
    SpriteRefract,
    EntitySprite,
    EntitySpriteAdditive,
}

pub(crate) const LUM_SUFFIX: &str = "|lum";
pub(crate) const LINEAR_SUFFIX: &str = "|linear";

fn wants_lum(kind: MatKind, a: &MatAnalysis) -> bool {
    !a.flags.contains(MaterialFlags::SPRITE_CARD)
        && (kind == MatKind::SpriteAdditive || (kind == MatKind::Sprite && a.flags.contains(MaterialFlags::ADDITIVE)))
}

pub(crate) fn texture_keys(kind: MatKind, a: &MatAnalysis) -> Vec<String> {
    let lum = wants_lum(kind, a);
    let mut keys: Vec<String> =
        [a.texture.as_deref(), a.texture2.as_deref(), a.texture3.as_deref(), a.texture4.as_deref()]
            .into_iter()
            .flatten()
            .map(|t| {
                if kind == MatKind::SpriteRefract {
                    format!("{t}{}", crate::bump::NORMAL_SUFFIX)
                } else if matches!(kind, MatKind::EntitySprite | MatKind::EntitySpriteAdditive) && !a.sprite_srgb {
                    format!("{t}{LINEAR_SUFFIX}")
                } else if lum {
                    format!("{t}{LUM_SUFFIX}")
                } else {
                    t.to_owned()
                }
            })
            .collect();
    if let Some(w) = a.water.as_ref().filter(|_| kind == MatKind::World) {
        keys.extend(crate::water::texture_keys(w).filter(|k| !keys.contains(k)).collect::<Vec<_>>());
    }
    if let Some(d) = &a.detail
        && !matches!(
            kind,
            MatKind::Sprite | MatKind::SpriteAdditive | MatKind::EntitySprite | MatKind::EntitySpriteAdditive
        )
    {
        keys.push(d.texture.clone());
    }
    if let Some(b) = a.bump.as_ref().filter(|_| shows_bump(kind)) {
        keys.extend(b.keys());
    }
    if let Some(k) = a.self_illum.as_ref().and_then(SelfIllumAnalysis::mask_key)
        && !keys.contains(&k)
    {
        keys.push(k);
    }
    if let Some(e) = a.envmap.as_ref().filter(|_| shows_envmap(kind)) {
        for k in e.keys(normal_maps_loaded(kind, a)) {
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
    }
    keys
}

pub(crate) fn shows_bump(kind: MatKind) -> bool {
    matches!(kind, MatKind::World | MatKind::Model) && hp_app::runtime::render::render_display().normal_maps
}

pub(crate) fn shows_envmap(kind: MatKind) -> bool {
    matches!(kind, MatKind::World | MatKind::Model) && hp_app::runtime::render::render_display().cubemaps
}

fn normal_maps_loaded(kind: MatKind, a: &MatAnalysis) -> bool {
    shows_bump(kind) && a.bump.is_some()
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MatAnalysis {
    pub texture: Option<String>,
    pub sprite_orientation: u32,
    pub sprite_origin: [f32; 2],
    pub sprite_srgb: bool,
    pub sprite_material_mode: Option<u32>,
    pub sprite_vertex_color: bool,
    pub sprite_vertex_alpha: bool,
    pub texture2: Option<String>,
    pub blend4: bool,
    pub four_way: Option<hp_vmt::FourWayBlend>,
    pub texture3: Option<String>,
    pub texture4: Option<String>,
    pub flags: MaterialFlags,
    pub tool: Option<ToolKind>,
    pub tint: [f32; 4],
    particle_tint_gamma22: bool,
    particle_clamp: [bool; 2],
    pub decal_scale: f32,
    pub scroll: [f32; 2],
    pub uv: [f32; 4],
    pub uv_cross: [f32; 2],
    pub alpha_ref: f32,
    pub shows_alpha: bool,
    pub detail: Option<crate::detail::DetailAnalysis>,
    pub water: Option<crate::water::WaterAnalysis>,
    pub bump: Option<crate::bump::BumpAnalysis>,
    pub envmap: Option<crate::envmap::EnvmapAnalysis>,
    pub self_illum: Option<SelfIllumAnalysis>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SelfIllumAnalysis {
    pub tint: [f32; 3],
    pub mask: Option<(String, bool)>,
}

impl SelfIllumAnalysis {
    fn mask_key(&self) -> Option<String> {
        self.mask.as_ref().map(|(name, _)| crate::envmap::mask_key(name, 0))
    }
}

pub(crate) fn texture_transform(mat: &Material) -> [f32; 4] {
    texture_transform_of(mat, "$basetexturetransform")
}

pub(crate) fn texture_transform_of(mat: &Material, key: &str) -> [f32; 4] {
    let rows = hp_vmt::TextureTransform::parse(mat.get(key).unwrap_or("")).matrix();
    [rows[0][0], rows[1][1], rows[0][2], rows[1][2]]
}

pub(crate) fn texture_transform_cross_of(mat: &Material, key: &str) -> [f32; 2] {
    let rows = hp_vmt::TextureTransform::parse(mat.get(key).unwrap_or("")).matrix();
    [rows[0][1], rows[1][0]]
}

pub(crate) fn texture_scroll(mat: &Material) -> [f32; 2] {
    texture_scroll_of(mat, "$basetexturetransform")
}

pub(crate) fn texture_scroll_of(mat: &Material, transform: &str) -> [f32; 2] {
    let Some(proxies) = mat.proxies.as_ref() else { return [0.0; 2] };
    for p in proxies.children_named("texturescroll") {
        let var = p.get("texturescrollvar").unwrap_or("").trim();
        if !var.eq_ignore_ascii_case(transform) {
            continue;
        }
        let num = |k: &str| p.get(k).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite());
        let rate = num("texturescrollrate").unwrap_or(0.0).clamp(-100.0, 100.0);
        let angle = num("texturescrollangle").unwrap_or(0.0).to_radians();
        return [rate * angle.cos(), rate * angle.sin()];
    }
    [0.0; 2]
}

fn tool_is_overlay(kind: ToolKind) -> bool {
    !matches!(kind, ToolKind::Skybox | ToolKind::Skybox2d | ToolKind::Nodraw | ToolKind::Other)
}

pub(crate) fn blocks_light(kind: ToolKind) -> bool {
    matches!(kind, ToolKind::Nodraw | ToolKind::BlockLight | ToolKind::Other)
}

pub(crate) fn tool_flags(tool: Option<ToolKind>, show_tools: bool) -> MaterialFlags {
    let mut flags = MaterialFlags::empty();
    if let Some(kind) = tool {
        if blocks_light(kind) {
            flags |= MaterialFlags::LIGHT_BLOCKER;
        }
        if tool_is_overlay(kind) {
            flags |= MaterialFlags::TOOL;
        }
        if matches!(kind, ToolKind::Skybox | ToolKind::Skybox2d) {
            flags |= MaterialFlags::SKY;
        }
        if !show_tools {
            flags |= MaterialFlags::NODRAW_IN_3D;
        }
    }
    flags
}

pub(crate) fn tool_group(kind: ToolKind) -> u8 {
    hp_app::pick::tool_hide_group(kind)
}

fn tool_hidden(tool: Option<ToolKind>) -> bool {
    let hidden = hp_app::runtime::render::render_display().hidden_tools;
    tool.is_some_and(|k| tool_group(k) & hidden != 0)
}

pub(crate) fn guess_tool(name: &str) -> Option<ToolKind> {
    if !name.starts_with("tools/") {
        return None;
    }
    Material::new("LightmappedGeneric").tool_kind(name)
}

pub(crate) fn shown_usage(mat: &Material, usage: hp_vmt::AlphaUsage, texture: Option<&str>) -> hp_vmt::AlphaUsage {
    let opacity = if mat.shader_kind() == hp_vmt::ShaderKind::Refract { mat.bump_map() } else { mat.base_texture() };
    let texture_alpha = usage.texture_alpha && texture.is_some() && texture == opacity.as_deref();
    hp_vmt::AlphaUsage { texture_alpha, ..usage }
}

pub(crate) fn preview_alpha_table(usage: &hp_vmt::AlphaUsage) -> [u8; 256] {
    std::array::from_fn(|i| (usage.coverage(i as f32 / 255.0) * 255.0).round() as u8)
}

fn alpha_flags(usage: &hp_vmt::AlphaUsage) -> (MaterialFlags, f32) {
    use hp_vmt::BlendMode;
    let mut flags = match usage.blend {
        BlendMode::Opaque => MaterialFlags::empty(),
        BlendMode::Alpha => MaterialFlags::TRANSLUCENT,
        BlendMode::Additive | BlendMode::AlphaAdditive => MaterialFlags::ADDITIVE,
    };
    if !usage.shows_texture_alpha() {
        flags |= MaterialFlags::OPAQUE_TEXTURE;
    }
    if usage.alpha_test.is_some() {
        flags |= MaterialFlags::ALPHA_TEST;
    }
    (flags, usage.alpha_test.unwrap_or(hp_vmt::quantize_alpha_reference(hp_vmt::DEFAULT_ALPHA_TEST_REFERENCE)))
}

#[cfg(test)]
pub(crate) fn analyze(name: &str, mat: &Material, texture_flags: &dyn Fn(&str) -> u32) -> MatAnalysis {
    let mat = mat.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false);
    analyze_resolved(name, &mat, texture_flags)
}

fn sprite_origin(mat: &Material) -> [f32; 2] {
    let fallback = if mat.shader_kind() == hp_vmt::ShaderKind::Sprite { [0.0; 2] } else { [0.5; 2] };
    let Some(value) = mat.get("$spriteorigin") else { return fallback };
    let mut parts =
        value.trim_matches(|c: char| c.is_ascii_whitespace() || c == '[' || c == ']').split_ascii_whitespace();
    let pair = [parts.next(), parts.next()].map(|v| v.and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite()));
    match pair {
        [Some(x), Some(y)] => [x, y],
        _ => [0.0; 2],
    }
}

fn analyze_resolved(name: &str, mat: &Material, texture_flags: &dyn Fn(&str) -> u32) -> MatAnalysis {
    let texture_has_alpha =
        |t: &str| texture_flags(t) & (hp_vtf::flags::ONEBITALPHA | hp_vtf::flags::EIGHTBITALPHA) != 0;
    let tool = mat.tool_kind(name);
    let texture = mat.preview_texture();
    let texture2 = if mat.is_blend() { mat.base_texture2() } else { None };
    let blend4 = mat.shader_kind() == hp_vmt::ShaderKind::Lightmapped4WayBlend;
    let (texture3, texture4) =
        if blend4 { (mat.texture("$basetexture3"), mat.texture("$basetexture4")) } else { (None, None) };
    let alpha = mat.get_f32("srgb?$alpha").filter(|v| v.is_finite()).unwrap_or_else(|| mat.alpha()).clamp(0.0, 1.0);
    let usage = shown_usage(mat, mat.alpha_usage(&texture_has_alpha), texture.as_deref());
    let (mut flags, alpha_ref) = if tool.is_some() {
        let mut f = MaterialFlags::empty();
        if mat.translucent() || alpha < 0.999 {
            f |= MaterialFlags::TRANSLUCENT;
        }
        if mat.alpha_test() {
            f |= MaterialFlags::ALPHA_TEST;
        }
        if mat.additive() {
            f |= MaterialFlags::ADDITIVE;
        }
        (f, hp_vmt::quantize_alpha_reference(mat.alpha_test_reference()))
    } else {
        alpha_flags(&usage)
    };
    if alpha < 0.999 && !flags.contains(MaterialFlags::ALPHA_TEST) {
        flags |= MaterialFlags::TRANSLUCENT;
    }
    match mat.shader_kind() {
        hp_vmt::ShaderKind::Modulate => {
            flags = MaterialFlags::MODULATE;
            if mat.flag("$mod2x") {
                flags |= MaterialFlags::MODULATE_2X;
            }
        }
        hp_vmt::ShaderKind::DecalModulate => flags = MaterialFlags::DECAL_MODULATE,
        _ => {}
    }
    if mat.no_cull() {
        flags |= MaterialFlags::TWO_SIDED;
    }
    if mat.is_unlit() {
        flags |= MaterialFlags::UNLIT;
    }
    if mat.shader_kind() == hp_vmt::ShaderKind::SpriteCard {
        flags |= MaterialFlags::SPRITE_CARD;
    }
    let self_illum = self_illum_of(mat).filter(|s| {
        tool.is_none()
            && (s.mask.is_some() || !flags.intersects(MaterialFlags::TRANSLUCENT | MaterialFlags::ALPHA_TEST))
    });
    if self_illum.is_some() {
        flags |= MaterialFlags::SELF_ILLUM;
    }
    flags |= tool_flags(tool, true);
    let c = mat.color();
    let tint = [c[0].clamp(0.0, 1.0), c[1].clamp(0.0, 1.0), c[2].clamp(0.0, 1.0), alpha];
    let decal_scale = mat.get_f32("$decalscale").filter(|s| s.is_finite() && *s > 0.0).unwrap_or(1.0);
    let scroll = texture_scroll(mat);
    let uv = texture_transform(mat);
    let modulates = flags.intersects(MaterialFlags::MODULATE | MaterialFlags::DECAL_MODULATE);
    let shows_alpha = tool.is_none() && (modulates || usage.is_see_through());
    let detail = (tool.is_none() && !modulates && texture.is_some() && texture == mat.base_texture())
        .then(|| crate::detail::analyze_detail(mat, texture_flags))
        .flatten();
    let water = crate::water::analyze_water(mat);
    let bump = (tool.is_none() && !modulates).then(|| crate::bump::analyze_bump(mat)).flatten();
    let envmap =
        (tool.is_none() && !modulates && water.is_none()).then(|| crate::envmap::analyze_envmap(mat)).flatten();
    MatAnalysis {
        sprite_origin: sprite_origin(mat),
        sprite_srgb: mat.shader_kind() != hp_vmt::ShaderKind::Sprite || mat.get_bool("$nosrgb") == Some(false),
        sprite_material_mode: (mat.shader_kind() != hp_vmt::ShaderKind::Sprite).then_some(if mat.additive() {
            5
        } else {
            2
        }),
        sprite_vertex_color: mat.shader_kind() == hp_vmt::ShaderKind::Sprite || mat.vertex_color(),
        sprite_vertex_alpha: mat.shader_kind() == hp_vmt::ShaderKind::Sprite || mat.vertex_alpha(),
        sprite_orientation: match mat.get("$spriteorientation").unwrap_or("").to_ascii_lowercase().as_str() {
            "facing_upright" | "1" => 1,
            "vp_parallel" | "2" => 2,
            "oriented" | "3" => 3,
            "vp_parallel_oriented" | "4" => 4,
            _ => {
                if mat.shader_kind() == hp_vmt::ShaderKind::Sprite {
                    0
                } else {
                    2
                }
            }
        },
        particle_clamp: {
            let flags = texture.as_deref().map_or(0, texture_flags);
            [flags & hp_vtf::flags::CLAMPS != 0, flags & hp_vtf::flags::CLAMPT != 0]
        },
        texture,
        texture2,
        blend4,
        four_way: mat.four_way_blend(),
        texture3,
        texture4,
        flags,
        tool,
        tint,
        particle_tint_gamma22: mat.shader_kind() == hp_vmt::ShaderKind::UnlitGeneric,
        decal_scale,
        scroll,
        uv,
        uv_cross: texture_transform_cross_of(mat, "$basetexturetransform"),
        alpha_ref,
        shows_alpha,
        detail,
        water,
        bump,
        envmap,
        self_illum,
    }
}

pub(crate) fn make_info(
    name: &str,
    mat: Option<&Material>,
    analysis: Option<&MatAnalysis>,
    size: (u32, u32),
    source: u32,
) -> MaterialInfo {
    let Some(mat) = mat else {
        return MaterialInfo { name: name.to_owned(), tool: guess_tool(name), missing: true, ..Default::default() };
    };
    MaterialInfo {
        name: name.to_owned(),
        width: size.0,
        height: size.1,
        shader: mat.shader.clone(),
        tool: analysis.and_then(|a| a.tool).or_else(|| mat.tool_kind(name)),
        translucent: mat.translucent(),
        alpha_test: mat.alpha_test(),
        shows_alpha: analysis.is_some_and(|a| a.shows_alpha),
        additive: mat.additive(),
        is_water: mat.is_water(),
        is_blend: mat.is_blend(),
        surface_prop: mat.surface_prop().map(str::to_owned),
        keywords: mat.keywords().map(str::to_owned).collect(),
        base_texture: mat.base_texture(),
        base_texture2: if mat.is_blend() { mat.base_texture2() } else { None },
        source,
        missing: false,
        decal_scale: decal_scale_of(mat),
        self_illum: mat.self_illum(),
        env_map: mat.env_map().is_some(),
    }
}

fn self_illum_of(mat: &Material) -> Option<SelfIllumAnalysis> {
    let alpha_mask = mat.flag("$selfillum_envmapmask_alpha");
    if !(mat.self_illum() || alpha_mask) || mat.is_unlit() || mat.translucent() {
        return None;
    }
    let tint = mat
        .get_vec3("$selfillumtint")
        .filter(|t| t.iter().all(|v| v.is_finite()))
        .map_or([1.0; 3], |t| t.map(|v| v.clamp(0.0, 64.0)));
    let mask = if alpha_mask {
        Some((mat.texture("$envmapmask")?, true))
    } else {
        mat.texture("$selfillummask").map(|m| (m, false))
    };
    Some(SelfIllumAnalysis { tint, mask })
}

pub(crate) fn decal_scale_of(mat: &Material) -> Option<f32> {
    mat.get_f32("$decalscale").filter(|s| s.is_finite() && *s > 0.0 && *s <= 1024.0)
}

#[derive(Clone, Copy)]
pub(crate) enum AnaLook<'a> {
    Loading,
    Missing,
    Ready(&'a MatAnalysis),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TexLook {
    Ready(TexId, [u32; 2]),
    Loading,
    Missing,
}

const PLACEHOLDER_SIZE: [u32; 2] = [64, 64];

pub(crate) fn asset_desc(
    kind: MatKind,
    name: &str,
    ana: AnaLook<'_>,
    tex: &dyn Fn(&str) -> TexLook,
    show_tools: bool,
) -> MaterialDesc {
    let fit = |size: [u32; 2]| match kind {
        MatKind::Model
        | MatKind::Sprite
        | MatKind::SpriteAdditive
        | MatKind::SpriteRefract
        | MatKind::EntitySprite
        | MatKind::EntitySpriteAdditive => [1.0, 1.0],
        MatKind::World => [size[0].max(1) as f32, size[1].max(1) as f32],
    };
    let resolve = |look: TexLook| match look {
        TexLook::Ready(id, size) => (id, fit(size)),
        TexLook::Loading => (TexId::LOADING, fit(PLACEHOLDER_SIZE)),
        TexLook::Missing => (TexId::MISSING, fit(PLACEHOLDER_SIZE)),
    };
    match ana {
        AnaLook::Loading | AnaLook::Missing => {
            let look = if matches!(ana, AnaLook::Loading) { TexLook::Loading } else { TexLook::Missing };
            let (id, tex_size) = resolve(look);
            let tool = guess_tool(name);
            MaterialDesc {
                texture: Some(id),
                tex_size,
                flags: tool_flags(tool, show_tools && !tool_hidden(tool)),
                tint: [1.0; 4],
                bump: None,
                texture2: None,
                scroll: [0.0; 2],
                base_uv: [1.0, 1.0, 0.0, 0.0],
                base_uv_cross: [0.0; 2],
                texture3: None,
                texture4: None,
                alpha_ref: hp_vmt::DEFAULT_ALPHA_TEST_REFERENCE,
                detail: None,
                water: None,
                envmap: None,
                self_illum: None,
                four_way: None,
            }
        }
        AnaLook::Ready(a) => {
            let lum = wants_lum(kind, a);
            let look = |t: &str| {
                if kind == MatKind::SpriteRefract {
                    tex(&format!("{t}{}", crate::bump::NORMAL_SUFFIX))
                } else if matches!(kind, MatKind::EntitySprite | MatKind::EntitySpriteAdditive) && !a.sprite_srgb {
                    tex(&format!("{t}{LINEAR_SUFFIX}"))
                } else if lum {
                    tex(&format!("{t}{LUM_SUFFIX}"))
                } else {
                    tex(t)
                }
            };
            let (id, tex_size) = resolve(a.texture.as_deref().map_or(TexLook::Missing, look));
            let texture2 = a.texture2.as_deref().map(|t| resolve(look(t)).0);
            let blend_tex =
                |t: &Option<String>| a.blend4.then(|| t.as_deref().map_or(TexId::WHITE, |t| resolve(look(t)).0));
            let (texture3, texture4) = (blend_tex(&a.texture3), blend_tex(&a.texture4));
            let texture2 = if a.blend4 { Some(texture2.unwrap_or(TexId::WHITE)) } else { texture2 };
            let mut flags = a.flags;
            if kind == MatKind::EntitySpriteAdditive {
                flags |= MaterialFlags::ADDITIVE;
            }
            let self_illum = a.self_illum.as_ref().and_then(|s| {
                let mask = match s.mask_key().map(|k| tex(&k)) {
                    None => None,
                    Some(TexLook::Ready(id, _)) => Some(id),
                    Some(TexLook::Loading | TexLook::Missing) => return None,
                };
                Some(SelfIllumDesc { tint: s.tint, mask, mask_alpha: s.mask.as_ref().is_some_and(|m| m.1) })
            });
            if self_illum.is_none() {
                flags.set(MaterialFlags::SELF_ILLUM, false);
            }
            if a.tool.is_some() && (!show_tools || tool_hidden(a.tool)) {
                flags |= MaterialFlags::NODRAW_IN_3D;
            }
            if a.tool.is_some_and(blocks_light) {
                flags |= MaterialFlags::LIGHT_BLOCKER;
            }
            let sprite = matches!(
                kind,
                MatKind::Sprite
                    | MatKind::SpriteAdditive
                    | MatKind::SpriteRefract
                    | MatKind::EntitySprite
                    | MatKind::EntitySpriteAdditive
            );
            flags.set(MaterialFlags::PARTICLE_TINT_GAMMA22, sprite && a.particle_tint_gamma22);
            flags.set(MaterialFlags::PARTICLE_CLAMP_U, sprite && a.particle_clamp[0]);
            flags.set(MaterialFlags::PARTICLE_CLAMP_V, sprite && a.particle_clamp[1]);
            let detail = a.detail.as_ref().filter(|_| !sprite).and_then(|d| match tex(&d.texture) {
                TexLook::Ready(id, _) => Some(d.desc(id)),
                TexLook::Loading | TexLook::Missing => None,
            });
            let bump = a.bump.as_ref().filter(|_| shows_bump(kind)).and_then(|b| b.desc(tex));
            let envmap =
                a.envmap.as_ref().filter(|_| shows_envmap(kind)).and_then(|e| e.desc(tex, normal_maps_loaded(kind, a)));
            MaterialDesc {
                texture: Some(id),
                tex_size,
                flags,
                tint: a.tint,
                bump,
                texture2,
                texture3,
                texture4,
                scroll: a.scroll,
                base_uv: a.uv,
                base_uv_cross: a.uv_cross,
                alpha_ref: a.alpha_ref,
                detail,
                water: a.water.as_ref().filter(|_| kind == MatKind::World).map(|w| crate::water::desc(w, tex)),
                envmap,
                self_illum,
                four_way: a.four_way.map(four_way_desc),
            }
        }
    }
}

fn four_way_desc(f: hp_vmt::FourWayBlend) -> hp_render::FourWayDesc {
    hp_render::FourWayDesc {
        lum: f.lum,
        blend: f.blend,
        lum_blend_factor: f.lum_blend_factor,
        uv_scale: f.uv_scale,
        modulate_2x: f.modulate_2x,
    }
}

pub(crate) fn flat_desc(rgba: [u8; 4]) -> MaterialDesc {
    let mut flags = MaterialFlags::empty();
    if rgba[3] < 255 {
        flags |= MaterialFlags::TRANSLUCENT;
    }
    MaterialDesc {
        texture: None,
        tex_size: [1.0, 1.0],
        flags,
        tint: rgba.map(|c| f32::from(c) / 255.0),
        bump: None,
        texture2: None,
        scroll: [0.0; 2],
        base_uv: [1.0, 1.0, 0.0, 0.0],
        base_uv_cross: [0.0; 2],
        texture3: None,
        texture4: None,
        alpha_ref: hp_vmt::DEFAULT_ALPHA_TEST_REFERENCE,
        detail: None,
        water: None,
        envmap: None,
        self_illum: None,
        four_way: None,
    }
}

#[derive(Clone)]
pub(crate) struct MaterialData {
    pub analysis: Option<Arc<MatAnalysis>>,
    pub info: Arc<MaterialInfo>,
}

pub(crate) fn load_fresh(fs: &hp_fs::GameFs, name: &str) -> Option<Material> {
    let bytes = fs.read(&format!("materials/{name}.vmt"))?;
    let mat = hp_vmt::parse_bytes(&bytes).ok()?;
    if !mat.is_patch() {
        return Some(mat);
    }
    match hp_vmt::resolve_patch(&mat, &|path| fs.read_text(path)) {
        Ok(resolved) => Some(resolved),
        Err(_) => Some(mat),
    }
}

pub(crate) fn material_job(fs: &hp_fs::GameFs, name: &str, fresh: bool, hdr: bool) -> MaterialData {
    let mat: Option<Arc<Material>> = if fresh { load_fresh(fs, name).map(Arc::new) } else { fs.material(name) };
    match mat {
        None => MaterialData { analysis: None, info: Arc::new(make_info(name, None, None, (0, 0), 0)) },
        Some(m) => {
            let m = m.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, hdr);
            let mut analysis = analyze_resolved(name, &m, &|t| fs.texture_flags(t).unwrap_or(0));
            if let Some(w) = analysis.water.as_mut() {
                crate::water::finish(fs, w, hdr);
            }
            let size = analysis.texture.as_deref().and_then(|t| fs.texture_size(t)).unwrap_or((0, 0));
            let source = fs.material_source(name).unwrap_or(0);
            let info = make_info(name, Some(&m), Some(&analysis), size, source);
            MaterialData { analysis: Some(Arc::new(analysis)), info: Arc::new(info) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_jobs_follow_the_native_hdr_and_ldr_parameter_sections() {
        let dir = std::env::temp_dir().join(format!("hp-water-hdr-job-{}", std::process::id()));
        let materials = dir.join("materials/hp_hdr");
        std::fs::create_dir_all(&materials).unwrap();
        std::fs::write(
            materials.join("water.vmt"),
            r#"Water {
            $normalmap dev/water_normal $reflecttexture _rt_WaterReflection $refracttexture _rt_WaterRefraction
            $reflectamount .87 $refractamount .91 $bottommaterial hp_hdr/bottom
            Water_DX9_HDR { $reflectamount .23 $refractamount .63 }
            Water_DX90 { $reflectamount .31 $refractamount .41 }
        }"#,
        )
        .unwrap();
        std::fs::write(
            materials.join("bottom.vmt"),
            r#"Water {
            $refractamount .17 Water_DX9_HDR { $refractamount .73 }
        }"#,
        )
        .unwrap();
        let fs = hp_fs::GameFs::empty();
        fs.add_game_dir(&dir).unwrap();
        for (hdr, reflect, refract, bottom) in [(false, 0.31, 0.41, 0.17), (true, 0.23, 0.63, 0.73)] {
            let job = material_job(&fs, "hp_hdr/water", false, hdr);
            let water = job.analysis.as_ref().unwrap().water.as_ref().unwrap();
            assert_eq!(water.desc.reflect_amount.to_bits(), f32::to_bits(reflect));
            assert_eq!(water.desc.refract_amount.to_bits(), f32::to_bits(refract));
            assert_eq!(water.desc.bottom.unwrap().refract_amount.to_bits(), f32::to_bits(bottom));
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    fn mat(src: &str) -> Material {
        hp_vmt::parse(src).unwrap_or_default()
    }

    fn analyze(name: &str, m: &Material) -> MatAnalysis {
        super::analyze(name, m, &|_| hp_vtf::flags::EIGHTBITALPHA)
    }

    fn flags(alpha: bool) -> u32 {
        if alpha { hp_vtf::flags::EIGHTBITALPHA } else { 0 }
    }

    #[test]
    fn game_materials_follow_the_source_shaders() {
        use MaterialFlags as F;
        let q = |r: f32| hp_vmt::quantize_alpha_reference(r);
        let cases: &[(&str, &str, bool, F, F, f32, bool)] = &[
            (
                "brick/brickwall014b",
                r#""LightmappedGeneric" { "$basetexture" "Brick/brickwall014b" "$envmap" "env_cubemap" "$basealphaenvmapmask" 1 }"#,
                true,
                F::OPAQUE_TEXTURE,
                F::TRANSLUCENT | F::ALPHA_TEST | F::ADDITIVE,
                0.7,
                false,
            ),
            (
                "demonslayerkido11/mi_b_011_cupboard_01a",
                r#""VertexlitGeneric" { "$basetexture" "demonslayerkido11/MI_b_011_Cupboard_01a" }"#,
                true,
                F::OPAQUE_TEXTURE,
                F::TRANSLUCENT | F::ALPHA_TEST,
                0.7,
                false,
            ),
            (
                "models/props_c17/door02a",
                r#""vertexlitGeneric" { "$basetexture" "models/props_c17/door02a" "$envmapmask" "m" "$translucent" 1 "$alphatest" 1 }"#,
                true,
                F::ALPHA_TEST,
                F::TRANSLUCENT | F::OPAQUE_TEXTURE,
                0.7,
                true,
            ),
            (
                "demonslayerkido11/mi_b_011_plant_01a",
                r#""VertexlitGeneric" { "$basetexture" "x" "$alphatest" 1 "$nocull" 1 "$translucent" 1 }"#,
                true,
                F::ALPHA_TEST | F::TWO_SIDED,
                F::TRANSLUCENT,
                0.7,
                true,
            ),
            (
                "metal/metalfence007a",
                r#""LightmappedGeneric" { "$AlphaTest" "1" "$AlphaTestReference" "0.35" "$basetexture" "Metal/metalfence007a" }"#,
                false,
                F::ALPHA_TEST,
                F::TRANSLUCENT,
                0.35,
                true,
            ),
            (
                "glass/glasswindow007a",
                r#""LightmappedGeneric" { "$basetexture" "Glass/glasswindow007a" "$translucent" 1 "$envmapmask" "m" }"#,
                true,
                F::TRANSLUCENT,
                F::ALPHA_TEST | F::OPAQUE_TEXTURE,
                0.7,
                true,
            ),
            (
                "glass/unflagged",
                r#""LightmappedGeneric" { "$basetexture" "Glass/unflagged" "$translucent" 1 }"#,
                false,
                F::OPAQUE_TEXTURE,
                F::TRANSLUCENT,
                0.7,
                false,
            ),
            (
                "overviews/de_dust",
                r#""UnlitGeneric" { "$translucent" "1" "$basetexture" "overviews/de_dust" "$vertexalpha" "1" }"#,
                false,
                F::TRANSLUCENT | F::UNLIT,
                F::OPAQUE_TEXTURE,
                0.7,
                true,
            ),
            (
                "models/props_combine/introomarea_disp",
                r#""VertexLitGeneric" { "$basetexture" "x" "$translucent" "1" "$additive" "1" "$selfillum" "1" }"#,
                true,
                F::ADDITIVE | F::OPAQUE_TEXTURE,
                F::TRANSLUCENT,
                0.7,
                false,
            ),
            (
                "effects/fire_glow",
                r#""UnlitGeneric" { "$basetexture" "effects/fire_glow" "$translucent" 1 "$selfillum" 1 }"#,
                true,
                F::TRANSLUCENT | F::UNLIT,
                F::OPAQUE_TEXTURE,
                0.7,
                true,
            ),
            (
                "nature/water",
                r#""Water" { "%tooltexture" "dev/water_normal" "$normalmap" "n" "%compilewater" 1 "$translucent" 1 }"#,
                true,
                F::OPAQUE_TEXTURE,
                F::empty(),
                0.7,
                false,
            ),
        ];
        for &(name, src, flagged, with, without, reference, see_through) in cases {
            let a = super::analyze(name, &mat(src), &|_| flags(flagged));
            assert!(a.flags.contains(with), "{name}: {:?} lacks {with:?}", a.flags);
            assert!(!a.flags.intersects(without), "{name}: {:?} has some of {without:?}", a.flags);
            assert_eq!(a.alpha_ref, q(reference), "{name}");
            assert_eq!(a.shows_alpha, see_through, "{name}");
            let info = make_info(name, Some(&mat(src)), Some(&a), (8, 8), 0);
            assert_eq!(info.shows_alpha, see_through, "{name}");
            let d = asset_desc(MatKind::Model, name, AnaLook::Ready(&a), &|_| TexLook::Loading, true);
            assert_eq!(d.alpha_ref, a.alpha_ref);
            assert_eq!(d.flags & (with | without), a.flags & (with | without));
        }
    }

    #[test]
    fn entity_sprite_materials_keep_origin_orientation_and_native_color_space() {
        let legacy = analyze(
            "sprites/glow",
            &mat(r#"Sprite { $basetexture sprites/glow $spriteorigin "[0.5 0.5]" $spriteorientation vp_parallel }"#),
        );
        assert_eq!(legacy.sprite_origin, [0.5; 2]);
        assert_eq!(legacy.sprite_orientation, 2);
        assert!(!legacy.sprite_srgb);
        assert_eq!(texture_keys(MatKind::EntitySpriteAdditive, &legacy), ["sprites/glow|linear"]);
        let corrected = analyze(
            "sprites/glow",
            &mat(r#"Sprite { $basetexture sprites/glow $nosrgb 0 $spriteorigin "[0.25 0.75 0]" }"#),
        );
        assert_eq!(corrected.sprite_origin, [0.25, 0.75]);
        assert!(corrected.sprite_srgb);
        assert!(analyze("sprites/glow", &mat(r#"Sprite { $nosrgb false }"#)).sprite_srgb);
        let generic = analyze("sprites/glow", &mat(r#"UnlitGeneric { $basetexture sprites/glow }"#));
        assert_eq!(generic.sprite_origin, [0.5; 2]);
        assert_eq!(generic.sprite_orientation, 2);
        assert!(generic.sprite_srgb);
        assert_eq!(generic.sprite_material_mode, Some(2));
        assert!(!generic.sprite_vertex_color);
        assert!(!generic.sprite_vertex_alpha);
        assert_eq!(texture_keys(MatKind::EntitySprite, &corrected), ["sprites/glow"]);
        assert_eq!(analyze("sprites/glow", &mat(r#"Sprite { $spriteorigin "[NaN 0.5]" }"#)).sprite_origin, [0.0; 2]);
    }

    #[test]
    fn brush_material() {
        let a = analyze(
            "concrete/floor",
            &mat(r#"LightmappedGeneric { "$basetexture" "Concrete/Floor01" "$color" "[0.5 1 2]" }"#),
        );
        assert_eq!(a.texture.as_deref(), Some("concrete/floor01"));
        assert_eq!(a.texture2, None);
        assert_eq!(a.flags, MaterialFlags::OPAQUE_TEXTURE, "opaque: the texture alpha is ignored");
        assert_eq!(a.tool, None);
        assert_eq!(a.tint, [0.5, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn refractive_particles_use_a_linear_normal_map_and_one_material_alpha() {
        let a = analyze("sprites/heat", &mat("Refract { $normalmap sprites/heat_normal $alpha 0.25 }"));
        assert_eq!(texture_keys(MatKind::SpriteRefract, &a), ["sprites/heat_normal|normal"]);
        let desc = asset_desc(
            MatKind::SpriteRefract,
            "sprites/heat",
            AnaLook::Ready(&a),
            &|key| {
                if key == "sprites/heat_normal|normal" {
                    TexLook::Ready(TexId::WHITE, [16, 16])
                } else {
                    TexLook::Missing
                }
            },
            true,
        );
        assert_eq!(desc.texture, Some(TexId::WHITE));
        assert_eq!(desc.tint[3], 0.25);
        let srgb = analyze("particle/puff", &mat("UnlitGeneric { $basetexture particle/puff \"srgb?$alpha\" 0.27 }"));
        assert_eq!(srgb.tint[3], 0.27);
        assert!(srgb.flags.contains(MaterialFlags::TRANSLUCENT));
    }

    #[test]
    fn render_flags() {
        let a = analyze(
            "glass/x",
            &mat(
                r#"UnlitGeneric { "$basetexture" "glass/x" "$translucent" 1 "$nocull" 1 "$additive" 1 "$alphatest" 1 }"#,
            ),
        );
        for f in [MaterialFlags::TWO_SIDED, MaterialFlags::ADDITIVE, MaterialFlags::ALPHA_TEST, MaterialFlags::UNLIT] {
            assert!(a.flags.contains(f), "{f:?}");
        }
        assert!(!a.flags.contains(MaterialFlags::TRANSLUCENT));
        let lamp = analyze("models/lamp", &mat(r#"VertexLitGeneric { "$basetexture" "lamp" "$selfillum" 1 }"#));
        assert!(lamp.flags.contains(MaterialFlags::SELF_ILLUM | MaterialFlags::OPAQUE_TEXTURE), "{:?}", lamp.flags);
        let wall = analyze("lit/wall", &mat(r#"LightmappedGeneric { "$basetexture" "wall" "$selfillum" 1 }"#));
        assert!(wall.flags.contains(MaterialFlags::SELF_ILLUM));
        assert_eq!(lamp.self_illum, Some(SelfIllumAnalysis { tint: [1.0; 3], mask: None }));
        let masked = analyze(
            "models/masked",
            &mat(
                r#"VertexLitGeneric { "$basetexture" "m" "$selfillum" 1 "$selfillummask" "m_mask" "$selfillumtint" "[2 1 0.5]" }"#,
            ),
        );
        assert!(masked.flags.contains(MaterialFlags::SELF_ILLUM));
        assert_eq!(
            masked.self_illum,
            Some(SelfIllumAnalysis { tint: [2.0, 1.0, 0.5], mask: Some(("m_mask".into(), false)) })
        );
        let key = crate::envmap::mask_key("m_mask", 0);
        assert!(texture_keys(MatKind::Model, &masked).contains(&key));
        let d = asset_desc(MatKind::Model, "models/masked", AnaLook::Ready(&masked), &|_| TexLook::Loading, true);
        assert!(!d.flags.contains(MaterialFlags::SELF_ILLUM) && d.self_illum.is_none());
        let ready = |k: &str| if k == key { TexLook::Ready(TexId::WHITE, [8, 8]) } else { TexLook::Loading };
        let d = asset_desc(MatKind::Model, "models/masked", AnaLook::Ready(&masked), &ready, true);
        assert!(d.flags.contains(MaterialFlags::SELF_ILLUM));
        assert_eq!(
            d.self_illum,
            Some(SelfIllumDesc { tint: [2.0, 1.0, 0.5], mask: Some(TexId::WHITE), mask_alpha: false })
        );
        let env_alpha = analyze(
            "models/env_alpha",
            &mat(r#"VertexLitGeneric { "$basetexture" "e" "$selfillum_envmapmask_alpha" 1 "$envmapmask" "e_mask" }"#),
        );
        assert_eq!(env_alpha.self_illum.and_then(|s| s.mask), Some(("e_mask".into(), true)));
        let glass = analyze(
            "models/glass",
            &mat(r#"VertexLitGeneric { "$basetexture" "g" "$translucent" 1 "$selfillum" 1 "$selfillummask" "m" }"#),
        );
        assert!(!glass.flags.contains(MaterialFlags::SELF_ILLUM) && glass.self_illum.is_none());
        let unlit = analyze("models/unlit", &mat(r#"UnlitGeneric { "$basetexture" "u" "$selfillum" 1 }"#));
        assert!(!unlit.flags.contains(MaterialFlags::SELF_ILLUM));
        let faded = analyze("x", &mat(r#"LightmappedGeneric { "$basetexture" "x" "$alpha" 0.5 }"#));
        assert!(faded.flags.contains(MaterialFlags::TRANSLUCENT));
        assert_eq!(faded.tint[3], 0.5);
        let sky = analyze("skybox/a", &mat(r#"Sky { "$basetexture" "skybox/a" }"#));
        assert!(sky.flags.contains(MaterialFlags::UNLIT));
    }

    #[test]
    fn texture_scroll_proxies() {
        let water = mat(
            r#"LightmappedGeneric { "$basetexture" "x" Proxies { TextureScroll { texturescrollvar $basetexturetransform texturescrollrate 0.5 texturescrollangle 90 } } }"#,
        );
        let s = texture_scroll(&water);
        assert!(s[0].abs() < 1e-6 && (s[1] - 0.5).abs() < 1e-6, "{s:?}");
        assert_eq!(analyze("x", &water).scroll, s);
        let other = mat(r#"X { Proxies { TextureScroll { texturescrollvar $bumptransform texturescrollrate 2 } } }"#);
        assert_eq!(texture_scroll(&other), [0.0; 2], "only the base texture scrolls");
        let hostile =
            mat(r#"X { Proxies { TextureScroll { texturescrollvar $basetexturetransform texturescrollrate nan } } }"#);
        assert_eq!(texture_scroll(&hostile), [0.0; 2]);
        assert_eq!(texture_scroll(&mat("X { $basetexture y }")), [0.0; 2]);
    }

    #[test]
    fn tools_and_blends() {
        let trig = analyze(
            "tools/toolstrigger",
            &mat(r#"LightmappedGeneric { "$basetexture" "tools/toolstrigger" "%compiletrigger" 1 }"#),
        );
        assert_eq!(trig.tool, Some(ToolKind::Trigger));
        assert!(trig.flags.contains(MaterialFlags::TOOL));
        let sky = analyze("tools/toolsskybox", &mat(r#"LightmappedGeneric { "$basetexture" "tools/toolsskybox" }"#));
        assert_eq!(sky.tool, Some(ToolKind::Skybox));
        assert!(!sky.flags.contains(MaterialFlags::TOOL));
        assert!(sky.flags.contains(MaterialFlags::SKY), "sky faces are sky holes");
        assert!(tool_flags(guess_tool("tools/toolsskybox2d"), true).contains(MaterialFlags::SKY));
        assert!(!trig.flags.contains(MaterialFlags::SKY));
        let blend = analyze(
            "nature/blend",
            &mat(r#"WorldVertexTransition { "$basetexture" "nature/grass" "$basetexture2" "nature/dirt" }"#),
        );
        assert_eq!(blend.texture.as_deref(), Some("nature/grass"));
        assert_eq!(blend.texture2.as_deref(), Some("nature/dirt"));
        assert!(!blend.blend4 && blend.texture3.is_none());
        let four = analyze(
            "nature/four",
            &mat(r#"Lightmapped_4WayBlend { "$basetexture" "a/a" "$basetexture2" "a/b" "$basetexture3" "a/c" }"#),
        );
        assert!(four.blend4);
        assert_eq!((four.texture3.as_deref(), four.texture4.as_deref()), (Some("a/c"), None));
        assert_eq!(texture_keys(MatKind::World, &four), ["a/a", "a/b", "a/c"]);
        let tex = |t: &str| if t == "a/c" { TexLook::Missing } else { TexLook::Loading };
        let d = asset_desc(MatKind::World, "nature/four", AnaLook::Ready(&four), &tex, true);
        assert_eq!(d.texture2, Some(TexId::LOADING));
        assert_eq!(
            (d.texture3, d.texture4),
            (Some(TexId::MISSING), Some(TexId::WHITE)),
            "absent fourth texture: white"
        );
        let d = asset_desc(MatKind::World, "nature/blend", AnaLook::Ready(&blend), &tex, true);
        assert_eq!((d.texture3, d.texture4), (None, None));
        let water = analyze(
            "water/x",
            &mat(r#"Water { "%tooltexture" "dev/water_normal" "$normalmap" "x/n" "%compilewater" 1 }"#),
        );
        assert_eq!(water.texture.as_deref(), Some("dev/water_normal"));
        assert_eq!(guess_tool("tools/toolsclip"), Some(ToolKind::Clip));
        assert_eq!(guess_tool("tools/toolsblack"), Some(ToolKind::Other));
        assert_eq!(guess_tool("concrete/x"), None);
    }

    #[test]
    fn descs() {
        let a = analyze("tools/toolsnodraw", &mat(r#"LightmappedGeneric { "$basetexture" "tools/toolsnodraw" }"#));
        let ready = |_: &str| TexLook::Ready(TexId::WHITE, [256, 128]);
        let d = asset_desc(MatKind::World, "tools/toolsnodraw", AnaLook::Ready(&a), &ready, true);
        assert_eq!(d.texture, Some(TexId::WHITE));
        assert_eq!(d.tex_size, [256.0, 128.0]);
        assert!(!d.flags.contains(MaterialFlags::TOOL));
        assert!(!d.flags.contains(MaterialFlags::NODRAW_IN_3D));
        assert!(d.flags.contains(MaterialFlags::LIGHT_BLOCKER), "nodraw brushes still stop the light");
        let hidden = asset_desc(MatKind::World, "tools/toolsnodraw", AnaLook::Ready(&a), &ready, false);
        assert!(hidden.flags.contains(MaterialFlags::NODRAW_IN_3D | MaterialFlags::LIGHT_BLOCKER));
        let clip = analyze(
            "tools/toolsclip",
            &mat(r#"LightmappedGeneric { "$basetexture" "tools/toolsclip" "%compileclip" 1 }"#),
        );
        let clip_desc = asset_desc(MatKind::World, "tools/toolsclip", AnaLook::Ready(&clip), &ready, true);
        assert!(!clip_desc.flags.contains(MaterialFlags::LIGHT_BLOCKER), "clip brushes let the light through");
        assert!(tool_flags(guess_tool("tools/toolsblocklight"), true).contains(MaterialFlags::LIGHT_BLOCKER));
        let model = asset_desc(MatKind::Model, "tools/toolsnodraw", AnaLook::Ready(&a), &ready, true);
        assert_eq!(model.tex_size, [1.0, 1.0]);

        let loading = |_: &str| TexLook::Loading;
        let d = asset_desc(MatKind::World, "x", AnaLook::Ready(&a), &loading, true);
        assert_eq!(d.texture, Some(TexId::LOADING));
        let d = asset_desc(MatKind::World, "tools/toolsclip", AnaLook::Loading, &loading, false);
        assert_eq!(d.texture, Some(TexId::LOADING));
        assert!(d.flags.contains(MaterialFlags::NODRAW_IN_3D | MaterialFlags::TOOL));
        let d = asset_desc(MatKind::World, "gone", AnaLook::Missing, &loading, true);
        assert_eq!(d.texture, Some(TexId::MISSING));

        let glow = analyze("sprites/glow", &mat(r#"Sprite { "$basetexture" "sprites/glow" "$additive" 1 }"#));
        assert_eq!(texture_keys(MatKind::Sprite, &glow), vec!["sprites/glow|lum".to_owned()]);
        assert_eq!(texture_keys(MatKind::World, &glow), vec!["sprites/glow".to_owned()]);
        let lum_only =
            |t: &str| if t.ends_with(LUM_SUFFIX) { TexLook::Ready(TexId::WHITE, [8, 8]) } else { TexLook::Missing };
        let d = asset_desc(MatKind::Sprite, "sprites/glow", AnaLook::Ready(&glow), &lum_only, true);
        assert_eq!(d.texture, Some(TexId::WHITE));

        let f = flat_desc([255, 0, 0, 200]);
        assert!(f.flags.contains(MaterialFlags::TRANSLUCENT));
        assert_eq!(f.texture, None);
        assert!(flat_desc([1, 2, 3, 255]).flags.is_empty());
    }

    #[test]
    fn spritecard_additive_materials_keep_the_authored_texture_alpha() {
        let card = analyze("particle/card", &mat(r#"SpriteCard { "$basetexture" "particle/card" "$additive" 1 }"#));
        for kind in [MatKind::Sprite, MatKind::SpriteAdditive] {
            assert_eq!(texture_keys(kind, &card), ["particle/card"]);
        }
    }

    #[test]
    fn infos() {
        let m = mat(
            r#"LightmappedGeneric { "$basetexture" "a/b" "$surfaceprop" "metal" "%keywords" "gm, metal" "$translucent" 1 }"#,
        );
        let a = analyze("a/b", &m);
        let info = make_info("a/b", Some(&m), Some(&a), (512, 256), 7);
        assert_eq!((info.width, info.height, info.source), (512, 256, 7));
        assert_eq!(info.surface_prop.as_deref(), Some("metal"));
        assert_eq!(info.keywords, vec!["gm".to_owned(), "metal".to_owned()]);
        assert!(info.translucent && !info.missing);
        assert_eq!(info.base_texture.as_deref(), Some("a/b"));
        let missing = make_info("tools/toolsclip", None, None, (0, 0), 0);
        assert!(missing.missing);
        assert_eq!(missing.tool, Some(ToolKind::Clip));
        assert_eq!(missing.decal_scale, None);
    }

    #[test]
    fn the_base_texture_transform_reaches_the_render_description() {
        let src = r#"LightmappedGeneric { "$basetexture" "x" "$basetexturetransform" "center .5 .5 scale 1.25 1.25 rotate 0 translate 0 0" }"#;
        let a = super::analyze("x", &mat(src), &|_| 0);
        let ready = |_: &str| TexLook::Ready(TexId::WHITE, [8, 8]);
        let d = asset_desc(MatKind::World, "x", AnaLook::Ready(&a), &ready, true);
        assert_eq!(d.base_uv, [1.25, 1.25, -0.125, -0.125], "scaled about the center like the engine");
        let plain = super::analyze("x", &mat(r#"LightmappedGeneric { "$basetexture" "x" }"#), &|_| 0);
        assert_eq!(asset_desc(MatKind::World, "x", AnaLook::Ready(&plain), &ready, true).base_uv, [1.0, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn native_affine_rows_reach_each_material_texture_layer() {
        let report: serde_json::Value =
            serde_json::from_str(include_str!("../../hp-vmt/tests/data/native_texture_transform.json")).unwrap();
        let ready = |_: &str| TexLook::Ready(TexId::WHITE, [4, 4]);
        for case in report["cases"].as_array().unwrap() {
            let text = case["input"].as_str().unwrap();
            let source = format!(
                r#"VertexLitGeneric {{
                "$basetexture" "x" "$basetexturetransform" "{text}"
                "$bumpmap" "n" "$bumptransform" "{text}"
                "$detail" "d" "$detailscale" "1" "$detailtexturetransform" "{text}"
                "$envmap" "env_cubemap" "$envmapmask" "m" "$envmapmasktransform" "{text}"
            }}"#
            );
            let a = super::analyze("x", &mat(&source), &|_| 0);
            let check = |uv: [f32; 4], cross: [f32; 2]| {
                let rows = [[uv[0], cross[0], uv[2]], [cross[1], uv[1], uv[3]]];
                for row in 0..2 {
                    for (out, column) in [0, 1, 3].into_iter().enumerate() {
                        let expected: f32 = case["matrix"][row][column].as_str().unwrap().parse().unwrap();
                        assert!(
                            (rows[row][out] - expected).abs() <= 2e-6 * expected.abs().max(1.0),
                            "{text:?}: {rows:?}"
                        );
                    }
                }
            };
            let desc = asset_desc(MatKind::Model, "x", AnaLook::Ready(&a), &ready, true);
            check(desc.base_uv, desc.base_uv_cross);
            let bump = a.bump.as_ref().unwrap().desc(&ready).unwrap();
            check(bump.uv, bump.uv_cross);
            let detail = a.detail.as_ref().unwrap().desc(TexId::WHITE);
            check(detail.uv, detail.uv_cross);
            let env = a.envmap.as_ref().unwrap().desc(&ready, true).unwrap();
            check(env.mask_uv, env.mask_uv_cross);
            let water = crate::water::analyze_water(&mat(&format!(
                r#"Water {{ "$normalmap" "n" "$bumptransform" "{text}" }}"#
            )))
            .unwrap();
            check(water.desc.bump_uv, water.desc.bump_uv_cross);
        }
    }

    #[test]
    fn normal_maps_reach_the_render_description() {
        use hp_app::runtime::render::{RenderDisplay, publish_render_display, render_display};
        let src = r#"LightmappedGeneric { "$basetexture" "brick/brickwall003a" "$bumpmap" "brick/brickwall003a_normal"
            "$envmap" "env_cubemap" "$normalmapalphaenvmapmask" 1 }"#;
        let a = super::analyze("brick/brickwall003a", &mat(src), &|_| flags(true));
        assert_eq!(a.bump.as_ref().map(|b| b.texture.as_str()), Some("brick/brickwall003a_normal"));
        assert!(a.flags.contains(MaterialFlags::OPAQUE_TEXTURE));
        assert!(!a.flags.intersects(MaterialFlags::TRANSLUCENT | MaterialFlags::ALPHA_TEST));
        let base_mask = super::analyze(
            "x",
            &mat(r#"LightmappedGeneric { "$basetexture" "x" "$bumpmap" "n" "$basealphaenvmapmask" 1 }"#),
            &|_| flags(true),
        );
        assert!(base_mask.flags.contains(MaterialFlags::OPAQUE_TEXTURE));
        let keys = texture_keys(MatKind::World, &a);
        assert!(keys.contains(&"brick/brickwall003a_normal|normal".to_owned()), "{keys:?}");
        assert!(!texture_keys(MatKind::Sprite, &a).iter().any(|k| k.ends_with("|normal")));
        let ready =
            |t: &str| TexLook::Ready(if t.ends_with("|normal") { TexId::LOADING } else { TexId::WHITE }, [8, 8]);
        let d = asset_desc(MatKind::World, "brick/brickwall003a", AnaLook::Ready(&a), &ready, true);
        assert_eq!(d.bump.map(|b| b.texture), Some(TexId::LOADING));
        let loading =
            |t: &str| if t.ends_with("|normal") { TexLook::Loading } else { TexLook::Ready(TexId::WHITE, [8, 8]) };
        assert!(asset_desc(MatKind::World, "x", AnaLook::Ready(&a), &loading, true).bump.is_none());
        let saved = render_display();
        publish_render_display(RenderDisplay { normal_maps: false, ..saved });
        assert!(!texture_keys(MatKind::World, &a).iter().any(|k| k.ends_with("|normal")));
        assert!(asset_desc(MatKind::World, "x", AnaLook::Ready(&a), &ready, true).bump.is_none());
        publish_render_display(saved);
        let tool = super::analyze(
            "tools/toolsclip",
            &mat(r#"LightmappedGeneric { "$basetexture" "tools/toolsclip" "$bumpmap" "n" "%compileclip" 1 }"#),
            &|_| 0,
        );
        assert!(tool.bump.is_none());
    }

    #[test]
    fn reflections_reach_the_render_description() {
        use hp_app::runtime::render::{RenderDisplay, publish_render_display, render_display};
        let original = render_display();
        let saved = RenderDisplay { cubemaps: true, ..original };
        publish_render_display(saved);
        let src = r#"LightmappedGeneric { "$basetexture" "metal/floor" "$envmap" "env_cubemap" "$envmapmask" "metal/floor_m"
            "$envmaptint" "[.5 .5 .5]" }"#;
        let a = super::analyze("metal/floor", &mat(src), &|_| flags(false));
        assert!(a.envmap.is_some());
        let keys = texture_keys(MatKind::World, &a);
        assert!(keys.contains(&"metal/floor_m|mask".to_owned()), "{keys:?}");
        assert!(!texture_keys(MatKind::Sprite, &a).iter().any(|k| k.ends_with("|mask")), "sprites reflect nothing");
        let ready = |_: &str| TexLook::Ready(TexId::WHITE, [8, 8]);
        let d = asset_desc(MatKind::World, "metal/floor", AnaLook::Ready(&a), &ready, true);
        let e = d.envmap.expect("reflection");
        assert_eq!((e.tint, e.mask, e.cubemap, e.texture), ([0.5; 3], Some(TexId::WHITE), None, None));
        let loading =
            |k: &str| if k.ends_with("|mask") { TexLook::Loading } else { TexLook::Ready(TexId::WHITE, [8, 8]) };
        assert!(asset_desc(MatKind::World, "metal/floor", AnaLook::Ready(&a), &loading, true).envmap.is_none());
        let prop = super::analyze(
            "models/car",
            &mat(r#"VertexLitGeneric { "$basetexture" "car" "$bumpmap" "car_n" "$phong" 1 "$envmap" "env_cubemap" }"#),
            &|_| flags(false),
        );
        let keys = texture_keys(MatKind::Model, &prop);
        assert_eq!(keys.iter().filter(|k| k.starts_with("car_n")).count(), 1, "{keys:?}");
        publish_render_display(RenderDisplay { cubemaps: false, ..saved });
        assert!(!texture_keys(MatKind::World, &a).iter().any(|k| k.ends_with("|mask")));
        assert!(asset_desc(MatKind::World, "metal/floor", AnaLook::Ready(&a), &ready, true).envmap.is_none());
        publish_render_display(RenderDisplay { normal_maps: false, ..saved });
        assert_eq!(texture_keys(MatKind::Model, &prop).iter().filter(|k| k.ends_with("|mask")).count(), 1);
        publish_render_display(original);
        let water = super::analyze("w", &mat(r#"Water { "$normalmap" "n" "$envmap" "env_cubemap" }"#), &|_| 0);
        assert!(water.envmap.is_none());
    }

    #[test]
    fn decal_scale_and_flags_reach_the_info() {
        let m = mat(
            r#"LightmappedGeneric { "$basetexture" "decals/x" "$decalscale" "0.25" "$selfillum" 1 "$envmap" "env_cubemap" }"#,
        );
        let a = analyze("decals/x", &m);
        let info = make_info("decals/x", Some(&m), Some(&a), (128, 128), 0);
        assert_eq!(info.decal_scale, Some(0.25));
        assert!(info.self_illum && info.env_map);
        for bad in ["0", "-2", "nan", "abc", "100000"] {
            let m = mat(&format!(r#"LightmappedGeneric {{ "$decalscale" "{bad}" }}"#));
            assert_eq!(decal_scale_of(&m), None, "{bad}");
        }
        let plain = mat(r#"LightmappedGeneric { "$basetexture" "a/b" }"#);
        let info = make_info("a/b", Some(&plain), None, (0, 0), 0);
        assert_eq!(info.decal_scale, None);
        assert!(!info.self_illum && !info.env_map);
    }
}
