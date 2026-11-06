#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{DMat4, DVec3};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Missing ragdoll asset: {0}")]
    Missing(String),
    #[error("Invalid ragdoll metadata: {0}")]
    Metadata(String),
    #[error(transparent)]
    Model(#[from] hp_mdl::MdlError),
}

pub struct Body {
    pub index: usize,
    pub bone: usize,
    pub parts: Vec<Vec<DVec3>>,
    pub mass: f64,
    pub damping: f64,
    pub angular_damping: f64,
    pub surface: String,
}

pub struct Joint {
    pub parent: usize,
    pub child: usize,
    pub limits: [[f64; 2]; 3],
    pub friction: [f64; 3],
}

pub struct Rig {
    pub model: Arc<hp_mdl::Model>,
    pub bodies: Vec<Body>,
    pub joints: Vec<Joint>,
    pub disabled_collisions: HashSet<(usize, usize)>,
    order: Vec<usize>,
    bind: Vec<DMat4>,
    initial: Vec<DMat4>,
    local: Vec<DMat4>,
    body_order: Vec<usize>,
}

pub fn matrix(rows: &hp_mdl::SkinMatrix) -> DMat4 {
    DMat4::from_cols_array(&[
        rows[0][0] as f64,
        rows[1][0] as f64,
        rows[2][0] as f64,
        0.0,
        rows[0][1] as f64,
        rows[1][1] as f64,
        rows[2][1] as f64,
        0.0,
        rows[0][2] as f64,
        rows[1][2] as f64,
        rows[2][2] as f64,
        0.0,
        rows[0][3] as f64,
        rows[1][3] as f64,
        rows[2][3] as f64,
        1.0,
    ])
}

fn rows(matrix: DMat4) -> hp_mdl::SkinMatrix {
    let m = matrix.as_mat4().to_cols_array_2d();
    std::array::from_fn(|r| std::array::from_fn(|c| m[c][r]))
}

pub fn metadata(bytes: &[u8]) -> Option<hp_kv::Block> {
    let read = |offset: usize| {
        bytes.get(offset..offset.checked_add(4)?).and_then(|b| <[u8; 4]>::try_from(b).ok()).map(i32::from_le_bytes)
    };
    let mut offset = usize::try_from(read(0)?).ok()?.max(16);
    let count = usize::try_from(read(8)?).ok().filter(|n| *n <= 1024)?;
    for _ in 0..count {
        let size = usize::try_from(read(offset)?).ok()?;
        offset = offset.checked_add(4)?.checked_add(size)?;
    }
    let bytes = bytes.get(offset..)?;
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    hp_kv::parse_bytes(&bytes[..end]).ok()
}

fn number(block: &hp_kv::Block, key: &str, default: f64) -> f64 {
    block.get_parsed::<f64>(key).filter(|n| n.is_finite()).unwrap_or(default)
}

impl Rig {
    pub fn load(path: &str, mut read: impl FnMut(&str) -> Option<Vec<u8>>, body: u32) -> Result<Self, Error> {
        let mdl = read(path).ok_or_else(|| Error::Missing(path.to_owned()))?;
        let phy_path = hp_mdl::phy_path(path);
        let phy = read(&phy_path).ok_or(Error::Missing(phy_path))?;
        let collision = hp_mdl::parse_phy(&phy)?;
        let info = metadata(&phy).ok_or_else(|| Error::Metadata(path.to_owned()))?;
        let model = hp_mdl::load_from(
            path,
            &mut read,
            &hp_mdl::LoadOptions {
                pose: hp_mdl::Pose::Bind,
                bone_weights: true,
                body_groups: hp_mdl::BodyGroupSelect::Body(body),
                ..Default::default()
            },
        )?;
        let bind: Vec<_> = model.bones.iter().map(|b| matrix(&b.pose_to_bone).inverse()).collect();
        if bind.iter().any(|m| !m.is_finite()) {
            return Err(Error::Metadata(path.to_owned()));
        }
        let ani = hp_mdl::animation_block_path(&mdl).and_then(|path| read(&path));
        let sequence =
            model.sequences.iter().position(|s| s.activity.eq_ignore_ascii_case("ACT_DIERAGDOLL")).unwrap_or(0);
        let skin = hp_mdl::skin_matrices_with(&mdl, ani.as_deref(), &model.bones, sequence, 0, 0.0);
        let initial: Vec<_> = bind
            .iter()
            .enumerate()
            .map(|(i, bind)| {
                skin.as_ref().and_then(|matrices| matrices.get(i)).map_or(*bind, |skin| matrix(skin) * *bind)
            })
            .collect();
        let mut done = vec![false; model.bones.len()];
        let mut order = Vec::new();
        while order.len() < model.bones.len() {
            let before = order.len();
            for (index, bone) in model.bones.iter().enumerate() {
                if !done[index] && (bone.parent < 0 || done.get(bone.parent as usize).copied().unwrap_or(false)) {
                    order.push(index);
                    done[index] = true;
                }
            }
            if before == order.len() {
                return Err(Error::Metadata("cyclic bone hierarchy".into()));
            }
        }
        let local = model
            .bones
            .iter()
            .enumerate()
            .map(|(i, bone)| {
                usize::try_from(bone.parent)
                    .ok()
                    .and_then(|p| initial.get(p))
                    .map_or(initial[i], |parent| parent.inverse() * initial[i])
            })
            .collect();
        let mut bodies = Vec::new();
        for block in info.children.iter().filter(|b| b.name.eq_ignore_ascii_case("solid")) {
            let Some(index) = block.get_parsed::<usize>("index").filter(|i| *i < collision.solids.len()) else {
                continue;
            };
            let name = block.get("name").unwrap_or("");
            let Some(bone) = model.bones.iter().position(|b| b.name.eq_ignore_ascii_case(name)) else {
                return Err(Error::Metadata(format!("unknown collision bone {name}")));
            };
            let solid = &collision.solids[index];
            let all = solid.ledges.is_empty().then_some(&solid.triangles[..]);
            let pieces = solid.ledges.iter().filter_map(|range| solid.triangles.get(range.clone()));
            let parts = all
                .into_iter()
                .chain(pieces)
                .map(|triangles| {
                    triangles
                        .iter()
                        .flat_map(|t| t.iter())
                        .filter_map(|i| solid.vertices.get(*i as usize))
                        .map(|p| DVec3::from(p.map(f64::from)))
                        .collect()
                })
                .collect();
            bodies.push(Body {
                index,
                bone,
                parts,
                mass: number(block, "mass", 10.0).max(0.001),
                damping: number(block, "damping", 0.01).max(0.0),
                angular_damping: number(block, "rotdamping", 1.5).max(0.0),
                surface: block.get("surfaceprop").unwrap_or("default").to_owned(),
            });
        }
        bodies.sort_by_key(|body| body.index);
        if bodies.is_empty() || bodies.iter().enumerate().any(|(i, body)| body.index != i) {
            return Err(Error::Metadata("non-contiguous collision bodies".into()));
        }
        let mut joints = Vec::new();
        for block in info.children.iter().filter(|b| b.name.eq_ignore_ascii_case("ragdollconstraint")) {
            let Some(parent) = block.get_parsed::<usize>("parent").filter(|i| *i < bodies.len()) else { continue };
            let Some(child) = block.get_parsed::<usize>("child").filter(|i| *i < bodies.len() && *i != parent) else {
                continue;
            };
            joints.push(Joint {
                parent,
                child,
                limits: ["x", "y", "z"].map(|axis| {
                    [
                        number(block, &format!("{axis}min"), 0.0).to_radians(),
                        number(block, &format!("{axis}max"), 0.0).to_radians(),
                    ]
                }),
                friction: ["x", "y", "z"].map(|axis| number(block, &format!("{axis}friction"), 0.0).max(0.0)),
            });
        }
        let mut disabled_collisions = HashSet::new();
        for block in info.children.iter().filter(|b| b.name.eq_ignore_ascii_case("collisionrules")) {
            let allow_self = block.get("selfcollisions") != Some("0");
            let allowed: HashSet<_> = block
                .get_all("collisionpair")
                .filter_map(|text| {
                    let (a, b) = text.split_once(',')?;
                    let (a, b) = (a.trim().parse::<usize>().ok()?, b.trim().parse::<usize>().ok()?);
                    Some((a.min(b), a.max(b)))
                })
                .collect();
            for a in 0..bodies.len() {
                for b in a + 1..bodies.len() {
                    if !allow_self || !allowed.contains(&(a, b)) {
                        disabled_collisions.insert((a, b));
                    }
                }
            }
        }
        if !info.children.iter().any(|b| b.name.eq_ignore_ascii_case("collisionrules")) {
            disabled_collisions.extend(joints.iter().map(|j| (j.parent.min(j.child), j.parent.max(j.child))));
        }
        let mut body_order = Vec::new();
        let mut visited = vec![false; bodies.len()];
        while body_order.len() < bodies.len() {
            let before = body_order.len();
            for body in &bodies {
                let index = body.index;
                if !visited[index] && joints.iter().filter(|j| j.child == index).all(|j| visited[j.parent]) {
                    visited[index] = true;
                    body_order.push(index);
                }
            }
            if body_order.len() == before {
                return Err(Error::Metadata("cyclic physical hierarchy".into()));
            }
        }
        Ok(Self {
            model: Arc::new(model),
            bodies,
            joints,
            disabled_collisions,
            order,
            bind,
            initial,
            local,
            body_order,
        })
    }

    pub fn body_order(&self) -> &[usize] {
        &self.body_order
    }

    pub fn body_poses(&self, entity: DMat4, overrides: &str) -> Vec<DMat4> {
        let mut poses: Vec<_> = self.bodies.iter().map(|body| entity * self.initial[body.bone]).collect();
        let mut tokens = overrides.split(',');
        while let (Some(index), Some(angles)) = (tokens.next(), tokens.next()) {
            let Some(index) = index.trim().parse::<usize>().ok().filter(|i| *i < poses.len()) else { continue };
            let angles: Vec<_> = angles.split_whitespace().filter_map(|v| v.parse::<f64>().ok()).collect();
            if angles.len() != 3 || angles.iter().any(|v| !v.is_finite()) {
                continue;
            }
            let parent = self.joints.iter().find(|joint| joint.child == index).map(|joint| joint.parent);
            let position = if let Some(parent) = parent {
                let offset = self.bind[self.bodies[parent].bone].inverse() * self.bind[self.bodies[index].bone];
                poses[parent].transform_point3(offset.w_axis.truncate())
            } else {
                entity.w_axis.truncate()
            };
            poses[index] = DMat4::from_scale_rotation_translation(
                entity.to_scale_rotation_translation().0,
                hp_geom::angles_to_quat(DVec3::from_slice(&angles)),
                position,
            );
        }
        poses
    }

    pub fn joint_frames(&self, joint: &Joint, scale: f64) -> (DMat4, DMat4) {
        let mut frame = self.bind[self.bodies[joint.parent].bone].inverse() * self.bind[self.bodies[joint.child].bone];
        frame.w_axis = (frame.w_axis.truncate() * scale).extend(1.0);
        (frame, DMat4::IDENTITY)
    }

    pub fn skin(&self, entity: DMat4, poses: &[DMat4]) -> Vec<hp_mdl::SkinMatrix> {
        let physics: HashMap<_, _> = self.bodies.iter().zip(poses).map(|(body, pose)| (body.bone, *pose)).collect();
        let mut world = vec![DMat4::IDENTITY; self.model.bones.len()];
        for &bone in &self.order {
            world[bone] = physics.get(&bone).copied().unwrap_or_else(|| {
                usize::try_from(self.model.bones[bone].parent)
                    .ok()
                    .and_then(|parent| world.get(parent))
                    .map_or(entity * self.local[bone], |parent| *parent * self.local[bone])
            });
        }
        world.into_iter().zip(&self.model.bones).map(|(world, bone)| rows(world * matrix(&bone.pose_to_bone))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_physics_headers_are_rejected_without_panicking() {
        for size in 0..100 {
            let mut bytes = vec![0; size];
            assert!(metadata(&bytes).is_none_or(|root| root.children.is_empty()));
            bytes.fill(255);
            assert!(metadata(&bytes).is_none());
        }
    }

    fn installed_rig() -> Option<Rig> {
        let game = std::env::var_os("HAMMERPRO_GMOD_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| "D:/SteamLibrary/steamapps/common/GarrysMod".into());
        let path = game.join("sourceengine/hl2_misc_dir.vpk");
        if !path.is_file() {
            eprintln!("skipping: installed Source models unavailable");
            return None;
        }
        let archive = hp_vpk::VpkArchive::open(path).unwrap();
        Some(Rig::load("models/humans/group01/male_07.mdl", |path| archive.read(path).unwrap(), 0).unwrap())
    }

    #[test]
    fn installed_ragdoll_keeps_real_bodies_masses_limits_and_skin_weights() {
        let Some(rig) = installed_rig() else { return };
        assert_eq!(rig.bodies.len(), 15);
        assert_eq!(rig.joints.len(), 14);
        assert!((rig.bodies.iter().map(|body| body.mass).sum::<f64>() - f64::from(rig.model.mass)).abs() < 0.01);
        assert!(rig.bodies.iter().all(|body| !body.parts.is_empty() && body.parts.iter().all(|part| part.len() >= 4)));
        assert!(rig.joints.iter().all(|joint| joint.limits.iter().all(|limit| limit[0] <= limit[1])));
        assert!(rig.model.meshes.iter().all(|mesh| mesh.vertices.len() == mesh.bone_weights.len()));
        let poses = rig.body_poses(DMat4::IDENTITY, "");
        assert!(rig.skin(DMat4::IDENTITY, &poses).iter().flatten().flatten().all(|v| v.is_finite()));
    }

    #[test]
    fn world_angle_overrides_reconstruct_joint_origins_and_animated_skin() {
        let Some(rig) = installed_rig() else { return };
        let entity = DMat4::from_translation(DVec3::new(100.0, 200.0, 300.0));
        let poses = rig.body_poses(entity, "0,0 90 0,1,0 0 0");
        assert!(poses[0].w_axis.truncate().distance(entity.w_axis.truncate()) < 1e-8);
        let joint = rig.joints.iter().find(|j| j.child == 1).unwrap();
        let (frame, _) = rig.joint_frames(joint, 1.0);
        assert!(poses[1].w_axis.truncate().distance(poses[0].transform_point3(frame.w_axis.truncate())) < 1e-8);
        let mut moved = poses.clone();
        moved[1] = DMat4::from_translation(DVec3::X * 10.0) * moved[1];
        let before = rig.skin(entity, &poses);
        let after = rig.skin(entity, &moved);
        let bone = rig.bodies[1].bone;
        assert!((after[bone][0][3] - before[bone][0][3] - 10.0).abs() < 0.001);
        assert_eq!(after[rig.bodies[0].bone], before[rig.bodies[0].bone]);
    }
}
