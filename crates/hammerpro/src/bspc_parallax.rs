use glam::DVec3;

const SPREAD: f64 = 48.0;
const REACH: f64 = 8192.0;
const MIN_HALF: f64 = 16.0;

fn origin_of(e: &hp_vmf::Entity) -> Option<DVec3> {
    let v: Vec<f64> = e.get("origin")?.split_ascii_whitespace().filter_map(|x| x.parse().ok()).collect();
    (v.len() == 3 && v.iter().all(|x| x.is_finite())).then(|| DVec3::new(v[0], v[1], v[2]))
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

pub(crate) fn room_box(idx: &hp_editor::optimize::SolidIndex, o: DVec3) -> Option<(DVec3, DVec3)> {
    if idx.contains(o).is_some() {
        return None;
    }
    let axes = [DVec3::X, DVec3::Y, DVec3::Z];
    let mut lo = DVec3::ZERO;
    let mut hi = DVec3::ZERO;
    for (a, axis) in axes.iter().enumerate() {
        let (u, v) = (axes[(a + 1) % 3], axes[(a + 2) % 3]);
        for sign in [-1.0, 1.0] {
            let dir = *axis * sign;
            let mut hits = Vec::new();
            for du in [-SPREAD, 0.0, SPREAD] {
                for dv in [-SPREAD, 0.0, SPREAD] {
                    let start = o + u * du + v * dv;
                    if idx.contains(start).is_some() || !idx.clear_line(o, start) {
                        continue;
                    }
                    if let Some(t) = idx.ray_hit(start, dir, REACH) {
                        hits.push(t);
                    }
                }
            }
            if hits.len() < 3 {
                return None;
            }
            let d = median(hits).max(MIN_HALF);
            if sign < 0.0 {
                lo[a] = o[a] - d;
            } else {
                hi[a] = o[a] + d;
            }
        }
    }
    Some((lo, hi))
}

fn brush_box(e: &hp_vmf::Entity) -> Option<(DVec3, DVec3)> {
    let mut lo = DVec3::INFINITY;
    let mut hi = DVec3::NEG_INFINITY;
    for g in hp_geom::build_solids(&e.solids).iter().filter(|g| g.valid) {
        lo = lo.min(g.aabb.min);
        hi = hi.max(g.aabb.max);
    }
    (lo.is_finite() && hi.is_finite() && (hi - lo).min_element() > 1.0).then_some((lo, hi))
}

pub(crate) fn place_boxes(map: &mut hp_vmf::Map) -> String {
    let doc = hp_editor::Document::from_map(map.clone());
    let idx = hp_editor::optimize::SolidIndex::new(&doc, false);
    let obbs: Vec<(String, (DVec3, DVec3))> = map
        .entities
        .iter()
        .filter(|e| e.classname.eq_ignore_ascii_case(hp_bspc::cubemap::PARALLAX_OBB_CLASS))
        .filter_map(|e| Some((e.get("targetname")?.trim().to_ascii_lowercase(), brush_box(e)?)))
        .collect();
    let (mut boxed, mut open, mut total) = (0usize, 0usize, 0usize);
    for e in map.entities.iter_mut().filter(|e| e.classname.eq_ignore_ascii_case("env_cubemap")) {
        total += 1;
        if e.get(hp_bspc::cubemap::boxproj::BOX_KEY).is_some_and(|v| hp_bspc::cubemap::boxproj::parse_box(v).is_some())
        {
            boxed += 1;
            continue;
        }
        let named = e.get("parallaxobb").map(|n| n.trim().to_ascii_lowercase()).filter(|n| !n.is_empty());
        let from_obb = named.and_then(|n| obbs.iter().find(|(name, _)| *name == n).map(|(_, b)| *b));
        let found = from_obb.or_else(|| origin_of(e).and_then(|o| room_box(&idx, o)));
        match found {
            Some((lo, hi)) => {
                e.set(hp_bspc::cubemap::boxproj::BOX_KEY, hp_bspc::cubemap::boxproj::box_text(lo, hi));
                boxed += 1;
            }
            None => open += 1,
        }
    }
    format!(
        "Parallax corrected cubemaps: {boxed} of {total} env_cubemap got the box of their room{}",
        if open > 0 {
            format!(", {open} outdoors or in an open area keep the plain reflection")
        } else {
            String::new()
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(id: &mut u32, a: DVec3, b: DVec3) -> String {
        let mut out = String::new();
        *id += 1;
        out.push_str(&format!("\tsolid\n\t{{\n\t\t\"id\" \"{id}\"\n"));
        let planes = [
            ((a.x, b.y, b.z), (b.x, b.y, b.z), (b.x, a.y, b.z)),
            ((a.x, a.y, a.z), (b.x, a.y, a.z), (b.x, b.y, a.z)),
            ((a.x, b.y, b.z), (a.x, a.y, b.z), (a.x, a.y, a.z)),
            ((b.x, b.y, a.z), (b.x, a.y, a.z), (b.x, a.y, b.z)),
            ((b.x, b.y, b.z), (a.x, b.y, b.z), (a.x, b.y, a.z)),
            ((b.x, a.y, a.z), (a.x, a.y, a.z), (a.x, a.y, b.z)),
        ];
        for (p, q, r) in planes {
            *id += 1;
            out.push_str(&format!(
                "\t\tside\n\t\t{{\n\t\t\t\"id\" \"{id}\"\n\t\t\t\"plane\" \"({} {} {}) ({} {} {}) ({} {} {})\"\n\t\t\t\"material\" \"BRICK/BRICKWALL001A\"\n\t\t\t\"uaxis\" \"[1 0 0 0] 0.25\"\n\t\t\t\"vaxis\" \"[0 -1 0 0] 0.25\"\n\t\t}}\n",
                p.0, p.1, p.2, q.0, q.1, q.2, r.0, r.1, r.2
            ));
        }
        out.push_str("\t}\n");
        out
    }

    #[test]
    fn cubemaps_get_the_box_of_their_room() {
        let mut id = 1;
        let (lo, hi) = (DVec3::new(0.0, 0.0, 0.0), DVec3::new(512.0, 384.0, 256.0));
        let t = 16.0;
        let mut world = String::new();
        world.push_str(&block(&mut id, DVec3::new(lo.x - t, lo.y - t, lo.z - t), DVec3::new(hi.x + t, hi.y + t, lo.z)));
        world.push_str(&block(&mut id, DVec3::new(lo.x - t, lo.y - t, hi.z), DVec3::new(hi.x + t, hi.y + t, hi.z + t)));
        world.push_str(&block(&mut id, DVec3::new(lo.x - t, lo.y - t, lo.z), DVec3::new(lo.x, hi.y + t, hi.z)));
        world.push_str(&block(&mut id, DVec3::new(hi.x, lo.y - t, lo.z), DVec3::new(hi.x + t, hi.y + t, hi.z)));
        world.push_str(&block(&mut id, DVec3::new(lo.x, lo.y - t, lo.z), DVec3::new(hi.x, lo.y, hi.z)));
        world.push_str(&block(&mut id, DVec3::new(lo.x, hi.y, lo.z), DVec3::new(hi.x, hi.y + t, hi.z)));
        let text = format!(
            "world\n{{\n\t\"id\" \"1\"\n\t\"classname\" \"worldspawn\"\n{world}}}\nentity\n{{\n\t\"id\" \"900\"\n\t\"classname\" \"env_cubemap\"\n\t\"origin\" \"128 100 64\"\n}}\nentity\n{{\n\t\"id\" \"901\"\n\t\"classname\" \"env_cubemap\"\n\t\"origin\" \"4000 4000 4000\"\n}}\n"
        );
        let mut map = hp_vmf::parse(&text).expect("vmf");
        let line = place_boxes(&mut map);
        assert!(line.contains("1 of 2"), "{line}");
        let key = map.entities[0].get(hp_bspc::cubemap::boxproj::BOX_KEY).expect("box");
        let (a, b) = hp_bspc::cubemap::boxproj::parse_box(key).expect("parsed");
        assert!(a.distance(lo) < 1.0 && b.distance(hi) < 1.0, "{a} {b}");
        assert!(
            map.entities[1].get(hp_bspc::cubemap::boxproj::BOX_KEY).is_none(),
            "the cubemap in the void has no box"
        );
    }
}
