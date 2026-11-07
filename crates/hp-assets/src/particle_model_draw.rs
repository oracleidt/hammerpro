use std::sync::{Arc, Weak};

use ahash::AHashMap;
use hp_app::state::ParticleModelDraw;
use hp_render::{ModelId, ModelInstance, ModelPart, Renderer, Vertex};

use crate::AssetManager;
use crate::material::MatKind;
use crate::models::{ModelEntry, PreparedModel, model_data_key};

pub(crate) const ANIMATION_KEY: &str = "@hp-particle-bind";

pub(crate) struct AnimationSource {
    pub mdl: Arc<Vec<u8>>,
    pub ani: Option<Arc<Vec<u8>>>,
    pub bones: Vec<hp_mdl::Bone>,
    pub meshes: Vec<hp_mdl::ModelMesh>,
    pub sequences: Vec<hp_mdl::Sequence>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_render::*;

    #[test]
    fn native_model_bodygroups_skins_sequences_and_activities_survive_asset_loading() {
        let Some((device, queue)) = crate::test_gpu::gpu() else { return };
        let mut renderer = Renderer::new(&device, &queue);
        renderer.set_tone_settings(ToneSettings { bloom: 0.0, ..Default::default() });
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
        let fs = hp_fs::GameFs::from_source_mounts(
            None,
            None,
            &[hp_fs::SourceMount {
                kind: hp_fs::SourceMountKind::Gma,
                label: "Owned native particle model variants".into(),
                path: folder.join("native_particle_model_variants.gma"),
                enabled: true,
                origin: hp_fs::SourceOrigin::Custom,
            }],
        )
        .unwrap();
        let path = "models/hp_modelanim117/probe.mdl";
        let normal = Arc::new(crate::models::model_job(&fs, path).unwrap());
        let key = model_data_key(path, Some(ANIMATION_KEY));
        let animated = Arc::new(crate::models::model_job(&fs, &key).unwrap());
        assert_eq!(normal.info.sequence_frames, vec![1, 5]);
        assert_eq!(animated.particle_animation.as_ref().unwrap().sequences[1].activity, "ACT_IDLE");
        let mut assets = AssetManager::new(&hp_app::settings::Settings::default());
        assets.models.map.insert(path.into(), ModelEntry::Ready(normal));
        assets.models.map.insert(key, ModelEntry::Ready(animated));
        for name in ["hp_modelanim117/gray", "hp_modelanim117/red"] {
            let material = fs.material(name).unwrap();
            let analysis = crate::material::analyze(name, &material, &|name| fs.texture_flags(name).unwrap_or(0));
            for key in crate::material::texture_keys(MatKind::Model, &analysis) {
                if assets.mats.textures.map.contains_key(&key) {
                    continue;
                }
                let texture = crate::texture::prepare_texture(&fs, &key, false).unwrap();
                let id = renderer.create_texture_exact(&texture.upload);
                assets.mats.textures.map.insert(
                    key,
                    crate::texture::TexEntry {
                        current: Some((id, texture.size)),
                        loading: false,
                        queued: false,
                        missing: false,
                    },
                );
            }
            assets.mats.analyses.insert(
                name.into(),
                crate::registry::AnaEntry { value: Some(Some(Arc::new(analysis))), ..Default::default() },
            );
        }
        assets.fs = Some(fs);
        let report: serde_json::Value =
            serde_json::from_str(include_str!("../tests/data/native_particle_model_variants.json")).unwrap();
        let definitions =
            hp_pcf::parse_file(include_bytes!("../tests/data/native_particle_model_variants.pcf")).unwrap();
        let proof = folder.join("../../../../target/hp-assets/model-particles");
        std::fs::create_dir_all(&proof).unwrap();
        let mut checked = 0;
        let mut maximum = 0;
        let mut static_added = false;
        let mut transform = None;
        let mut cached = Vec::new();
        for definition in definitions {
            let name = definition.name.strip_prefix("hp_modelanim117_").unwrap();
            let plan = Arc::new(hp_pcf::Plan::new(&definition));
            assert!(plan.unsupported.is_empty());
            let mut effect = hp_pcf::Effect::new(&definition.name, &|_| Some(plan.clone()), 17).unwrap();
            effect.step(0.02, [0.0, 0.0, 64.0], 1);
            let p = &effect.sims[0].1.particles[0];
            let model = &plan.model_renderers[0];
            let draw = ParticleModelDraw {
                entity: 17,
                draw_order: 0,
                model: model.path().into(),
                skin: model.skin,
                body_group: p.sequence as i32,
                sequence: p.sequence2 as i32,
                activity: model.activity.as_str().into(),
                animation_time: effect.sims[0].1.time(),
                animation_rate: model.rate(p),
                transform: model.transform(p),
                tint: [1.0; 4],
            };
            transform.get_or_insert(draw.transform);
            let instances = assets.particle_draw_models_impl(&mut renderer, std::slice::from_ref(&draw));
            assert_eq!(instances.len(), 1, "{name}");
            let again = assets.particle_draw_models_impl(&mut renderer, &[draw]);
            assert_eq!(instances[0].model, again[0].model, "identical poses reuse GPU geometry");
            cached.push(instances[0].model);
            if !static_added {
                renderer.set_model_instances(&instances);
                static_added = true;
            }
            assets.mats.apply_dirty(&mut renderer);
            let eye = std::array::from_fn(|i| report["cases"][name]["eye"][i].as_f64().unwrap() as f32);
            let params = ViewParams {
                size: [128; 2],
                game_pixel_grid: true,
                kind: ViewKind::Perspective {
                    eye,
                    forward: [0.0, 1.0, 0.0],
                    up: [0.0, 0.0, 1.0],
                    fov_y_deg: 60.0,
                    near: 1.0,
                    far: 2000.0,
                },
                mode: RenderMode::Unshaded,
                grid: None,
                background: [0.0; 3],
                selection_color: [0.0; 4],
                show_sprites: false,
                show_models: false,
                msaa: false,
            };
            let target =
                image::open(folder.join(format!("native_particle_model_variants_{name}.png"))).unwrap().to_rgb8();
            for preview in [false, true] {
                if preview {
                    renderer.render_particle_preview_with_models(1, &params, &[], &instances);
                } else {
                    renderer.set_particle_models(&instances);
                    renderer.render_view(1, &params, &Overlay::default());
                }
                let (_, _, pixels) = renderer.read_view_rgba8(1).unwrap();
                image::save_buffer(
                    proof.join(format!("{name}_{preview}.png")),
                    &pixels,
                    128,
                    128,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                let delta = pixels
                    .chunks_exact(4)
                    .zip(target.pixels())
                    .flat_map(|(a, b)| (0..3).map(move |c| a[c].abs_diff(b[c])))
                    .max()
                    .unwrap();
                assert!(delta <= 3, "{name} preview={preview}: native RGB delta {delta}");
                maximum = maximum.max(delta);
                checked += 1;
            }
        }
        let poses: serde_json::Value =
            serde_json::from_str(include_str!("../tests/data/native_particle_model_pose.json")).unwrap();
        for (name, pose) in poses["cases"].as_object().unwrap() {
            let cycle = pose["cycle"].as_f64().unwrap() as f32;
            let draws = assets.particle_draw_models_impl(
                &mut renderer,
                &[ParticleModelDraw {
                    entity: 17,
                    draw_order: 0,
                    model: path.into(),
                    skin: 0,
                    body_group: 0,
                    sequence: 1,
                    activity: "".into(),
                    animation_time: cycle * 4.0,
                    animation_rate: 1.0,
                    transform: transform.unwrap(),
                    tint: [1.0; 4],
                }],
            );
            assert_eq!(draws.len(), 1);
            assets.mats.apply_dirty(&mut renderer);
            cached.push(draws[0].model);
            let params = ViewParams {
                size: [128; 2],
                game_pixel_grid: true,
                kind: ViewKind::Perspective {
                    eye: [10.0, -256.0, 64.0],
                    forward: [0.0, 1.0, 0.0],
                    up: [0.0, 0.0, 1.0],
                    fov_y_deg: 60.0,
                    near: 1.0,
                    far: 2000.0,
                },
                mode: RenderMode::Unshaded,
                grid: None,
                background: [0.0; 3],
                selection_color: [0.0; 4],
                show_sprites: false,
                show_models: false,
                msaa: false,
            };
            let target = image::open(folder.join(format!("native_particle_model_pose_{name}.png"))).unwrap().to_rgb8();
            for preview in [false, true] {
                if preview {
                    renderer.render_particle_preview_with_models(1, &params, &[], &draws);
                } else {
                    renderer.set_particle_models(&draws);
                    renderer.render_view(1, &params, &Overlay::default());
                }
                let (_, _, pixels) = renderer.read_view_rgba8(1).unwrap();
                image::save_buffer(
                    proof.join(format!("{name}_{preview}.png")),
                    &pixels,
                    128,
                    128,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                let delta = pixels
                    .chunks_exact(4)
                    .zip(target.pixels())
                    .flat_map(|(a, b)| (0..3).map(move |c| a[c].abs_diff(b[c])))
                    .max()
                    .unwrap();
                assert!(delta <= 3, "{name} preview={preview}: native pose RGB delta {delta}");
                maximum = maximum.max(delta);
                checked += 1;
            }
        }
        assert_eq!(checked, 26);
        let parked = renderer.park_scene();
        assert_eq!(parked.model_instances(), 1, "particle draws never replace scene model instances");
        assert_eq!(renderer.particle_model_count(), 0);
        assets.shared.frame.fetch_add(3, std::sync::atomic::Ordering::Relaxed);
        assets.collect_particle_draw_models(&mut renderer);
        assert!(assets.particle_draws.poses.is_empty());
        assert!(cached.iter().all(|id| !renderer.has_model(*id)), "expired poses release GPU geometry");
        eprintln!("26 native model variant/pose images through asset loading; maximum RGB delta {maximum}/255");
    }
}

impl AnimationSource {
    pub(crate) fn bytes(&self) -> usize {
        self.mdl.len()
            + self.ani.as_ref().map_or(0, |ani| ani.len())
            + self.bones.len() * size_of::<hp_mdl::Bone>()
            + self
                .meshes
                .iter()
                .map(|mesh| {
                    mesh.vertices.len() * size_of::<hp_mdl::ModelVertex>()
                        + mesh.bone_weights.len() * size_of::<hp_mdl::BoneWeight>()
                        + mesh.indices.len() * size_of::<u32>()
                        + mesh.tangents.len() * size_of::<[f32; 4]>()
                })
                .sum::<usize>()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct PoseKey {
    path: String,
    skin: u32,
    body: u32,
    sequence: usize,
    frame: usize,
    fraction: u32,
}

struct Posed {
    id: ModelId,
    source: Weak<PreparedModel>,
    materials: Vec<u32>,
    used: u64,
}

#[derive(Default)]
pub(crate) struct DrawCache {
    mount: u64,
    poses: AHashMap<PoseKey, Posed>,
}

impl AssetManager {
    pub(crate) fn collect_particle_draw_models(&mut self, renderer: &mut Renderer) {
        let frame = self.shared.current_frame();
        let same_mount = self.particle_draws.mount == self.mount_gen;
        self.particle_draws.poses.retain(|_, pose| {
            if same_mount && frame.saturating_sub(pose.used) <= 1 && pose.source.strong_count() > 0 {
                true
            } else {
                renderer.destroy_transient_model(pose.id);
                false
            }
        });
        self.particle_draws.mount = self.mount_gen;
    }

    pub(crate) fn particle_draw_models_impl(
        &mut self,
        renderer: &mut Renderer,
        models: &[ParticleModelDraw],
    ) -> Vec<ModelInstance> {
        models
            .iter()
            .filter_map(|draw| {
                let path = crate::util::model_key(&draw.model);
                let model = match self.particle_draw_model(renderer, &path, draw) {
                    Some(id) => id,
                    None if matches!(self.models.map.get(&path), Some(ModelEntry::Failed(_))) => {
                        self.render_model(renderer, "models/error.mdl", 0)?
                    }
                    None => return None,
                };
                Some(ModelInstance { model, transform: draw.transform, object: 0, tint: draw.tint, fade: [0.0; 2] })
            })
            .collect()
    }

    fn particle_draw_model(
        &mut self,
        renderer: &mut Renderer,
        path: &str,
        draw: &ParticleModelDraw,
    ) -> Option<ModelId> {
        let data = self.model_data(path)?;
        let skin = u32::try_from(draw.skin).unwrap_or(0);
        let skin = if (skin as usize) < data.skin_count() { skin } else { 0 };
        // Source sets bodygroup `sequence_number` to value 1; sequence_number 1
        // independently selects the model animation.
        let body = usize::try_from(draw.body_group)
            .ok()
            .and_then(|index| data.body_parts.get(index))
            .filter(|(_, count)| *count > 1)
            .map_or(0, |(base, _)| u32::try_from(*base).unwrap_or(0));
        let animated = draw.sequence != 0
            || !draw.activity.is_empty()
            || data.info.sequence_frames.iter().any(|frames| *frames > 1);
        if !animated {
            return self.render_model_for(renderer, path, skin, body, None);
        }
        let key = model_data_key(path, Some(ANIMATION_KEY));
        let data = self.model_data(&key)?;
        let source = data.particle_animation.as_ref()?;
        let sequence = source
            .sequences
            .iter()
            .position(|sequence| !draw.activity.is_empty() && sequence.activity.eq_ignore_ascii_case(&draw.activity))
            .or_else(|| usize::try_from(draw.sequence).ok().filter(|index| *index < source.sequences.len()))
            .unwrap_or(0);
        let clip = hp_mdl::animation_clip(&source.mdl, sequence)?;
        let cycle = hp_pcf::model_render::ModelRender::animation_cycle(
            draw.animation_time,
            draw.animation_rate,
            clip.frames.saturating_sub(1),
        );
        let position = cycle.clamp(0.0, 1.0) * clip.frames.saturating_sub(1) as f32;
        let frame = position.floor() as usize;
        let fraction = position - frame as f32;
        let pose_key = PoseKey { path: path.into(), skin, body, sequence, frame, fraction: fraction.to_bits() };
        if let Some(pose) = self.particle_draws.poses.get_mut(&pose_key)
            && pose.source.ptr_eq(&Arc::downgrade(&data))
        {
            pose.used = self.shared.current_frame();
            let (id, materials) = (pose.id, pose.materials.clone());
            for material in materials {
                self.touch_material(material);
            }
            return Some(id);
        }
        if let Some(old) = self.particle_draws.poses.remove(&pose_key) {
            renderer.destroy_transient_model(old.id);
        }
        let matrices = hp_mdl::skin_matrices_with(
            &source.mdl,
            source.ani.as_deref().map(Vec::as_slice),
            &source.bones,
            sequence,
            frame,
            fraction,
        )?;
        let choices = data.body_choices(body);
        let mut parts = Vec::new();
        for mesh in &source.meshes {
            if choices.get(mesh.body_part).copied().unwrap_or(0) as usize != mesh.model_index || mesh.indices.is_empty()
            {
                continue;
            }
            let material = self.asset_material(data.part_material(skin as usize, mesh.skin_ref), MatKind::Model);
            let mut vertices = Vec::with_capacity(mesh.vertices.len());
            let mut tangents = Vec::with_capacity(mesh.vertices.len());
            for (index, vertex) in mesh.vertices.iter().enumerate() {
                let tangent = mesh.tangents.get(index).copied();
                let (pos, normal, tangent) = match mesh.bone_weights.get(index) {
                    Some(weight) => {
                        hp_mdl::pose_vertex_with_tangent(&matrices, weight, vertex.pos, vertex.normal, tangent)
                    }
                    None => (vertex.pos, vertex.normal, tangent),
                };
                vertices.push(Vertex { pos, normal, uv: vertex.uv, alpha: 1.0 });
                if let Some(tangent) = tangent {
                    tangents.push(tangent);
                }
            }
            parts.push(ModelPart { material, vertices, indices: mesh.indices.clone(), tangents });
        }
        if parts.is_empty() {
            return None;
        }
        let materials = parts.iter().map(|part| part.material).collect();
        let id = renderer.create_model(&parts);
        self.particle_draws.poses.insert(
            pose_key,
            Posed { id, source: Arc::downgrade(&data), materials, used: self.shared.current_frame() },
        );
        Some(id)
    }
}
