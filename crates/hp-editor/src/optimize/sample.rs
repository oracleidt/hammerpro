use glam::DVec3;
use hp_geom::{Aabb, Primitive, PrimitiveParams};

use crate::scene::Owner;
use crate::selection::ObjRef;
use crate::{Document, Id};

pub const WALL: &str = "BRICK/BRICKWALL001A";

pub fn block(doc: &mut Document, min: DVec3, max: DVec3, material: &str) -> Id {
    let params = PrimitiveParams {
        kind: Primitive::Block,
        bounds: Aabb::new(min, max),
        material: material.to_owned(),
        ..PrimitiveParams::default()
    };
    doc.create_primitive(&params, Owner::World).ok().and_then(|ids| ids.first().copied()).unwrap_or(0)
}

pub struct Hole {
    pub x_side: f64,
    pub y0: f64,
    pub y1: f64,
    pub z1: f64,
}

pub fn shell(doc: &mut Document, min: DVec3, max: DVec3, t: f64, holes: &[Hole]) {
    let v = DVec3::new;
    block(doc, v(min.x - t, min.y - t, min.z - t), v(max.x + t, max.y + t, min.z), WALL);
    block(doc, v(min.x - t, min.y - t, max.z), v(max.x + t, max.y + t, max.z + t), WALL);
    block(doc, v(min.x - t, min.y - t, min.z), v(max.x + t, min.y, max.z), WALL);
    block(doc, v(min.x - t, max.y, min.z), v(max.x + t, max.y + t, max.z), WALL);
    for (x0, x1, side) in [(min.x - t, min.x, -1.0), (max.x, max.x + t, 1.0)] {
        match holes.iter().find(|h| h.x_side == side) {
            None => {
                block(doc, v(x0, min.y, min.z), v(x1, max.y, max.z), WALL);
            }
            Some(h) => {
                block(doc, v(x0, min.y, min.z), v(x1, h.y0, max.z), WALL);
                block(doc, v(x0, h.y1, min.z), v(x1, max.y, max.z), WALL);
                block(doc, v(x0, h.y0, h.z1), v(x1, h.y1, max.z), WALL);
            }
        }
    }
}

pub struct Sample {
    pub doc: Document,
    pub crates: Vec<Id>,
    pub door: Id,
    pub floating_light: Id,
    pub lamp: Id,
}

pub fn two_rooms() -> Sample {
    let mut doc = Document::new_empty();
    let v = DVec3::new;
    let t = 16.0;
    shell(
        &mut doc,
        v(0.0, 0.0, 0.0),
        v(512.0, 512.0, 256.0),
        t,
        &[Hole { x_side: 1.0, y0: 192.0, y1: 320.0, z1: 128.0 }],
    );
    shell(
        &mut doc,
        v(640.0, 0.0, 0.0),
        v(1152.0, 512.0, 256.0),
        t,
        &[Hole { x_side: -1.0, y0: 192.0, y1: 320.0, z1: 128.0 }],
    );
    block(&mut doc, v(528.0, 176.0, -16.0), v(624.0, 336.0, 0.0), WALL);
    block(&mut doc, v(528.0, 176.0, 128.0), v(624.0, 336.0, 144.0), WALL);
    block(&mut doc, v(528.0, 176.0, 0.0), v(624.0, 192.0, 128.0), WALL);
    block(&mut doc, v(528.0, 320.0, 0.0), v(624.0, 336.0, 128.0), WALL);
    let mut crates = Vec::new();
    for i in 0..4 {
        let x = 64.0 + f64::from(i) * 96.0;
        crates.push(block(&mut doc, v(x, 64.0, 0.0), v(x + 32.0, 96.0, 32.0), "WOOD/WOODCRATE001A"));
    }
    crates.push(block(&mut doc, v(880.0, 240.0, 0.0), v(912.0, 272.0, 256.0), "CONCRETE/CONCRETEWALL001A"));
    for i in 0..3 {
        let x = 700.0 + f64::from(i) * 24.0;
        crates.push(block(
            &mut doc,
            v(x, 400.0, 0.0),
            v(x + 24.0, 512.0, 8.0 * f64::from(i + 1)),
            "CONCRETE/CONCRETEFLOOR001A",
        ));
    }
    let door_solid = block(&mut doc, v(514.0, 192.0, 0.0), v(526.0, 320.0, 128.0), "WOOD/WOODDOOR001A");
    let door = doc.tie_to_entity(&[ObjRef::Solid(door_solid)], "func_door", &[]).unwrap_or(0);
    let kv = |k: &str, val: &str| (k.to_owned(), val.to_owned());
    doc.create_point_entity("info_player_start", v(256.0, 256.0, 16.0), &[]).ok();
    let floating_light =
        doc.create_point_entity("light", v(256.0, 256.0, 128.0), &[kv("_light", "255 255 255 200")]).unwrap_or(0);
    let lamp = doc
        .create_point_entity(
            "prop_static",
            v(896.0, 128.0, 240.0),
            &[kv("model", "models/props_c17/light_cagelight02_on.mdl")],
        )
        .unwrap_or(0);
    doc.create_point_entity("light", v(80.0, 80.0, 16.0), &[kv("_light", "255 200 150 100")]).ok();
    doc.create_point_entity("light", v(400.0, 400.0, 200.0), &[kv("_light", "255 255 255 100")]).ok();
    doc.create_point_entity("light", v(410.0, 400.0, 200.0), &[kv("_light", "255 255 255 50")]).ok();
    Sample { doc, crates, door, floating_light, lamp }
}

pub fn row_of_rooms(n: usize) -> Document {
    let mut doc = Document::new_empty();
    let v = DVec3::new;
    let t = 16.0;
    let (w, gap) = (512.0, 128.0);
    let kv = |k: &str, val: &str| (k.to_owned(), val.to_owned());
    for i in 0..n {
        let x0 = i as f64 * (w + gap);
        let mut holes = Vec::new();
        if i > 0 {
            holes.push(Hole { x_side: -1.0, y0: 192.0, y1: 320.0, z1: 128.0 });
        }
        if i + 1 < n {
            holes.push(Hole { x_side: 1.0, y0: 192.0, y1: 320.0, z1: 128.0 });
        }
        shell(&mut doc, v(x0, 0.0, 0.0), v(x0 + w, 512.0, 256.0), t, &holes);
        for k in 0..6 {
            let x = x0 + 48.0 + f64::from(k) * 72.0;
            block(&mut doc, v(x, 40.0, 0.0), v(x + 40.0, 80.0, 40.0), "WOOD/WOODCRATE001A");
        }
        block(&mut doc, v(x0 + 120.0, 380.0, 0.0), v(x0 + 152.0, 412.0, 256.0), "CONCRETE/CONCRETEWALL001A");
        block(&mut doc, v(x0 + 360.0, 380.0, 0.0), v(x0 + 392.0, 412.0, 256.0), "CONCRETE/CONCRETEWALL001A");
        for s in 0..4 {
            let y = 420.0 + f64::from(s) * 20.0;
            block(
                &mut doc,
                v(x0 + 200.0, y, 0.0),
                v(x0 + 312.0, y + 20.0, 8.0 * f64::from(s + 1)),
                "CONCRETE/CONCRETEFLOOR001A",
            );
        }
        block(&mut doc, v(x0, 0.0, 240.0), v(x0 + w, 8.0, 248.0), "WOOD/WOODTRIM001A");
        let lamp = v(x0 + 256.0, 256.0, 240.0);
        doc.create_point_entity("prop_static", lamp, &[kv("model", "models/props_c17/light_cagelight02_on.mdl")]).ok();
        let light = if i % 2 == 0 { v(x0 + 256.0, 256.0, 224.0) } else { v(x0 + 100.0, 300.0, 128.0) };
        doc.create_point_entity("light", light, &[kv("_light", "255 220 180 200")]).ok();
        if i + 1 < n {
            let cx = x0 + w + t;
            block(&mut doc, v(cx, 176.0, -16.0), v(cx + gap - 2.0 * t, 336.0, 0.0), WALL);
            block(&mut doc, v(cx, 176.0, 128.0), v(cx + gap - 2.0 * t, 336.0, 144.0), WALL);
            block(&mut doc, v(cx, 176.0, 0.0), v(cx + gap - 2.0 * t, 192.0, 128.0), WALL);
            block(&mut doc, v(cx, 320.0, 0.0), v(cx + gap - 2.0 * t, 336.0, 128.0), WALL);
            let d = block(&mut doc, v(x0 + w + 2.0, 192.0, 0.0), v(x0 + w + 14.0, 320.0, 128.0), "WOOD/WOODDOOR001A");
            doc.tie_to_entity(&[ObjRef::Solid(d)], "func_door", &[]).ok();
        }
    }
    doc.create_point_entity("info_player_start", v(256.0, 256.0, 16.0), &[]).ok();
    doc
}
