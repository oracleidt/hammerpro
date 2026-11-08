use glam::DVec3;
use hp_fgd::{ResolvedClass, ResolvedFgd};
use hp_vmf::Entity;

use crate::lights::parse_light;
use crate::util::{is_model_path, model_key, parse_floats, sprite_material};

pub(crate) const ENTITY_BIT: u32 = 0x8000_0000;
pub(crate) const WORLD_COLOR: [u8; 3] = [255, 255, 255];
pub(crate) const DEFAULT_EDITOR_COLOR: [u8; 3] = [0, 180, 255];
pub(crate) const BOX_ALPHA: u8 = 210;

pub(crate) const DECAL_BIT: u32 = 0x4000_0000;

pub(crate) fn entity_object(id: hp_vmf::Id) -> u32 {
    id | ENTITY_BIT
}

pub(crate) fn decal_object(id: hp_vmf::Id) -> Option<u32> {
    (id & (ENTITY_BIT | DECAL_BIT) == 0).then_some(id | ENTITY_BIT | DECAL_BIT)
}

pub(crate) fn object_entity(rid: u32) -> Option<hp_vmf::Id> {
    (rid & ENTITY_BIT != 0).then_some(rid & !(ENTITY_BIT | DECAL_BIT))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EntityDesc {
    pub origin: DVec3,
    pub angles: DVec3,
    pub scale: f64,
    pub color: [u8; 3],
    pub classic: [u8; 3],
    pub bounds: (DVec3, DVec3),
    pub model: Option<(String, u32)>,
    pub body: u32,
    pub anim: Option<String>,
    pub fade: [f32; 2],
    pub tint: [f32; 4],
    pub sprite: Option<String>,
    pub sprite_is_icon: bool,
    pub sprite_additive: bool,
    pub sprite_size: f32,
    pub sprite_mode: u32,
    pub sprite_fx: u32,
    pub sprite_hdr_scale: f32,
    pub sprite_glow_proxy: f32,
    pub sprite_color: [f32; 4],
    pub light: Option<crate::lights::SceneLight>,
    pub decal: Option<String>,
}

pub(crate) fn class_of<'a>(fgd: Option<&'a ResolvedFgd>, e: &Entity) -> Option<&'a ResolvedClass> {
    fgd.and_then(|f| f.class(&e.classname))
}

pub(crate) fn model_path<'a>(e: &'a Entity, class: Option<&ResolvedClass>) -> Option<std::borrow::Cow<'a, str>> {
    if let Some(fixed) = class.and_then(ResolvedClass::studio).and_then(|s| s.model).filter(|m| is_model_path(m)) {
        return Some(std::borrow::Cow::Owned(fixed));
    }
    if hp_app::runtime::helpers::fish::is_fish_pool(&e.classname) {
        return None;
    }
    e.get("model").filter(|m| is_model_path(m)).map(std::borrow::Cow::Borrowed)
}

pub(crate) fn local_bounds(class: Option<&ResolvedClass>) -> (DVec3, DVec3) {
    let (min, max) = class.and_then(ResolvedClass::size).unwrap_or(hp_fgd::DEFAULT_POINT_BOUNDS);
    let (min, max) = (DVec3::from(min), DVec3::from(max));
    if min.is_finite() && max.is_finite() && min.cmple(max).all() {
        (min, max)
    } else {
        (DVec3::splat(-8.0), DVec3::splat(8.0))
    }
}

pub(crate) fn model_scale(e: &Entity) -> f64 {
    let read = |key: &str| e.get(key).and_then(|s| s.trim().parse::<f64>().ok()).filter(|s| s.is_finite() && *s > 0.0);
    read("modelscale").or_else(|| read("uniformscale")).map_or(1.0, |s| s.min(1.0e4))
}

fn int_key(e: &Entity, key: &str) -> Option<i64> {
    e.get(key).and_then(|s| {
        let s = s.trim();
        s.parse::<i64>().ok().or_else(|| s.parse::<f64>().ok().filter(|f| f.is_finite()).map(|f| f as i64))
    })
}

fn render_amount_dropped(e: &Entity) -> bool {
    ["prop_static", "func_detail"].iter().any(|c| e.classname.eq_ignore_ascii_case(c))
}

pub(crate) fn render_tint(e: &Entity) -> [f32; 4] {
    let rgb = match e.get("rendercolor").map(parse_floats::<3>) {
        Some((c, 3)) => [(c[0] / 255.0) as f32, (c[1] / 255.0) as f32, (c[2] / 255.0) as f32],
        _ => [1.0; 3],
    };
    let mode = int_key(e, "rendermode").unwrap_or(0);
    let alpha = if matches!(mode, 0 | 10) || render_amount_dropped(e) {
        1.0
    } else {
        e.get("renderamt")
            .and_then(|s| s.trim().parse::<f32>().ok())
            .filter(|a| a.is_finite())
            .map_or(1.0, |a| a / 255.0)
    };
    [rgb[0], rgb[1], rgb[2], alpha].map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 1.0 })
}

pub(crate) fn point_color(e: &Entity, class: Option<&ResolvedClass>) -> [u8; 3] {
    class.and_then(ResolvedClass::color).unwrap_or(e.editor.color)
}

pub(crate) fn brush_entity_color(e: &Entity, class: Option<&ResolvedClass>) -> [u8; 3] {
    let c = e.editor.color;
    if c == DEFAULT_EDITOR_COLOR
        && let Some(fc) = class.and_then(ResolvedClass::color)
    {
        return fc;
    }
    c
}

pub(crate) fn entity_desc(e: &Entity, fgd: Option<&ResolvedFgd>) -> EntityDesc {
    let class = class_of(fgd, e);
    let origin = e.origin().filter(|o| o.is_finite()).unwrap_or(DVec3::ZERO);
    let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
    let bounds = local_bounds(class);
    let skin = e.get("skin").and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0).clamp(0, i64::from(u32::MAX));
    let model = model_path(e, class).map(|p| (model_key(&p), skin as u32));
    let tint = render_tint(e);
    let body =
        int_key(e, "body").or_else(|| int_key(e, "SetBodyGroup")).unwrap_or(0).clamp(0, i64::from(u32::MAX)) as u32;
    let anim = e.get("DefaultAnim").map(str::trim).filter(|a| !a.is_empty()).map(str::to_owned);
    let fade = {
        let read = |k: &str| e.get(k).and_then(|s| s.trim().parse::<f32>().ok()).filter(|v| v.is_finite());
        match read("fademaxdist") {
            Some(max) if max > 0.0 => {
                let min = match read("fademindist").filter(|m| *m >= 0.0) {
                    Some(m) => m.min(max),
                    None if e.classname.trim().eq_ignore_ascii_case("prop_static") => max,
                    None => (max - 400.0).max(0.0),
                };
                [min, max]
            }
            _ => [0.0; 2],
        }
    };
    let (sprite, world_sprite) = match class {
        Some(c) if c.has_helper("iconsprite") => {
            (c.iconsprite().map(str::to_owned).or_else(|| e.get("model").map(str::to_owned)), false)
        }
        Some(c) if c.has_helper("sprite") => {
            (c.sprite().map(str::to_owned).or_else(|| e.get("model").map(str::to_owned)), true)
        }
        _ if e.classname.eq_ignore_ascii_case("env_sprite")
            || e.classname.eq_ignore_ascii_case("env_sprite_oriented") =>
        {
            (e.get("model").map(str::to_owned), true)
        }
        _ => (None, false),
    };
    let sprite = sprite.and_then(|s| sprite_material(&s));
    let sprite_is_icon = sprite.is_some() && !world_sprite;
    let render_mode = e.get("rendermode").and_then(|s| s.trim().parse::<i32>().ok()).unwrap_or(0);
    let sprite_additive = world_sprite && matches!(render_mode, 3 | 5 | 7 | 9);
    let extent = (bounds.1 - bounds.0).max_element();
    let sprite_size = if world_sprite {
        let scale = e.get("scale").and_then(|s| s.trim().parse::<f64>().ok()).filter(|s| s.is_finite() && *s > 0.0);
        scale.unwrap_or(1.0).min(1.0e6) as f32
    } else if extent.is_finite() {
        extent.clamp(16.0, 32.0) as f32
    } else {
        16.0
    };
    let (light, light_rgb) = parse_light(e);
    let sprite_color = match light_rgb {
        Some(rgb) => {
            let m = rgb[0].max(rgb[1]).max(rgb[2]);
            if m > 0.0 { [rgb[0] / m, rgb[1] / m, rgb[2] / m, 1.0] } else { [1.0; 4] }
        }
        None if world_sprite => {
            let amt = e
                .get("renderamt")
                .and_then(|s| s.trim().parse::<f32>().ok())
                .filter(|a| a.is_finite())
                .unwrap_or(255.0);
            [tint[0], tint[1], tint[2], (amt / 255.0).clamp(0.0, 1.0)]
        }
        None => [1.0; 4],
    };
    let tint = if light_rgb.is_some() && model.is_some() { sprite_color } else { tint };
    let decal = decal_material(e, class);
    EntityDesc {
        origin,
        angles,
        scale: model_scale(e),
        color: hp_editor::display_color::chosen(e.editor.color).unwrap_or_else(|| point_color(e, class)),
        classic: point_color(e, class),
        bounds,
        model,
        body,
        anim,
        fade,
        tint,
        sprite,
        sprite_is_icon,
        sprite_additive,
        sprite_size,
        sprite_mode: render_mode.clamp(0, 10) as u32,
        sprite_fx: int_key(e, "renderfx").unwrap_or(0).clamp(0, 255) as u32,
        sprite_hdr_scale: e
            .get("HDRColorScale")
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(1.0)
            .max(0.0),
        sprite_glow_proxy: e
            .get("GlowProxySize")
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(2.0)
            .max(0.0),
        sprite_color,
        light,
        decal,
    }
}

pub(crate) fn decal_material(e: &Entity, class: Option<&ResolvedClass>) -> Option<String> {
    let is_decal = match class {
        Some(c) => c.has_helper("decal"),
        None => e.classname.eq_ignore_ascii_case("infodecal"),
    };
    if !is_decal {
        return None;
    }
    let key = crate::util::material_key(e.get("texture")?.trim());
    (!key.is_empty()).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(class: &str, props: &[(&str, &str)]) -> Entity {
        let mut e = Entity { classname: class.to_owned(), ..Default::default() };
        for (k, v) in props {
            e.set(k, *v);
        }
        e
    }

    const FGD: &str = r#"
        @PointClass size(-16 -16 0, 16 16 72) color(0 255 0) studio("models/editor/playerstart.mdl") = info_player_start : "" []
        @PointClass studioprop() = prop_static : "" []
        @PointClass iconsprite("editor/light.vmt") color(255 255 0) = light : "" []
        @PointClass iconsprite() = env_icon : "" []
        @PointClass sprite() = env_sprite : "" []
        @SolidClass color(220 30 220) = trigger_multiple : "" []
        @PointClass decal() studio("models/editor/axis_helper_thick.mdl") = infodecal : "" []
    "#;

    fn fgd() -> ResolvedFgd {
        hp_fgd::Fgd::parse(FGD, None).resolve()
    }

    #[test]
    fn descriptions() {
        let f = fgd();
        let start = ent("info_player_start", &[("origin", "1 2 3"), ("angles", "0 90 0")]);
        let d = entity_desc(&start, Some(&f));
        assert_eq!(d.origin, DVec3::new(1.0, 2.0, 3.0));
        assert_eq!(d.angles, DVec3::new(0.0, 90.0, 0.0));
        assert_eq!(d.color, [0, 255, 0]);
        assert_eq!(d.bounds, (DVec3::new(-16.0, -16.0, 0.0), DVec3::new(16.0, 16.0, 72.0)));
        assert_eq!(d.model, Some(("models/editor/playerstart.mdl".to_owned(), 0)));
        assert_eq!(d.sprite, None);

        let prop = ent(
            "prop_static",
            &[
                ("model", "Models\\Props_C17\\OilDrum001.mdl"),
                ("skin", "2"),
                ("rendercolor", "255 0 0"),
                ("modelscale", "2"),
            ],
        );
        let d = entity_desc(&prop, Some(&f));
        assert_eq!(d.model, Some(("models/props_c17/oildrum001.mdl".to_owned(), 2)));
        assert_eq!(d.tint, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(d.scale, 2.0);
        assert_eq!(d.origin, DVec3::ZERO);

        let light = ent("light", &[("origin", "0 0 64"), ("_light", "255 128 0 200")]);
        let d = entity_desc(&light, Some(&f));
        assert_eq!(d.sprite.as_deref(), Some("editor/light"));
        assert_eq!(d.sprite_size, 16.0);
        assert_eq!(d.sprite_color, [1.0, 128.0 / 255.0, 0.0, 1.0]);
        assert!(d.light.is_some());

        let icon = ent("env_icon", &[("model", "editor/custom.vmt")]);
        assert_eq!(entity_desc(&icon, Some(&f)).sprite.as_deref(), Some("editor/custom"));
        let spr = ent(
            "env_sprite",
            &[
                ("model", "sprites/glow01.spr"),
                ("rendermode", "5"),
                ("rendercolor", "0 255 0"),
                ("renderamt", "128"),
                ("scale", "0.25"),
            ],
        );
        let d = entity_desc(&spr, Some(&f));
        assert_eq!(d.sprite.as_deref(), Some("sprites/glow01"));
        assert!(d.sprite_additive);
        assert_eq!(d.sprite_size, 0.25);
        assert_eq!(d.sprite_color, [0.0, 1.0, 0.0, 128.0 / 255.0]);
        assert!(!entity_desc(&ent("env_sprite", &[("model", "sprites/glow01.vmt")]), Some(&f)).sprite_additive);

        let fx = ent(
            "prop_dynamic",
            &[
                ("model", "models/x.mdl"),
                ("rendermode", "1"),
                ("renderamt", "51"),
                ("rendercolor", "255 128 0"),
                ("SetBodyGroup", "3"),
                ("DefaultAnim", " Open "),
                ("fademindist", "-1"),
                ("fademaxdist", "800"),
                ("uniformscale", "0"),
                ("modelscale", "0"),
            ],
        );
        let d = entity_desc(&fx, Some(&f));
        let want = [1.0, 128.0 / 255.0, 0.0, 0.2];
        assert!(d.tint.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6), "{:?}", d.tint);
        assert_eq!(d.body, 3);
        assert_eq!(d.anim.as_deref(), Some("Open"));
        assert_eq!(d.fade, [400.0, 800.0], "the client fades over 400 units");
        assert_eq!(d.scale, 1.0, "a scale of 0 counts as 1");
        let normal = ent("prop_dynamic", &[("renderamt", "10"), ("rendermode", "0")]);
        assert_eq!(render_tint(&normal)[3], 1.0, "render amount needs a render mode");
        let faded = ent("prop_static", &[("rendermode", "1"), ("renderamt", "60"), ("rendercolor", "255 0 0")]);
        assert_eq!(render_tint(&faded), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(render_tint(&ent("func_detail", &[("rendermode", "1"), ("renderamt", "60")])), [1.0; 4]);
        let dynamic = ent("prop_dynamic", &[("rendermode", "1"), ("renderamt", "51"), ("rendercolor", "255 0 0")]);
        assert_eq!(render_tint(&dynamic), [1.0, 0.0, 0.0, 0.2]);
        let stat = ent("prop_static", &[("model", "models/x.mdl"), ("uniformscale", "2.5"), ("body", "1")]);
        let d = entity_desc(&stat, Some(&f));
        assert_eq!((d.scale, d.body, d.fade), (2.5, 1, [0.0; 2]));
        let hostile =
            ent("prop_static", &[("body", "-7"), ("renderamt", "nan"), ("rendermode", "5"), ("modelscale", "1e30")]);
        let d = entity_desc(&hostile, Some(&f));
        assert_eq!(d.body, 0);
        assert!(d.tint.iter().all(|c| c.is_finite()));
        assert!(d.scale <= 1.0e4);
        assert!(!d.sprite_is_icon);
        assert!(entity_desc(&ent("light", &[]), Some(&f)).sprite_is_icon);
        assert!(!entity_desc(&ent("env_sprite", &[("model", "sprites/glow01.vmt")]), Some(&f)).sprite_is_icon);

        let unknown = ent("my_unknown_ent", &[("model", "models/x.mdl")]);
        let d = entity_desc(&unknown, Some(&f));
        assert_eq!(d.bounds, (DVec3::splat(-8.0), DVec3::splat(8.0)));
        assert_eq!(d.model.map(|m| m.0).as_deref(), Some("models/x.mdl"));
        assert_eq!(d.color, DEFAULT_EDITOR_COLOR);
        let no_fgd = entity_desc(&light, None);
        assert_eq!(no_fgd.sprite, None);
    }

    #[test]
    fn decals() {
        let f = fgd();
        let decal = ent("infodecal", &[("texture", r"Decals\DecalIndustrialNumber01b"), ("origin", "0 -439 64")]);
        assert_eq!(entity_desc(&decal, Some(&f)).decal.as_deref(), Some("decals/decalindustrialnumber01b"));
        assert_eq!(entity_desc(&decal, None).decal.as_deref(), Some("decals/decalindustrialnumber01b"));
        assert_eq!(entity_desc(&ent("infodecal", &[]), Some(&f)).decal, None);
        let not_decal = ent("light", &[("texture", "decals/x")]);
        assert_eq!(entity_desc(&not_decal, Some(&f)).decal, None);
    }

    #[test]
    fn brush_colors() {
        let f = fgd();
        let mut t = ent("trigger_multiple", &[]);
        assert_eq!(brush_entity_color(&t, class_of(Some(&f), &t)), [220, 30, 220]);
        t.editor.color = [1, 2, 3];
        assert_eq!(brush_entity_color(&t, class_of(Some(&f), &t)), [1, 2, 3]);
    }

    #[test]
    fn point_entities_show_their_chosen_color() {
        let f = fgd();
        let mut light = ent("light", &[("origin", "0 0 0")]);
        for unset in [[0, 180, 255], [220, 30, 220]] {
            light.editor.color = unset;
            let d = entity_desc(&light, Some(&f));
            assert_eq!((d.color, d.classic), ([255, 255, 0], [255, 255, 0]), "game data color by default");
        }
        light.editor.color = [40, 165, 225];
        let d = entity_desc(&light, Some(&f));
        assert_eq!((d.color, d.classic), ([40, 165, 225], [255, 255, 0]));
        let d = entity_desc(&light, None);
        assert_eq!((d.color, d.classic), ([40, 165, 225], [40, 165, 225]));
    }
}
