use std::path::PathBuf;
use std::sync::OnceLock;

use hp_render::lightmap::LightmapDisplay;
use hp_render::*;

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    static GPU: OnceLock<Option<(wgpu::Device, wgpu::Queue)>> = OnceLock::new();
    GPU.get_or_init(|| {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .ok()?;
        if !adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VERTEX_STORAGE) {
            return None;
        }
        let features = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("hp-render lit props test"),
            required_features: features,
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()
    })
    .clone()
}

macro_rules! gpu_or_skip {
    () => {
        match gpu() {
            Some(g) => g,
            None => {
                eprintln!("no GPU adapter available: test skipped");
                return;
            }
        }
    };
}

type Img = (u32, u32, Vec<u8>);

const SIZE: u32 = 200;
const PROP: MaterialId = 1;
const WORLD: MaterialId = 2;
const SKY: MaterialId = 3;
const WALLS: MaterialId = 9;
const MODEL_OBJ: ObjectId = 50;

fn shot(r: &mut Renderer, view: u32, p: &ViewParams, name: &str) -> Img {
    r.render_view(view, p, &Overlay::default());
    let (w, h, px) = r.read_view_rgba8(view).expect("readback");
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("hp-render-scratch");
    let _ = std::fs::create_dir_all(&dir);
    image::save_buffer(dir.join(format!("lit_props_{name}.png")), &px, w, h, image::ColorType::Rgba8).expect("png");
    (w, h, px)
}

fn px(img: &Img, x: u32, y: u32) -> [u8; 3] {
    let o = ((y * img.0 + x) * 4) as usize;
    [img.2[o], img.2[o + 1], img.2[o + 2]]
}

fn srgb(linear: f32) -> f32 {
    let c = linear.clamp(0.0, 1.0);
    if c <= 0.0031308 { c * 12.92 * 255.0 } else { (1.055 * c.powf(1.0 / 2.4) - 0.055) * 255.0 }
}

fn linear(srgb_byte: u8) -> f32 {
    let c = f32::from(srgb_byte) / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn close(got: u8, want: f32, tol: f32, what: &str) {
    assert!((f32::from(got) - want).abs() <= tol, "{what}: got {got}, want {want:.1}");
}

fn mat(r: &mut Renderer, id: MaterialId, rgba: [u8; 4], flags: MaterialFlags) {
    let up = TextureUpload { width: 4, height: 4, format: TexFormat::Rgba8Srgb, levels: vec![rgba.repeat(16)] };
    let t = r.create_texture(&up);
    r.set_material(id, &MaterialDesc { texture: Some(t), tex_size: [4.0, 4.0], flags, ..Default::default() });
}

fn front_view() -> ViewParams {
    ViewParams {
        game_pixel_grid: false,
        size: [SIZE, SIZE],
        kind: ViewKind::Perspective {
            eye: [0.0, 0.0, 0.0],
            forward: [0.0, 1.0, 0.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 90.0,
            near: 1.0,
            far: 10000.0,
        },
        mode: RenderMode::Lit,
        grid: None,
        background: [0.0, 0.0, 0.0],
        selection_color: [1.0, 0.1, 0.1, 0.45],
        show_sprites: false,
        show_models: true,
        msaa: false,
    }
}

fn on_plane(x: u32, y: u32) -> [f32; 3] {
    [x as f32 + 0.5 - 100.0, 100.0, 100.0 - (y as f32 + 0.5)]
}

fn square_model(r: &mut Renderer) -> ModelId {
    let n = [1.0, 0.0, 0.0];
    let v = |y: f32, z: f32| Vertex { pos: [0.0, y, z], normal: n, uv: [0.0, 0.0], alpha: 1.0 };
    r.create_model(&[ModelPart {
        material: PROP,
        vertices: vec![v(-100.0, -100.0), v(100.0, -100.0), v(100.0, 100.0), v(-100.0, 100.0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        tangents: Vec::new(),
    }])
}

fn facing_eye(model: ModelId, object: ObjectId) -> ModelInstance {
    ModelInstance {
        model,
        transform: [[0.0, -1.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 100.0, 0.0, 1.0]],
        object,
        tint: [1.0; 4],
        fade: [0.0; 2],
    }
}

fn quadratic() -> LightFalloff {
    LightFalloff { quadratic: 1.0, cap: 1.0e9, ..LightFalloff::default() }
}

fn point(pos: [f32; 3], intensity: f32, falloff: Option<LightFalloff>) -> PreviewLight {
    PreviewLight { kind: LightKind::Point, pos, color: [1.0; 3], intensity, range: 16384.0, falloff }
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[test]
fn props_take_point_and_spot_lights_with_vrad_falloff() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, PROP, [200, 200, 200, 255], MaterialFlags::empty());
    let model = square_model(&mut r);
    r.set_model_instances(&[facing_eye(model, MODEL_OBJ)]);
    let albedo = linear(200);
    let view = front_view();

    r.set_lights(&[point([0.0, 50.0, 0.0], 2500.0, Some(quadratic()))], [0.0; 3]);
    let img = shot(&mut r, 1, &view, "point_near");
    let lit_at = |x: u32, y: u32, light: [f32; 3], i: f32| {
        let p = on_plane(x, y);
        let d = dist(p, light);
        let ndl = (light[1] - p[1]).abs() / d;
        albedo * i / (d * d) * ndl
    };
    close(px(&img, 100, 100)[0], srgb(lit_at(100, 100, [0.0, 50.0, 0.0], 2500.0)), 3.0, "point at 50 units");
    close(px(&img, 60, 130)[0], srgb(lit_at(60, 130, [0.0, 50.0, 0.0], 2500.0)), 3.0, "point off center");
    r.set_lights(&[point([0.0, 0.0, 0.0], 10000.0, Some(quadratic()))], [0.0; 3]);
    let far = shot(&mut r, 1, &view, "point_far");
    close(px(&far, 100, 100)[0], srgb(albedo * 10000.0 / 10000.0), 3.0, "point at 100 units");
    r.set_lights(&[point([0.0, -100.0, 0.0], 10000.0, Some(quadratic()))], [0.0; 3]);
    let farther = shot(&mut r, 1, &view, "point_farther");
    close(px(&farther, 100, 100)[0], srgb(albedo * 0.25), 3.0, "point at 200 units");
    r.set_lights(&[point([0.0, 150.0, 0.0], 2500.0, Some(quadratic()))], [0.0; 3]);
    let behind = shot(&mut r, 1, &view, "point_behind");
    assert!(px(&behind, 100, 100)[0] <= 2, "lit from behind: {:?}", px(&behind, 100, 100));
    r.set_lights(&[], [0.2; 3]);
    let ambient = shot(&mut r, 1, &view, "ambient");
    close(px(&ambient, 100, 100)[0], srgb(albedo * 0.2), 3.0, "ambient");

    let spot = PreviewLight {
        kind: LightKind::Spot { dir: [0.0, 1.0, 0.0], inner_deg: 10.0, outer_deg: 30.0 },
        pos: [0.0; 3],
        color: [1.0; 3],
        intensity: 10000.0,
        range: 16384.0,
        falloff: Some(LightFalloff { exponent: 2.0, ..quadratic() }),
    };
    r.set_lights(&[spot], [0.0; 3]);
    let img = shot(&mut r, 1, &view, "spot");
    let (ci, co) = (10f32.to_radians().cos(), 30f32.to_radians().cos());
    for x in [100u32, 112, 125, 135, 150, 170] {
        let p = on_plane(x, 100);
        let d = dist(p, [0.0; 3]);
        let s = p[1] / d;
        let cone = if s >= ci {
            1.0
        } else if s <= co {
            0.0
        } else {
            ((s - co) / (ci - co)).powi(2)
        };
        let want = albedo * 10000.0 / (d * d) * s * cone * s;
        close(px(&img, x, 100)[0], srgb(want), 3.0, &format!("spot at pixel {x}"));
    }
    r.set_lights(&[PreviewLight { falloff: None, intensity: 1.0, ..spot }], [0.0; 3]);
    let smooth = shot(&mut r, 1, &view, "spot_smooth");
    assert!(px(&smooth, 100, 100)[0] > 100 && px(&smooth, 190, 100)[0] <= 2, "smooth spot");
}

#[test]
fn sun_shadows_props_and_props_shadow_the_world() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, PROP, [200, 200, 200, 255], MaterialFlags::empty());
    mat(&mut r, WORLD, [200, 200, 200, 255], MaterialFlags::empty());
    mat(&mut r, SKY, [120, 160, 255, 255], MaterialFlags::SKY);
    r.upsert_object(1, box_mesh([-1024.0, -1024.0, -16.0], [1024.0, 1024.0, 0.0], WORLD));
    let cube = r.create_model(&cube_parts(64.0));
    r.set_model_instances(&[ModelInstance {
        model: cube,
        transform: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 96.0, 1.0]],
        object: MODEL_OBJ,
        tint: [1.0; 4],
        fade: [0.0; 2],
    }]);
    r.set_lights(
        &[PreviewLight {
            kind: LightKind::Directional { dir: [0.0, 0.0, -1.0] },
            pos: [0.0; 3],
            color: [1.0; 3],
            intensity: 0.8,
            range: 0.0,
            falloff: None,
        }],
        [0.05; 3],
    );
    let view = ViewParams {
        kind: ViewKind::Perspective {
            eye: [0.0, -700.0, 700.0],
            forward: [0.0, 1.0, -1.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 60.0,
            near: 4.0,
            far: 10000.0,
        },
        size: [256, 256],
        ..front_view()
    };
    let at = |p: [f32; 3]| {
        let s = view.world_to_screen(p).expect("visible");
        (s[0] as u32, s[1] as u32)
    };
    let (under, beside, top) = (at([0.0, 0.0, 0.0]), at([300.0, 0.0, 0.0]), at([0.0, 0.0, 128.0]));
    let img = shot(&mut r, 2, &view, "sun_prop_shadow");
    let (a, b) = (px(&img, under.0, under.1), px(&img, beside.0, beside.1));
    assert!(i32::from(b[0]) > i32::from(a[0]) + 80, "the prop shadows the floor: {a:?} vs {b:?}");
    let lit_top = px(&img, top.0, top.1);
    assert!(lit_top[0] > 150, "the top of the prop is in the sun: {lit_top:?}");
    r.set_object_flags(MODEL_OBJ, ObjectFlags::NO_SHADOW);
    let img = shot(&mut r, 2, &view, "sun_prop_no_shadow");
    let c = px(&img, under.0, under.1);
    assert!(i32::from(c[0]) > i32::from(a[0]) + 80, "no shadow with NO_SHADOW: {c:?}");
    r.set_object_flags(MODEL_OBJ, ObjectFlags::empty());
    mat(&mut r, PROP, [200, 200, 200, 255], MaterialFlags::ALPHA_TEST);
    let img = shot(&mut r, 2, &view, "sun_alpha_tested_opaque");
    let d = px(&img, under.0, under.1);
    assert!(i32::from(b[0]) > i32::from(d[0]) + 80, "opaque texels shadow: {d:?}");
    mat(&mut r, PROP, [200, 200, 200, 0], MaterialFlags::ALPHA_TEST);
    let img = shot(&mut r, 2, &view, "sun_alpha_tested_clear");
    let e = px(&img, under.0, under.1);
    assert!(i32::from(e[0]) > i32::from(a[0]) + 80, "clear texels let the sun in: {e:?}");
    mat(&mut r, PROP, [200, 200, 200, 255], MaterialFlags::empty());
    r.upsert_object(2, box_mesh([-100.0, -100.0, 400.0], [100.0, 100.0, 416.0], WORLD));
    let img = shot(&mut r, 2, &view, "sun_world_shadows_prop");
    let shaded_top = px(&img, top.0, top.1);
    assert!(i32::from(lit_top[0]) > i32::from(shaded_top[0]) + 80, "the world shadows the prop: {shaded_top:?}");
    r.upsert_object(2, box_mesh([-100.0, -100.0, 400.0], [100.0, 100.0, 416.0], SKY));
    let img = shot(&mut r, 2, &view, "sun_through_sky");
    let sky_top = px(&img, top.0, top.1);
    assert!((i32::from(sky_top[0]) - i32::from(lit_top[0])).abs() <= 3, "sky faces cast no shadow: {sky_top:?}");
}

#[test]
fn baked_preview_lights_models_like_the_game() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, PROP, [200, 200, 200, 255], MaterialFlags::empty());
    let model = square_model(&mut r);
    let mut left = facing_eye(model, MODEL_OBJ);
    left.transform[1] = [0.5, 0.0, 0.0, 0.0];
    left.transform[3][0] = -50.0;
    let mut right = facing_eye(model, MODEL_OBJ + 1);
    right.transform[1] = [0.5, 0.0, 0.0, 0.0];
    right.transform[3][0] = 50.0;
    r.set_model_instances(&[left, right]);
    let albedo = linear(200);
    r.set_lights(&[point([0.0, 0.0, 0.0], 10000.0, Some(quadratic()))], [0.0; 3]);
    let mut ambient = [[0.0f32; 3]; 6];
    ambient[3] = [0.3; 3];
    r.set_model_lighting(&[ModelLighting { object: MODEL_OBJ, ambient, ..Default::default() }]);
    let view = front_view();
    let (lx, rx) = (50u32, 150u32);
    let want_light = |x: u32| {
        let p = on_plane(x, 100);
        let d = dist(p, [0.0; 3]);
        albedo * 10000.0 / (d * d) * (p[1] / d)
    };
    let lit = shot(&mut r, 3, &view, "lit");
    close(px(&lit, lx, 100)[0], srgb(want_light(lx)), 3.0, "lit left");
    close(px(&lit, rx, 100)[0], srgb(want_light(rx)), 3.0, "lit right");
    r.set_lightmap_display(LightmapDisplay::Baked, 1.0);
    let baked = shot(&mut r, 3, &view, "baked");
    close(px(&baked, lx, 100)[0], srgb(albedo * 0.3), 3.0, "baked ambient cube");
    close(px(&baked, rx, 100)[0], srgb(want_light(rx)), 3.0, "no record: preview light");
    r.set_lightmap_display(LightmapDisplay::Baked, 2.0);
    let bright = shot(&mut r, 3, &view, "baked_bright");
    close(px(&bright, lx, 100)[0], srgb(albedo * 0.6), 3.0, "baked brightness");
    let light = ModelLocalLight {
        kind: 0,
        origin: [-100.0, 0.0, 0.0],
        color: [5000.0; 3],
        attenuation: [0.0, 0.0, 1.0],
        ..Default::default()
    };
    r.set_lightmap_display(LightmapDisplay::Baked, 1.0);
    r.set_model_lighting(&[ModelLighting { object: MODEL_OBJ, ambient, lights: vec![light], ..Default::default() }]);
    let local = shot(&mut r, 3, &view, "baked_local_light");
    assert!(f32::from(px(&local, lx, 100)[0]) > srgb(albedo * 0.3) + 20.0, "the bake's world light adds up");
    r.set_lightmap_display(LightmapDisplay::Off, 1.0);
    let back = shot(&mut r, 3, &view, "lit_again");
    close(px(&back, lx, 100)[0], srgb(want_light(lx)), 3.0, "lit again");
}

#[test]
fn self_illumination_glows_in_the_lit_views() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, PROP, [200, 200, 200, 128], MaterialFlags::SELF_ILLUM | MaterialFlags::OPAQUE_TEXTURE);
    let model = square_model(&mut r);
    r.set_model_instances(&[facing_eye(model, MODEL_OBJ)]);
    let albedo = linear(200);
    let glow = 128.0 / 255.0;
    r.set_lights(&[], [0.0; 3]);
    let view = front_view();
    let dark = shot(&mut r, 4, &view, "selfillum_dark");
    close(px(&dark, 100, 100)[0], srgb(albedo * glow), 3.0, "glow in the dark");
    r.set_lights(&[point([0.0, 0.0, 0.0], 10000.0, Some(quadratic()))], [0.0; 3]);
    let lit = shot(&mut r, 4, &view, "selfillum_lit");
    close(px(&lit, 100, 100)[0], srgb(albedo), 3.0, "lit glow");
    let mut ambient = [[0.0f32; 3]; 6];
    ambient[3] = [0.2; 3];
    r.set_model_lighting(&[ModelLighting { object: MODEL_OBJ, ambient, ..Default::default() }]);
    r.set_lightmap_display(LightmapDisplay::Baked, 1.0);
    let baked = shot(&mut r, 4, &view, "selfillum_baked");
    close(px(&baked, 100, 100)[0], srgb(albedo * (0.2 * (1.0 - glow) + glow)), 3.0, "baked glow");
    r.set_lightmap_display(LightmapDisplay::Off, 1.0);
    let textured = shot(&mut r, 4, &ViewParams { mode: RenderMode::Textured, ..view }, "selfillum_textured");
    mat(&mut r, PROP, [200, 200, 200, 128], MaterialFlags::OPAQUE_TEXTURE);
    let plain = shot(&mut r, 4, &ViewParams { mode: RenderMode::Textured, ..view }, "selfillum_textured_plain");
    assert_eq!(px(&textured, 100, 100), px(&plain, 100, 100));
}

#[test]
fn self_illumination_mask_and_tint() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    let base = TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Srgb,
        levels: vec![[200, 200, 200, 0].repeat(16)],
    };
    let base = r.create_texture(&base);
    let mask = TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Linear,
        levels: vec![[255, 128, 0, 255].repeat(16)],
    };
    let mask = r.create_texture(&mask);
    let desc = |s: Option<SelfIllumDesc>| MaterialDesc {
        texture: Some(base),
        tex_size: [4.0, 4.0],
        flags: MaterialFlags::SELF_ILLUM | MaterialFlags::OPAQUE_TEXTURE,
        self_illum: s,
        ..Default::default()
    };
    r.set_material(PROP, &desc(Some(SelfIllumDesc { tint: [1.0, 0.5, 1.0], mask: Some(mask), mask_alpha: false })));
    let model = square_model(&mut r);
    r.set_model_instances(&[facing_eye(model, MODEL_OBJ)]);
    r.set_lights(&[], [0.0; 3]);
    let view = front_view();
    let albedo = linear(200);
    let img = shot(&mut r, 5, &view, "selfillum_mask");
    let c = px(&img, 100, 100);
    close(c[0], srgb(albedo), 3.0, "red: full mask");
    close(c[1], srgb(albedo * 0.5 * (128.0 / 255.0)), 3.0, "green: half mask, half tint");
    assert!(c[2] <= 2, "blue: no mask: {c:?}");
    r.set_material(PROP, &desc(Some(SelfIllumDesc { tint: [1.0; 3], mask: Some(mask), mask_alpha: true })));
    let img = shot(&mut r, 5, &view, "selfillum_mask_alpha");
    close(px(&img, 100, 100)[2], srgb(albedo), 3.0, "mask alpha");
    r.set_material(PROP, &desc(None));
    let img = shot(&mut r, 5, &view, "selfillum_base_alpha");
    assert!(px(&img, 100, 100)[0] <= 2);
}

#[test]
fn point_and_spot_lights_cast_shadows() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, PROP, [200, 200, 200, 255], MaterialFlags::empty());
    mat(&mut r, WORLD, [200, 200, 200, 255], MaterialFlags::empty());
    r.upsert_object(1, box_mesh([-1024.0, -1024.0, -16.0], [1024.0, 1024.0, 0.0], WORLD));
    let cube = r.create_model(&cube_parts(64.0));
    r.set_model_instances(&[ModelInstance {
        model: cube,
        transform: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 96.0, 1.0]],
        object: MODEL_OBJ,
        tint: [1.0; 4],
        fade: [0.0; 2],
    }]);
    let light_at = [0.0, 0.0, 300.0];
    r.set_lights(&[point(light_at, 300.0 * 300.0, Some(quadratic()))], [0.02; 3]);
    let view = ViewParams {
        kind: ViewKind::Perspective {
            eye: [0.0, -700.0, 700.0],
            forward: [0.0, 1.0, -1.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 60.0,
            near: 4.0,
            far: 10000.0,
        },
        size: [256, 256],
        ..front_view()
    };
    let s = view.world_to_screen([0.0, 0.0, 0.0]).expect("visible");
    let under = (s[0] as u32, s[1] as u32);
    let s = view.world_to_screen([0.0, 0.0, 128.0]).expect("visible");
    let top = (s[0] as u32, s[1] as u32);
    let opts = |n: u32| ViewOptions { light_shadows: n, ..ViewOptions::default() };
    r.set_view_options(6, opts(0));
    let off = shot(&mut r, 6, &view, "local_shadow_off");
    let lit = px(&off, under.0, under.1)[0];
    assert!(lit > 150, "lit without shadows: {lit}");
    assert_eq!(r.stats().shadowed_lights, 0);
    r.set_view_options(6, opts(4));
    let first = shot(&mut r, 6, &view, "local_shadow_point");
    let draws_first = r.stats().draw_calls;
    assert_eq!(r.stats().shadowed_lights, 1);
    let shaded = px(&first, under.0, under.1)[0];
    assert!(i32::from(lit) > i32::from(shaded) + 80, "the prop shadows the floor: {shaded} vs {lit}");
    let lit_top = px(&first, top.0, top.1)[0];
    assert!(lit_top > 150, "the top of the prop is lit: {lit_top}");
    let again = shot(&mut r, 6, &view, "local_shadow_cached");
    assert!(r.stats().draw_calls < draws_first, "{} vs {draws_first}", r.stats().draw_calls);
    assert_eq!(px(&again, under.0, under.1), px(&first, under.0, under.1));
    r.set_object_flags(MODEL_OBJ, ObjectFlags::NO_SHADOW);
    let img = shot(&mut r, 6, &view, "local_shadow_no_shadow");
    assert!(i32::from(px(&img, under.0, under.1)[0]) > i32::from(shaded) + 80, "NO_SHADOW casts none");
    r.set_object_flags(MODEL_OBJ, ObjectFlags::empty());
    mat(&mut r, PROP, [200, 200, 200, 0], MaterialFlags::ALPHA_TEST);
    let img = shot(&mut r, 6, &view, "local_shadow_alpha_clear");
    assert!(i32::from(px(&img, under.0, under.1)[0]) > i32::from(shaded) + 80, "clear texels");
    mat(&mut r, PROP, [200, 200, 200, 255], MaterialFlags::ALPHA_TEST);
    let img = shot(&mut r, 6, &view, "local_shadow_alpha_opaque");
    assert!(i32::from(lit) > i32::from(px(&img, under.0, under.1)[0]) + 80, "opaque texels");
    mat(&mut r, PROP, [200, 200, 200, 255], MaterialFlags::empty());
    r.set_lights(
        &[PreviewLight {
            kind: LightKind::Spot { dir: [0.0, 0.0, -1.0], inner_deg: 50.0, outer_deg: 60.0 },
            ..point(light_at, 300.0 * 300.0, Some(quadratic()))
        }],
        [0.02; 3],
    );
    let img = shot(&mut r, 6, &view, "local_shadow_spot");
    assert_eq!(r.stats().shadowed_lights, 1);
    assert!(i32::from(lit) > i32::from(px(&img, under.0, under.1)[0]) + 80, "the spot is shadowed");
    r.upsert_object(2, box_mesh([-50.0, -50.0, 200.0], [50.0, 50.0, 216.0], WORLD));
    let img = shot(&mut r, 6, &view, "local_shadow_world");
    assert!(i32::from(lit_top) > i32::from(px(&img, top.0, top.1)[0]) + 80, "the world shadows the prop");
    r.set_view_options(6, opts(0));
    let _ = shot(&mut r, 6, &view, "local_shadow_off_again");
    assert_eq!(r.stats().shadowed_lights, 0);
}

fn box_mesh(min: [f32; 3], max: [f32; 3], material: MaterialId) -> ObjectMesh {
    let c = |i: usize| -> [f32; 3] {
        [
            if i & 1 != 0 { max[0] } else { min[0] },
            if i & 2 != 0 { max[1] } else { min[1] },
            if i & 4 != 0 { max[2] } else { min[2] },
        ]
    };
    let faces: [([usize; 4], [f32; 3]); 6] = [
        ([4, 5, 7, 6], [0.0, 0.0, 1.0]),
        ([0, 2, 3, 1], [0.0, 0.0, -1.0]),
        ([1, 3, 7, 5], [1.0, 0.0, 0.0]),
        ([0, 4, 6, 2], [-1.0, 0.0, 0.0]),
        ([2, 6, 7, 3], [0.0, 1.0, 0.0]),
        ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
    ];
    let mut batches = Vec::new();
    for (corners, n) in faces {
        let vertices = corners.iter().map(|&i| Vertex { pos: c(i), normal: n, uv: [0.0, 0.0], alpha: 1.0 }).collect();
        batches.push(MeshBatch {
            material,
            vertices,
            indices: vec![0, 1, 2, 0, 2, 3],
            blend: Vec::new(),
            cubemaps: Vec::new(),
        });
    }
    ObjectMesh { batches, aabb_min: min, aabb_max: max, wire_color: [255; 3], ..Default::default() }
}

fn cube_parts(s: f32) -> Vec<ModelPart> {
    let h = s * 0.5;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let mut n = [0.0f32; 3];
            n[axis] = sign;
            let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
            let (a, b) = if sign > 0.0 { (a, b) } else { (b, a) };
            let base = vertices.len() as u32;
            for (du, dv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let mut p = [0.0f32; 3];
                p[axis] = sign * h;
                p[a] = du * h;
                p[b] = dv * h;
                vertices.push(Vertex { pos: p, normal: n, uv: [0.0, 0.0], alpha: 1.0 });
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    vec![ModelPart { material: PROP, vertices, indices, tangents: Vec::new() }]
}

fn sky_brush(min: [f32; 3], max: [f32; 3], back: MaterialId) -> ObjectMesh {
    let mut m = box_mesh(min, max, back);
    m.batches[1].material = SKY;
    m
}

fn down_sun(intensity: f32) -> PreviewLight {
    PreviewLight {
        kind: LightKind::Directional { dir: [0.0, 0.0, -1.0] },
        pos: [0.0; 3],
        color: [1.0; 3],
        intensity,
        range: 0.0,
        falloff: None,
    }
}

fn looking_down(eye: [f32; 3]) -> ViewParams {
    ViewParams {
        kind: ViewKind::Perspective {
            eye,
            forward: [0.0, 0.0001, -1.0],
            up: [0.0, 1.0, 0.0],
            fov_y_deg: 90.0,
            near: 4.0,
            far: 20000.0,
        },
        size: [256, 256],
        ..front_view()
    }
}

fn screen(v: &ViewParams, p: [f32; 3]) -> (u32, u32) {
    let s = v.world_to_screen(p).expect("visible");
    (s[0] as u32, s[1] as u32)
}

#[test]
fn a_sealed_room_above_the_sky_does_not_shadow_the_map_below() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, WORLD, [200, 200, 200, 255], MaterialFlags::empty());
    mat(&mut r, SKY, [120, 160, 255, 255], MaterialFlags::SKY);
    r.upsert_object(1, box_mesh([-1024.0, -1024.0, -16.0], [1024.0, 1024.0, 0.0], WORLD));
    r.upsert_object(2, sky_brush([-1024.0, -1024.0, 1000.0], [1024.0, 1024.0, 1016.0], WORLD));
    r.upsert_object(3, box_mesh([-400.0, -400.0, 1500.0], [400.0, 400.0, 1516.0], WORLD));
    r.upsert_object(4, sky_brush([-400.0, -400.0, 2000.0], [400.0, 400.0, 2016.0], WORLD));
    r.upsert_object(5, box_mesh([120.0, -50.0, 300.0], [220.0, 50.0, 332.0], WORLD));
    r.upsert_object(6, box_mesh([100.0, -50.0, 1700.0], [200.0, 50.0, 1732.0], WORLD));
    r.set_object_flags(2, ObjectFlags::NO_SHADOW);
    r.set_object_flags(4, ObjectFlags::NO_SHADOW);
    r.set_lights(&[down_sun(0.8)], [0.05; 3]);
    let below = looking_down([0.0, 0.0, 900.0]);
    let img = shot(&mut r, 7, &below, "stacked_sky_below");
    let open_at = screen(&below, [-600.0, 0.0, 0.0]);
    let under_at = screen(&below, [-150.0, 0.0, 0.0]);
    let shaded_at = screen(&below, [170.0, 0.0, 0.0]);
    let open = px(&img, open_at.0, open_at.1);
    let under_room = px(&img, under_at.0, under_at.1);
    let shaded = px(&img, shaded_at.0, shaded_at.1);
    assert!(open[0] > 150, "the open floor is in the sun: {open:?}");
    assert!(
        (i32::from(under_room[0]) - i32::from(open[0])).abs() <= 6,
        "the room above the sky casts nothing: {under_room:?} vs {open:?}"
    );
    assert!(i32::from(open[0]) > i32::from(shaded[0]) + 80, "what lies under the sky still casts: {shaded:?}");
    let inside = looking_down([0.0, 0.0, 1900.0]);
    let img = shot(&mut r, 8, &inside, "stacked_sky_room");
    let room_open_at = screen(&inside, [-150.0, 0.0, 1516.0]);
    let room_shaded_at = screen(&inside, [150.0, 0.0, 1516.0]);
    let room_open = px(&img, room_open_at.0, room_open_at.1);
    let room_shaded = px(&img, room_shaded_at.0, room_shaded_at.1);
    assert!(room_open[0] > 150, "the room under its own sky is in the sun: {room_open:?}");
    assert!(i32::from(room_open[0]) > i32::from(room_shaded[0]) + 80, "the room keeps its shadows: {room_shaded:?}");
    r.upsert_object(7, box_mesh([-700.0, -100.0, 500.0], [-500.0, 100.0, 532.0], WORLD));
    r.set_object_flags(7, ObjectFlags::SKY_3D);
    r.upsert_object(8, sky_brush([-700.0, -100.0, 700.0], [-500.0, 100.0, 716.0], WORLD));
    r.set_object_flags(8, ObjectFlags::SKY_3D | ObjectFlags::NO_SHADOW);
    let img = shot(&mut r, 7, &below, "stacked_sky_room_3d");
    let sky_room = px(&img, open_at.0, open_at.1);
    assert!((i32::from(sky_room[0]) - i32::from(open[0])).abs() <= 6, "the 3D skybox room casts nothing: {sky_room:?}");
    r.remove_object(7);
    r.remove_object(8);
    r.upsert_object(4, box_mesh([-400.0, -400.0, 2000.0], [400.0, 400.0, 2016.0], WORLD));
    let img = shot(&mut r, 7, &below, "stacked_sky_roofed");
    let roofed = px(&img, under_at.0, under_at.1);
    assert!(
        (i32::from(roofed[0]) - i32::from(under_room[0])).abs() <= 6,
        "with one sky left the map below is unchanged: {roofed:?}"
    );
    let img = shot(&mut r, 8, &inside, "stacked_sky_room_roofed");
    let dark = px(&img, room_open_at.0, room_open_at.1);
    assert!(i32::from(room_open[0]) > i32::from(dark[0]) + 80, "a room without sky gets no sun: {dark:?}");
}

fn sky_shell(r: &mut Renderer, first: u32, half: f32, low: f32, high: f32, t: f32) {
    let boxes = [
        ([-half - t, -half - t, high], [half + t, half + t, high + t]),
        ([-half - t, -half - t, low - t], [half + t, half + t, low]),
        ([-half - t, -half - t, low], [-half, half + t, high]),
        ([half, -half - t, low], [half + t, half + t, high]),
        ([-half, -half - t, low], [half, -half, high]),
        ([-half, half, low], [half, half + t, high]),
    ];
    for (k, (min, max)) in boxes.into_iter().enumerate() {
        let id = first + k as u32;
        r.upsert_object(id, box_mesh(min, max, SKY));
        r.set_object_flags(id, ObjectFlags::NO_SHADOW);
    }
}

fn slanted_sun(intensity: f32) -> PreviewLight {
    let d = glam::Vec3::new(0.45, -0.3, -0.84).normalize().to_array();
    PreviewLight { kind: LightKind::Directional { dir: d }, ..down_sun(intensity) }
}

#[test]
fn a_map_sealed_in_sky_on_every_side_gets_the_sun() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, WORLD, [200, 200, 200, 255], MaterialFlags::empty());
    mat(&mut r, SKY, [120, 160, 255, 255], MaterialFlags::SKY);
    r.upsert_object(1, box_mesh([-1500.0, -1500.0, -64.0], [1500.0, 1500.0, 0.0], WORLD));
    r.upsert_object(2, box_mesh([-100.0, -100.0, 0.0], [100.0, 100.0, 400.0], WORLD));
    sky_shell(&mut r, 10, 2048.0, -512.0, 2048.0, 64.0);
    r.set_lights(&[slanted_sun(0.8)], [0.05; 3]);
    let v = looking_down([0.0, 0.0, 1500.0]);
    let img = shot(&mut r, 7, &v, "sealed_sky_sun");
    let open_at = screen(&v, [-700.0, 500.0, 0.0]);
    let open = px(&img, open_at.0, open_at.1);
    assert!(open[0] > 150, "the ground of a map sealed in sky is in the sun: {open:?}");
    let shade_at = screen(&v, [200.0, -140.0, 0.0]);
    let shade = px(&img, shade_at.0, shade_at.1);
    assert!(i32::from(open[0]) > i32::from(shade[0]) + 80, "the pillar still casts a shadow: {shade:?}");
    sky_shell(&mut r, 20, 2048.0, -4000.0, -1500.0, 64.0);
    sky_shell(&mut r, 30, 2048.0, -7000.0, -5000.0, 64.0);
    let img = shot(&mut r, 7, &v, "sealed_sky_sun_stacked");
    let stacked = px(&img, open_at.0, open_at.1);
    assert!(stacked[0] > 150, "more sky shells below the map do not put it in the dark: {stacked:?}");
    let stacked_shade = px(&img, shade_at.0, shade_at.1);
    assert!(i32::from(stacked[0]) > i32::from(stacked_shade[0]) + 80, "the pillar still casts: {stacked_shade:?}");
}

#[test]
fn sky_light_under_a_room_above_the_sky_is_open_sky() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, WORLD, [200, 200, 200, 255], MaterialFlags::empty());
    mat(&mut r, SKY, [120, 160, 255, 255], MaterialFlags::SKY);
    r.upsert_object(1, box_mesh([-2048.0, -2048.0, -16.0], [2048.0, 2048.0, 0.0], WORLD));
    r.upsert_object(2, sky_brush([-2048.0, -2048.0, 1000.0], [2048.0, 2048.0, 1016.0], WORLD));
    r.upsert_object(3, box_mesh([-600.0, -600.0, 1500.0], [600.0, 600.0, 1516.0], WORLD));
    r.upsert_object(4, sky_brush([-600.0, -600.0, 2000.0], [600.0, 600.0, 2016.0], WORLD));
    r.set_lights(&[down_sun(0.0)], [0.5; 3]);
    r.set_sun(SunParams { spread_deg: 0.0, sky_ambient: true });
    let below = looking_down([0.0, 0.0, 900.0]);
    let img = shot(&mut r, 9, &below, "stacked_sky_ambient");
    let under_at = screen(&below, [0.0, 0.0, 0.0]);
    let open_at = screen(&below, [-800.0, 0.0, 0.0]);
    let (under, open) = (px(&img, under_at.0, under_at.1), px(&img, open_at.0, open_at.1));
    assert!(open[0] > 100, "the open floor sees the sky: {open:?}");
    assert!((i32::from(under[0]) - i32::from(open[0])).abs() <= 8, "under the room: {under:?} vs {open:?}");
    r.upsert_object(4, box_mesh([-600.0, -600.0, 2000.0], [600.0, 600.0, 2016.0], WORLD));
    let img = shot(&mut r, 9, &below, "stacked_sky_ambient_one_sky");
    let again = px(&img, under_at.0, under_at.1);
    assert!((i32::from(again[0]) - i32::from(under[0])).abs() <= 8, "{again:?} vs {under:?}");
}

fn yard_sky_fraction(half: f32, h: f32) -> f32 {
    let n = 400;
    let step = 2.0 * half / n as f32;
    let mut sum = 0.0f64;
    for i in 0..n {
        for j in 0..n {
            let x = -half + (i as f32 + 0.5) * step;
            let y = -half + (j as f32 + 0.5) * step;
            let r2 = f64::from(x * x + y * y + h * h);
            sum += f64::from(step * step) / (r2 * r2);
        }
    }
    (f64::from(h * h) / std::f64::consts::PI * sum) as f32
}

fn yard_wall_sky_fraction(half: f32, height: f32, z: f32) -> f32 {
    let (nt, np) = (360, 720);
    let mut sum = 0.0f64;
    for i in 0..nt {
        let theta = (i as f32 + 0.5) / nt as f32 * std::f32::consts::FRAC_PI_2;
        for j in 0..np {
            let phi = (j as f32 + 0.5) / np as f32 * std::f32::consts::TAU;
            let d = [theta.cos() * 0.0 + theta.sin() * phi.cos(), theta.sin() * phi.sin(), theta.cos()];
            let n_dot = d[0];
            if n_dot <= 0.0 || d[2] <= 0.0 {
                continue;
            }
            let t = (height - z) / d[2];
            let (x, y) = (-half + d[0] * t, d[1] * t);
            if x.abs() < half && y.abs() < half {
                sum += f64::from(
                    n_dot
                        * theta.sin()
                        * (std::f32::consts::FRAC_PI_2 / nt as f32)
                        * (std::f32::consts::TAU / np as f32),
                );
            }
        }
    }
    (sum / std::f64::consts::PI) as f32
}

fn yard(r: &mut Renderer, half: f32, thick: f32, height: f32) {
    mat(r, WORLD, [200, 200, 200, 255], MaterialFlags::empty());
    mat(r, SKY, [120, 160, 255, 255], MaterialFlags::SKY);
    r.upsert_object(1, box_mesh([-4096.0, -4096.0, -16.0], [4096.0, 4096.0, 0.0], WORLD));
    r.upsert_object(2, sky_brush([-4096.0, -4096.0, 3000.0], [4096.0, 4096.0, 3016.0], WORLD));
    r.set_object_flags(2, ObjectFlags::NO_SHADOW);
    let o = half + thick;
    mat(r, WALLS, [200, 200, 200, 255], MaterialFlags::empty());
    r.upsert_object(3, box_mesh([-o, -o, 0.0], [-half, o, height], WALLS));
    r.upsert_object(4, box_mesh([half, -o, 0.0], [o, o, height], WALLS));
    r.upsert_object(5, box_mesh([-half, -o, 0.0], [half, -half, height], WALLS));
    r.upsert_object(6, box_mesh([-half, half, 0.0], [half, o, height], WALLS));
    r.set_lights(&[down_sun(0.0)], [0.5; 3]);
    r.set_sun(SunParams { spread_deg: 0.0, sky_ambient: true });
}

#[test]
fn sky_light_is_shadowed_by_the_thin_walls_around_a_yard() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    yard(&mut r, 256.0, 32.0, 256.0);
    let view = looking_down([0.0, 0.0, 1400.0]);
    let img = shot(&mut r, 11, &view, "yard_sky");
    let (c, o) = (screen(&view, [0.0, 0.0, 0.0]), screen(&view, [-1200.0, 0.0, 0.0]));
    let centre = linear(px(&img, c.0, c.1)[0]);
    let open = linear(px(&img, o.0, o.1)[0]);
    let want = yard_sky_fraction(256.0, 252.0);
    let got = centre / open;
    eprintln!("YARD got {got:.3} want {want:.3}");
    assert!((got - want).abs() < 0.12, "the yard floor sees {got:.3} of the sky, {want:.3} expected");
    assert!(got < 0.7, "the thin walls shade the floor: {got:.3}");
    let side = ViewParams {
        kind: ViewKind::Perspective {
            eye: [200.0, 0.0, 120.0],
            forward: [-1.0, 0.0001, 0.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 90.0,
            near: 4.0,
            far: 20000.0,
        },
        size: [256, 256],
        ..front_view()
    };
    let img = shot(&mut r, 12, &side, "yard_sky_wall");
    let w = screen(&side, [-256.0, 0.0, 100.0]);
    let wall = linear(px(&img, w.0, w.1)[0]) / open;
    let want_wall = yard_wall_sky_fraction(256.0, 256.0, 106.0);
    eprintln!("YARD wall got {wall:.3} want {want_wall:.3}");
    assert!((wall - want_wall).abs() < 0.08, "the wall sees {wall:.3} of the sky, {want_wall:.3} expected");
    mat(&mut r, WALLS, [200, 200, 200, 255], MaterialFlags::NODRAW_IN_3D | MaterialFlags::LIGHT_BLOCKER);
    let img = shot(&mut r, 11, &view, "yard_sky_hidden_walls");
    let hidden = linear(px(&img, c.0, c.1)[0]) / linear(px(&img, o.0, o.1)[0]);
    assert!((hidden - got).abs() < 0.05, "hidden nodraw walls still shade: {hidden:.3} vs {got:.3}");
    mat(&mut r, WALLS, [200, 200, 200, 255], MaterialFlags::NODRAW_IN_3D);
    let img = shot(&mut r, 11, &view, "yard_sky_hidden_clip");
    let clip = linear(px(&img, c.0, c.1)[0]) / linear(px(&img, o.0, o.1)[0]);
    assert!(clip > 0.9, "hidden clip brushes shade nothing: {clip:.3}");
}

#[test]
fn an_indirect_light_volume_lights_the_surfaces_it_covers() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, WORLD, [255, 255, 255, 255], MaterialFlags::empty());
    r.upsert_object(1, box_mesh([-1024.0, -1024.0, -16.0], [1024.0, 1024.0, 0.0], WORLD));
    r.set_lights(&[], [0.0; 3]);
    let on = ViewOptions { indirect_light: true, ..ViewOptions::default() };
    r.set_view_options(13, on);
    let view = looking_down([0.0, 0.0, 900.0]);
    let img = shot(&mut r, 13, &view, "indirect_none");
    let (c, hole, outside) =
        (screen(&view, [-100.0, -100.0, 0.0]), screen(&view, [200.0, 200.0, 0.0]), screen(&view, [600.0, 0.0, 0.0]));
    assert!(px(&img, c.0, c.1)[0] < 4, "no volume, no ambient: black");
    let mut coords = Vec::new();
    let mut cubes = Vec::new();
    for y in 0..4 {
        for x in 0..4 {
            if (x, y) == (3, 3) {
                continue;
            }
            let mut cube = [[0.0f32; 3]; 6];
            cube[4] = [0.5, 0.25, 0.1];
            cube[5] = [0.0, 0.0, 1.0];
            coords.push([x - 2, y - 2, 0]);
            cubes.push(cube);
        }
    }
    let volume = IndirectVolume::new([0.0, 0.0, 0.0], 128.0, &coords, &cubes).expect("volume");
    r.set_indirect_volume(Some(std::sync::Arc::new(volume)));
    assert_eq!(r.indirect_volume().map(|v| v.cells()), Some(15));
    let img = shot(&mut r, 13, &view, "indirect_on");
    let lit = px(&img, c.0, c.1);
    close(lit[0], srgb(0.5), 6.0, "the +z face of the cells lights the floor");
    close(lit[1], srgb(0.25), 6.0, "green");
    close(lit[2], srgb(0.1), 6.0, "blue");
    assert!(
        px(&img, hole.0, hole.1)[0] < 4,
        "a cell without a probe falls back to the plain ambient: {:?}",
        px(&img, hole.0, hole.1)
    );
    assert!(px(&img, outside.0, outside.1)[0] < 4, "nothing outside the volume");
    r.set_view_options(13, ViewOptions { indirect_light: false, ..ViewOptions::default() });
    let img = shot(&mut r, 13, &view, "indirect_off");
    assert!(px(&img, c.0, c.1)[0] < 4, "the view option turns it off");
    r.set_view_options(13, on);
    r.set_indirect_volume(None);
    let img = shot(&mut r, 13, &view, "indirect_removed");
    assert!(px(&img, c.0, c.1)[0] < 4);
}

#[test]
fn hull_shadow_flag_casts_the_shadow_hull() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, PROP, [200, 200, 200, 255], MaterialFlags::empty());
    mat(&mut r, WORLD, [200, 200, 200, 255], MaterialFlags::empty());
    r.upsert_object(1, box_mesh([-1024.0, -1024.0, -16.0], [1024.0, 1024.0, 0.0], WORLD));
    let cube = r.create_model(&cube_parts(64.0));
    let place = |x: f32, object: ObjectId| ModelInstance {
        model: cube,
        transform: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [x, 0.0, 96.0, 1.0]],
        object,
        tint: [1.0; 4],
        fade: [0.0; 2],
    };
    r.set_model_instances(&[place(0.0, MODEL_OBJ), place(-300.0, MODEL_OBJ + 1)]);
    let slanted = PreviewLight {
        kind: LightKind::Directional { dir: [std::f32::consts::FRAC_1_SQRT_2, 0.0, -std::f32::consts::FRAC_1_SQRT_2] },
        ..down_sun(0.8)
    };
    r.set_lights(&[slanted], [0.05; 3]);
    let view = looking_down([0.0, 0.0, 900.0]);
    let under = screen(&view, [96.0, 0.0, 0.0]);
    let aside = screen(&view, [96.0, 120.0, 0.0]);
    let lit_at = screen(&view, [96.0, -150.0, 0.0]);
    let other = screen(&view, [-204.0, 0.0, 0.0]);
    let img = shot(&mut r, 10, &view, "hull_mesh");
    let mesh_under = px(&img, under.0, under.1)[0];
    let lit = px(&img, lit_at.0, lit_at.1)[0];
    assert!((i32::from(px(&img, aside.0, aside.1)[0]) - i32::from(lit)).abs() <= 6, "nothing casts there yet");
    assert!(i32::from(lit) > i32::from(mesh_under) + 80, "the mesh casts: {mesh_under} vs {lit}");
    r.set_object_flags(MODEL_OBJ, ObjectFlags::HULL_SHADOW);
    let img = shot(&mut r, 10, &view, "hull_missing");
    assert!(i32::from(lit) > i32::from(px(&img, under.0, under.1)[0]) + 80, "without a hull the mesh still casts");
    let p = |x: f32, y: f32| [x, y, 0.0];
    let hull: Vec<[[f32; 3]; 3]> =
        vec![[p(-20.0, 100.0), p(20.0, 100.0), p(20.0, 140.0)], [p(-20.0, 100.0), p(20.0, 140.0), p(-20.0, 140.0)]];
    r.set_model_shadow_hull(cube, &hull);
    assert!(r.has_model_shadow_hull(cube));
    let img = shot(&mut r, 10, &view, "hull_shadow");
    let (u, a, o) = (px(&img, under.0, under.1)[0], px(&img, aside.0, aside.1)[0], px(&img, other.0, other.1)[0]);
    assert!(i32::from(u) > i32::from(mesh_under) + 80, "the mesh no longer casts: {u}");
    assert!(i32::from(lit) > i32::from(a) + 80, "the hull casts where it lies: {a}");
    assert!(i32::from(lit) > i32::from(o) + 80, "the other prop keeps its mesh shadow: {o}");
    r.set_object_flags(MODEL_OBJ, ObjectFlags::HULL_SHADOW | ObjectFlags::NO_SHADOW);
    let img = shot(&mut r, 10, &view, "hull_no_shadow");
    assert!(i32::from(px(&img, aside.0, aside.1)[0]) + 6 >= i32::from(lit), "NO_SHADOW wins");
    r.set_object_flags(MODEL_OBJ, ObjectFlags::empty());
    let img = shot(&mut r, 10, &view, "hull_back_to_mesh");
    assert!(i32::from(lit) > i32::from(px(&img, under.0, under.1)[0]) + 80, "the mesh casts again");
}
