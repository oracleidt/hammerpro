use std::sync::OnceLock;

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
            label: Some("hp-render display colors test"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()
    })
    .clone()
}

type Img = (u32, u32, Vec<u8>);

fn pixel(img: &Img, x: u32, y: u32) -> [u8; 3] {
    let o = ((y * img.0 + x) * 4) as usize;
    [img.2[o], img.2[o + 1], img.2[o + 2]]
}

fn square(min: [f32; 2], max: [f32; 2], wire: [u8; 3]) -> ObjectMesh {
    let c = [[min[0], min[1], 0.0], [max[0], min[1], 0.0], [max[0], max[1], 0.0], [min[0], max[1], 0.0]];
    let edges = (0..4).flat_map(|i| [c[i], c[(i + 1) % 4]]).collect();
    ObjectMesh {
        edges,
        aabb_min: [min[0], min[1], 0.0],
        aabb_max: [max[0], max[1], 0.0],
        wire_color: wire,
        ..Default::default()
    }
}

fn top(background: [f32; 3], grid: bool) -> ViewParams {
    ViewParams {
        size: [101, 101],
        kind: ViewKind::Ortho { axis: OrthoAxis::Top, center: [0.5, 0.5, 0.0], pixels_per_unit: 1.0 },
        mode: RenderMode::Wireframe,
        grid: grid.then_some(GridParams { size: 16.0, major_every: 8, show_1024: false, opacity: 1.0 }),
        background,
        selection_color: [1.0, 0.1, 0.1, 0.45],
        show_sprites: true,
        show_models: true,
        msaa: false,
    }
}

fn luma(p: [u8; 3]) -> f32 {
    0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])
}

const DARK: [f32; 3] = [20.0 / 255.0, 20.0 / 255.0, 22.0 / 255.0];
const LIGHT: [f32; 3] = [236.0 / 255.0, 236.0 / 255.0, 232.0 / 255.0];

#[test]
fn views_pick_object_or_classic_colors_and_stay_readable_on_light() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    r.upsert_object(1, square([-30.0, -30.0], [30.0, 30.0], [40, 165, 225]));
    r.set_object_wire_colors(1, [40, 165, 225], [255, 255, 255]);
    let edge = (20, 50);
    let shot = |r: &mut Renderer, options: ViewOptions, bg: [f32; 3]| {
        r.set_view_options(1, options);
        r.render_view(1, &top(bg, false), &Overlay::default());
        let img = r.read_view_rgba8(1).expect("readback");
        let bg = pixel(&img, 50, 50);
        let far = |p: [u8; 3]| (0..3).map(|i| (i32::from(p[i]) - i32::from(bg[i])).abs()).sum::<i32>();
        (edge.0 - 5..edge.0 + 6).map(|x| pixel(&img, x, edge.1)).max_by_key(|p| far(*p)).unwrap_or(bg)
    };
    let object = shot(&mut r, ViewOptions::default(), DARK);
    assert!(object[2] > 180 && object[0] < 90, "object color on dark: {object:?}");
    let classic = shot(&mut r, ViewOptions { classic_colors: true, ..Default::default() }, DARK);
    assert!(classic.iter().all(|c| *c > 230), "classic white on dark: {classic:?}");
    let on_light = shot(&mut r, ViewOptions::default(), LIGHT);
    assert!(on_light[2] > on_light[0] + 40, "keeps its hue on light: {on_light:?}");
    assert!(luma(on_light) < 150.0, "dark enough to read on light: {on_light:?}");
    let white_on_light = shot(&mut r, ViewOptions { classic_colors: true, ..Default::default() }, LIGHT);
    assert!(luma(white_on_light) < 150.0, "white brushes stay visible on light: {white_on_light:?}");
}

#[test]
fn the_grid_is_lighter_on_light_backgrounds() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    let line = |r: &mut Renderer, bg: [f32; 3]| {
        r.render_view(1, &top(bg, true), &Overlay::default());
        let img = r.read_view_rgba8(1).expect("readback");
        let bg_luma = luma(pixel(&img, 50, 3));
        let darkest = (0..40).map(|x| luma(pixel(&img, x, 3))).fold(255.0f32, f32::min);
        let brightest = (0..40).map(|x| luma(pixel(&img, x, 3))).fold(0.0f32, f32::max);
        (bg_luma, darkest, brightest)
    };
    let (_, darkest, _) = line(&mut r, LIGHT);
    assert!(darkest > 120.0, "light grid lines stay soft: {darkest}");
    let (_, _, brightest) = line(&mut r, DARK);
    assert!(brightest > 50.0 && brightest < 140.0, "dark grid unchanged: {brightest}");
}
