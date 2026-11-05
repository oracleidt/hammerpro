use std::sync::{Arc, Weak};

use ahash::AHashMap;
use hp_render::{ModelId, ModelLod, ModelPart, Renderer};

use crate::AssetManager;
use crate::material::MatKind;
use crate::models::{ANIM_SEPARATOR, ModelEntry, PreparedModel, PreparedPart};
use crate::queue::Lane;

pub(crate) struct PreparedLods {
    pub switch: Vec<f32>,
    pub lods: Vec<Vec<PreparedPart>>,
}

impl PreparedLods {
    pub(crate) fn geometry_bytes(&self) -> usize {
        self.lods
            .iter()
            .flatten()
            .map(|part| {
                part.vertices.capacity() * size_of::<hp_render::Vertex>()
                    + part.indices.capacity() * size_of::<u32>()
                    + part.tangents.capacity() * size_of::<[f32; 4]>()
                    + part.color_index.capacity() * size_of::<u32>()
            })
            .sum()
    }
}

#[derive(Default)]
pub(crate) struct Lods {
    pub on: bool,
    pub requested: AHashMap<String, Weak<PreparedModel>>,
    pub data: AHashMap<String, (Weak<PreparedModel>, Arc<PreparedLods>)>,
    pub ready: Vec<String>,
}

pub(crate) fn lod_job(fs: &hp_fs::GameFs, key: &str) -> Result<PreparedLods, String> {
    let (path, anim) = match key.split_once(ANIM_SEPARATOR) {
        Some((p, a)) => (p, Some(a)),
        None => (key, None),
    };
    let load = |lod: u32| -> Result<hp_mdl::Model, String> {
        let opts = hp_mdl::LoadOptions { lod, body_groups: hp_mdl::BodyGroupSelect::All, ..Default::default() };
        let model = hp_mdl::load_from(path, |p| fs.read(p), &opts).map_err(|e| e.to_string())?;
        if let Some(anim) = anim
            && let Some(index) = model.sequences.iter().position(|s| s.name.eq_ignore_ascii_case(anim))
            && model.posed_sequence != Some(index)
        {
            let posed = hp_mdl::LoadOptions { pose: hp_mdl::Pose::Sequence(index), ..opts };
            if let Ok(m) = hp_mdl::load_from(path, |p| fs.read(p), &posed) {
                return Ok(m);
            }
        }
        Ok(model)
    };
    let first = load(1)?;
    let count = first.num_lods as usize;
    let switch = first.lod_switch_points.clone();
    let mut lods = Vec::new();
    if count > 1 {
        lods.push(parts_of(&first));
        for lod in 2..count {
            lods.push(parts_of(&load(lod as u32)?));
        }
    }
    Ok(PreparedLods { switch, lods })
}

fn parts_of(model: &hp_mdl::Model) -> Vec<PreparedPart> {
    model
        .meshes
        .iter()
        .filter(|m| !m.indices.is_empty())
        .map(|m| PreparedPart {
            skin_ref: m.skin_ref,
            body_part: m.body_part,
            choice: m.model_index,
            vertices: m
                .vertices
                .iter()
                .map(|v| hp_render::Vertex { pos: v.pos, normal: v.normal, uv: v.uv, alpha: 1.0 })
                .collect(),
            indices: m.indices.clone(),
            tangents: Vec::new(),
            color_index: m.color_index.clone(),
        })
        .collect()
}

fn switch_point(switch: &[f32], i: usize) -> f32 {
    switch.get(i).copied().filter(|v| v.is_finite()).unwrap_or(0.0)
}

impl AssetManager {
    pub(crate) fn set_model_lods_impl(&mut self, on: bool) {
        self.lods.on |= on;
    }

    pub(crate) fn request_model_lods(&mut self) {
        if !self.lods.on {
            return;
        }
        let Some(env) = self.shared.env() else { return };
        let requested = &self.lods.requested;
        let wanted: Vec<(String, Weak<PreparedModel>)> = self
            .models
            .map
            .iter()
            .filter_map(|(k, m)| match m {
                ModelEntry::Ready(d) if d.lod_switch.len() > 1 => {
                    let fresh = requested.get(k).is_some_and(|w| w.ptr_eq(&Arc::downgrade(d)));
                    (!fresh).then(|| (k.clone(), Arc::downgrade(d)))
                }
                _ => None,
            })
            .collect();
        for (key, made_from) in wanted {
            self.lods.requested.insert(key.clone(), made_from);
            self.shared.push(Lane::Low, crate::jobs::Job::ModelLods { key, env: env.clone() });
        }
    }

    pub(crate) fn on_model_lods(&mut self, key: String, data: Result<Arc<PreparedLods>, String>) {
        match data {
            Ok(lods) => {
                let Some(made_from) = self.lods.requested.get(&key).cloned() else { return };
                self.lods.data.insert(key.clone(), (made_from, lods));
                self.lods.ready.push(key);
            }
            Err(e) => log::debug!("model LODs {key}: {e}"),
        }
    }

    fn lods_of(&self, key: &str, data: &Arc<PreparedModel>) -> Option<Arc<PreparedLods>> {
        let (made_from, lods) = self.lods.data.get(key)?;
        (made_from.ptr_eq(&Arc::downgrade(data)) && !lods.lods.is_empty()).then(|| Arc::clone(lods))
    }

    pub(crate) fn render_lods(
        &mut self,
        key: &str,
        data: &Arc<PreparedModel>,
        skin: u32,
        body: Option<u32>,
        first: Vec<ModelPart>,
    ) -> Result<Vec<ModelLod>, Vec<ModelPart>> {
        let Some(lods) = self.lods_of(key, data) else { return Err(first) };
        let choices = body.map(|b| data.body_choices(b));
        let shown = |p: &PreparedPart| match &choices {
            Some(c) => c.get(p.body_part).copied().unwrap_or(0) as usize == p.choice,
            None => p.choice == 0,
        };
        let mut out = vec![ModelLod { switch_point: switch_point(&lods.switch, 0), parts: first }];
        for (i, parts) in lods.lods.iter().enumerate() {
            let parts = parts
                .iter()
                .filter(|p| shown(p))
                .map(|p| {
                    let name = data.part_material(skin as usize, p.skin_ref).to_owned();
                    ModelPart {
                        material: self.asset_material(&name, MatKind::Model),
                        vertices: p.vertices.clone(),
                        indices: p.indices.clone(),
                        tangents: p.tangents.clone(),
                    }
                })
                .collect();
            out.push(ModelLod { switch_point: switch_point(&lods.switch, i + 1), parts });
        }
        Ok(out)
    }

    pub(crate) fn create_render_model(
        &mut self,
        renderer: &mut Renderer,
        key: &str,
        data: &Arc<PreparedModel>,
        skin: u32,
        body: Option<u32>,
        parts: Vec<ModelPart>,
    ) -> ModelId {
        match self.render_lods(key, data, skin, body, parts) {
            Ok(lods) => renderer.create_model_lods(&lods),
            Err(parts) => renderer.create_model(&parts),
        }
    }

    pub(crate) fn apply_model_lods(&mut self, renderer: &mut Renderer) {
        if self.lods.ready.is_empty() {
            return;
        }
        let ready = std::mem::take(&mut self.lods.ready);
        for key in ready {
            let Some(ModelEntry::Ready(data)) = self.models.map.get(&key) else { continue };
            let data = Arc::clone(data);
            let defaults: Vec<(u32, ModelId)> = self
                .models
                .render
                .iter()
                .filter(|((path, _), _)| *path == key)
                .filter_map(|((_, skin), m)| Some((*skin, m.id?)))
                .collect();
            let variants: Vec<(u32, u32, ModelId)> = self
                .models
                .variants
                .iter()
                .filter(|((k, _, _), (_, made_from))| *k == key && made_from.ptr_eq(&Arc::downgrade(&data)))
                .filter_map(|((_, skin, body), (m, _))| Some((*skin, *body, m.id?)))
                .collect();
            let looks = defaults.into_iter().map(|(s, id)| (s, None, id));
            let looks = looks.chain(variants.into_iter().map(|(s, b, id)| (s, Some(b), id))).collect::<Vec<_>>();
            for (skin, body, id) in looks {
                let first: Vec<ModelPart> = match body {
                    Some(b) => data.parts_for_body(b).into_iter(),
                    None => data.parts.iter().collect::<Vec<_>>().into_iter(),
                }
                .map(|p| {
                    let name = data.part_material(skin as usize, p.skin_ref).to_owned();
                    ModelPart {
                        material: self.asset_material(&name, MatKind::Model),
                        vertices: p.vertices.clone(),
                        indices: p.indices.clone(),
                        tangents: p.tangents.clone(),
                    }
                })
                .collect();
                if let Ok(lods) = self.render_lods(&key, &data, skin, body, first) {
                    renderer.set_model_lods(id, &lods);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_points_default_to_zero() {
        assert_eq!(switch_point(&[0.0, 12.5], 1), 12.5);
        assert_eq!(switch_point(&[0.0], 3), 0.0);
        assert_eq!(switch_point(&[f32::NAN], 0), 0.0);
    }

    #[test]
    fn real_model_lods() {
        let root = std::env::var_os("HP_GMOD_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("D:/SteamLibrary/steamapps/common/GarrysMod"));
        if !root.join("garrysmod").is_dir() {
            eprintln!("no Garry's Mod install: test skipped");
            return;
        }
        let fs = hp_fs::GameFs::for_gmod(&root, &hp_fs::MountOptions::default());
        let Ok(fs) = fs else {
            eprintln!("mount failed: test skipped");
            return;
        };
        let key = "models/props_c17/furniturecouch001a.mdl";
        let Ok(lods) = lod_job(&fs, key) else {
            eprintln!("{key} missing: test skipped");
            return;
        };
        assert!(lods.switch.len() >= 2, "{:?}", lods.switch);
        assert_eq!(lods.lods.len(), lods.switch.len() - 1);
        assert!(lods.switch.windows(2).all(|w| w[0] <= w[1]), "{:?}", lods.switch);
        let tris = |parts: &[PreparedPart]| parts.iter().map(|p| p.indices.len() / 3).sum::<usize>();
        let first = hp_mdl::load_from(
            key,
            |p| fs.read(p),
            &hp_mdl::LoadOptions { body_groups: hp_mdl::BodyGroupSelect::All, ..Default::default() },
        )
        .expect("lod 0");
        assert_eq!(first.lod_switch_points, lods.switch);
        assert!(tris(&lods.lods[0]) < first.triangle_count(), "LOD 1 is lighter");
    }
}
