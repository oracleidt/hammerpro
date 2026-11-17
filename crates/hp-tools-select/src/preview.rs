use egui::Color32;
use glam::{DMat4, DVec3};
use hp_editor::{Document, ObjRef};
use hp_geom::Aabb;
use hp_render::{Overlay, OverlayLine};

pub const MAX_PREVIEW_EDGES: usize = 20_000;

#[derive(Clone, Debug, Default)]
pub struct PreviewGeom {
    pub edges: Vec<[DVec3; 2]>,
    pub bounds: Option<Aabb>,
    pub simplified: bool,
}

impl PreviewGeom {
    pub fn capture(doc: &Document, objs: &[ObjRef], entity_bounds: &dyn Fn(&hp_vmf::Entity) -> Option<Aabb>) -> Self {
        let scene = doc.scene();
        let bounds = scene.objects_bounds(objs, entity_bounds);
        let mut edges = Vec::new();
        let mut simplified = false;
        'collect: {
            for id in scene.expand_solids(objs) {
                let Some(node) = scene.solid(id) else { continue };
                if !push_solid_edges(&mut edges, &node.geom, MAX_PREVIEW_EDGES) {
                    simplified = true;
                    break 'collect;
                }
            }
            for id in scene.expand_entities(objs) {
                if scene.is_brush_entity(id) {
                    continue;
                }
                if let Some(b) = scene.object_bounds(ObjRef::Entity(id), entity_bounds) {
                    if edges.len() + 12 > MAX_PREVIEW_EDGES {
                        simplified = true;
                        break 'collect;
                    }
                    edges.extend_from_slice(&box_edges(&b));
                }
            }
        }
        if simplified {
            edges.clear();
            if let Some(b) = bounds {
                edges.extend_from_slice(&box_edges(&b));
            }
        }
        Self { edges, bounds, simplified }
    }

    pub fn push_ghost(&self, out: &mut Overlay, color: [f32; 4]) {
        out.lines.reserve(self.edges.len());
        out.lines.extend(self.edges.iter().map(|[a, b]| line(*a, *b, color)));
    }

    pub fn push(&self, out: &mut Overlay, m: &DMat4, color: [f32; 4]) {
        out.lines_on_top.reserve(self.edges.len());
        for [a, b] in &self.edges {
            out.lines_on_top.push(line(m.transform_point3(*a), m.transform_point3(*b), color));
        }
    }
}

pub fn push_solid_edges(out: &mut Vec<[DVec3; 2]>, geom: &hp_geom::SolidGeom, limit: usize) -> bool {
    for f in &geom.faces {
        let n = f.vertices.len();
        for i in 0..n {
            let a = f.vertices[i];
            let b = f.vertices[if i + 1 == n { 0 } else { i + 1 }];
            if geom.valid && !lex_less(a, b) {
                continue;
            }
            if out.len() >= limit {
                return false;
            }
            out.push([a, b]);
        }
    }
    true
}

fn lex_less(a: DVec3, b: DVec3) -> bool {
    const EPS: f64 = 1e-3;
    for k in 0..3 {
        if a[k] < b[k] - EPS {
            return true;
        }
        if a[k] > b[k] + EPS {
            return false;
        }
    }
    false
}

pub fn box_edges(b: &Aabb) -> [[DVec3; 2]; 12] {
    let (n, x) = (b.min, b.max);
    let c = |i: u8| {
        DVec3::new(
            if i & 1 != 0 { x.x } else { n.x },
            if i & 2 != 0 { x.y } else { n.y },
            if i & 4 != 0 { x.z } else { n.z },
        )
    };
    [
        [c(0), c(1)],
        [c(2), c(3)],
        [c(4), c(5)],
        [c(6), c(7)],
        [c(0), c(2)],
        [c(1), c(3)],
        [c(4), c(6)],
        [c(5), c(7)],
        [c(0), c(4)],
        [c(1), c(5)],
        [c(2), c(6)],
        [c(3), c(7)],
    ]
}

pub fn line(a: DVec3, b: DVec3, color: [f32; 4]) -> OverlayLine {
    OverlayLine { a: a.as_vec3().to_array(), b: b.as_vec3().to_array(), color }
}

pub fn rgba(c: Color32, alpha: f32) -> [f32; 4] {
    [
        f32::from(c.r()) / 255.0,
        f32::from(c.g()) / 255.0,
        f32::from(c.b()) / 255.0,
        (f32::from(c.a()) / 255.0 * alpha).clamp(0.0, 1.0),
    ]
}

pub fn push_outline(
    out: &mut Overlay,
    doc: &Document,
    objs: &[ObjRef],
    entity_bounds: &dyn Fn(&hp_vmf::Entity) -> Option<Aabb>,
    color: [f32; 4],
    limit: usize,
) {
    let scene = doc.scene();
    let mut count = 0usize;
    for id in scene.expand_solids(objs) {
        let Some(node) = scene.solid(id) else { continue };
        for f in &node.geom.faces {
            let n = f.vertices.len();
            for i in 0..n {
                let a = f.vertices[i];
                let b = f.vertices[if i + 1 == n { 0 } else { i + 1 }];
                if node.geom.valid && !lex_less(a, b) {
                    continue;
                }
                if count >= limit {
                    return;
                }
                count += 1;
                out.lines_on_top.push(line(a, b, color));
            }
        }
    }
    for id in scene.expand_entities(objs) {
        if scene.is_brush_entity(id) {
            continue;
        }
        if let Some(b) = scene.object_bounds(ObjRef::Entity(id), entity_bounds) {
            for [a, c] in box_edges(&b) {
                if count >= limit {
                    return;
                }
                count += 1;
                out.lines_on_top.push(line(a, c, color));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_edges_are_axis_aligned_and_unique() {
        let b = Aabb::new(DVec3::ZERO, DVec3::new(1.0, 2.0, 3.0));
        let e = box_edges(&b);
        let mut set = std::collections::HashSet::new();
        for [a, c] in e {
            let d = c - a;
            let nonzero = (0..3).filter(|k| d[*k].abs() > 0.0).count();
            assert_eq!(nonzero, 1);
            set.insert(format!("{a:?}{c:?}"));
        }
        assert_eq!(set.len(), 12);
        assert_eq!(rgba(Color32::from_rgb(255, 0, 0), 0.5), [1.0, 0.0, 0.0, 0.5]);
    }

    #[test]
    fn capture_test_map() {
        let state = hp_app::testing::test_state();
        let doc = &state.doc;
        let eb = |e: &hp_vmf::Entity| state.entity_bounds(e);
        let g = PreviewGeom::capture(doc, &[ObjRef::Solid(2)], &eb);
        assert_eq!(g.edges.len(), 12);
        assert!(!g.simplified);
        let g = PreviewGeom::capture(doc, &[ObjRef::Entity(5)], &eb);
        assert_eq!(g.edges.len(), 12);
        let all = [ObjRef::Solid(2), ObjRef::Solid(3), ObjRef::Entity(4), ObjRef::Entity(5), ObjRef::Entity(6)];
        let g = PreviewGeom::capture(doc, &all, &eb);
        assert_eq!(g.edges.len(), 12 * 5);
        let mut out = Overlay::default();
        g.push(&mut out, &DMat4::from_translation(DVec3::X), [1.0; 4]);
        assert_eq!(out.lines_on_top.len(), 60);
        g.push_ghost(&mut out, [1.0; 4]);
        assert_eq!(out.lines.len(), 60);
        let mut out = Overlay::default();
        push_outline(&mut out, doc, &all, &eb, [1.0; 4], 30);
        assert_eq!(out.lines_on_top.len(), 30);
    }
}
