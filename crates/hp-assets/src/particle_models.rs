use std::collections::HashMap;
use std::sync::Arc;

use glam::{DMat4, DVec3};
use hp_app::state::ParticleHitbox as ModelHitbox;
use hp_app::state::ParticleModelSnapshot;

use crate::AssetManager;
use crate::models::{ModelEntry, entity_matrix, model_data_key};

fn matrix(value: hp_mdl::SkinMatrix) -> DMat4 {
    DMat4::from_cols_array(&[
        value[0][0] as f64,
        value[1][0] as f64,
        value[2][0] as f64,
        0.0,
        value[0][1] as f64,
        value[1][1] as f64,
        value[2][1] as f64,
        0.0,
        value[0][2] as f64,
        value[1][2] as f64,
        value[2][2] as f64,
        0.0,
        value[0][3] as f64,
        value[1][3] as f64,
        value[2][3] as f64,
        1.0,
    ])
}

fn rows(matrix: DMat4) -> [[f32; 4]; 3] {
    let cols = matrix.as_mat4().to_cols_array_2d();
    std::array::from_fn(|row| [cols[0][row], cols[1][row], cols[2][row], cols[3][row]])
}

impl AssetManager {
    pub(crate) fn particle_model_snapshot_impl(
        &mut self,
        entities: &[(u32, hp_vmf::Entity)],
        editor: bool,
    ) -> ParticleModelSnapshot {
        let mut result = ParticleModelSnapshot::default();
        for (id, entity) in entities {
            let Some(path) = entity.get("model").filter(|path| path.to_ascii_lowercase().ends_with(".mdl")) else {
                continue;
            };
            let path = crate::util::model_key(path);
            let key = model_data_key(&path, entity.get("DefaultAnim").or_else(|| entity.get("defaultanim")));
            if self.fs.is_none() && self.status != hp_app::state::AssetStatus::Mounting {
                continue;
            }
            let Some(data) = self.model_data(&key) else {
                if !matches!(self.models.map.get(&key), Some(ModelEntry::Failed(_))) {
                    result.pending.insert(*id);
                }
                continue;
            };
            let scale = entity
                .get("modelscale")
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|value| value.is_finite() && *value > 0.0)
                .unwrap_or(1.0);
            let mut transform = entity_matrix(
                entity.origin().filter(|v| v.is_finite()).unwrap_or(DVec3::ZERO),
                entity.angles().filter(|v| v.is_finite()).unwrap_or(DVec3::ZERO),
                scale,
            );
            if editor {
                if let Some(delta) = self.play_transforms.get(id) {
                    transform = *delta * transform;
                }
                if let Some(delta) = self.physics_transforms.get(&hp_editor::ObjRef::Entity(*id)) {
                    transform = *delta * transform;
                }
            }
            let skin = editor.then(|| self.ragdolls.bones.get(id)).flatten();
            let mut sets = HashMap::new();
            for (index, set) in data.particle_sets.iter().enumerate() {
                let boxes: Arc<[ModelHitbox]> = set
                    .hitboxes
                    .iter()
                    .filter_map(|hitbox| {
                        let bone = usize::try_from(hitbox.bone).ok()?;
                        let posed = if let Some(skin) = skin {
                            matrix(*skin.get(bone)?) * matrix(*data.bind_bones.get(bone)?)
                        } else {
                            transform * matrix(*data.particle_bones.get(bone)?)
                        };
                        Some(ModelHitbox { minimum: hitbox.min, maximum: hitbox.max, transform: rows(posed) })
                    })
                    .collect::<Vec<_>>()
                    .into();
                if index == 0 {
                    sets.insert(String::new(), Arc::clone(&boxes));
                }
                sets.insert(set.name.to_ascii_lowercase(), boxes);
            }
            result.models.insert(*id, sets);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn center(hitbox: &ModelHitbox) -> [f32; 3] {
        let p: [f32; 3] = std::array::from_fn(|axis| (hitbox.minimum[axis] + hitbox.maximum[axis]) * 0.5);
        std::array::from_fn(|axis| {
            hitbox.transform[axis][0] * p[0]
                + hitbox.transform[axis][1] * p[1]
                + hitbox.transform[axis][2] * p[2]
                + hitbox.transform[axis][3]
        })
    }

    #[test]
    fn snapshots_keep_named_sets_native_bones_and_live_physics_transforms() {
        let mut assets = AssetManager::new(&hp_app::settings::Settings::default());
        assets.fs = Some(hp_fs::GameFs::empty());
        let bones: Vec<hp_mdl::SkinMatrix> = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../hp-mdl/tests/data/hitbox_native_bones.json"
        )))
        .unwrap();
        let data = crate::models::PreparedModel {
            particle_sets: vec![
                hp_mdl::HitboxSet {
                    name: "default".into(),
                    hitboxes: vec![hp_mdl::Hitbox { min: [-16.0; 3], max: [16.0; 3], ..Default::default() }],
                },
                hp_mdl::HitboxSet {
                    name: "effects".into(),
                    hitboxes: vec![hp_mdl::Hitbox {
                        min: [4.0, 14.0, 24.0],
                        max: [16.0, 26.0, 36.0],
                        ..Default::default()
                    }],
                },
            ],
            particle_bones: bones.clone(),
            bind_bones: bones,
            ..Default::default()
        };
        assets.models.map.insert("models/hp_parity/hitboxes.mdl".into(), ModelEntry::Ready(Arc::new(data)));
        let mut entity = hp_vmf::Entity { id: 17, classname: "prop_dynamic".into(), ..Default::default() };
        entity.set("model", "models/hp_parity/hitboxes.mdl");
        entity.set("origin", "0 0 0");
        let requests = vec![(17, entity)];
        let check = |snapshot: &ParticleModelSnapshot, expected: [f32; 3]| {
            assert!(snapshot.pending.is_empty());
            let point = center(&snapshot.hitboxes(17, "effects").unwrap()[0]);
            for axis in 0..3 {
                assert!((point[axis] - expected[axis]).abs() < 0.0001, "{point:?} vs {expected:?}");
            }
            let fallback = center(&snapshot.hitboxes(17, "missing").unwrap()[0]);
            let expected_fallback = [expected[0] + 20.0, expected[1] - 10.0, expected[2] - 30.0];
            for axis in 0..3 {
                assert!((fallback[axis] - expected_fallback[axis]).abs() < 0.0001);
            }
        };
        check(&assets.particle_model_snapshot_impl(&requests, true), [-20.0, 10.0, 30.0]);
        assets
            .physics_transforms
            .insert(hp_editor::ObjRef::Entity(17), DMat4::from_translation(DVec3::new(100.0, 0.0, 0.0)));
        check(&assets.particle_model_snapshot_impl(&requests, true), [80.0, 10.0, 30.0]);
        check(&assets.particle_model_snapshot_impl(&requests, false), [-20.0, 10.0, 30.0]);
        assets
            .ragdolls
            .bones
            .insert(17, Arc::new(vec![[[1.0, 0.0, 0.0, 100.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]]; 2]));
        let point = center(&assets.particle_model_snapshot_impl(&requests, true).hitboxes(17, "effects").unwrap()[0]);
        assert!((point[0] - 80.0).abs() < 0.0001, "ragdoll pose must not receive the entity transform twice");
    }

    #[test]
    fn missing_models_finish_loading_and_unrelated_entities_do_not_request_models() {
        let mut assets = AssetManager::new(&hp_app::settings::Settings::default());
        assets.fs = Some(hp_fs::GameFs::empty());
        let mut entity = hp_vmf::Entity { id: 17, classname: "prop_dynamic".into(), ..Default::default() };
        assert!(assets.particle_model_snapshot_impl(&[(17, entity.clone())], true).pending.is_empty());
        entity.set("model", "models/missing.mdl");
        assert!(assets.particle_model_snapshot_impl(&[(17, entity.clone())], true).pending.contains(&17));
        assets.models.map.insert("models/missing.mdl".into(), ModelEntry::Failed("not found".into()));
        let snapshot = assets.particle_model_snapshot_impl(&[(17, entity)], true);
        assert!(snapshot.pending.is_empty());
        assert!(snapshot.models.is_empty());
    }
}
