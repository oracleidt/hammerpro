use std::collections::HashMap;

use glam::{DVec2, DVec3};
use hp_app::runtime::DispSent;
use hp_meshio::{Material, MeshObject, Model, Triangle, UNITS_PER_METER, UpAxis};

use super::{ObjInput, axis_coord, is_tool};

impl ObjInput {
    pub fn to_model(&self) -> Model {
        let mut triangles: Vec<Triangle> = Vec::new();
        let mut slots: HashMap<u32, usize> = HashMap::new();
        let mut materials: Vec<Material> = Vec::new();
        for solid in &self.solids {
            for face in &solid.geom.faces {
                let Some(side) = solid.sides.get(face.side_index) else { continue };
                let Some((name, size)) = self.materials.get(side.material as usize) else { continue };
                if is_tool(name) || face.vertices.len() < 3 {
                    continue;
                }
                let slot = *slots.entry(side.material).or_insert_with(|| {
                    materials.push(Material::named(name.clone()));
                    materials.len() - 1
                });
                let uv = |p: DVec3| DVec2::new(axis_coord(&side.u, p) / size[0], axis_coord(&side.v, p) / size[1]);
                match solid.disp.get(face.side_index).and_then(Option::as_ref) {
                    Some(mesh) => {
                        let count = mesh.positions.len();
                        for tri in mesh.indices.chunks_exact(3) {
                            let idx = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
                            if idx.iter().any(|&i| i >= count || i >= mesh.base_positions.len()) {
                                continue;
                            }
                            let normal = |i: usize| mesh.normals.get(i).copied().unwrap_or(DVec3::Z);
                            triangles.push(Triangle {
                                pos: idx.map(|i| mesh.positions[i]),
                                normal: idx.map(normal),
                                uv: idx.map(|i| uv(mesh.base_positions[i])),
                                material: slot,
                            });
                        }
                    }
                    None => {
                        let normal = face.normal();
                        for t in face.triangles() {
                            let pos = [face.vertices[t[0]], face.vertices[t[1]], face.vertices[t[2]]];
                            triangles.push(Triangle { pos, normal: [normal; 3], uv: pos.map(uv), material: slot });
                        }
                    }
                }
            }
        }
        Model { triangles, materials, up: UpAxis::Z, meters_per_unit: 1.0 / UNITS_PER_METER, objects: Vec::new() }
    }

    pub fn to_displacement_model(&self) -> (Model, Vec<DispSent>) {
        let mut triangles: Vec<Triangle> = Vec::new();
        let mut slots: HashMap<u32, usize> = HashMap::new();
        let mut materials: Vec<Material> = Vec::new();
        let mut objects: Vec<MeshObject> = Vec::new();
        let mut sent: Vec<DispSent> = Vec::new();
        for solid in &self.solids {
            for face in &solid.geom.faces {
                let Some(side) = solid.sides.get(face.side_index) else { continue };
                let Some((name, size)) = self.materials.get(side.material as usize) else { continue };
                let Some(mesh) = solid.disp.get(face.side_index).and_then(Option::as_ref) else { continue };
                let n = mesh.row_len;
                if n < 3 || mesh.positions.len() != n * n || mesh.base_positions.len() != n * n {
                    continue;
                }
                let power = (n - 1).trailing_zeros();
                let slot = *slots.entry(side.material).or_insert_with(|| {
                    materials.push(Material::named(name.clone()));
                    materials.len() - 1
                });
                let uv = |p: DVec3| DVec2::new(axis_coord(&side.u, p) / size[0], axis_coord(&side.v, p) / size[1]);
                let first = triangles.len();
                for tri in mesh.indices.chunks_exact(3) {
                    let idx = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
                    let normal = |i: usize| mesh.normals.get(i).copied().unwrap_or(DVec3::Z);
                    triangles.push(Triangle {
                        pos: idx.map(|i| mesh.positions[i]),
                        normal: idx.map(normal),
                        uv: idx.map(|i| uv(mesh.base_positions[i])),
                        material: slot,
                    });
                }
                objects.push(MeshObject {
                    name: format!("HPDISP_{}_{}_p{power}", solid.id, face.side_index),
                    extras: Some(serde_json::json!({
                        "hammerpro": {
                            "kind": "displacement",
                            "solid": solid.id,
                            "side": face.side_index,
                            "power": power,
                            "material": name,
                        }
                    })),
                    first,
                    count: triangles.len() - first,
                });
                sent.push(DispSent {
                    solid: solid.id,
                    side: face.side_index as u32,
                    power,
                    positions: mesh.positions.iter().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect(),
                });
            }
        }
        let model = Model { triangles, materials, up: UpAxis::Z, meters_per_unit: 1.0 / UNITS_PER_METER, objects };
        (model, sent)
    }
}
