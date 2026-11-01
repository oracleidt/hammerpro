use glam::DVec3;

use super::{Vmf, block, block_sides, convex, disp_block, room};

const WALL: &str = "dev/dev_measuregeneric01b";
const OTHER: &str = "dev/dev_measurewall01c";
const NODRAW: &str = "tools/toolsnodraw";
const WATER: &str = "dev/dev_water2";

fn v(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

fn room_map(lo: DVec3, hi: DVec3, start: DVec3) -> Vmf {
    let mut m = Vmf::new();
    m.world = room(lo, hi, WALL);
    m.point("info_player_start", start, &[("angles", "0 0 0")]);
    m
}

pub fn chop() -> Vmf {
    let mut m = room_map(v(-256.0, -256.0, 0.0), v(256.0, 256.0, 192.0), v(160.0, 160.0, 8.0));
    m.world.extend([
        block(v(-128.0, -128.0, 0.0), v(0.0, 0.0, 128.0), WALL),
        block(v(-64.0, -64.0, 0.0), v(64.0, 64.0, 96.0), OTHER),
        block(v(-112.0, -112.0, 32.0), v(-80.0, -80.0, 64.0), WALL),
        block(v(200.0, -32.0, 32.0), v(300.0, 32.0, 64.0), WALL),
        block(v(-200.0, 100.0, 0.0), v(-136.0, 164.0, 48.0), WALL),
        block(v(-200.0, 100.0, 0.0), v(-136.0, 164.0, 48.0), WALL),
        block(v(-160.0, 140.0, 24.0), v(-100.0, 200.0, 80.0), OTHER),
        block(v(-266.0, -266.0, 150.0), v(-200.0, -200.0, 200.0), OTHER),
    ]);
    m
}

pub fn tjunc() -> Vmf {
    let mut m = room_map(v(-512.0, -320.0, 0.0), v(640.0, 320.0, 160.0), v(500.0, 200.0, 8.0));
    let mut tall = block(v(-128.0, -64.0, 0.0), v(0.0, 64.0, 64.0), WALL);
    tall.sides[3].lightmap_scale = 8;
    m.world.extend([
        tall,
        block(v(0.0, -64.0, 0.0), v(128.0, 64.0, 32.0), WALL),
        block(v(0.0, -64.0, 32.0), v(48.0, 0.0, 48.0), OTHER),
        block(v(-96.0, -32.0, 64.0), v(-32.0, 32.0, 96.0), WALL),
        block(v(-300.0, 100.0, 0.0), v(-236.0, 300.0, 8.0), WALL),
        block(v(-236.0, 100.0, 0.0), v(-200.0, 200.0, 24.0), WALL),
        block(v(200.0, -320.0, 0.0), v(232.0, -200.0, 100.0), WALL),
    ]);
    m
}

pub fn detail() -> Vmf {
    let mut m = room_map(v(-256.0, -256.0, 0.0), v(256.0, 256.0, 160.0), v(160.0, -160.0, 8.0));
    m.world.push(block(v(-64.0, -64.0, 0.0), v(64.0, 64.0, 64.0), WALL));
    m.brush_entity(
        "func_detail",
        &[],
        vec![
            block(v(0.0, 0.0, 0.0), v(96.0, 96.0, 96.0), OTHER),
            block(v(-200.0, 100.0, 0.0), v(-150.0, 150.0, 40.0), WALL),
            block(v(200.0, 200.0, 0.0), v(256.0, 256.0, 120.0), WALL),
            block_sides(v(-200.0, -200.0, 0.0), v(-120.0, -120.0, 32.0), [WALL, NODRAW, WALL, WALL, WALL, WALL]),
        ],
    );
    m
}

pub fn water() -> Vmf {
    let mut m = room_map(v(-256.0, -256.0, 0.0), v(256.0, 256.0, 192.0), v(200.0, 200.0, 80.0));
    m.world.extend([
        block(v(-224.0, -224.0, 0.0), v(160.0, 160.0, 64.0), WATER),
        block(v(-32.0, -32.0, 0.0), v(32.0, 32.0, 128.0), WALL),
        block(v(-160.0, 64.0, 0.0), v(-96.0, 128.0, 96.0), OTHER),
    ]);
    m
}

pub fn areaportal() -> Vmf {
    let mut m = Vmf::new();
    m.world = room(v(-400.0, -200.0, 0.0), v(400.0, 200.0, 160.0), WALL);
    m.world.extend([
        block(v(-8.0, -200.0, 0.0), v(8.0, -32.0, 160.0), WALL),
        block(v(-8.0, 32.0, 0.0), v(8.0, 200.0, 160.0), WALL),
        block(v(-8.0, -32.0, 96.0), v(8.0, 32.0, 160.0), WALL),
        block(v(-300.0, -100.0, 0.0), v(-200.0, 0.0, 48.0), OTHER),
    ]);
    m.brush_entity(
        "func_areaportal",
        &[("StartOpen", "1"), ("PortalVersion", "1"), ("target", "door1")],
        vec![block(v(-2.0, -32.0, 0.0), v(2.0, 32.0, 96.0), "tools/toolsareaportal")],
    );
    m.brush_entity(
        "func_door",
        &[
            ("targetname", "door1"),
            ("movedir", "-90 0 0"),
            ("speed", "100"),
            ("spawnflags", "256"),
            ("lip", "0"),
            ("wait", "4"),
        ],
        vec![block(v(-6.0, -32.0, 0.0), v(-2.0, 32.0, 96.0), OTHER)],
    );
    m.point("info_player_start", v(-200.0, 100.0, 8.0), &[("angles", "0 0 0")]);
    m.point("info_player_start", v(200.0, 100.0, 8.0), &[("angles", "0 0 0")]);
    m
}

pub fn brush_entity() -> Vmf {
    let mut m = room_map(v(-256.0, -256.0, 0.0), v(256.0, 256.0, 192.0), v(0.0, 200.0, 8.0));
    m.world.push(block(v(-256.0, -64.0, 0.0), v(-192.0, 64.0, 64.0), WALL));
    m.brush_entity(
        "func_door",
        &[
            ("targetname", "door"),
            ("movedir", "90 0 0"),
            ("speed", "100"),
            ("spawnflags", "0"),
            ("lip", "8"),
            ("wait", "4"),
        ],
        vec![block(v(-192.0, -32.0, 0.0), v(-176.0, 32.0, 112.0), OTHER)],
    );
    m.brush_entity(
        "func_rotating",
        &[("spawnflags", "1"), ("maxspeed", "60"), ("origin", "48 0 32")],
        vec![block(v(40.0, -80.0, 0.0), v(56.0, 80.0, 64.0), WALL)],
    );
    m.brush_entity(
        "func_brush",
        &[("solidity", "0"), ("Solidbsp", "0")],
        vec![block(v(100.0, 100.0, 0.0), v(164.0, 164.0, 64.0), WALL)],
    );
    m.brush_entity(
        "func_wall_toggle",
        &[],
        vec![
            block(v(-100.0, -200.0, 0.0), v(-36.0, -136.0, 48.0), WALL),
            block(v(-60.0, -170.0, 40.0), v(-20.0, -130.0, 90.0), OTHER),
        ],
    );
    m
}

pub fn origin_brush() -> Vmf {
    let mut m = room_map(v(-256.0, -256.0, 0.0), v(256.0, 256.0, 192.0), v(0.0, 200.0, 8.0));
    m.brush_entity(
        "func_rotating",
        &[("spawnflags", "1"), ("maxspeed", "60")],
        vec![
            block(v(40.0, -80.0, 0.0), v(56.0, 80.0, 64.0), WALL),
            block(v(40.0, -8.0, 24.0), v(56.0, 8.0, 40.0), "tools/toolsorigin"),
        ],
    );
    m
}

pub fn trains() -> Vmf {
    let mut m = room_map(v(-512.0, -512.0, 0.0), v(512.0, 512.0, 256.0), v(0.0, 0.0, 8.0));
    for (i, (x, y)) in [(-256.0, -256.0), (256.0, -256.0), (256.0, 256.0), (-256.0, 256.0)].into_iter().enumerate() {
        let name = format!("t{}", i + 1);
        let next = format!("t{}", (i + 1) % 4 + 1);
        m.point("path_track", v(x, y, 32.0), &[("targetname", &name), ("target", &next)]);
    }
    m.brush_entity(
        "func_tracktrain",
        &[("targetname", "train"), ("target", "t1"), ("speed", "100"), ("startspeed", "200")],
        vec![block(v(-320.0, -288.0, 32.0), v(-192.0, -224.0, 96.0), WALL)],
    );
    m.brush_entity(
        "func_tanktrain",
        &[("targetname", "tank"), ("target", "t3"), ("origin", "256 256 64")],
        vec![block(v(224.0, 240.0, 48.0), v(288.0, 272.0, 80.0), WALL)],
    );
    m
}

pub fn displacement() -> Vmf {
    let mut m = room_map(v(-256.0, -256.0, 0.0), v(256.0, 256.0, 192.0), v(160.0, 160.0, 48.0));
    let mut patch = block(v(-192.0, -192.0, 0.0), v(64.0, 64.0, 16.0), WALL);
    patch.sides[0].disp = Some(disp_block(3, v(-192.0, -192.0, 16.0), 3.5));
    m.world.extend([
        patch,
        block(v(64.0, -192.0, 0.0), v(128.0, 0.0, 48.0), OTHER),
        block(v(-64.0, 64.0, 0.0), v(0.0, 128.0, 64.0), WALL),
    ]);
    m
}

pub fn slanted() -> Vmf {
    let mut m = room_map(v(-256.0, -256.0, 0.0), v(256.0, 256.0, 192.0), v(180.0, 180.0, 8.0));
    let ramp = convex(
        &[
            [v(-200.5, -100.25, 0.0), v(-20.25, -100.25, 70.125), v(-20.25, -40.75, 70.125)],
            [v(-200.5, -100.25, 0.0), v(-20.25, -100.25, 0.0), v(-20.25, -40.75, 0.0)],
            [v(-20.25, -100.25, 0.0), v(-20.25, -40.75, 0.0), v(-20.25, -40.75, 70.125)],
            [v(-200.5, -100.25, 0.0), v(-20.25, -100.25, 0.0), v(-20.25, -100.25, 70.125)],
            [v(-200.5, -40.75, 0.0), v(-20.25, -40.75, 0.0), v(-20.25, -40.75, 70.125)],
        ],
        v(-60.0, -70.0, 10.0),
        WALL,
    );
    let (s, c) = 23f64.to_radians().sin_cos();
    let center = v(60.3, 80.7, 0.0);
    let corner = |x: f64, y: f64, z: f64| {
        let p = v(x * c - y * s, x * s + y * c, z) + center;
        v((p.x * 1000.0).round() / 1000.0, (p.y * 1000.0).round() / 1000.0, z)
    };
    let (hx, hy, h) = (40.0, 24.0, 56.5);
    let turned = convex(
        &[
            [corner(-hx, -hy, h), corner(hx, -hy, h), corner(hx, hy, h)],
            [corner(-hx, -hy, 0.0), corner(hx, -hy, 0.0), corner(hx, hy, 0.0)],
            [corner(-hx, -hy, 0.0), corner(-hx, hy, 0.0), corner(-hx, hy, h)],
            [corner(hx, -hy, 0.0), corner(hx, hy, 0.0), corner(hx, hy, h)],
            [corner(-hx, -hy, 0.0), corner(hx, -hy, 0.0), corner(hx, -hy, h)],
            [corner(-hx, hy, 0.0), corner(hx, hy, 0.0), corner(hx, hy, h)],
        ],
        center + v(0.0, 0.0, 20.0),
        OTHER,
    );
    let wedge = convex(
        &[
            [v(80.0, -256.0, 120.5), v(200.0, -256.0, 120.5), v(200.0, -190.333, 0.0)],
            [v(80.0, -256.0, 0.0), v(200.0, -256.0, 0.0), v(200.0, -190.333, 0.0)],
            [v(80.0, -256.0, 0.0), v(200.0, -256.0, 0.0), v(200.0, -256.0, 120.5)],
            [v(80.0, -256.0, 0.0), v(80.0, -190.333, 0.0), v(80.0, -256.0, 120.5)],
            [v(200.0, -256.0, 0.0), v(200.0, -190.333, 0.0), v(200.0, -256.0, 120.5)],
        ],
        v(140.0, -240.0, 10.0),
        WALL,
    );
    m.world.extend([ramp, turned, wedge]);
    m
}

pub fn golden() -> Vec<(&'static str, Vmf)> {
    vec![
        ("golden_chop", chop()),
        ("golden_tjunc", tjunc()),
        ("golden_detail", detail()),
        ("golden_water", water()),
        ("golden_areaportal", areaportal()),
        ("golden_brush_entity", brush_entity()),
        ("golden_origin_brush", origin_brush()),
        ("golden_displacement", displacement()),
        ("golden_slanted", slanted()),
        ("golden_leak", leaking()),
    ]
}

pub fn leaking() -> Vmf {
    let mut m = chop();
    m.world.remove(3);
    m.world.extend([
        block(v(256.0, -272.0, 0.0), v(272.0, -16.0, 192.0), WALL),
        block(v(256.0, 16.0, 0.0), v(272.0, 272.0, 192.0), WALL),
        block(v(256.0, -16.0, 0.0), v(272.0, 16.0, 64.0), WALL),
        block(v(256.0, -16.0, 96.0), v(272.0, 16.0, 192.0), WALL),
    ]);
    m
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next()
    }
    fn grid(&mut self, lo: f64, hi: f64, step: f64) -> f64 {
        (self.range(lo, hi) / step).round() * step
    }
}

fn turned_box(center: DVec3, half: DVec3, angle: f64, material: &str) -> super::Solid {
    let (s, c) = angle.to_radians().sin_cos();
    let corner = |x: f64, y: f64, z: f64| {
        let p = v(x * c - y * s, x * s + y * c, z) + center;
        v((p.x * 1000.0).round() / 1000.0, (p.y * 1000.0).round() / 1000.0, (p.z * 1000.0).round() / 1000.0)
    };
    let (hx, hy, hz) = (half.x, half.y, half.z);
    convex(
        &[
            [corner(-hx, -hy, hz), corner(hx, -hy, hz), corner(hx, hy, hz)],
            [corner(-hx, -hy, -hz), corner(hx, -hy, -hz), corner(hx, hy, -hz)],
            [corner(-hx, -hy, -hz), corner(-hx, hy, -hz), corner(-hx, hy, hz)],
            [corner(hx, -hy, -hz), corner(hx, hy, -hz), corner(hx, hy, hz)],
            [corner(-hx, -hy, -hz), corner(hx, -hy, -hz), corner(hx, -hy, hz)],
            [corner(-hx, hy, -hz), corner(hx, hy, -hz), corner(hx, hy, hz)],
        ],
        center,
        material,
    )
}

pub fn generated(seed: u64) -> Vmf {
    let mut r = Lcg(seed);
    let start = v(0.0, 0.0, 8.0);
    let mut m = room_map(v(-512.0, -512.0, 0.0), v(512.0, 512.0, 384.0), start);
    let clear = |lo: DVec3, hi: DVec3| hi.x < -40.0 || lo.x > 40.0 || hi.y < -40.0 || lo.y > 40.0 || lo.z > 80.0;
    let mut boxes = 0;
    while boxes < 36 {
        let step = if boxes % 3 == 0 { 0.5 } else { 8.0 };
        let lo = v(r.grid(-560.0, 480.0, step), r.grid(-560.0, 480.0, step), r.grid(0.0, 300.0, step).max(0.0));
        let hi = lo + v(r.grid(8.0, 200.0, step), r.grid(8.0, 200.0, step), r.grid(8.0, 160.0, step));
        if !clear(lo, hi) {
            continue;
        }
        m.world.push(block(lo, hi, if boxes % 2 == 0 { WALL } else { OTHER }));
        boxes += 1;
    }
    let mut turned = 0;
    while turned < 10 {
        let center = v(r.range(-440.0, 440.0), r.range(-440.0, 440.0), r.range(20.0, 300.0));
        let half = v(r.range(8.0, 70.0), r.range(8.0, 70.0), r.range(8.0, 60.0));
        let reach = half.x.max(half.y) * 1.5;
        if !clear(center - v(reach, reach, half.z), center + v(reach, reach, half.z)) {
            continue;
        }
        m.world.push(turned_box(center, half, r.range(1.0, 89.0), WALL));
        turned += 1;
    }
    let mut detail = Vec::new();
    while detail.len() < 8 {
        let lo = v(r.grid(-500.0, 400.0, 1.0), r.grid(-500.0, 400.0, 1.0), r.grid(0.0, 200.0, 1.0));
        let hi = lo + v(r.grid(4.0, 100.0, 1.0), r.grid(4.0, 100.0, 1.0), r.grid(4.0, 100.0, 1.0));
        if clear(lo, hi) {
            detail.push(block(lo, hi, OTHER));
        }
    }
    m.brush_entity("func_detail", &[], detail);
    m.world.push(block(v(200.0, -500.0, 0.0), v(500.0, -200.0, 48.0), WATER));
    m.brush_entity(
        "func_door",
        &[("movedir", "90 0 0"), ("speed", "100"), ("spawnflags", "0"), ("lip", "8"), ("wait", "4")],
        vec![block(v(-300.0, 300.0, 0.0), v(-284.0, 380.0, 112.0), OTHER)],
    );
    m
}

pub fn generated_leaking(seed: u64) -> Vmf {
    let mut m = generated(seed);
    m.world.remove(5);
    m.world.extend([
        block(v(-512.0, 512.0, 0.0), v(-24.0, 528.0, 384.0), WALL),
        block(v(24.0, 512.0, 0.0), v(512.0, 528.0, 384.0), WALL),
        block(v(-24.0, 512.0, 0.0), v(24.0, 528.0, 200.0), WALL),
        block(v(-24.0, 512.0, 260.0), v(24.0, 528.0, 384.0), WALL),
    ]);
    m
}
