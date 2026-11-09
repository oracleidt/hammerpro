use std::sync::OnceLock;

use hp_render::lightmap::{LightmapBatch, LightmapDisplay, LightmapFace, LightmapRect, LightmapVertex};
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
            label: Some("hp-render fog test"),
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

fn grab(r: &Renderer, name: &str) -> Img {
    let (w, h, px) = r.read_view_rgba8(1).expect("readback");
    if let Some(dir) = std::env::var_os("HP_TEST_SCRATCH") {
        let dir = std::path::PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        image::save_buffer(dir.join(format!("fog_{name}.png")), &px, w, h, image::ColorType::Rgba8).expect("png");
    }
    (w, h, px)
}

fn center(img: &Img) -> [i32; 3] {
    let o = (((img.1 / 2) * img.0 + img.0 / 2) * 4) as usize;
    [i32::from(img.2[o]), i32::from(img.2[o + 1]), i32::from(img.2[o + 2])]
}

fn near(c: [i32; 3], want: [i32; 3], tol: i32) -> bool {
    (0..3).all(|i| (c[i] - want[i]).abs() <= tol)
}

fn material(r: &mut Renderer, id: MaterialId, rgba: [u8; 4]) {
    let t = r.create_texture(&TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Srgb,
        levels: vec![rgba.repeat(16)],
    });
    r.set_material(id, &MaterialDesc { texture: Some(t), tex_size: [4.0, 4.0], ..Default::default() });
}

fn slab(min: [f32; 3], max: [f32; 3], material: MaterialId) -> ObjectMesh {
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
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (corners, n) in faces {
        let s = vertices.len() as u32;
        for i in corners {
            vertices.push(Vertex { pos: c(i), normal: n, uv: [0.0; 2], alpha: 1.0 });
        }
        indices.extend_from_slice(&[s, s + 1, s + 2, s, s + 2, s + 3]);
    }
    ObjectMesh {
        batches: vec![MeshBatch { material, vertices, indices, blend: Vec::new(), cubemaps: Vec::new() }],
        aabb_min: min,
        aabb_max: max,
        wire_color: [255; 3],
        ..Default::default()
    }
}

fn wall_at(r: &mut Renderer, x: f32) {
    r.upsert_object(1, slab([x, -8000.0, -8000.0], [x + 16.0, 8000.0, 8000.0], 1));
}

fn look(forward: [f32; 3], mode: RenderMode) -> ViewParams {
    ViewParams {
        game_pixel_grid: false,
        size: [64, 64],
        kind: ViewKind::Perspective {
            eye: [0.0; 3],
            forward,
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 70.0,
            near: 1.0,
            far: 30000.0,
        },
        mode,
        grid: None,
        background: [0.0, 0.0, 0.0],
        selection_color: [1.0, 0.1, 0.1, 0.45],
        show_sprites: true,
        show_models: true,
        msaa: false,
    }
}

const RED: Fog = Fog {
    color: [200.0 / 255.0, 0.0, 0.0],
    color2: [200.0 / 255.0, 0.0, 0.0],
    dir: [1.0, 0.0, 0.0],
    blend: false,
    start: 0.0,
    end: 1000.0,
    max_density: 1.0,
    far_z: 0.0,
};

#[test]
fn fog_and_sun_passes_preserve_the_scene_across_msaa_and_lighting_transitions() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    material(&mut r, 1, [180, 180, 180, 255]);
    wall_at(&mut r, 256.0);
    r.set_fog_volumes(&[FogVolume {
        min: [64.0, -1000.0, -1000.0],
        max: [200.0, 1000.0, 1000.0],
        fog: Some(RED),
        preview: RED,
        active: true,
    }]);
    let sprite = SunGlowSprite { material: 1, size: 8.0, color: [0.0; 3], modulate: false };
    r.set_sun_glow(Some(SunGlow { direction: [1.0, 0.0, 0.0], core: sprite, overlay: sprite, hdr_scale: 1.0 }));
    r.set_view_options(1, ViewOptions { fog_volumes: true, sun_glow: true, ..Default::default() });
    let mut reference = None;
    for msaa in [false, true] {
        for iteration in 0..8 {
            r.set_lightmap_display(if iteration % 2 == 0 { LightmapDisplay::Baked } else { LightmapDisplay::Off }, 1.0);
            let mut params = look([1.0, 0.0, 0.0], RenderMode::Textured);
            params.msaa = msaa;
            r.render_view(1, &params, &Overlay::default());
            let image = grab(&r, &format!("multipass_{msaa}_{iteration}"));
            let sample = center(&image);
            assert!(sample.iter().sum::<i32>() > 100, "scene vanished: {sample:?}, msaa {msaa}, frame {iteration}");
            if let Some(expected) = reference {
                assert!(near(sample, expected, 6), "scene changed: {sample:?} versus {expected:?}, msaa {msaa}");
            } else {
                reference = Some(sample);
            }
        }
    }
}

fn shot(r: &mut Renderer, fog: Fog, name: &str) -> [i32; 3] {
    r.set_environment(Environment { fog: Some(fog), ..Default::default() });
    r.render_view(1, &look([1.0, 0.0, 0.0], RenderMode::Unshaded), &Overlay::default());
    center(&grab(r, name))
}

#[test]
fn linear_fog_is_mixed_in_gamma_space_and_capped_by_max_density() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    material(&mut r, 1, [100, 100, 100, 255]);
    r.set_view_options(1, ViewOptions { show_fog: true, ..Default::default() });
    wall_at(&mut r, 500.0);
    let c = shot(&mut r, RED, "half");
    assert!(near(c, [150, 50, 50], 3), "half way: linear factor 0.5 in gamma space: {c:?}");
    let c = shot(&mut r, Fog { start: -1000.0, ..RED }, "negative_start");
    assert!(near(c, [175, 25, 25], 3), "a negative start is already foggy at the eye: {c:?}");
    wall_at(&mut r, 3000.0);
    let c = shot(&mut r, Fog { max_density: 0.25, ..RED }, "capped");
    assert!(near(c, [125, 75, 75], 3), "max density 0.25 caps the fog at a quarter: {c:?}");
    let c = shot(&mut r, RED, "full");
    assert!(near(c, [200, 0, 0], 3), "past the end only the fog color: {c:?}");
    r.set_view_options(1, ViewOptions::default());
    r.render_view(1, &look([1.0, 0.0, 0.0], RenderMode::Unshaded), &Overlay::default());
    let c = center(&grab(&r, "preview_off"));
    assert!(near(c, [100, 100, 100], 3), "the preview toggle turns it off: {c:?}");
}

#[test]
fn fog_follows_the_camera_distance_in_every_render_mode() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    material(&mut r, 1, [100, 100, 100, 255]);
    r.set_view_options(1, ViewOptions { show_fog: true, ..Default::default() });
    r.set_environment(Environment { fog: Some(Fog { start: 400.0, end: 800.0, ..RED }), ..Default::default() });
    for mode in [RenderMode::Textured, RenderMode::Lit, RenderMode::Unshaded, RenderMode::Flat] {
        wall_at(&mut r, 300.0);
        r.render_view(1, &look([1.0, 0.0, 0.0], mode), &Overlay::default());
        let before = center(&grab(&r, &format!("{mode:?}_before")));
        wall_at(&mut r, 2000.0);
        r.render_view(1, &look([1.0, 0.0, 0.0], mode), &Overlay::default());
        let after = center(&grab(&r, &format!("{mode:?}_after")));
        assert!(before[1] > 8 || before[0] < 180, "{mode:?}: no fog before the start: {before:?}");
        assert!(near(after, [200, 0, 0], 3), "{mode:?}: full fog past the end: {after:?}");
    }
}

#[test]
fn the_blend_color_follows_the_view_direction() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    material(&mut r, 1, [100, 100, 100, 255]);
    r.set_view_options(1, ViewOptions { show_fog: true, ..Default::default() });
    wall_at(&mut r, 3000.0);
    let blend = Fog { color: [1.0, 0.0, 0.0], color2: [0.0, 0.0, 1.0], blend: true, ..RED };
    let c = shot(&mut r, blend, "blend_primary");
    assert!(near(c, [255, 0, 0], 3), "looking along fogdir shows the primary color: {c:?}");
    let c = shot(&mut r, Fog { dir: [-1.0, 0.0, 0.0], ..blend }, "blend_secondary");
    assert!(near(c, [0, 0, 255], 3), "looking against fogdir shows the secondary color: {c:?}");
    let c = shot(&mut r, Fog { dir: [0.0, 1.0, 0.0], ..blend }, "blend_side");
    assert!(near(c, [128, 0, 128], 3), "side on: half of each, mixed in gamma: {c:?}");
}

#[test]
fn nothing_is_drawn_past_farz_and_the_sky_or_the_fog_color_shows() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    material(&mut r, 1, [100, 100, 100, 255]);
    r.set_view_options(1, ViewOptions { show_fog: true, show_sky: true, ..Default::default() });
    wall_at(&mut r, 500.0);
    let thin = Fog { max_density: 0.0, far_z: 300.0, ..RED };
    let c = shot(&mut r, thin, "farz_clear");
    assert!(near(c, [200, 0, 0], 3), "without a sky the background is the fog color: {c:?}");
    let green = r.create_texture(&TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Srgb,
        levels: vec![[0, 255, 0, 255].repeat(16)],
    });
    r.set_environment(Environment {
        sky: Some(SkyView::new([Some(green); 6], 0.0)),
        fog: Some(thin),
        ..Default::default()
    });
    r.render_view(1, &look([1.0, 0.0, 0.0], RenderMode::Unshaded), &Overlay::default());
    let c = center(&grab(&r, "farz_sky"));
    assert!(near(c, [0, 255, 0], 3), "past farz the 2D sky shows, never fogged: {c:?}");
    r.set_environment(Environment {
        sky: Some(SkyView::new([Some(green); 6], 0.0)),
        fog: Some(Fog { far_z: 0.0, ..RED }),
        ..Default::default()
    });
    r.upsert_object(1, slab([100_000.0, -1.0, -1.0], [100_001.0, 1.0, 1.0], 1));
    r.render_view(1, &look([1.0, 0.0, 0.0], RenderMode::Unshaded), &Overlay::default());
    let c = center(&grab(&r, "sky_unfogged"));
    assert!(near(c, [0, 255, 0], 3), "the 2D sky is not fogged: {c:?}");
}

#[test]
fn the_3d_skybox_uses_the_sky_camera_fog_in_skybox_units() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    material(&mut r, 1, [100, 100, 100, 255]);
    r.upsert_object(2, slab([40.0, 20_000.0 - 400.0, -400.0], [48.0, 20_000.0 + 400.0, 400.0], 1));
    r.set_object_flags(2, ObjectFlags::SKY_3D);
    let blue = Fog { color: [0.0, 0.0, 200.0 / 255.0], color2: [0.0, 0.0, 200.0 / 255.0], end: 80.0, ..RED };
    let sky_3d = Some(Sky3d { origin: [0.0, 20_000.0, 0.0], scale: 16.0 });
    let main = Fog { end: 10.0, ..RED };
    r.set_view_options(
        1,
        ViewOptions { show_fog: true, show_sky: true, show_sky_3d: true, sky_see_through: true, ..Default::default() },
    );
    r.set_environment(Environment { sky_3d, fog: Some(main), sky_fog: Some(blue), ..Default::default() });
    r.render_view(1, &look([1.0, 0.0, 0.0], RenderMode::Unshaded), &Overlay::default());
    let c = center(&grab(&r, "sky3d_fog"));
    assert!(near(c, [50, 50, 150], 3), "half the sky fog at 40 skybox units, none of the map fog: {c:?}");
    r.set_environment(Environment { sky_3d, fog: Some(main), sky_fog: None, ..Default::default() });
    r.render_view(1, &look([1.0, 0.0, 0.0], RenderMode::Unshaded), &Overlay::default());
    let c = center(&grab(&r, "sky3d_nofog"));
    assert!(near(c, [100, 100, 100], 3), "a sky_camera with fogenable off leaves the skybox clear: {c:?}");
}

fn texel(r: f32, g: f32, b: f32) -> u32 {
    let enc = |v: f32, m: u32| -> u32 {
        if v <= 0.0 {
            return 0;
        }
        let bits = v.to_bits();
        let e = ((bits >> 23) & 0xff) as i32 - 127 + 15;
        let mant = (bits & 0x7f_ffff) >> (23 - m);
        ((e.clamp(1, 30) as u32) << m) | mant
    };
    enc(r, 6) | (enc(g, 6) << 11) | (enc(b, 5) << 22)
}

#[test]
fn baked_faces_get_the_same_fog() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    r.set_material(1, &MaterialDesc { texture: None, tint: [1.0; 4], ..Default::default() });
    r.set_lightmap_tracking(true);
    let corners = [[-256.0f32, -256.0, 0.0], [256.0, -256.0, 0.0], [256.0, 256.0, 0.0], [-256.0, 256.0, 0.0]];
    r.upsert_object(
        7,
        ObjectMesh {
            batches: vec![MeshBatch {
                material: 1,
                vertices: corners
                    .iter()
                    .map(|p| Vertex { pos: *p, normal: [0.0, 0.0, 1.0], uv: [p[0] / 4.0, p[1] / 4.0], alpha: 0.0 })
                    .collect(),
                indices: vec![0, 1, 2, 0, 2, 3],
                blend: Vec::new(),
                cubemaps: Vec::new(),
            }],
            aabb_min: [-256.0, -256.0, 0.0],
            aabb_max: [256.0, 256.0, 0.0],
            wire_color: [200, 200, 200],
            ..Default::default()
        },
    );
    let generation = r.lightmap_generation(7);
    r.add_lightmap_faces(&[(0, LightmapFace { object: 7, generation, scale: 16.0, flags: 0 })]);
    let verts: Vec<LightmapVertex> = corners
        .iter()
        .map(|p| LightmapVertex {
            pos: *p,
            uv: [p[0] / 4.0, p[1] / 4.0],
            lm: [(p[0] + 256.0) / 16.0, (p[1] + 256.0) / 16.0],
            alpha: 0.0,
            face: 0,
            blend: 0,
        })
        .collect();
    r.add_lightmap_block(1, &[LightmapBatch { material: 1, vertices: &verts, indices: &[0, 1, 2, 0, 2, 3] }]);
    assert!(r.ensure_lightmap_layers(64, 1));
    let texels = vec![texel(0.25, 0.25, 0.25); 33 * 33];
    r.write_lightmap(0, LightmapRect { layer: 0, x: 3, y: 5, w: 33, h: 33, density: 1.0, bumped: false }, &texels);
    r.set_lightmap_display(LightmapDisplay::Baked, 1.0);
    let down = ViewParams {
        size: [64, 64],
        kind: ViewKind::Perspective {
            eye: [0.0, 0.0, 400.0],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            fov_y_deg: 80.0,
            near: 4.0,
            far: 10000.0,
        },
        ..look([1.0, 0.0, 0.0], RenderMode::Lit)
    };
    r.render_view(1, &down, &Overlay::default());
    let plain = center(&grab(&r, "baked_plain"));
    r.set_view_options(1, ViewOptions { show_fog: true, ..Default::default() });
    r.set_environment(Environment { fog: Some(Fog { end: 800.0, ..RED }), ..Default::default() });
    r.render_view(1, &down, &Overlay::default());
    let fogged = center(&grab(&r, "baked_fog"));
    let want = [(plain[0] + 200) / 2, plain[1] / 2, plain[2] / 2];
    assert!(near(fogged, want, 4), "baked faces: half fog at 400 units: {plain:?} -> {fogged:?}, want {want:?}");
}
