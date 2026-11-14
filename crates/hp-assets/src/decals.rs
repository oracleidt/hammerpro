use ahash::{AHashMap, AHashSet};
use glam::{DMat4, DVec2, DVec3};
use hp_editor::{Document, Id};
use hp_geom::Aabb;
use hp_render::{MaterialId, MeshBatch, ObjectFlags, ObjectMesh, Vertex};

use crate::AssetManager;
use crate::entities::decal_object;
use crate::material::{AnaLook, MatKind};
use crate::producer::{DerivedBuild, DerivedObject, DerivedProducer, ProducerChanges, ProducerCx};

const SURFACE_OFFSET: f64 = 0.2;
const COPLANAR_DIST: f64 = 0.5;
const INSIDE_EPS: f64 = 0.05;

pub(crate) fn decal_basis(normal: DVec3) -> (DVec3, DVec3) {
    let n = normal.try_normalize().unwrap_or(DVec3::Z);
    let (s, t) = if n.z.abs() > std::f64::consts::FRAC_1_SQRT_2 {
        let t = DVec3::X.cross(n);
        (n.cross(t), t)
    } else {
        let s = n.cross(DVec3::NEG_Z);
        (s, s.cross(n))
    };
    (s.normalize_or_zero(), t.normalize_or_zero())
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DecalMesh {
    pub polygons: Vec<Vec<(DVec3, [f32; 2])>>,
    pub normal: DVec3,
    pub solids: Vec<Id>,
}

impl DecalMesh {
    pub(crate) fn object_mesh(&self, material: MaterialId, wire: [u8; 3]) -> ObjectMesh {
        let normal = self.normal.as_vec3().to_array();
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut bounds = Aabb::empty();
        for poly in &self.polygons {
            let base = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
            for (p, uv) in poly {
                bounds.extend(*p);
                vertices.push(Vertex { pos: p.as_vec3().to_array(), normal, uv: *uv, alpha: 0.0 });
            }
            hp_geom::triangulate_fan_into(base, poly.len(), &mut indices);
        }
        let (aabb_min, aabb_max) = if bounds.is_empty() {
            ([0.0; 3], [0.0; 3])
        } else {
            (bounds.min.as_vec3().to_array(), bounds.max.as_vec3().to_array())
        };
        ObjectMesh {
            batches: vec![MeshBatch { material, vertices, indices, blend: Vec::new(), cubemaps: Vec::new() }],
            edges: Vec::new(),
            aabb_min,
            aabb_max,
            wire_color: wire,
            ..Default::default()
        }
    }
}

pub(crate) fn decal_reach(origin: DVec3, size: DVec2) -> Aabb {
    let r = size.max_element() * 0.5;
    Aabb::new(origin - DVec3::splat(r), origin + DVec3::splat(r))
}

fn inside_polygon(p: DVec3, vertices: &[DVec3], normal: DVec3) -> bool {
    let n = vertices.len();
    n >= 3
        && (0..n).all(|i| {
            let (a, b) = (vertices[i], vertices[(i + 1) % n]);
            normal.cross(b - a).normalize_or_zero().dot(p - a) >= -INSIDE_EPS
        })
}

fn clip_polygon(mut poly: Vec<DVec3>, clipper: &[DVec3], normal: DVec3) -> Vec<DVec3> {
    let n = clipper.len();
    for i in 0..n {
        if poly.len() < 3 {
            return Vec::new();
        }
        let (a, b) = (clipper[i], clipper[(i + 1) % n]);
        let inward = normal.cross(b - a).normalize_or_zero();
        if inward == DVec3::ZERO {
            continue;
        }
        let side = |p: DVec3| inward.dot(p - a);
        let mut out = Vec::with_capacity(poly.len() + 1);
        for j in 0..poly.len() {
            let (p, q) = (poly[j], poly[(j + 1) % poly.len()]);
            let (dp, dq) = (side(p), side(q));
            if dp >= 0.0 {
                out.push(p);
            }
            if (dp >= 0.0) != (dq >= 0.0) {
                let t = dp / (dp - dq);
                out.push(p + (q - p) * t);
            }
        }
        poly = out;
    }
    if poly.len() >= 3 { poly } else { Vec::new() }
}

pub(crate) fn project_decal(doc: &Document, origin: DVec3, tex_size: DVec2, scale: f64) -> Option<DecalMesh> {
    project_decal_layers(&[(doc, DMat4::IDENTITY)], origin, tex_size, scale)
}

fn receives(node: &hp_editor::SolidNode, side_index: usize) -> bool {
    node.solid.sides.get(side_index).is_some_and(|s| {
        let m = s.material.trim();
        !(m.len() >= 6 && m[..6].eq_ignore_ascii_case("tools/"))
    })
}

fn layer_solids<'a>(doc: &'a Document, reach: &Aabb) -> Vec<(Id, &'a hp_editor::SolidNode)> {
    doc.scene()
        .solids()
        .iter()
        .filter(|(_, n)| !n.bounds.is_empty() && n.bounds.intersects(reach))
        .map(|(id, n)| (*id, n))
        .collect()
}

type Layer<'a> = (DMat4, DMat4, Vec<(Id, &'a hp_editor::SolidNode)>);

fn local_reach(reach: &Aabb, inverse: &DMat4) -> Aabb {
    let mut out = Aabb::empty();
    for i in 0..8 {
        let p = DVec3::new(
            if i & 1 == 0 { reach.min.x } else { reach.max.x },
            if i & 2 == 0 { reach.min.y } else { reach.max.y },
            if i & 4 == 0 { reach.min.z } else { reach.max.z },
        );
        out.extend(inverse.transform_point3(p));
    }
    out
}

pub(crate) fn project_decal_layers(
    layers: &[(&Document, DMat4)],
    origin: DVec3,
    tex_size: DVec2,
    scale: f64,
) -> Option<DecalMesh> {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let size = tex_size.max(DVec2::ONE) * scale;
    let reach = decal_reach(origin, size);
    let max_dist = size.max_element() * 0.5;
    let prepared: Vec<Layer<'_>> = layers
        .iter()
        .filter_map(|(doc, m)| {
            let inverse = m.inverse();
            inverse.is_finite().then(|| (*m, inverse, layer_solids(doc, &local_reach(&reach, &inverse))))
        })
        .collect();
    let mut best: Option<(f64, DVec3, f64)> = None;
    for (m, inverse, solids) in &prepared {
        let local_origin = inverse.transform_point3(origin);
        for (_, node) in solids {
            for face in &node.geom.faces {
                let (n, d) = (face.plane.normal, face.plane.dist);
                let dist = n.dot(local_origin) - d;
                if !(-INSIDE_EPS..=max_dist).contains(&dist) || !receives(node, face.side_index) {
                    continue;
                }
                if best.is_some_and(|(b, _, _)| b <= dist) {
                    continue;
                }
                if inside_polygon(local_origin - n * dist, &face.vertices, n) {
                    let normal = m.transform_vector3(n).normalize_or_zero();
                    let on_plane = m.transform_point3(n * d);
                    best = Some((dist, normal, normal.dot(on_plane)));
                }
            }
        }
    }
    let (dist, normal, plane_dist) = best?;
    let center = origin - normal * dist;
    let (s, t) = decal_basis(normal);
    let (hw, hh) = (size.x * 0.5, size.y * 0.5);
    let quad = [center - s * hw - t * hh, center + s * hw - t * hh, center + s * hw + t * hh, center - s * hw + t * hh];
    let mut out = DecalMesh { polygons: Vec::new(), normal, solids: Vec::new() };
    for (layer, (m, inverse, solids)) in prepared.iter().enumerate() {
        let n_local = inverse.transform_vector3(normal).normalize_or_zero();
        let d_local = n_local.dot(inverse.transform_point3(normal * plane_dist));
        let quad_local: Vec<DVec3> = quad.iter().map(|p| inverse.transform_point3(*p)).collect();
        for (id, node) in solids {
            let mut touched = false;
            for face in &node.geom.faces {
                let n = face.plane.normal;
                if n.dot(n_local) < 0.999 || (face.plane.dist - d_local).abs() > COPLANAR_DIST {
                    continue;
                }
                if !receives(node, face.side_index) {
                    continue;
                }
                let face_poly: Vec<DVec3> =
                    face.vertices.iter().map(|v| *v - n_local * (n_local.dot(*v) - d_local)).collect();
                let clipped = clip_polygon(quad_local.clone(), &face_poly, n_local);
                if clipped.is_empty() {
                    continue;
                }
                touched = true;
                let poly = clipped
                    .into_iter()
                    .map(|p| {
                        let p = m.transform_point3(p);
                        let rel = p - center;
                        let uv = [
                            (rel.dot(s) / scale + tex_size.x * 0.5) as f32,
                            (rel.dot(t) / scale + tex_size.y * 0.5) as f32,
                        ];
                        (p + normal * SURFACE_OFFSET, uv)
                    })
                    .collect();
                out.polygons.push(poly);
            }
            if touched && layer == 0 {
                out.solids.push(*id);
            }
        }
    }
    (!out.polygons.is_empty()).then_some(out)
}

struct DecalState {
    pending: bool,
    reach: Option<Aabb>,
    solids: Vec<Id>,
}

pub(crate) enum DecalSize {
    Loading,
    Missing,
    Ready(DVec2, f64),
}

pub(crate) fn decal_size(assets: &mut AssetManager, material: &str) -> DecalSize {
    match assets.material_info_impl(material) {
        None => DecalSize::Loading,
        Some(info) if info.missing || info.width == 0 || info.height == 0 => DecalSize::Missing,
        Some(info) => {
            let scale = match assets.mats.ana_look(material) {
                AnaLook::Ready(a) => f64::from(a.decal_scale),
                _ => 1.0,
            };
            DecalSize::Ready(DVec2::new(f64::from(info.width), f64::from(info.height)), scale)
        }
    }
}

#[derive(Default)]
pub(crate) struct DecalProducer {
    decals: AHashMap<Id, DecalState>,
    dirty: AHashSet<Id>,
}

impl DecalProducer {
    fn update(&mut self, cx: &mut ProducerCx<'_>, id: Id, out: &mut DerivedBuild) {
        let wanted =
            cx.assets.scene.ents.get(&id).and_then(|s| Some((s.desc.decal.clone()?, s.desc.origin, s.desc.color)));
        let (Some((material, origin, color)), Some(_)) = (wanted, decal_object(id)) else {
            if self.decals.remove(&id).is_some() {
                out.remove.push(u64::from(id));
            }
            return;
        };
        let mut state = DecalState { pending: false, reach: None, solids: Vec::new() };
        let mut shown = false;
        match decal_size(cx.assets, &material) {
            DecalSize::Loading => state.pending = true,
            DecalSize::Missing => {}
            DecalSize::Ready(size, scale) => {
                state.reach = Some(decal_reach(origin, size * scale));
                if let Some(mesh) = project_decal(cx.doc, origin, size, scale) {
                    let mat = cx.assets.asset_material(&material, MatKind::World);
                    out.upsert.push(DerivedObject {
                        key: u64::from(id),
                        mesh: mesh.object_mesh(mat, color),
                        owner: Some(id),
                        flags: ObjectFlags::NO_GAME_WIRE,
                    });
                    state.solids = mesh.solids;
                    shown = true;
                }
            }
        }
        if !shown {
            out.remove.push(u64::from(id));
        }
        self.decals.insert(id, state);
    }
}

impl DerivedProducer for DecalProducer {
    fn name(&self) -> &'static str {
        "decals"
    }

    fn object_id(&self, key: u64) -> Option<u32> {
        u32::try_from(key).ok().and_then(decal_object)
    }

    fn changed(&mut self, changes: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool {
        if changes.full {
            self.decals.clear();
            self.dirty.clear();
        }
        let doc = changes.doc;
        if changes.full || changes.entities {
            self.dirty.extend(cx.doc.scene().entities().keys().copied());
        } else {
            self.dirty.extend(doc.entities_changed.iter().copied());
            self.dirty.extend(doc.entities_removed.iter().copied().filter(|id| self.decals.contains_key(id)));
            if (!doc.solids_changed.is_empty() || !doc.solids_removed.is_empty()) && !self.decals.is_empty() {
                let changed: AHashSet<Id> =
                    doc.solids_changed.iter().chain(doc.solids_removed.iter()).copied().collect();
                let scene = cx.doc.scene();
                let boxes: Vec<Aabb> = changed.iter().filter_map(|id| scene.solid(*id)).map(|n| n.bounds).collect();
                for (id, d) in &self.decals {
                    if d.solids.iter().any(|s| changed.contains(s))
                        || d.reach.is_some_and(|r| boxes.iter().any(|b| !b.is_empty() && b.intersects(&r)))
                    {
                        self.dirty.insert(*id);
                    }
                }
            }
        }
        self.dirty.extend(self.decals.iter().filter(|(_, d)| d.pending).map(|(id, _)| *id));
        !self.dirty.is_empty()
    }

    fn build(&mut self, cx: &mut ProducerCx<'_>) -> DerivedBuild {
        let mut out = DerivedBuild::default();
        let mut ids: Vec<Id> = self.dirty.drain().collect();
        ids.sort_unstable();
        for id in ids {
            self.update(cx, id, &mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Document {
        let state = hp_app::testing::test_state();
        state.doc
    }

    #[test]
    fn basis_like_the_engine() {
        let (s, t) = decal_basis(DVec3::Z);
        assert!((s - DVec3::X).length() < 1e-9 && (t - DVec3::NEG_Y).length() < 1e-9, "{s} {t}");
        let (s, t) = decal_basis(DVec3::NEG_Y);
        assert!((t - DVec3::NEG_Z).length() < 1e-9, "{t}");
        assert!((s - DVec3::X).length() < 1e-9, "texture right is +X seen from the front: {s}");
    }

    #[test]
    fn decal_on_a_wall() {
        let doc = doc();
        let origin = DVec3::new(0.0, 255.0, 64.0);
        let mesh = project_decal(&doc, origin, DVec2::new(64.0, 32.0), 1.0).unwrap();
        assert_eq!(mesh.solids, [3]);
        assert!((mesh.normal - DVec3::NEG_Y).length() < 1e-9);
        assert_eq!(mesh.polygons.len(), 1);
        let poly = &mesh.polygons[0];
        assert_eq!(poly.len(), 4);
        for (p, uv) in poly {
            assert!((p.y - (256.0 - SURFACE_OFFSET)).abs() < 1e-9, "on the wall, slightly in front: {p}");
            assert!((p.x.abs() - 32.0).abs() < 1e-9 && (p.z - 64.0).abs() - 16.0 < 1e-9, "{p}");
            assert!((0.0..=64.0).contains(&uv[0]) && (0.0..=32.0).contains(&uv[1]), "{uv:?}");
        }
        let top = poly.iter().find(|(_, uv)| uv[1] < 1.0).unwrap();
        assert!(top.0.z > 64.0);
        let big = project_decal(&doc, origin, DVec2::new(64.0, 32.0), 2.0).unwrap();
        let xs: Vec<f64> = big.polygons[0].iter().map(|(p, _)| p.x).collect();
        assert!(xs.iter().any(|x| (*x - 64.0).abs() < 1e-9), "{xs:?}");
        assert!(big.polygons[0].iter().all(|(_, uv)| uv[0] <= 64.0 + 1e-3));
    }

    #[test]
    fn decal_clipped_to_the_floor_edge() {
        let doc = doc();
        let origin = DVec3::new(250.0, 0.0, 1.0);
        let mesh = project_decal(&doc, origin, DVec2::new(32.0, 32.0), 1.0).unwrap();
        assert!((mesh.normal - DVec3::Z).length() < 1e-9);
        let max_x = mesh.polygons[0].iter().map(|(p, _)| p.x).fold(f64::MIN, f64::max);
        assert!((max_x - 256.0).abs() < 1e-9, "clipped to the face: {max_x}");
        assert_eq!(mesh.polygons[0].len(), 4);
    }

    #[test]
    fn no_decal_in_the_air_or_on_tool_faces() {
        let doc = doc();
        assert!(project_decal(&doc, DVec3::new(0.0, 0.0, 600.0), DVec2::splat(64.0), 1.0).is_none());
        assert!(project_decal(&doc, DVec3::new(96.0, 32.0, 63.0), DVec2::splat(16.0), 1.0).is_none());
        assert!(project_decal(&doc, DVec3::new(0.0, 264.0, 64.0), DVec2::splat(16.0), 1.0).is_none());
    }

    #[test]
    fn mesh_has_one_batch_and_bounds() {
        let doc = doc();
        let mesh = project_decal(&doc, DVec3::new(0.0, 255.0, 64.0), DVec2::splat(32.0), 1.0).unwrap();
        let obj = mesh.object_mesh(7, [1, 2, 3]);
        assert_eq!(obj.batches.len(), 1);
        assert_eq!(obj.batches[0].material, 7);
        assert_eq!(obj.batches[0].vertices.len(), 4);
        assert_eq!(obj.batches[0].indices.len(), 6);
        assert!(obj.edges.is_empty());
        assert!(obj.aabb_min[0] < obj.aabb_max[0]);
    }
}
