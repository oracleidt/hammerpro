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
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("hp-render lightmap test"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()
    })
    .clone()
}

const FLOOR: ObjectId = 1;
const MAT: MaterialId = 7;

fn floor_mesh() -> ObjectMesh {
    let corners = [[-256.0, -256.0, 0.0], [256.0, -256.0, 0.0], [256.0, 256.0, 0.0], [-256.0, 256.0, 0.0]];
    let vertices = corners
        .iter()
        .map(|p| Vertex { pos: *p, normal: [0.0, 0.0, 1.0], uv: [p[0] / 4.0, p[1] / 4.0], alpha: 0.0 })
        .collect();
    ObjectMesh {
        batches: vec![MeshBatch {
            material: MAT,
            vertices,
            indices: vec![0, 1, 2, 0, 2, 3],
            blend: Vec::new(),
            cubemaps: Vec::new(),
        }],
        edges: Vec::new(),
        aabb_min: [-256.0, -256.0, 0.0],
        aabb_max: [256.0, 256.0, 0.0],
        wire_color: [200, 200, 200],
        ..Default::default()
    }
}

fn floor_lm(face: u32) -> Vec<LightmapVertex> {
    let corners = [[-256.0f32, -256.0, 0.0], [256.0, -256.0, 0.0], [256.0, 256.0, 0.0], [-256.0, 256.0, 0.0]];
    corners
        .iter()
        .map(|p| LightmapVertex {
            pos: *p,
            uv: [p[0] / 4.0, p[1] / 4.0],
            lm: [(p[0] + 256.0) / 16.0, (p[1] + 256.0) / 16.0],
            alpha: 0.0,
            face,
            blend: 0,
        })
        .collect()
}

fn params(mode: RenderMode) -> ViewParams {
    ViewParams {
        game_pixel_grid: false,
        size: [128, 128],
        kind: ViewKind::Perspective {
            eye: [0.0, 0.0, 400.0],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            fov_y_deg: 80.0,
            near: 4.0,
            far: 10000.0,
        },
        mode,
        grid: None,
        background: [0.0, 0.0, 0.0],
        selection_color: [1.0, 0.1, 0.1, 0.45],
        show_sprites: false,
        show_models: true,
        msaa: false,
    }
}

fn pixel(r: &Renderer, x: u32, y: u32) -> [u8; 4] {
    let (w, _, px) = r.read_view_rgba8(0).expect("readback");
    let o = ((y * w + x) * 4) as usize;
    [px[o], px[o + 1], px[o + 2], px[o + 3]]
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
fn baked_faces_draw_over_the_world() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    assert!(r.pass_hook_names().contains(&"lightmap"));
    r.set_material(MAT, &MaterialDesc { texture: None, tint: [1.0; 4], ..Default::default() });
    r.set_lightmap_tracking(true);
    r.upsert_object(FLOOR, floor_mesh());
    assert_eq!(r.take_lightmap_changes(), vec![FLOOR]);
    let generation = r.lightmap_generation(FLOOR);
    assert!(generation > 0);

    r.render_view(0, &params(RenderMode::Textured), &Overlay::default());
    let plain = pixel(&r, 32, 64);
    assert!(plain[0] > 100, "{plain:?}");

    r.add_lightmap_faces(&[(0, LightmapFace { object: FLOOR, generation, scale: 16.0, flags: 0 })]);
    let verts = floor_lm(0);
    r.add_lightmap_block(1, &[LightmapBatch { material: MAT, vertices: &verts, indices: &[0, 1, 2, 0, 2, 3] }]);
    assert!(r.ensure_lightmap_layers(64, 1));
    let (w, h) = (33u32, 33u32);
    let texels: Vec<u32> =
        (0..w * h).map(|i| if i % w < 16 { texel(1.0, 0.0, 0.0) } else { texel(0.0, 0.0, 0.25) }).collect();
    r.write_lightmap(0, LightmapRect { layer: 0, x: 3, y: 5, w, h, density: 1.0, bumped: false }, &texels);
    r.set_lightmap_display(LightmapDisplay::Baked, 1.0);
    let s = r.lightmap_stats();
    assert_eq!((s.faces, s.valid_faces, s.baked_faces, s.blocks), (1, 1, 1, 1));

    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    let left = pixel(&r, 32, 64);
    let right = pixel(&r, 96, 64);
    assert!(left[0] > 200 && left[1] < 40 && left[2] < 40, "left {left:?}");
    assert!(right[0] < 40 && right[2] > 60 && right[2] < 200, "right {right:?}");

    let green: Vec<u8> = (0..32 * 32 * 32).flat_map(|_| [0u8, 255, 0, 255]).collect();
    r.set_color_correction(Some(&green));
    assert!(r.has_color_correction());
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    let cc = pixel(&r, 32, 64);
    assert!(cc[1] > 200 && cc[0] < 40, "color corrected {cc:?}");
    r.set_color_correction(Some(&green[..10]));
    assert!(!r.has_color_correction(), "wrong sizes are ignored");
    r.set_color_correction(None);
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    assert_eq!(pixel(&r, 32, 64), left);

    r.upsert_object(FLOOR, floor_mesh());
    assert_eq!(r.lightmap_stats().valid_faces, 0);
    r.render_view(0, &params(RenderMode::Textured), &Overlay::default());
    assert_eq!(pixel(&r, 32, 64), plain);
    r.revalidate_lightmap_object(FLOOR, generation);
    assert_eq!(r.lightmap_stats().valid_faces, 0);
    r.revalidate_lightmap_object(FLOOR, r.lightmap_generation(FLOOR));
    assert_eq!(r.lightmap_stats().valid_faces, 1);
    r.render_view(0, &params(RenderMode::Textured), &Overlay::default());
    assert!(pixel(&r, 32, 64)[0] > 200);

    r.set_object_flags(FLOOR, ObjectFlags::HIDDEN);
    r.render_view(0, &params(RenderMode::Textured), &Overlay::default());
    assert_eq!(pixel(&r, 32, 64), [0, 0, 0, 255]);
    r.set_object_flags(FLOOR, ObjectFlags::empty());

    r.set_lightmap_display(LightmapDisplay::Grid, 1.0);
    r.render_view(0, &params(RenderMode::Flat), &Overlay::default());
    let mut colors = std::collections::HashSet::new();
    for x in (20..108).step_by(3) {
        colors.insert(pixel(&r, x, 64));
    }
    assert!(colors.len() > 2, "grid lines expected: {colors:?}");

    r.set_lightmap_display(LightmapDisplay::Off, 1.0);
    r.render_view(0, &params(RenderMode::Textured), &Overlay::default());
    assert_eq!(pixel(&r, 32, 64), plain);

    r.set_lightmap_display(LightmapDisplay::Baked, 1.0);
    assert!(r.ensure_lightmap_layers(64, 3));
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    assert!(pixel(&r, 32, 64)[0] > 200);
    r.remove_lightmap_faces(&[0]);
    r.remove_lightmap_block(1);
    r.clear_lightmaps();
    let s = r.lightmap_stats();
    assert_eq!((s.faces, s.blocks), (0, 0));
    r.write_lightmap(9, LightmapRect { layer: 99, x: 0, y: 0, w: 1, h: 1, density: f32::NAN, bumped: true }, &[0]);
    r.add_lightmap_block(2, &[LightmapBatch { material: MAT, vertices: &verts, indices: &[0, 1, 99] }]);
    assert!(!r.ensure_lightmap_layers(64, 0));
}

#[test]
fn animated_bake_baseline_captured_during_dark_phase_does_not_add_the_light_twice() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue);
    r.set_material(MAT, &MaterialDesc { tint: [1.0; 4], ..Default::default() });
    r.upsert_object(FLOOR, floor_mesh());
    r.add_lightmap_faces(&[(
        0,
        LightmapFace { object: FLOOR, generation: r.lightmap_generation(FLOOR), scale: 16.0, flags: 0 },
    )]);
    let vertices = floor_lm(0);
    r.add_lightmap_block(1, &[LightmapBatch { material: MAT, vertices: &vertices, indices: &[0, 1, 2, 0, 2, 3] }]);
    assert!(r.ensure_lightmap_layers(64, 1));
    r.write_lightmap(
        0,
        LightmapRect { layer: 0, x: 0, y: 0, w: 33, h: 33, density: 1.0, bumped: false },
        &vec![texel(0.25, 0.25, 0.25); 33 * 33],
    );
    r.set_lightmap_display(LightmapDisplay::Baked, 1.0);
    let light = PreviewLight {
        kind: LightKind::Point,
        pos: [0.0, 0.0, 128.0],
        color: [1.0; 3],
        intensity: 20.0,
        range: 400.0,
        falloff: None,
    };
    let patterns = ["az".to_owned()];
    r.set_animated_lights(&[light], &patterns, [0.0; 3], 0.0);
    r.capture_baked_lights();
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    let reference = pixel(&r, 64, 64);
    for time in [0.1, 0.2, 0.3, 0.4] {
        r.set_animated_lights(&[light], &patterns, [0.0; 3], time);
        r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
        assert_eq!(pixel(&r, 64, 64), reference, "baked direct delta changed during style phase {time}");
    }
}

#[test]
fn baked_faces_survive_display_remeshing_with_tracking_disabled() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    r.set_material(MAT, &MaterialDesc { texture: None, tint: [1.0; 4], ..Default::default() });
    r.upsert_object(FLOOR, floor_mesh());
    r.add_lightmap_faces(&[(
        0,
        LightmapFace { object: FLOOR, generation: r.lightmap_generation(FLOOR), scale: 16.0, flags: 0 },
    )]);
    let vertices = floor_lm(0);
    r.add_lightmap_block(1, &[LightmapBatch { material: MAT, vertices: &vertices, indices: &[0, 1, 2, 0, 2, 3] }]);
    assert!(r.ensure_lightmap_layers(64, 1));
    r.write_lightmap(
        0,
        LightmapRect { layer: 0, x: 0, y: 0, w: 33, h: 33, density: 1.0, bumped: false },
        &[texel(1.0, 0.0, 0.0); 33 * 33],
    );
    r.set_lightmap_display(LightmapDisplay::Baked, 1.0);
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    let before = r.read_view_rgba8(0).expect("baked image");
    assert!(pixel(&r, 32, 64)[0] > 200);

    r.set_lightmap_display(LightmapDisplay::Off, 1.0);
    r.set_lightmap_tracking(false);
    let mut remeshed = floor_mesh();
    remeshed.wire_color = [17, 18, 19];
    r.upsert_object(FLOOR, remeshed);
    assert_eq!(r.lightmap_stats().valid_faces, 0);
    r.set_lightmap_tracking(false);
    assert_eq!(r.take_lightmap_changes(), vec![FLOOR]);
    let generation = r.lightmap_generation(FLOOR);
    r.revalidate_lightmap_objects(&[(FLOOR, generation)]);
    assert_eq!(r.lightmap_stats().valid_faces, 1);
    r.set_lightmap_display(LightmapDisplay::Baked, 1.0);
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    assert_eq!(r.read_view_rgba8(0).expect("restored image"), before);

    let mut changed = floor_mesh();
    for batch in &mut changed.batches {
        for vertex in &mut batch.vertices {
            vertex.pos[2] += 64.0;
        }
    }
    changed.aabb_min[2] += 64.0;
    changed.aabb_max[2] += 64.0;
    r.upsert_object(FLOOR, changed);
    assert_eq!(r.take_lightmap_changes(), vec![FLOOR]);
    r.revalidate_lightmap_objects(&[(FLOOR, generation)]);
    assert_eq!(r.lightmap_stats().valid_faces, 0);
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    assert_ne!(r.read_view_rgba8(0).expect("changed geometry"), before);

    r.remove_lightmap_faces(&[0]);
    r.revalidate_lightmap_objects(&[(FLOOR, r.lightmap_generation(FLOOR))]);
    assert_eq!(r.lightmap_stats().valid_faces, 0);
    r.add_lightmap_faces(&[(
        1,
        LightmapFace { object: FLOOR, generation: r.lightmap_generation(FLOOR), scale: 16.0, flags: 0 },
    )]);
    r.upsert_object(FLOOR, floor_mesh());
    r.revalidate_lightmap_objects(&[(FLOOR, r.lightmap_generation(FLOOR))]);
    let stats = r.lightmap_stats();
    assert_eq!((stats.faces, stats.valid_faces, stats.baked_faces), (1, 0, 0));
}

#[test]
fn the_baked_view_draws_the_lightmaps_without_real_time_light() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    r.set_material(MAT, &MaterialDesc { texture: None, tint: [1.0; 4], ..Default::default() });
    r.upsert_object(FLOOR, floor_mesh());
    let sun = PreviewLight {
        kind: LightKind::Directional { dir: [0.2, 0.1, -1.0] },
        pos: [0.0; 3],
        color: [1.0, 1.0, 1.0],
        intensity: 1.0,
        range: 0.0,
        falloff: None,
    };
    let lamp = PreviewLight { kind: LightKind::Point, pos: [0.0, 0.0, 100.0], range: 2000.0, ..sun };
    r.set_lights(&[sun, lamp], [0.1; 3]);
    r.set_sun(SunParams { spread_deg: 0.0, sky_ambient: true });
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    let lit_draws = r.stats().draw_calls;
    r.render_view(0, &params(RenderMode::Textured), &Overlay::default());
    let textured_draws = r.stats().draw_calls;
    assert!(lit_draws > textured_draws, "the Lit mode draws its shadow maps: {lit_draws} vs {textured_draws}");

    r.add_lightmap_faces(&[(
        0,
        LightmapFace { object: FLOOR, generation: r.lightmap_generation(FLOOR), scale: 16.0, flags: 0 },
    )]);
    let verts = floor_lm(0);
    r.add_lightmap_block(1, &[LightmapBatch { material: MAT, vertices: &verts, indices: &[0, 1, 2, 0, 2, 3] }]);
    assert!(r.ensure_lightmap_layers(64, 1));
    let texels = vec![texel(0.0, 1.0, 0.0); 33 * 33];
    r.write_lightmap(0, LightmapRect { layer: 0, x: 0, y: 0, w: 33, h: 33, density: 1.0, bumped: false }, &texels);
    r.set_lightmap_display(LightmapDisplay::Baked, 1.0);
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    let baked_draws = r.stats().draw_calls;
    assert!(baked_draws <= textured_draws, "no shadow map, sky map or light work: {baked_draws} vs {textured_draws}");
    let p = pixel(&r, 64, 64);
    assert!(p[1] > 150 && p[0] < 60, "the lightmap shows {p:?}");

    r.upsert_object(FLOOR, floor_mesh());
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    assert!(r.stats().draw_calls <= textured_draws, "still no real time light work");
    let stale = pixel(&r, 64, 64);
    assert!(stale[0] > 60 && stale[0].abs_diff(stale[1]) < 30, "an edited face waits, lit by the sun alone: {stale:?}");
}

#[test]
fn local_light_delta_updates_baked_faces_without_replacing_the_atlas() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue);
    r.set_material(MAT, &MaterialDesc { texture: None, tint: [1.0; 4], ..Default::default() });
    r.set_lightmap_tracking(true);
    r.upsert_object(FLOOR, floor_mesh());
    r.add_lightmap_faces(&[(
        0,
        LightmapFace { object: FLOOR, generation: r.lightmap_generation(FLOOR), scale: 16.0, flags: 0 },
    )]);
    let vertices = floor_lm(0);
    r.add_lightmap_block(1, &[LightmapBatch { material: MAT, vertices: &vertices, indices: &[0, 1, 2, 0, 2, 3] }]);
    assert!(r.ensure_lightmap_layers(64, 1));
    r.write_lightmap(
        0,
        LightmapRect { layer: 0, x: 0, y: 0, w: 33, h: 33, density: 1.0, bumped: false },
        &vec![texel(0.2, 0.2, 0.2); 33 * 33],
    );
    let light = PreviewLight {
        kind: LightKind::Point,
        pos: [0.0, 0.0, 128.0],
        color: [1.0; 3],
        intensity: 0.2,
        range: 4096.0,
        falloff: Some(LightFalloff { constant: 1.0, ..Default::default() }),
    };
    r.set_lights(&[light], [0.0; 3]);
    let pending_baseline = r.snapshot_baked_lights();
    r.set_view_options(0, ViewOptions { light_shadows: 16, ..Default::default() });
    r.set_lightmap_display(LightmapDisplay::Baked, 1.0);
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    let original = pixel(&r, 64, 64);
    assert!(original[0] > 80 && original[0].abs_diff(original[1]) <= 1, "{original:?}");
    assert_eq!(r.stats().shadowed_lights, 0);
    r.set_lights(&[PreviewLight { color: [1.0, 0.0, 0.0], ..light }], [0.0; 3]);
    r.restore_baked_lights(&pending_baseline);
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    let red = pixel(&r, 64, 64);
    assert!(
        red[0] > red[1] + 60 && red[1] < 30 && red[2] < 30,
        "the edited local light must recolor the retained atlas: {original:?} -> {red:?}"
    );
    let reference = pixel(&r, 32, 64);
    let mut occluder = floor_mesh();
    for vertex in &mut occluder.batches[0].vertices {
        vertex.pos = [vertex.pos[0] / 8.0 - 80.0, vertex.pos[1] / 6.0, 64.0];
    }
    occluder.aabb_min = [-112.0, -43.0, 64.0];
    occluder.aabb_max = [-48.0, 43.0, 64.0];
    r.upsert_object(2, occluder);
    r.set_object_flags(2, ObjectFlags::NO_BAKED_SHADOW);
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    let under_dynamic_caster = pixel(&r, 32, 64);
    assert!(
        reference.iter().zip(under_dynamic_caster).all(|(a, b)| a.abs_diff(b) <= 2),
        "a caster absent from VRAD must not hide the subtraction: {reference:?} -> {under_dynamic_caster:?}"
    );
    r.remove_object(2);
    assert_eq!(r.lightmap_stats().baked_faces, 1);
    r.set_lights(&[], [0.0; 3]);
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    let removed = pixel(&r, 64, 64);
    assert!(removed[..3].iter().all(|v| *v < 30), "the removed direct contribution must be subtracted: {removed:?}");
    r.set_lights(&[light], [0.0; 3]);
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    assert_eq!(pixel(&r, 64, 64), original);
    r.clear_baked_lights();
    let mut crowded = vec![PreviewLight { intensity: 1.0, ..light }; 128];
    r.set_lights(&crowded, [0.0; 3]);
    r.capture_baked_lights();
    crowded.push(PreviewLight { color: [1.0, 0.0, 0.0], ..light });
    r.set_lights(&crowded, [0.0; 3]);
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    let added = pixel(&r, 64, 64);
    assert!(
        added[0] > original[0] + 20,
        "unchanged lights must not crowd out the new local contribution: {original:?} -> {added:?}"
    );
    assert!(r.stats().shadowed_lights <= 1, "only the changed light needs a dynamic shadow");
}

#[test]
fn retained_model_light_changes_test_visibility_at_the_lighting_origin() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue);
    r.set_material(MAT, &MaterialDesc { texture: None, tint: [1.0; 4], ..Default::default() });
    let face = floor_mesh().batches.remove(0);
    let model = r.create_model(&[ModelPart {
        material: MAT,
        vertices: face.vertices,
        indices: face.indices,
        tangents: Vec::new(),
    }]);
    r.set_model_instances(&[ModelInstance { model, object: 100, ..Default::default() }]);
    r.set_object_flags(100, ObjectFlags::NO_BAKED_SHADOW);
    let mut wall = floor_mesh();
    for vertex in &mut wall.batches[0].vertices {
        vertex.pos = [256.0, vertex.pos[1], vertex.pos[0]];
        vertex.normal = [-1.0, 0.0, 0.0];
    }
    wall.aabb_min = [256.0, -256.0, -256.0];
    wall.aabb_max = [256.0, 256.0, 256.0];
    r.upsert_object(2, wall);
    r.set_model_lighting(&[ModelLighting {
        object: 100,
        sample_origin: Some([512.0, 0.0, 0.0]),
        ..Default::default()
    }]);
    r.capture_baked_lights();
    r.set_lightmap_display(LightmapDisplay::Baked, 1.0);
    r.set_view_options(0, ViewOptions { light_shadows: 16, ..Default::default() });
    r.set_lights(
        &[PreviewLight {
            kind: LightKind::Point,
            pos: [0.0, 0.0, 128.0],
            color: [0.0, 1.0, 0.0],
            intensity: 0.4,
            range: 4096.0,
            falloff: Some(LightFalloff { constant: 1.0, ..Default::default() }),
        }],
        [0.0; 3],
    );
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    let blocked = pixel(&r, 64, 64);
    assert!(blocked[..3].iter().all(|v| *v < 10), "the lighting origin is behind the wall: {blocked:?}");
    assert_eq!(r.stats().shadowed_lights, 1);
    r.set_model_lighting(&[ModelLighting { object: 100, sample_origin: Some([0.0, 0.0, 1.0]), ..Default::default() }]);
    r.render_view(0, &params(RenderMode::Lit), &Overlay::default());
    let visible = pixel(&r, 64, 64);
    assert!(
        visible[1] > 80 && visible[0] < 10 && visible[2] < 10,
        "the moved lighting origin sees the light: {visible:?}"
    );
}
