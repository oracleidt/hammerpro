use glam::{DMat3, DMat4, DVec2, DVec3, Vec3};
use hp_editor::instances::{Fixup, KeyFixupFn, is_hidden_object, is_instance};
use hp_editor::side_index::overlay::{self, OverlayFragment};
use hp_editor::{Document, IdSet};
use hp_fgd::ResolvedFgd;
use hp_render::{MaterialId, MeshBatch, ObjectMesh};
use hp_vmf::{Entity, Map};

use crate::AssetManager;
use crate::decals::{DecalSize, decal_size, project_decal_layers};
use crate::entities::{class_of, decal_material, point_color};
use crate::helpers::overlays::{OverlayKind, fragments_mesh, overlay_kind};
use crate::material::MatKind;
use crate::util::material_key;

enum DecorItem {
    Overlay { index: usize, render_order: u8, frags: Vec<OverlayFragment> },
    Decal { index: usize, origin: DVec3 },
}

pub(crate) struct LocalDecor {
    doc: Document,
    items: Vec<DecorItem>,
}

fn is_decor(e: &Entity, fgd: Option<&ResolvedFgd>) -> bool {
    if !e.solids.is_empty() || is_instance(e) || is_hidden_object(e.hidden, &e.editor) {
        return false;
    }
    let class = class_of(fgd, e);
    overlay_kind(e, class).is_some() || decal_material(e, class).is_some()
}

pub(crate) fn local_decor(map: &Map, fgd: Option<&ResolvedFgd>) -> Option<LocalDecor> {
    if !map.entities.iter().any(|e| is_decor(e, fgd)) {
        return None;
    }
    let mut local = map.clone();
    local.entities.retain(|e| !e.solids.is_empty());
    let doc = Document::from_map(local);
    let mut items = Vec::new();
    let mut overlays = Vec::new();
    let mut wanted = IdSet::default();
    for (index, e) in map.entities.iter().enumerate() {
        if !is_decor(e, fgd) {
            continue;
        }
        let class = class_of(fgd, e);
        match overlay_kind(e, class) {
            Some(kind) => {
                wanted.extend(kind_sides(&kind));
                overlays.push((index, kind));
            }
            None => {
                if let Some(origin) = e.origin().filter(|o| o.is_finite()) {
                    items.push(DecorItem::Decal { index, origin });
                }
            }
        }
    }
    let found = doc.find_sides(&wanted);
    let lookup = |s: hp_editor::Id| found.get(&s).copied();
    for (index, kind) in overlays {
        let (frags, render_order) = match &kind {
            OverlayKind::Quad(s) => (overlay::fragments(&doc, s, &lookup), s.render_order),
            OverlayKind::Transition(t) => (overlay::transition_fragments(&doc, t, &lookup), 0),
        };
        if !frags.is_empty() {
            items.push(DecorItem::Overlay { index, render_order, frags });
        }
    }
    Some(LocalDecor { doc, items })
}

fn kind_sides(kind: &OverlayKind) -> Vec<hp_editor::Id> {
    match kind {
        OverlayKind::Quad(s) => s.sides.clone(),
        OverlayKind::Transition(t) => t.all_sides().collect(),
    }
}

fn kind_material(kind: &OverlayKind) -> &str {
    match kind {
        OverlayKind::Quad(s) => &s.material,
        OverlayKind::Transition(t) => &t.material,
    }
}

pub(crate) struct DecorPlacement<'a> {
    pub matrix: DMat4,
    pub fixup: Option<&'a Fixup>,
    pub kinds: Option<KeyFixupFn<'a>>,
    pub overlays: bool,
    pub parent: Option<&'a Document>,
}

#[derive(Default)]
pub(crate) struct PlacedDecor {
    pub meshes: Vec<ObjectMesh>,
    pub pending: bool,
    pub decals: bool,
}

fn place(mesh: ObjectMesh, m: &DMat4) -> ObjectMesh {
    let rot = DMat3::from_mat4(*m);
    let point = |p: [f32; 3]| m.transform_point3(Vec3::from(p).as_dvec3()).as_vec3().to_array();
    let batches: Vec<MeshBatch> = mesh
        .batches
        .into_iter()
        .map(|mut b| {
            for v in &mut b.vertices {
                v.pos = point(v.pos);
                v.normal = (rot * Vec3::from(v.normal).as_dvec3()).normalize_or_zero().as_vec3().to_array();
            }
            b
        })
        .collect();
    let edges: Vec<[f32; 3]> = mesh.edges.into_iter().map(point).collect();
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for p in batches.iter().flat_map(|b| b.vertices.iter().map(|v| v.pos)).chain(edges.iter().copied()) {
        lo = lo.min(Vec3::from(p));
        hi = hi.max(Vec3::from(p));
    }
    let (aabb_min, aabb_max) = if lo.cmple(hi).all() { (lo.to_array(), hi.to_array()) } else { ([0.0; 3], [0.0; 3]) };
    ObjectMesh { batches, edges, aabb_min, aabb_max, ..mesh }
}

fn overlay_texture(assets: &mut AssetManager, material: &str) -> Option<DVec2> {
    match assets.material_info_impl(material) {
        None => None,
        Some(info) if info.missing || info.width == 0 || info.height == 0 => Some(DVec2::new(64.0, 64.0)),
        Some(info) => Some(DVec2::new(f64::from(info.width), f64::from(info.height))),
    }
}

pub(crate) fn place_decor(
    assets: &mut AssetManager,
    map: &Map,
    decor: &LocalDecor,
    at: &DecorPlacement<'_>,
    out: &mut PlacedDecor,
) {
    let fgd = assets.fgd.clone();
    for item in &decor.items {
        let index = match item {
            DecorItem::Overlay { index, .. } | DecorItem::Decal { index, .. } => *index,
        };
        let Some(source) = map.entities.get(index) else { continue };
        let mut fixed = source.clone();
        if let Some(f) = at.fixup {
            f.fix_entity(&mut fixed, at.kinds);
        }
        let class = class_of(fgd.as_deref(), &fixed);
        let color = point_color(&fixed, class);
        match item {
            DecorItem::Overlay { render_order, frags, .. } => {
                if !at.overlays {
                    continue;
                }
                let Some(kind) = overlay_kind(&fixed, class) else { continue };
                let key = material_key(kind_material(&kind));
                let Some(tex) = overlay_texture(assets, &key) else {
                    out.pending = true;
                    continue;
                };
                let material: MaterialId = assets.asset_material(&key, MatKind::World);
                let mesh = fragments_mesh(frags, *render_order, tex, material, color);
                out.meshes.push(place(mesh, &at.matrix));
            }
            DecorItem::Decal { origin, .. } => {
                let Some(material) = decal_material(&fixed, class) else { continue };
                out.decals = true;
                match decal_size(assets, &material) {
                    DecalSize::Loading => out.pending = true,
                    DecalSize::Missing => {}
                    DecalSize::Ready(size, scale) => {
                        let mut layers: Vec<(&Document, DMat4)> = vec![(&decor.doc, at.matrix)];
                        if let Some(parent) = at.parent {
                            layers.push((parent, DMat4::IDENTITY));
                        }
                        let world = at.matrix.transform_point3(*origin);
                        if let Some(mesh) = project_decal_layers(&layers, world, size, scale) {
                            let mat = assets.asset_material(&material, MatKind::World);
                            out.meshes.push(mesh.object_mesh(mat, color));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHILD: &str = r#"
world
{
    "id" "1"
    "classname" "worldspawn"
    solid
    {
        "id" "2"
        side { "id" "1" "plane" "(0 0 0) (0 64 0) (64 64 0)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
        side { "id" "2" "plane" "(0 64 -16) (0 0 -16) (64 0 -16)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
        side { "id" "3" "plane" "(0 0 0) (0 0 -16) (0 64 -16)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
        side { "id" "4" "plane" "(64 64 0) (64 64 -16) (64 0 -16)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
        side { "id" "5" "plane" "(0 64 0) (0 64 -16) (64 64 -16)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
        side { "id" "6" "plane" "(64 0 0) (64 0 -16) (0 0 -16)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
    }
}
entity
{
    "id" "10"
    "classname" "info_overlay"
    "material" "decals/lambdaspray_2a"
    "sides" "1"
    "BasisOrigin" "32 32 0"
    "BasisU" "1 0 0"
    "BasisV" "0 1 0"
    "BasisNormal" "0 0 1"
    "StartU" "0"
    "EndU" "1"
    "StartV" "0"
    "EndV" "1"
    "uv0" "-8 -8 0"
    "uv1" "-8 8 0"
    "uv2" "8 8 0"
    "uv3" "8 -8 0"
    "origin" "32 32 0"
}
"#;

    #[test]
    fn instance_overlays_follow_the_instance() {
        let map = hp_vmf::parse(CHILD).expect("vmf");
        let decor = local_decor(&map, None).expect("decor");
        assert_eq!(decor.items.len(), 1, "one overlay on side 1");
        let DecorItem::Overlay { frags, .. } = &decor.items[0] else { panic!("overlay") };
        let local = frags[0].points.iter().fold(DVec3::ZERO, |a, p| a + *p) / frags[0].points.len() as f64;
        assert!((local - DVec3::new(32.0, 32.0, 0.0)).length() < 1e-6, "{local}");
        let m = DMat4::from_rotation_translation(
            glam::DQuat::from_rotation_z(90f64.to_radians()),
            DVec3::new(1000.0, 0.0, 256.0),
        );
        let mesh = fragments_mesh(frags, 0, DVec2::splat(64.0), 1, [255; 3]);
        let placed = place(mesh, &m);
        let pts: Vec<DVec3> = placed.batches[0].vertices.iter().map(|v| Vec3::from(v.pos).as_dvec3()).collect();
        let center = pts.iter().fold(DVec3::ZERO, |a, p| a + *p) / pts.len() as f64;
        assert!((center - DVec3::new(968.0, 32.0, 256.2)).length() < 1e-3, "{center}");
        for v in &placed.batches[0].vertices {
            assert!((Vec3::from(v.normal) - Vec3::Z).length() < 1e-5);
        }
    }

    #[test]
    fn instance_decals_land_on_the_parent_brushes() {
        let mut child = hp_vmf::parse(CHILD).expect("vmf");
        child.entities.clear();
        let parent = Document::from_map(hp_vmf::parse(CHILD).expect("vmf"));
        let empty = Document::from_map(child);
        let m = DMat4::from_translation(DVec3::new(0.0, 0.0, 512.0));
        let origin = DVec3::new(32.0, 32.0, 2.0);
        assert!(project_decal_layers(&[(&empty, m)], origin, DVec2::splat(16.0), 1.0).is_none());
        let mesh = project_decal_layers(&[(&empty, m), (&parent, DMat4::IDENTITY)], origin, DVec2::splat(16.0), 1.0)
            .expect("decal on the parent floor");
        assert!((mesh.normal - DVec3::Z).length() < 1e-9);
        assert!(mesh.polygons[0].iter().all(|(p, _)| (p.z - 0.2).abs() < 1e-6));
        assert!(mesh.solids.is_empty(), "only the instance's own solids are tracked");
        let own = project_decal_layers(&[(&parent, m)], origin + DVec3::Z * 512.0, DVec2::splat(16.0), 1.0)
            .expect("decal on the instance floor");
        assert!(own.polygons[0].iter().all(|(p, _)| (p.z - 512.2).abs() < 1e-6));
    }

    #[test]
    fn maps_without_overlays_build_nothing() {
        let mut map = hp_vmf::parse(CHILD).expect("vmf");
        map.entities.clear();
        assert!(local_decor(&map, None).is_none());
    }
}
