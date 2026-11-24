use crate::types::*;
use crate::{ObjectFlags, Renderer};

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
    if !adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VERTEX_STORAGE) {
        return None;
    }
    pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() }),
    )
    .ok()
}

const MAT: MaterialId = 7;

fn cube(min: [f32; 3], max: [f32; 3]) -> ObjectMesh {
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
    let batches = faces
        .iter()
        .enumerate()
        .map(|(fi, (corners, n))| MeshBatch {
            material: MAT,
            vertices: corners
                .iter()
                .map(|&i| Vertex { pos: c(i), normal: *n, uv: [c(i)[0], c(i)[1] + c(i)[2]], alpha: 1.0 })
                .collect(),
            indices: if fi.is_multiple_of(2) { vec![0, 1, 2, 0, 2, 3] } else { vec![0, 2, 1, 0, 3, 2] },
            blend: Vec::new(),
            cubemaps: Vec::new(),
        })
        .collect();
    let pairs = [(0, 1), (1, 3), (3, 2), (2, 0), (4, 5), (5, 7), (7, 6), (6, 4), (0, 4), (1, 5), (2, 6), (3, 7)];
    ObjectMesh {
        batches,
        edges: pairs.iter().flat_map(|&(a, b)| [c(a), c(b)]).collect(),
        aabb_min: min,
        aabb_max: max,
        wire_color: [200, 220, 255],
        ..Default::default()
    }
}

fn scene(r: &mut Renderer) {
    r.set_material(MAT, &MaterialDesc { tint: [0.8, 0.5, 0.3, 1.0], ..Default::default() });
    r.upsert_object(1, cube([-2000.0, -2000.0, -64.0], [2000.0, 2000.0, 0.0]));
    for i in 0..24u32 {
        let x = (i % 6) as f32 * 300.0 - 750.0;
        let y = (i / 6) as f32 * 300.0 + 200.0;
        r.upsert_object(10 + i, cube([x, y, 0.0], [x + 96.0, y + 96.0, 64.0 + i as f32 * 8.0]));
    }
}

fn persp(eye: [f32; 3]) -> ViewParams {
    ViewParams {
        game_pixel_grid: false,
        size: [200, 150],
        kind: ViewKind::Perspective {
            eye,
            forward: [0.0, 1.0, -0.3],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 80.0,
            near: 1.0,
            far: 20000.0,
        },
        mode: RenderMode::Textured,
        grid: None,
        background: [0.1, 0.1, 0.1],
        selection_color: [1.0, 0.1, 0.1, 0.45],
        show_sprites: true,
        show_models: true,
        msaa: false,
    }
}

fn top(center: [f32; 3], ppu: f32) -> ViewParams {
    ViewParams {
        kind: ViewKind::Ortho { axis: OrthoAxis::Top, center, pixels_per_unit: ppu },
        mode: RenderMode::Wireframe,
        grid: Some(GridParams::default()),
        background: [0.0; 3],
        ..persp([0.0; 3])
    }
}

fn pixels(r: &mut Renderer, id: u32, p: &ViewParams, overlay: &Overlay) -> Vec<u8> {
    r.render_view(id, p, overlay);
    r.read_view_rgba8(id).expect("readback").2
}

fn uncached(r: &mut Renderer, id: u32, p: &ViewParams, overlay: &Overlay) -> Vec<u8> {
    r.frame_caching = false;
    let px = pixels(r, id, p, overlay);
    r.frame_caching = true;
    px
}

fn list_serial(r: &Renderer, id: u32) -> u64 {
    r.views.get(&id).and_then(|v| v.frame.as_ref()).map_or(0, |f| f.serial)
}

fn bundle_serial(r: &Renderer, id: u32) -> u64 {
    r.views.get(&id).and_then(|v| v.bundle_2d.as_ref()).map_or(0, |b| b.serial)
}

#[test]
fn scene_lists_are_kept_until_the_camera_or_the_scene_changes() {
    let _serial = crate::memory::gpu_test_lock();
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue);
    scene(&mut r);
    let none = Overlay::default();
    let view = persp([0.0, -600.0, 400.0]);
    let first = pixels(&mut r, 1, &view, &none);
    let s1 = list_serial(&r, 1);
    assert_ne!(s1, 0, "the 3D view keeps its draw lists");
    assert_eq!(pixels(&mut r, 1, &view, &none), first);
    assert_eq!(list_serial(&r, 1), s1, "an unchanged frame replays the kept lists");

    r.set_view_options(1, ViewOptions::default());
    r.render_view(1, &view, &none);
    let s1 = list_serial(&r, 1);
    let animated = ViewOptions { time: 3.5, ..r.view_options(1) };
    let content = r.content_generation();
    r.set_view_options(1, animated);
    assert_eq!(r.content_generation(), content, "the animation clock is not a scene change");
    r.render_view(1, &view, &none);
    assert_eq!(list_serial(&r, 1), s1, "animation frames keep the lists");

    let moved = persp([40.0, -600.0, 400.0]);
    r.render_view(1, &moved, &none);
    let s2 = list_serial(&r, 1);
    assert_ne!(s2, s1, "a camera move collects again");
    r.render_view(1, &moved, &none);
    assert_eq!(list_serial(&r, 1), s2);

    r.upsert_object(10, cube([-750.0, 200.0, 0.0], [-500.0, 450.0, 400.0]));
    let edited = pixels(&mut r, 1, &moved, &none);
    let s3 = list_serial(&r, 1);
    assert_ne!(s3, s2, "an edited brush rebuilds the lists");
    assert_eq!(edited, uncached(&mut r, 1, &moved, &none), "kept lists draw what a fresh frame draws");

    r.render_view(1, &moved, &none);
    let s3 = list_serial(&r, 1);
    r.set_object_flags(11, ObjectFlags::HIDDEN);
    let hidden = pixels(&mut r, 1, &moved, &none);
    assert_ne!(list_serial(&r, 1), s3, "hiding an object rebuilds the lists");
    assert_eq!(hidden, uncached(&mut r, 1, &moved, &none));

    r.set_object_flags(12, ObjectFlags::SELECTED);
    let selected = pixels(&mut r, 1, &moved, &none);
    assert_eq!(selected, uncached(&mut r, 1, &moved, &none), "selection draws the same with kept lists");

    r.remove_object(13);
    let removed = pixels(&mut r, 1, &moved, &none);
    assert_eq!(removed, uncached(&mut r, 1, &moved, &none), "a removed brush is gone from the kept lists");
    assert_ne!(removed, selected);
}

#[test]
fn the_2d_scene_bundle_replays_until_the_view_leaves_its_window() {
    let _serial = crate::memory::gpu_test_lock();
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue);
    scene(&mut r);
    let none = Overlay::default();
    let view = top([0.0, 400.0, 0.0], 0.1);
    let first = pixels(&mut r, 2, &view, &none);
    let b1 = bundle_serial(&r, 2);
    assert_ne!(b1, 0, "the 2D view records its scene once");
    assert_eq!(first, uncached(&mut r, 2, &view, &none), "the bundle draws what direct drawing draws");

    r.render_view(2, &view, &none);
    let b1 = bundle_serial(&r, 2);
    let cursor = Overlay {
        lines: vec![OverlayLine { a: [-100.0, 0.0, 0.0], b: [100.0, 300.0, 0.0], color: [1.0, 1.0, 0.0, 1.0] }],
        ..Default::default()
    };
    let with_cursor = pixels(&mut r, 2, &view, &cursor);
    assert_eq!(bundle_serial(&r, 2), b1, "a cursor overlay reuses the bundle");
    assert_ne!(with_cursor, first, "the overlay is drawn over the bundle");
    assert_eq!(with_cursor, uncached(&mut r, 2, &view, &cursor));

    r.render_view(2, &view, &none);
    let b1 = bundle_serial(&r, 2);
    let nudged = top([150.0, 450.0, 0.0], 0.1);
    let panned = pixels(&mut r, 2, &nudged, &none);
    assert_eq!(bundle_serial(&r, 2), b1, "a small pan stays inside the recorded window");
    assert_eq!(panned, uncached(&mut r, 2, &nudged, &none), "a reused window draws like a fresh frame");

    r.render_view(2, &nudged, &none);
    let b1 = bundle_serial(&r, 2);
    let far = top([6000.0, 400.0, 0.0], 0.1);
    r.render_view(2, &far, &none);
    let b2 = bundle_serial(&r, 2);
    assert_ne!(b2, b1, "leaving the window records again");

    let zoomed = top([0.0, 400.0, 0.0], 0.2);
    let z = pixels(&mut r, 2, &zoomed, &none);
    let b3 = bundle_serial(&r, 2);
    assert_ne!(b3, b2, "a zoom records again");
    assert_eq!(z, uncached(&mut r, 2, &zoomed, &none));

    r.render_view(2, &zoomed, &none);
    let b3 = bundle_serial(&r, 2);
    r.upsert_object(14, cube([-150.0, 500.0, 0.0], [300.0, 900.0, 64.0]));
    let edited = pixels(&mut r, 2, &zoomed, &none);
    assert_ne!(bundle_serial(&r, 2), b3, "an edit records again");
    assert_eq!(edited, uncached(&mut r, 2, &zoomed, &none));

    r.render_view(2, &zoomed, &none);
    let b4 = bundle_serial(&r, 2);
    r.set_object_flags(15, ObjectFlags::SELECTED);
    let selected = pixels(&mut r, 2, &zoomed, &none);
    assert_ne!(bundle_serial(&r, 2), b4, "a selection records again");
    assert_eq!(selected, uncached(&mut r, 2, &zoomed, &none));

    r.render_view(2, &zoomed, &none);
    let b5 = bundle_serial(&r, 2);
    r.set_object_flags(17, ObjectFlags::HOVERED);
    let hovered = pixels(&mut r, 2, &zoomed, &none);
    assert_eq!(bundle_serial(&r, 2), b5, "hovering only changes what the graphics card already reads");
    assert_eq!(hovered, uncached(&mut r, 2, &zoomed, &none));
    r.render_view(2, &zoomed, &none);
    let b5 = bundle_serial(&r, 2);
    r.set_object_flags(16, ObjectFlags::HIDDEN);
    let hidden = pixels(&mut r, 2, &zoomed, &none);
    assert_ne!(bundle_serial(&r, 2), b5, "hiding records again");
    assert_eq!(hidden, uncached(&mut r, 2, &zoomed, &none));
}

fn quad(r: &mut Renderer) -> ModelId {
    let v = |y: f32, z: f32| Vertex { pos: [0.0, y, z], normal: [1.0, 0.0, 0.0], uv: [0.0, 0.0], alpha: 1.0 };
    r.create_model(&[ModelPart {
        material: MAT,
        vertices: vec![v(-20.0, -20.0), v(20.0, -20.0), v(20.0, 20.0), v(-20.0, 20.0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        tangents: Vec::new(),
    }])
}

fn at(model: ModelId, object: ObjectId, x: f32, y: f32) -> ModelInstance {
    ModelInstance {
        model,
        transform: [[0.0, -1.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [x, y, 32.0, 1.0]],
        object,
        tint: [1.0; 4],
        fade: [0.0; 2],
    }
}

#[test]
fn props_draw_once_per_model_and_pack_only_when_they_change() {
    let _serial = crate::memory::gpu_test_lock();
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue);
    scene(&mut r);
    let (a, b) = (quad(&mut r), quad(&mut r));
    let mut list = Vec::new();
    for i in 0..300u32 {
        let model = if i % 3 == 0 { b } else { a };
        let y = if i % 2 == 0 { 400.0 + i as f32 } else { -5000.0 - i as f32 };
        list.push(at(model, 1000 + i, (i % 20) as f32 * 30.0 - 300.0, y));
    }
    r.set_model_instances(&list);
    let none = Overlay::default();
    let view = persp([0.0, -600.0, 400.0]);
    let first = pixels(&mut r, 3, &view, &none);
    let packed = |r: &Renderer| r.views.get(&3).and_then(|v| v.packed.as_ref()).map(|p| (p.serial, p.groups.clone()));
    let (s1, groups) = packed(&r).expect("visible props are packed");
    assert_eq!(groups.len(), 2, "one draw group per model");
    assert!(groups.windows(2).all(|w| w[0].key != w[1].key));
    let drawn: u32 = groups.iter().map(|g| g.count).sum();
    assert!(drawn > 0 && drawn <= 150, "props behind the camera are left out ({drawn})");

    assert_eq!(pixels(&mut r, 3, &view, &none), first);
    assert_eq!(packed(&r).map(|p| p.0), Some(s1), "a still camera keeps the packed props");
    assert_eq!(first, uncached(&mut r, 3, &view, &none));

    r.render_view(3, &view, &none);
    let s1 = packed(&r).map(|p| p.0);
    list.truncate(200);
    r.set_model_instances(&list);
    let fewer = pixels(&mut r, 3, &view, &none);
    assert_ne!(packed(&r).map(|p| p.0), s1, "changed props pack again");
    assert_eq!(fewer, uncached(&mut r, 3, &view, &none));
}
