use std::sync::Arc;

use ahash::{AHashMap, AHashSet};
use glam::DVec3;
use hp_app::runtime::render::RenderDisplay;
use hp_app::settings::StaticPropShadows;
use hp_render::{ModelId, Renderer};

use crate::AssetManager;
use crate::models::{ANIM_SEPARATOR, ModelEntry, PreparedModel};

pub(crate) type HullTriangles = Arc<Vec<[[f32; 3]; 3]>>;

pub(crate) struct ShadowHulls {
    tx: crossbeam_channel::Sender<(String, Option<HullTriangles>)>,
    rx: crossbeam_channel::Receiver<(String, Option<HullTriangles>)>,
    loaded: AHashMap<String, Option<HullTriangles>>,
    pending: AHashSet<String>,
    applied: AHashSet<ModelId>,
}

impl Default for ShadowHulls {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self { tx, rx, loaded: AHashMap::new(), pending: AHashSet::new(), applied: AHashSet::new() }
    }
}

fn collision_hull(fs: &hp_fs::GameFs, path: &str) -> Vec<[[f32; 3]; 3]> {
    let Some(phy) = fs.read(&hp_mdl::phy_path(path)) else { return Vec::new() };
    let model = match fs.read(path) {
        Some(mdl) => hp_mdl::parse_phy_for_model(&phy, &mdl, hp_mdl::Pose::FirstSequence),
        None => hp_mdl::parse_phy(&phy),
    };
    let Ok(model) = model else { return Vec::new() };
    let Some(solid) = model.solids.first() else { return Vec::new() };
    solid
        .piece_triangles()
        .filter_map(|t| {
            Some([
                *solid.vertices.get(t[0] as usize)?,
                *solid.vertices.get(t[1] as usize)?,
                *solid.vertices.get(t[2] as usize)?,
            ])
        })
        .collect()
}

fn mesh_hulls(model: &PreparedModel) -> Vec<[[f32; 3]; 3]> {
    let mut out = Vec::new();
    for part in model.parts.iter().chain(&model.variant_parts) {
        let pts: Vec<DVec3> =
            part.vertices.iter().map(|v| DVec3::from(v.pos.map(f64::from))).filter(|p| p.is_finite()).collect();
        let Ok(hull) = hp_geom::convex_hull(&pts) else { continue };
        for f in &hull.faces {
            let poly = hull.face_polygon(f);
            let Some((&p0, rest)) = poly.split_first() else { continue };
            for w in rest.windows(2) {
                out.push([p0, w[0], w[1]].map(|p| p.as_vec3().to_array()));
            }
        }
    }
    out
}

fn box_hull(lo: DVec3, hi: DVec3) -> Vec<[[f32; 3]; 3]> {
    let c = |i: usize| {
        [
            if i & 1 != 0 { hi.x } else { lo.x } as f32,
            if i & 2 != 0 { hi.y } else { lo.y } as f32,
            if i & 4 != 0 { hi.z } else { lo.z } as f32,
        ]
    };
    let faces = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    faces.iter().flat_map(|f| [[c(f[0]), c(f[1]), c(f[2])], [c(f[0]), c(f[2]), c(f[3])]]).collect()
}

pub(crate) fn shadow_hull(
    fs: Option<&hp_fs::GameFs>,
    key: &str,
    prepared: Option<&PreparedModel>,
) -> Option<HullTriangles> {
    let path = key.split_once(ANIM_SEPARATOR).map_or(key, |(p, _)| p);
    let mut tris = fs.map(|fs| collision_hull(fs, path)).unwrap_or_default();
    if tris.is_empty()
        && let Some(m) = prepared
    {
        tris = mesh_hulls(m);
        if tris.is_empty()
            && let (Some(lo), Some(hi)) = (m.info.hull_min, m.info.hull_max)
        {
            tris = box_hull(lo, hi);
        }
    }
    (!tris.is_empty()).then(|| Arc::new(tris))
}

impl AssetManager {
    pub(crate) fn sync_shadow_hulls(&mut self, renderer: &mut Renderer, display: &RenderDisplay) {
        let h = &mut self.scene.shadow_hulls;
        while let Ok((key, tris)) = h.rx.try_recv() {
            h.pending.remove(&key);
            h.loaded.insert(key, tris);
        }
        if display.prop_shadows != StaticPropShadows::Collision {
            return;
        }
        let models: Vec<(String, ModelId)> = self
            .models
            .render
            .iter()
            .filter_map(|((path, _), m)| Some((path.clone(), m.id?)))
            .chain(self.models.variants.iter().filter_map(|((key, _, _), (m, _))| Some((key.clone(), m.id?))))
            .filter(|(_, id)| !self.scene.shadow_hulls.applied.contains(id))
            .collect();
        for (key, id) in models {
            let h = &mut self.scene.shadow_hulls;
            if let Some(tris) = h.loaded.get(&key) {
                if let Some(t) = tris {
                    renderer.set_model_shadow_hull(id, t);
                }
                h.applied.insert(id);
                continue;
            }
            let prepared = match self.models.map.get(&key) {
                Some(ModelEntry::Ready(m)) => Some(Arc::clone(m)),
                Some(ModelEntry::Failed(_)) => None,
                Some(ModelEntry::Loading | ModelEntry::Cached(_)) | None => continue,
            };
            if !h.pending.insert(key.clone()) {
                continue;
            }
            let tx = h.tx.clone();
            let fs = self.fs.clone();
            rayon::spawn(move || {
                let tris = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    shadow_hull(fs.as_ref(), &key, prepared.as_deref())
                }))
                .unwrap_or_default();
                let _ = tx.send((key, tris));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_render::Vertex;

    #[test]
    fn models_without_a_collision_model_cast_their_mesh_hulls() {
        let v = |x: f32, y: f32, z: f32| Vertex { pos: [x, y, z], ..Default::default() };
        let part = crate::models::PreparedPart {
            skin_ref: 0,
            body_part: 0,
            choice: 0,
            vertices: vec![v(0.0, 0.0, 0.0), v(10.0, 0.0, 0.0), v(0.0, 10.0, 0.0), v(0.0, 0.0, 10.0), v(2.0, 2.0, 2.0)],
            indices: vec![0, 1, 2],
            tangents: Vec::new(),
            color_index: Vec::new(),
        };
        let flat = crate::models::PreparedPart {
            skin_ref: 0,
            body_part: 0,
            choice: 1,
            vertices: vec![v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0)],
            indices: Vec::new(),
            tangents: Vec::new(),
            color_index: Vec::new(),
        };
        let model = PreparedModel {
            info: Arc::new(hp_app::state::ModelInfo {
                hull_min: Some(DVec3::splat(-1.0)),
                hull_max: Some(DVec3::splat(1.0)),
                ..Default::default()
            }),
            parts: vec![part],
            variant_parts: vec![flat],
            body_parts: Vec::new(),
            skin_materials: Vec::new(),
            bounds: ([0.0; 3], [10.0; 3]),
            helpers: crate::models::PreviewHelpers::default(),
            lod_switch: Vec::new(),
        };
        let tris = shadow_hull(None, "models/a.mdl", Some(&model)).expect("hull");
        assert_eq!(tris.len(), 4, "a tetrahedron, the inner point and the flat part dropped");
        assert!(tris.iter().flatten().all(|p| p.iter().all(|c| (0.0..=10.0).contains(c))));
        let boxed = PreparedModel { parts: Vec::new(), variant_parts: Vec::new(), ..model };
        assert_eq!(shadow_hull(None, "models/a.mdl|idle", Some(&boxed)).expect("box").len(), 12);
        let none = PreparedModel { info: Arc::new(hp_app::state::ModelInfo::default()), ..boxed };
        assert!(shadow_hull(None, "models/a.mdl", Some(&none)).is_none());
        assert!(shadow_hull(None, "models/a.mdl", None).is_none());
    }
}
