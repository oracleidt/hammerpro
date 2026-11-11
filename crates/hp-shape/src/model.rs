use glam::{DMat3, DVec3};

use crate::mesh::TriMesh;

#[derive(Clone, Debug, PartialEq)]
pub struct Attachment {
    pub name: String,
    pub pos: DVec3,
    pub forward: DVec3,
    pub up: DVec3,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelShape {
    pub path: String,
    pub render: TriMesh,
    pub collision: Vec<TriMesh>,
    pub attachments: Vec<Attachment>,
    pub hull: Option<(DVec3, DVec3)>,
    pub view: Option<(DVec3, DVec3)>,
    pub surface_prop: String,
    pub static_prop: bool,
    pub mass: f32,
    pub body_groups: Vec<(String, usize)>,
    pub body: u32,
}

fn v(a: [f32; 3]) -> DVec3 {
    DVec3::new(f64::from(a[0]), f64::from(a[1]), f64::from(a[2]))
}

fn valid_box(lo: [f32; 3], hi: [f32; 3]) -> Option<(DVec3, DVec3)> {
    let (a, b) = (v(lo), v(hi));
    (a.is_finite() && b.is_finite() && a.cmple(b).all() && (b - a).max_element() > 0.0).then_some((a, b))
}

pub fn render_mesh(model: &hp_mdl::Model) -> TriMesh {
    let mut out = TriMesh::new();
    for (mi, mesh) in model.meshes.iter().enumerate() {
        if mesh.indices.is_empty() {
            continue;
        }
        let base = out.verts.len() as u32;
        out.verts.extend(mesh.vertices.iter().map(|x| v(x.pos)));
        for t in mesh.indices.chunks_exact(3) {
            if t.iter().any(|&i| i as usize >= mesh.vertices.len()) {
                continue;
            }
            let facing = t.iter().map(|&i| v(mesh.vertices[i as usize].normal)).sum::<DVec3>();
            let tri = [t[0] + base, t[1] + base, t[2] + base];
            if facing.length_squared() > 1e-12 {
                out.add_tri_facing(tri, facing, mi as u32);
            } else {
                out.add_tri(tri, mi as u32);
            }
        }
    }
    out
}

pub fn collision_pieces(c: &hp_mdl::CollisionModel) -> Vec<TriMesh> {
    let mut out = Vec::new();
    for solid in &c.solids {
        let ranges: Vec<std::ops::Range<usize>> = if solid.ledges.is_empty() {
            std::iter::once(0..solid.triangles.len()).collect()
        } else {
            solid.ledges.clone()
        };
        for r in ranges {
            let Some(tris) = solid.triangles.get(r) else { continue };
            let mut piece = TriMesh::new();
            let mut map: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
            let mut local: Vec<[u32; 3]> = Vec::new();
            for t in tris {
                if t.iter().any(|&i| i as usize >= solid.vertices.len()) {
                    continue;
                }
                local.push(t.map(|i| *map.entry(i).or_insert_with(|| piece.add_vertex(v(solid.vertices[i as usize])))));
            }
            if local.is_empty() {
                continue;
            }
            let centroid = piece.verts.iter().copied().sum::<DVec3>() / piece.verts.len() as f64;
            for t in local {
                let c = (piece.verts[t[0] as usize] + piece.verts[t[1] as usize] + piece.verts[t[2] as usize]) / 3.0;
                piece.add_tri_facing(t, c - centroid, out.len() as u32);
            }
            piece.closed = true;
            if !piece.is_empty() {
                out.push(piece);
            }
        }
    }
    out
}

fn bone_to_model(bone: &hp_mdl::Bone) -> (DMat3, DVec3) {
    let m = bone.pose_to_bone;
    let rot = DMat3::from_cols(
        DVec3::new(f64::from(m[0][0]), f64::from(m[1][0]), f64::from(m[2][0])),
        DVec3::new(f64::from(m[0][1]), f64::from(m[1][1]), f64::from(m[2][1])),
        DVec3::new(f64::from(m[0][2]), f64::from(m[1][2]), f64::from(m[2][2])),
    );
    let t = DVec3::new(f64::from(m[0][3]), f64::from(m[1][3]), f64::from(m[2][3]));
    let inv = rot.transpose();
    (inv, -(inv * t))
}

pub fn attachments(model: &hp_mdl::Model) -> Vec<Attachment> {
    let mats: Vec<(DMat3, DVec3)> = model.bones.iter().map(bone_to_model).collect();
    let mut out = Vec::new();
    for a in &model.attachments {
        let Some((rot, pos)) = usize::try_from(a.bone).ok().and_then(|b| mats.get(b)) else { continue };
        let l = a.local;
        let local_pos = DVec3::new(f64::from(l[0][3]), f64::from(l[1][3]), f64::from(l[2][3]));
        let col = |k: usize| DVec3::new(f64::from(l[0][k]), f64::from(l[1][k]), f64::from(l[2][k]));
        let p = *rot * local_pos + *pos;
        let forward = (*rot * col(0)).try_normalize().unwrap_or(DVec3::X);
        let up = (*rot * col(2)).try_normalize().unwrap_or(DVec3::Z);
        if p.is_finite() {
            out.push(Attachment { name: a.name.clone(), pos: p, forward, up });
        }
    }
    out
}

pub fn load_shape(path: &str, mut read: impl FnMut(&str) -> Option<Vec<u8>>, body: u32) -> Result<ModelShape, String> {
    let files = hp_mdl::read_files(path, &mut read).map_err(|e| e.to_string())?;
    let opts =
        hp_mdl::LoadOptions { body_groups: hp_mdl::BodyGroupSelect::Body(body), ..hp_mdl::LoadOptions::default() };
    let model = files.load(&opts).map_err(|e| e.to_string())?;
    let render = render_mesh(&model);
    if render.is_empty() {
        return Err(format!("{path} has no triangles"));
    }
    let collision = read(&hp_mdl::phy_path(path))
        .and_then(|phy| hp_mdl::parse_phy_for_model(&phy, &files.mdl, hp_mdl::Pose::FirstSequence).ok())
        .map(|c| collision_pieces(&c))
        .unwrap_or_default();
    Ok(ModelShape {
        path: path.to_owned(),
        attachments: attachments(&model),
        hull: valid_box(model.hull_min, model.hull_max),
        view: valid_box(model.view_bb_min, model.view_bb_max),
        surface_prop: model.surface_prop.clone(),
        static_prop: model.is_static_prop,
        mass: model.mass,
        body_groups: model.body_parts.iter().map(|b| (b.name.clone(), b.models.len())).collect(),
        body,
        render,
        collision,
    })
}

impl ModelShape {
    pub fn collision_mesh(&self) -> TriMesh {
        let mut out = TriMesh::new();
        for p in &self.collision {
            out.append(p);
        }
        out
    }
}
