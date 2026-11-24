use glam::DVec3;

use crate::smd::Collision;
use crate::{MeshObject, Model};

pub use hp_geom::{MAX_COLLISION_PIECES, MAX_HULL_VERTICES, MIN_HULL_VERTICES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionLimits {
    pub max_pieces: usize,
    pub max_hull_vertices: usize,
}

impl Default for CollisionLimits {
    fn default() -> Self {
        Self { max_pieces: 16, max_hull_vertices: 32 }
    }
}

impl CollisionLimits {
    pub fn clamped(self) -> Self {
        Self {
            max_pieces: self.max_pieces.clamp(1, MAX_COLLISION_PIECES),
            max_hull_vertices: self.max_hull_vertices.clamp(MIN_HULL_VERTICES, MAX_HULL_VERTICES),
        }
    }

    pub fn options(self) -> hp_geom::DecomposeOptions {
        let limits = self.clamped();
        hp_geom::DecomposeOptions {
            max_pieces: limits.max_pieces,
            max_hull_vertices: limits.max_hull_vertices,
            ..hp_geom::DecomposeOptions::default()
        }
    }
}

pub fn is_collision_object(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    let name = name.rsplit(['|', ':', '/']).next().unwrap_or(&name);
    ["ucx_", "ubx_", "ucp_", "usp_", "col_", "collision", "collider", "phys_"].iter().any(|p| name.starts_with(p))
        || ["_col", "_coll", "_collision", "_collider", "_phys", "_physics"].iter().any(|s| name.ends_with(s))
        || name == "phys"
}

fn triangles_of(model: &Model, objects: &[&MeshObject]) -> Vec<[DVec3; 3]> {
    objects.iter().flat_map(|o| model.object_triangles(o).iter().map(|t| t.pos)).collect()
}

fn merge_closest(mut pieces: Vec<Vec<DVec3>>, max_pieces: usize, max_vertices: usize) -> Vec<Vec<DVec3>> {
    let center = |p: &[DVec3]| p.iter().copied().sum::<DVec3>() / p.len().max(1) as f64;
    while pieces.len() > max_pieces.max(1) {
        let centers: Vec<DVec3> = pieces.iter().map(|p| center(p)).collect();
        let mut best = (0, 1, f64::INFINITY);
        for i in 0..centers.len() {
            for j in i + 1..centers.len() {
                let d = centers[i].distance_squared(centers[j]);
                if d < best.2 {
                    best = (i, j, d);
                }
            }
        }
        let other = pieces.swap_remove(best.1);
        let mut merged = std::mem::take(&mut pieces[best.0]);
        merged.extend(other);
        pieces[best.0] = hp_geom::limited_hull(&merged, max_vertices).unwrap_or(merged);
    }
    pieces
}

pub fn own_collision(model: &Model, limits: CollisionLimits) -> Option<(Model, Vec<Vec<DVec3>>)> {
    let limits = limits.clamped();
    let own: Vec<&MeshObject> = model.objects.iter().filter(|o| o.count > 0 && is_collision_object(&o.name)).collect();
    if own.is_empty() {
        return None;
    }
    let mut taken = vec![false; model.triangles.len()];
    for o in &own {
        let end = o.first.saturating_add(o.count).min(taken.len());
        for slot in taken.iter_mut().take(end).skip(o.first.min(end)) {
            *slot = true;
        }
    }
    if taken.iter().all(|t| *t) {
        return None;
    }
    let pieces: Vec<Vec<DVec3>> = own
        .iter()
        .filter_map(|o| {
            let points: Vec<DVec3> = triangles_of(model, &[o]).into_iter().flatten().collect();
            hp_geom::limited_hull(&points, limits.max_hull_vertices)
        })
        .collect();
    if pieces.is_empty() {
        return None;
    }
    let pieces = merge_closest(pieces, limits.max_pieces, limits.max_hull_vertices);
    let mut render = model.clone();
    render.triangles.clear();
    render.objects.clear();
    for o in &model.objects {
        if own.iter().any(|x| std::ptr::eq(*x, o)) {
            continue;
        }
        let first = render.triangles.len();
        render.triangles.extend_from_slice(model.object_triangles(o));
        render.objects.push(MeshObject { first, count: render.triangles.len() - first, ..o.clone() });
    }
    let covered: usize = model.objects.iter().map(|o| o.count).sum();
    if covered < model.triangles.len() {
        for (index, tri) in model.triangles.iter().enumerate() {
            if !taken[index] && !model.objects.iter().any(|o| index >= o.first && index < o.first + o.count) {
                render.triangles.push(*tri);
            }
        }
    }
    Some((render, pieces))
}

pub fn own_collision_count(model: &Model) -> usize {
    model.objects.iter().filter(|o| o.count > 0 && is_collision_object(&o.name)).count()
}

pub fn generated(model: &Model, kind: Collision, limits: CollisionLimits) -> Option<Vec<Vec<DVec3>>> {
    let limits = limits.clamped();
    let triangles: Vec<[DVec3; 3]> = model.triangles.iter().map(|t| t.pos).collect();
    match kind {
        Collision::Hull => {
            let points: Vec<DVec3> = triangles.iter().flatten().copied().collect();
            Some(hp_geom::limited_hull(&points, limits.max_hull_vertices).into_iter().collect())
        }
        Collision::Decompose => Some(hp_geom::decompose(&triangles, limits.options())),
        Collision::Pieces | Collision::Box | Collision::None => None,
    }
}

#[derive(Clone, Debug, Default)]
pub struct CollisionPlan {
    pub render: Option<Model>,
    pub pieces: Option<Vec<Vec<DVec3>>>,
    pub own: bool,
}

pub fn plan(model: &Model, kind: Collision, limits: CollisionLimits, use_own: bool) -> CollisionPlan {
    if use_own && let Some((render, pieces)) = own_collision(model, limits) {
        return CollisionPlan { render: Some(render), pieces: Some(pieces), own: true };
    }
    CollisionPlan { render: None, pieces: generated(model, kind, limits), own: false }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Material, Triangle, UpAxis};
    use glam::DVec2;

    fn cube(lo: DVec3, hi: DVec3) -> Vec<Triangle> {
        let c = |i: usize| {
            DVec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            )
        };
        let quads = [[0, 2, 6, 4], [1, 5, 7, 3], [0, 4, 5, 1], [2, 3, 7, 6], [0, 1, 3, 2], [4, 6, 7, 5]];
        quads
            .iter()
            .flat_map(|q| {
                [[q[0], q[1], q[2]], [q[0], q[2], q[3]]].map(|k| Triangle {
                    pos: k.map(c),
                    normal: [DVec3::Z; 3],
                    uv: [DVec2::ZERO; 3],
                    material: 0,
                })
            })
            .collect()
    }

    fn with_objects(parts: &[(&str, Vec<Triangle>)]) -> Model {
        let mut model = Model {
            materials: vec![Material::named("m")],
            up: UpAxis::Z,
            meters_per_unit: 1.0 / crate::UNITS_PER_METER,
            ..Model::default()
        };
        for (name, tris) in parts {
            let first = model.triangles.len();
            model.triangles.extend_from_slice(tris);
            model.objects.push(MeshObject { name: (*name).to_owned(), extras: None, first, count: tris.len() });
        }
        model
    }

    #[test]
    fn collision_objects_are_recognised_by_their_names() {
        for name in
            ["UCX_Chair_00", "ubx_box", "Chair_col", "collision", "Collider_Seat", "seat_PHYS", "Armature|UCX_a"]
        {
            assert!(is_collision_object(name), "{name}");
        }
        for name in ["Chair", "colour_panel", "physics_book_cover_render", "column"] {
            assert!(!is_collision_object(name), "{name}");
        }
    }

    #[test]
    fn the_models_own_collision_is_split_from_the_render_mesh() {
        let model = with_objects(&[
            ("Chair", cube(DVec3::ZERO, DVec3::splat(10.0))),
            ("UCX_Chair_00", cube(DVec3::ZERO, DVec3::new(10.0, 10.0, 5.0))),
            ("UCX_Chair_01", cube(DVec3::new(0.0, 0.0, 5.0), DVec3::new(10.0, 2.0, 20.0))),
        ]);
        assert_eq!(own_collision_count(&model), 2);
        let (render, pieces) = own_collision(&model, CollisionLimits::default()).unwrap();
        assert_eq!(render.triangles.len(), 12);
        assert_eq!(render.objects.len(), 1);
        assert_eq!(pieces.len(), 2);
        assert!(pieces.iter().all(|p| p.len() == 8));
        let capped = own_collision(&model, CollisionLimits { max_pieces: 1, max_hull_vertices: 8 }).unwrap().1;
        assert_eq!(capped.len(), 1);
        let only = with_objects(&[("UCX_a", cube(DVec3::ZERO, DVec3::ONE))]);
        assert!(own_collision(&only, CollisionLimits::default()).is_none(), "a model that is only collision keeps it");
        let plain = with_objects(&[("Chair", cube(DVec3::ZERO, DVec3::ONE))]);
        let planned = plan(&plain, Collision::Hull, CollisionLimits::default(), true);
        assert!(!planned.own && planned.render.is_none());
        assert_eq!(planned.pieces.unwrap().len(), 1, "no own collision falls back to the chosen mode");
        assert!(plan(&plain, Collision::Pieces, CollisionLimits::default(), true).pieces.is_none());
    }
}
