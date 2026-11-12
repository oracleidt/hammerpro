use hp_render::*;
use std::sync::OnceLock;

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    static GPU: OnceLock<Option<(wgpu::Device, wgpu::Queue)>> = OnceLock::new();
    GPU.get_or_init(|| {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
        pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() }),
        )
        .ok()
    })
    .clone()
}

fn view(eye: [f32; 3], forward: [f32; 3]) -> ViewParams {
    ViewParams {
        size: [128; 2],
        game_pixel_grid: true,
        kind: ViewKind::Perspective { eye, forward, up: [0.0, 0.0, 1.0], fov_y_deg: 90.0, near: 1.0, far: 10000.0 },
        mode: RenderMode::Textured,
        grid: None,
        background: [0.0; 3],
        selection_color: [0.0; 4],
        show_models: false,
        show_sprites: true,
        msaa: false,
    }
}

const BEAM: WorldBeam = WorldBeam {
    object: 10,
    material: 1,
    start: [-100.0, 0.0, 0.0],
    end: [100.0, 0.0, 0.0],
    width: 10.0,
    color: [1.0, 0.5, 0.25],
    srgb_color: false,
    hdr_scale: 1.0,
    halo_scale: 0.0,
};

fn setup(r: &mut Renderer) {
    r.set_tone_settings(ToneSettings { mode: ToneMode::Off, ..Default::default() });
    r.set_material(1, &MaterialDesc { texture: Some(TexId::WHITE), flags: MaterialFlags::UNLIT, ..Default::default() });
}

fn shot(r: &mut Renderer, v: &ViewParams) -> Vec<u8> {
    r.render_view(119, v, &Overlay::default());
    r.read_view_rgba8(119).unwrap().2
}

fn red(p: &[u8], x: usize, y: usize) -> u8 {
    p[(y * 128 + x) * 4]
}

#[test]
fn the_beam_is_a_camera_facing_strip_that_shades_out_toward_its_end() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue);
    setup(&mut r);
    r.set_world_beams(&[BEAM]);
    let side = view([0.0, -100.0, 0.0], [0.0, 1.0, 0.0]);
    let p = shot(&mut r, &side);
    let (near_start, middle, near_end) = (red(&p, 16, 64), red(&p, 64, 64), red(&p, 112, 64));
    assert!(
        near_start > middle && middle > near_end,
        "brightness falls along the beam: {near_start} {middle} {near_end}"
    );
    assert!(red(&p, 64, 64 - 3) > 100 && red(&p, 64, 64 + 3) > 100, "the strip has SpotlightWidth on each side");
    assert_eq!(red(&p, 64, 64 - 12), 0, "nothing past the width");
    assert!(p[(64 * 128 + 64) * 4 + 1] < middle, "the beam keeps its color");
    let above = view([0.0, -60.0, 80.0], [0.0, 0.6, -0.8]);
    let q = shot(&mut r, &above);
    assert!(red(&q, 64, 64) > 100, "seen from above the strip still faces the camera");
    r.set_object_flags(10, ObjectFlags::HIDDEN);
    assert!(shot(&mut r, &side).chunks_exact(4).all(|p| p[..3] == [0; 3]), "hidden entity, no beam");
    r.set_object_flags(10, ObjectFlags::default());
    r.set_world_beams(&[]);
    assert!(shot(&mut r, &side).chunks_exact(4).all(|p| p[..3] == [0; 3]));
}

#[test]
fn the_beam_fades_when_seen_along_its_axis_and_the_halo_shows_from_the_front() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue);
    setup(&mut r);
    r.set_world_beams(&[BEAM]);
    let off_axis = shot(&mut r, &view([150.0, -45.0, 0.0], [-1.0, 0.0, 0.0]));
    let near_axis = shot(&mut r, &view([150.0, -15.0, 0.0], [-1.0, 0.0, 0.0]));
    let peak = |p: &[u8]| p.chunks_exact(4).map(|c| c[0]).max().unwrap_or(0);
    assert!(peak(&off_axis) > 200, "4.5 widths from the axis: full beam {}", peak(&off_axis));
    assert!(
        peak(&near_axis) < peak(&off_axis) / 2,
        "1.5 widths from the axis the beam is dimmed: {}",
        peak(&near_axis)
    );
    let on_axis = view([150.0, 0.0, 0.0], [-1.0, 0.0, 0.0]);
    assert!(shot(&mut r, &on_axis).chunks_exact(4).all(|p| p[..3] == [0; 3]), "on the axis the beam vanishes");

    let halo = WorldBeam { halo_scale: 20.0, ..BEAM };
    r.set_world_beams(&[halo]);
    let front = shot(&mut r, &view([0.0, 0.0, 0.0], [-1.0, 0.0, 0.0]));
    assert!(
        red(&front, 64, 64) > 200,
        "looking down the beam at the spotlight shows the halo: {}",
        red(&front, 64, 64)
    );
    let behind = shot(&mut r, &view([-200.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
    assert!(behind.chunks_exact(4).all(|p| p[..3] == [0; 3]), "no halo from behind the spotlight");
    let side = shot(&mut r, &view([-100.0, -100.0, 0.0], [0.0, 1.0, 0.0]));
    assert!(side.chunks_exact(4).all(|p| p[..3] == [0; 3]), "no halo from the side (fade with the angle)");
    let oblique = shot(
        &mut r,
        &view([0.0, -100.0, 0.0], [-std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2, 0.0]),
    );
    let v = red(&oblique, 64, 64);
    assert!(v > 100, "45 degrees off the axis the halo is full: {v}");
}

fn wall(x: f32, half: f32) -> ObjectMesh {
    ObjectMesh {
        batches: vec![MeshBatch {
            material: 2,
            vertices: [[x, -half, -half], [x, half, -half], [x, half, half], [x, -half, half]]
                .map(|pos| Vertex { pos, normal: [1.0, 0.0, 0.0], uv: [0.0; 2], alpha: 1.0 })
                .to_vec(),
            indices: vec![0, 1, 2, 0, 2, 3],
            blend: Vec::new(),
            cubemaps: Vec::new(),
        }],
        aabb_min: [x, -half, -half],
        aabb_max: [x, half, half],
        ..Default::default()
    }
}

#[test]
fn the_halo_hides_behind_the_world_like_the_engine_pixel_visibility() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue);
    setup(&mut r);
    r.set_material(
        2,
        &MaterialDesc {
            texture: Some(TexId::WHITE),
            flags: MaterialFlags::UNLIT | MaterialFlags::TWO_SIDED,
            tint: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        },
    );
    let halo = WorldBeam { halo_scale: 20.0, ..BEAM };
    r.set_world_beams(&[halo]);
    let front = view([0.0, 0.0, 0.0], [-1.0, 0.0, 0.0]);
    r.upsert_object(20, wall(-104.0, 200.0));
    assert!(red(&shot(&mut r, &front), 64, 64) > 200, "a wall right behind the spotlight leaves the halo");
    r.upsert_object(20, wall(-50.0, 4.0));
    let hidden = shot(&mut r, &front);
    assert!(red(&hidden, 64, 20) < 10, "something between the eye and the spotlight hides the whole halo");
}
