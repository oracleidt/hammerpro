use glam::{DVec2, DVec3};
use hp_app::runtime::DispSent;
use hp_meshio::{Model, Triangle};
use hp_vmf::DispInfo;

pub const TOUCHED: f64 = 0.02;
pub const SEAM_SPREAD: f64 = 0.05;
pub const UV_TOLERANCE: f64 = 2e-3;

pub fn parse_name(name: &str) -> Option<(u32, u32)> {
    let trimmed = match name.rsplit_once('.') {
        Some((head, tail)) if tail.len() == 3 && tail.chars().all(|c| c.is_ascii_digit()) => head,
        _ => name,
    };
    let rest = trimmed.strip_prefix("HPDISP_")?;
    let mut parts = rest.split('_');
    let solid = parts.next()?.parse().ok()?;
    let side = parts.next()?.parse().ok()?;
    Some((solid, side))
}

pub fn grid_indices(n: usize) -> Vec<u32> {
    let quads = n.saturating_sub(1);
    let mut idx = Vec::with_capacity(quads * quads * 6);
    for i in 0..quads {
        for j in 0..quads {
            let v = (i * n + j) as u32;
            let w = n as u32;
            if v % 2 == 1 {
                idx.extend_from_slice(&[v, v + 1, v + w, v + 1, v + w + 1, v + w]);
            } else {
                idx.extend_from_slice(&[v, v + w + 1, v + w, v, v + 1, v + w + 1]);
            }
        }
    }
    idx
}

pub fn grid_from_triangles(tris: &[Triangle], n: usize) -> Option<Vec<DVec3>> {
    if n < 3 || tris.len() != 2 * (n - 1) * (n - 1) {
        return None;
    }
    let indices = grid_indices(n);
    let mut sum = vec![DVec3::ZERO; n * n];
    let mut count = vec![0u32; n * n];
    let mut uv: Vec<Option<DVec2>> = vec![None; n * n];
    for (t, tri) in tris.iter().enumerate() {
        for k in 0..3 {
            let g = *indices.get(t * 3 + k)? as usize;
            if !tri.pos[k].is_finite() {
                return None;
            }
            sum[g] += tri.pos[k];
            count[g] += 1;
            match uv[g] {
                Some(u) if (u - tri.uv[k]).length() > UV_TOLERANCE => return None,
                Some(_) => {}
                None => uv[g] = Some(tri.uv[k]),
            }
        }
    }
    let positions: Vec<DVec3> = sum.iter().zip(&count).map(|(s, c)| *s / f64::from((*c).max(1))).collect();
    for (t, tri) in tris.iter().enumerate() {
        for k in 0..3 {
            let g = indices[t * 3 + k] as usize;
            if (tri.pos[k] - positions[g]).length() > SEAM_SPREAD {
                return None;
            }
        }
    }
    let at = |i: usize, j: usize| uv[i * n + j];
    let (u00, u10, u01) = (at(0, 0)?, at(n - 1, 0)?, at(0, n - 1)?);
    let last = (n - 1) as f64;
    for i in 0..n {
        for j in 0..n {
            let expect = u00 + (u10 - u00) * (i as f64 / last) + (u01 - u00) * (j as f64 / last);
            if (uv[i * n + j]? - expect).length() > UV_TOLERANCE * 4.0 {
                return None;
            }
        }
    }
    Some(positions)
}

#[derive(Clone, Debug, PartialEq)]
pub struct DispUpdate {
    pub solid: u32,
    pub side: u32,
    pub positions: Vec<DVec3>,
}

pub type Tri = [DVec3; 3];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub updates: Vec<DispUpdate>,
    pub conform: Vec<(u32, u32)>,
    pub conform_tris: Vec<Tri>,
    pub created: Vec<Tri>,
}

fn world(tris: &[Triangle]) -> impl Iterator<Item = Tri> + '_ {
    tris.iter().map(|t| t.pos)
}

pub fn plan(source: &Model, sent: &[DispSent]) -> Plan {
    let mut plan = Plan::default();
    let objects: Vec<(String, &[Triangle])> = if source.objects.is_empty() {
        vec![(String::new(), &source.triangles[..])]
    } else {
        source.objects.iter().map(|o| (o.name.clone(), source.object_triangles(o))).collect()
    };
    for (name, tris) in objects {
        if tris.is_empty() {
            continue;
        }
        let known =
            parse_name(&name).and_then(|(solid, side)| sent.iter().find(|s| s.solid == solid && s.side == side));
        match known {
            Some(s) => {
                let n = (1usize << s.power) + 1;
                if plan.updates.iter().any(|u| u.solid == s.solid && u.side == s.side)
                    || plan.conform.contains(&(s.solid, s.side))
                {
                    plan.created.extend(world(tris));
                    continue;
                }
                match grid_from_triangles(tris, n) {
                    Some(positions) => plan.updates.push(DispUpdate { solid: s.solid, side: s.side, positions }),
                    None => {
                        plan.conform.push((s.solid, s.side));
                        plan.conform_tris.extend(world(tris));
                    }
                }
            }
            None => plan.created.extend(world(tris)),
        }
    }
    plan
}

pub fn rebuild(
    disp: &DispInfo,
    base: &[DVec3],
    face_normal: DVec3,
    sent: &[[f32; 3]],
    new: &[DVec3],
) -> Option<(DispInfo, usize)> {
    let n = disp.row_len();
    let total = n * n;
    if base.len() != total || sent.len() != total || new.len() != total {
        return None;
    }
    let elevation = if disp.elevation.is_finite() { face_normal * disp.elevation } else { DVec3::ZERO };
    let mut out = disp.clone();
    out.distances.resize(total, 0.0);
    out.offsets.resize(total, DVec3::ZERO);
    out.normals.resize(total, face_normal);
    let mut moved = 0;
    for k in 0..total {
        let before = DVec3::new(f64::from(sent[k][0]), f64::from(sent[k][1]), f64::from(sent[k][2]));
        let p = new[k];
        if !p.is_finite() || (p - before).length() <= TOUCHED {
            continue;
        }
        let normal = out.normals[k].try_normalize().unwrap_or(face_normal);
        let rel = p - base[k] - elevation;
        let distance = rel.dot(normal);
        let lateral = rel - normal * distance;
        out.distances[k] = distance;
        out.offsets[k] = if lateral.length() < 1e-4 { DVec3::ZERO } else { lateral };
        moved += 1;
    }
    Some((out, moved))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(power: u32) -> (DispInfo, Vec<DVec3>) {
        let face = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(0.0, 256.0, 0.0),
            DVec3::new(256.0, 256.0, 0.0),
            DVec3::new(256.0, 0.0, 0.0),
        ];
        let disp = DispInfo::new_flat(power, face[0], DVec3::Z);
        let mesh = hp_geom::build_disp(&face, &disp).expect("mesh");
        (disp, mesh.positions)
    }

    fn triangles_of(disp: &DispInfo, positions: &[DVec3]) -> Vec<Triangle> {
        let n = disp.row_len();
        let face = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(0.0, 256.0, 0.0),
            DVec3::new(256.0, 256.0, 0.0),
            DVec3::new(256.0, 0.0, 0.0),
        ];
        let mesh = hp_geom::build_disp(&face, disp).expect("mesh");
        let uv = |p: DVec3| DVec2::new(p.x / 256.0, p.y / 256.0);
        assert_eq!(mesh.row_len, n);
        grid_indices(n)
            .chunks_exact(3)
            .map(|t| {
                let idx = [t[0] as usize, t[1] as usize, t[2] as usize];
                Triangle {
                    pos: idx.map(|i| positions[i]),
                    normal: [DVec3::Z; 3],
                    uv: idx.map(|i| uv(mesh.base_positions[i])),
                    material: 0,
                }
            })
            .collect()
    }

    #[test]
    fn object_names_carry_the_displacement() {
        assert_eq!(parse_name("HPDISP_12_3_p4"), Some((12, 3)));
        assert_eq!(parse_name("HPDISP_12_3_p4.001"), Some((12, 3)));
        assert_eq!(parse_name("Cube"), None);
        assert_eq!(parse_name("HPDISP_x_3"), None);
        assert_eq!(parse_name("HPDISP_7"), None);
    }

    #[test]
    fn the_grid_comes_back_from_its_triangles_and_is_validated() {
        let (disp, positions) = flat(2);
        let tris = triangles_of(&disp, &positions);
        let grid = grid_from_triangles(&tris, 5).expect("grid");
        assert_eq!(grid.len(), 25);
        for (a, b) in grid.iter().zip(&positions) {
            assert!((*a - *b).length() < 1e-9);
        }
        assert!(grid_from_triangles(&tris, 9).is_none(), "wrong size");
        assert!(grid_from_triangles(&tris[..tris.len() - 1], 5).is_none(), "missing triangle");
        let mut split = tris.clone();
        split[5].pos[1].z += 3.0;
        assert!(grid_from_triangles(&split, 5).is_none(), "a split vertex means a changed topology");
        let mut shuffled = tris.clone();
        shuffled.swap(0, 7);
        assert!(grid_from_triangles(&shuffled, 5).is_none(), "a reordered mesh is refused");
        let mut moved = tris.clone();
        for t in &mut moved {
            for p in &mut t.pos {
                if (p.x - 128.0).abs() < 1.0 && (p.y - 128.0).abs() < 1.0 {
                    p.z += 10.0;
                }
            }
        }
        let grid = grid_from_triangles(&moved, 5).expect("grid");
        assert!((grid[12].z - 10.0).abs() < 1e-9);
    }

    #[test]
    fn only_the_moved_vertices_are_rewritten() {
        let (mut disp, positions) = flat(2);
        disp.alphas[3] = 77.0;
        let face = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(0.0, 256.0, 0.0),
            DVec3::new(256.0, 256.0, 0.0),
            DVec3::new(256.0, 0.0, 0.0),
        ];
        let mesh = hp_geom::build_disp(&face, &disp).expect("mesh");
        let sent: Vec<[f32; 3]> = positions.iter().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect();
        let mut new = positions.clone();
        new[12] += DVec3::new(0.0, 0.0, 24.0);
        new[7] += DVec3::new(5.0, -3.0, 8.0);
        new[3] += DVec3::new(0.001, 0.0, -0.001);
        let (out, moved) = rebuild(&disp, &mesh.base_positions, mesh.face_normal, &sent, &new).expect("rebuild");
        assert_eq!(moved, 2, "a change below the tolerance is a rounding error, not an edit");
        assert_eq!(out.alphas, disp.alphas, "alpha is kept");
        assert_eq!(out.distances[3], disp.distances[3]);
        let built = hp_geom::build_disp(&face, &out).expect("mesh");
        for (k, p) in built.positions.iter().enumerate() {
            let want = if k == 3 { positions[3] } else { new[k] };
            assert!((*p - want).length() < 1e-9, "vertex {k}: {p} {want}");
        }
        assert!(out.offsets[7].length() > 1.0, "a sideways move goes into the offset");
        assert!(out.offsets[12].length() < 1e-9, "a vertical move is a distance");
        assert!(rebuild(&disp, &mesh.base_positions, mesh.face_normal, &sent[..4], &new).is_none());
    }

    #[test]
    fn a_plan_sorts_named_meshes_from_new_ones() {
        let (disp, positions) = flat(2);
        let tris = triangles_of(&disp, &positions);
        let mut all = tris.clone();
        let mut broken = tris.clone();
        broken.truncate(10);
        all.extend(broken.clone());
        all.extend(tris.clone());
        let n = tris.len();
        let sent = vec![
            DispSent { solid: 4, side: 1, power: 2, positions: Vec::new() },
            DispSent { solid: 4, side: 2, power: 2, positions: Vec::new() },
        ];
        let model = Model {
            triangles: all,
            materials: vec![],
            up: hp_meshio::UpAxis::Z,
            meters_per_unit: 1.0,
            objects: vec![
                hp_meshio::MeshObject { name: "HPDISP_4_1_p2.001".into(), extras: None, first: 0, count: n },
                hp_meshio::MeshObject { name: "HPDISP_4_2_p2".into(), extras: None, first: n, count: 10 },
                hp_meshio::MeshObject { name: "Sculpt".into(), extras: None, first: n + 10, count: n },
                hp_meshio::MeshObject { name: "HPDISP_9_9_p2".into(), extras: None, first: n, count: 2 },
            ],
        };
        let plan = plan(&model, &sent);
        assert_eq!(plan.updates.len(), 1);
        assert_eq!((plan.updates[0].solid, plan.updates[0].side), (4, 1));
        assert_eq!(plan.conform, [(4, 2)]);
        assert_eq!(plan.conform_tris.len(), 10);
        assert_eq!(plan.created.len(), n + 2, "an unknown mesh and a name nobody sent become new displacements");
    }
}
