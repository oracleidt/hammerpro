use std::collections::BTreeSet;

use hp_app::runtime::{PreloadMaterial, PreloadResource, PreloadResult};
use hp_app::state::Gfx;
use hp_editor::{Document, deps_roots::RootKind};

use crate::AssetManager;
use crate::material::{AnaLook, MatKind, texture_keys};
use crate::models::{ModelEntry, model_data_key};

impl AssetManager {
    pub(crate) fn preload_resources_impl(&self, doc: &Document) -> Vec<PreloadResource> {
        let mut resources = BTreeSet::new();
        let material =
            |name: &str, context| PreloadResource::Material { name: crate::util::material_key(name), context };
        for root in doc.asset_roots(&|_, _| None) {
            match root.kind {
                RootKind::Material => {
                    resources.insert(material(&root.name, PreloadMaterial::World));
                }
                RootKind::Skybox => {
                    resources.insert(PreloadResource::Sky(crate::util::material_key(&root.name)));
                }
                RootKind::Particle => {
                    resources.insert(PreloadResource::Particle(root.name));
                }
                _ => {}
            }
        }
        for node in doc.scene().entities().values() {
            let desc = crate::entities::entity_desc(&node.entity, self.fgd.as_deref());
            if let Some((name, skin)) = desc.model {
                resources.insert(PreloadResource::Model { name, skin, body: desc.body, animation: desc.anim });
            }
            if let Some(name) = desc.sprite {
                resources.insert(material(
                    &name,
                    if desc.sprite_is_icon {
                        PreloadMaterial::Icon
                    } else if desc.sprite_additive {
                        PreloadMaterial::AdditiveSprite
                    } else {
                        PreloadMaterial::Sprite
                    },
                ));
            }
        }
        resources.into_iter().collect()
    }

    fn preload_material(&mut self, name: &str, kind: MatKind) -> PreloadResult {
        self.asset_material(name, kind);
        match self.mats.ana_look(name) {
            AnaLook::Loading => PreloadResult::Pending,
            AnaLook::Missing => PreloadResult::Missing,
            AnaLook::Ready(analysis) => {
                let mut missing = false;
                for key in texture_keys(kind, analysis) {
                    match self.mats.textures.map.get(&key) {
                        Some(entry) if entry.current.is_some() && !entry.loading => {}
                        Some(entry) if entry.missing && !entry.loading => missing = true,
                        _ => return PreloadResult::Pending,
                    }
                }
                if missing { PreloadResult::Missing } else { PreloadResult::Ready }
            }
        }
    }

    fn preload_model_materials(&mut self, key: &str, skin: u32, body: u32) -> Result<Vec<String>, PreloadResult> {
        let Some(model) = self.model_data(key) else {
            return Err(if matches!(self.models.map.get(key), Some(ModelEntry::Failed(_))) {
                PreloadResult::Missing
            } else {
                PreloadResult::Pending
            });
        };
        Ok(model
            .parts_for_body(body)
            .into_iter()
            .map(|part| model.part_material(skin as usize, part.skin_ref).to_owned())
            .collect())
    }

    pub(crate) fn preload_resource_impl(&mut self, resource: &PreloadResource, gfx: &mut Gfx) -> PreloadResult {
        if self.fs.is_none() {
            return PreloadResult::Missing;
        }
        match resource {
            PreloadResource::Material { name, context } => {
                let kind = match context {
                    PreloadMaterial::World => MatKind::World,
                    PreloadMaterial::Icon => MatKind::Sprite,
                    PreloadMaterial::Sprite => MatKind::EntitySprite,
                    PreloadMaterial::AdditiveSprite => MatKind::EntitySpriteAdditive,
                };
                self.preload_material(name, kind)
            }
            PreloadResource::Model { name, skin, body, animation } => {
                let key = model_data_key(name, animation.as_deref());
                let materials = match self.preload_model_materials(&key, *skin, *body) {
                    Ok(materials) => materials,
                    Err(result) => return result,
                };
                self.render_model_for(&mut gfx.renderer, name, *skin, *body, animation.as_deref());
                let mut result = PreloadResult::Ready;
                for material in materials {
                    match self.preload_material(&material, MatKind::Model) {
                        PreloadResult::Pending => return PreloadResult::Pending,
                        PreloadResult::Missing => result = PreloadResult::Missing,
                        PreloadResult::Ready => {}
                    }
                }
                result
            }
            PreloadResource::Sky(name) => {
                let mut result = PreloadResult::Ready;
                for suffix in ["rt", "lf", "bk", "ft", "up", "dn"] {
                    let mut face = self.preload_material(&format!("skybox/{name}{suffix}"), MatKind::World);
                    if face == PreloadResult::Missing {
                        face = self.preload_material(&format!("skybox/{name}_hdr{suffix}"), MatKind::World);
                    }
                    match face {
                        PreloadResult::Pending => result = PreloadResult::Pending,
                        PreloadResult::Missing if result != PreloadResult::Pending => result = PreloadResult::Missing,
                        _ => {}
                    }
                }
                result
            }
            PreloadResource::Particle(name) => {
                let Some(catalog) = self.particles.get(self.fs.as_ref(), self.generation) else {
                    return PreloadResult::Pending;
                };
                let Some(system) = catalog.systems.iter().find(|system| system.name.eq_ignore_ascii_case(name)) else {
                    return PreloadResult::Missing;
                };
                if system.material.is_empty() {
                    return PreloadResult::Ready;
                }
                self.preload_material(&system.material, MatKind::Sprite)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preload_waits_for_all_blend_textures_and_finishes_missing_dependencies() {
        use crate::registry::AnaEntry;
        use crate::texture::TexEntry;
        use std::sync::Arc;
        let mut assets = AssetManager::new(&hp_app::settings::Settings::default());
        let material =
            hp_vmt::parse(r#"WorldVertexTransition { "$basetexture" "test/a" "$basetexture2" "test/b" }"#).unwrap();
        let analysis = Arc::new(crate::material::analyze("test/blend", &material, &|_| 0));
        assets
            .mats
            .analyses
            .insert("test/blend".to_owned(), AnaEntry { value: Some(Some(analysis)), ..Default::default() });
        assert_eq!(assets.preload_material("test/blend", MatKind::World), PreloadResult::Pending);
        assets.mats.textures.map.insert(
            "test/a".to_owned(),
            TexEntry { current: Some((hp_render::TexId::WHITE, [4, 4])), ..Default::default() },
        );
        assert_eq!(assets.preload_material("test/blend", MatKind::World), PreloadResult::Pending);
        assets.mats.textures.map.insert("test/b".to_owned(), TexEntry { missing: true, ..Default::default() });
        assert_eq!(assets.preload_material("test/blend", MatKind::World), PreloadResult::Missing);
        assets.mats.textures.map.insert(
            "test/b".to_owned(),
            TexEntry { current: Some((hp_render::TexId::WHITE, [4, 4])), ..Default::default() },
        );
        assert_eq!(assets.preload_material("test/blend", MatKind::World), PreloadResult::Ready);
    }

    #[test]
    fn preloading_an_evicted_model_requests_its_geometry_again() {
        let mut assets = AssetManager::new(&hp_app::settings::Settings::default());
        let key = "models/test.mdl";
        assets
            .models
            .map
            .insert(key.to_owned(), ModelEntry::Cached(std::sync::Arc::new(hp_app::state::ModelInfo::default())));
        assert_eq!(assets.preload_model_materials(key, 0, 0), Err(PreloadResult::Pending));
        assert!(matches!(assets.models.map.get(key), Some(ModelEntry::Loading)));
        assets.models.map.insert(key.to_owned(), ModelEntry::Failed("missing".to_owned()));
        assert_eq!(assets.preload_model_materials(key, 0, 0), Err(PreloadResult::Missing));
    }

    #[test]
    fn manifest_deduplicates_world_materials_and_retains_prop_variants() {
        let doc = Document::from_map(
            hp_vmf::parse(
                r#"
            world { "id" "1" "skyname" "test_sky" }
            entity { "id" "2" "classname" "prop_static" "model" "models/a.mdl" "skin" "0" }
            entity { "id" "3" "classname" "prop_static" "model" "models/a.mdl" "skin" "1" }
            entity { "id" "4" "classname" "prop_static" "model" "models/a.mdl" "skin" "0" }
        "#,
            )
            .unwrap(),
        );
        let assets = AssetManager::new(&hp_app::settings::Settings::default());
        let plan = assets.preload_resources_impl(&doc);
        assert_eq!(plan.iter().filter(|resource| matches!(resource, PreloadResource::Model { .. })).count(), 2);
        assert!(plan.contains(&PreloadResource::Sky("test_sky".to_owned())));
    }
}
