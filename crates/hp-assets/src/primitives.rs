use std::collections::BTreeSet;

use hp_app::runtime::primitives::{PrimitiveClass, PrimitiveStats, ResourceProbes};
use hp_editor::Document;

use crate::AssetManager;
use crate::models::{ModelEntry, PreparedModel};

fn model_probes(probes: &mut ResourceProbes, model: &std::sync::Arc<PreparedModel>) {
    let class = PrimitiveClass::Props;
    probes.hold(model, class);
    probes.hold(&model.info, class);
    probes.string(&model.info.path, class);
    probes.string(&model.info.surface_prop, class);
    probes.strings(&model.info.materials, class);
    probes.strings(&model.info.sequences, class);
    probes.vec(&model.info.sequence_frames, class);
    probes.vec(&model.info.body_groups, class);
    for group in &model.info.body_groups {
        probes.string(&group.name, class);
        probes.strings(&group.choices, class);
    }
    probes.vec(&model.parts, class);
    probes.vec(&model.variant_parts, class);
    for part in model.parts.iter().chain(&model.variant_parts) {
        probes.vec(&part.vertices, class);
        probes.vec(&part.indices, class);
        probes.vec(&part.tangents, class);
        probes.vec(&part.color_index, class);
    }
    probes.vec(&model.skin_materials, class);
    for row in &model.skin_materials {
        probes.strings(row, class);
    }
    probes.vec(&model.body_parts, class);
    probes.vec(&model.lod_switch, class);
    probes.vec(&model.particle_sets, class);
    probes.vec(&model.particle_bones, class);
    probes.vec(&model.bind_bones, class);
    probes.vec(&model.helpers.hitbox_edges, class);
    probes.vec(&model.helpers.bone_lines, class);
    probes.vec(&model.helpers.joints, class);
    probes.vec(&model.helpers.attachments, class);
    probes.vec(&model.helpers.seq_boxes, class);
}

impl AssetManager {
    pub(crate) fn primitive_resources_impl(&self, doc: &Document, probes: &mut ResourceProbes) -> PrimitiveStats {
        let mut stats = PrimitiveStats::default();
        let mut materials = BTreeSet::new();
        let mut seen = BTreeSet::new();
        let render_materials: std::collections::HashMap<_, _> = self
            .models
            .render
            .values()
            .chain(self.models.variants.values().map(|(model, _)| model))
            .filter_map(|model| model.id.map(|id| (id, &model.materials)))
            .collect();
        for node in doc.scene().entities().values() {
            let entity = &node.entity;
            if !entity.classname.to_ascii_lowercase().starts_with("prop_") {
                continue;
            }
            let Some(path) = entity.get("model").filter(|path| path.to_ascii_lowercase().ends_with(".mdl")) else {
                continue;
            };
            stats.props += 1;
            let desc = crate::entities::entity_desc(entity, self.fgd.as_deref());
            let path = crate::util::model_key(path);
            let path = crate::models::model_data_key(&path, desc.anim.as_deref());
            let skin = entity.get("skin").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            let body = desc.body;
            if !seen.insert((path.clone(), skin, body)) {
                continue;
            }
            match self.models.map.get(&path) {
                Some(ModelEntry::Ready(model)) => {
                    model_probes(probes, model);
                    for part in model.parts_for_body(body) {
                        let material = model.part_material(skin, part.skin_ref);
                        if !material.is_empty() {
                            materials.insert(material.to_owned());
                        }
                    }
                }
                Some(ModelEntry::Cached(info)) => {
                    probes.hold(info, PrimitiveClass::Props);
                    probes.string(&info.path, PrimitiveClass::Props);
                    probes.strings(&info.materials, PrimitiveClass::Props);
                    probes.strings(&info.sequences, PrimitiveClass::Props);
                    if let Some(ids) = self
                        .scene
                        .ents
                        .get(&entity.id)
                        .and_then(|state| state.model)
                        .and_then(|model| render_materials.get(&model))
                    {
                        for id in *ids {
                            if let Some(crate::registry::MatRecord {
                                source: crate::registry::MatSource::Asset { name, .. },
                                ..
                            }) = self.mats.records.get((*id as usize).wrapping_sub(1))
                            {
                                materials.insert(name.clone());
                            }
                        }
                    } else {
                        stats.models_loading += 1;
                    }
                }
                Some(ModelEntry::Failed(_)) => stats.models_missing += 1,
                Some(ModelEntry::Loading) | None => stats.models_loading += 1,
            }
        }
        for name in &materials {
            self.primitive_material_probes(name, PrimitiveClass::Props, probes);
        }
        let world_materials: BTreeSet<&str> = doc
            .scene()
            .solids()
            .values()
            .flat_map(|node| node.solid.sides.iter().map(|side| side.material.as_str()))
            .collect();
        for material in world_materials {
            self.primitive_material_probes(material, PrimitiveClass::World, probes);
        }
        stats.prop_materials = materials.len();
        stats
    }

    fn primitive_material_probes(&self, name: &str, class: PrimitiveClass, probes: &mut ResourceProbes) {
        let name = crate::util::material_key(name);
        let name = name.as_str();
        if let Some(Some(analysis)) = self.mats.analyses.get(name).and_then(|entry| entry.value.as_ref()) {
            probes.hold(analysis, class);
            for value in
                [&analysis.texture, &analysis.texture2, &analysis.texture3, &analysis.texture4].into_iter().flatten()
            {
                probes.string(value, class);
            }
            if let Some(illum) = &analysis.self_illum
                && let Some((name, _)) = &illum.mask
            {
                probes.string(name, class);
            }
            if let Some(detail) = &analysis.detail {
                probes.string(&detail.texture, class);
            }
            if let Some(bump) = &analysis.bump {
                probes.string(&bump.texture, class);
                if let Some(texture) = &bump.texture2 {
                    probes.string(texture, class);
                }
            }
            if let Some(water) = &analysis.water {
                for texture in [&water.normal_map, &water.base_texture, &water.bottom_material].into_iter().flatten() {
                    probes.string(texture, class);
                }
            }
            if let Some(env) = &analysis.envmap {
                if let Some(texture) = &env.texture {
                    probes.string(texture, class);
                }
                if let Some(mask) = &env.mask {
                    let (crate::envmap::MaskSource::Texture { name, .. }
                    | crate::envmap::MaskSource::NormalAlpha { name, .. }) = mask;
                    probes.string(name, class);
                }
            }
        }
        if let Some(info) = self.mats.infos.get(name) {
            probes.hold(info, class);
            probes.string(&info.name, class);
            probes.string(&info.shader, class);
            probes.strings(&info.keywords, class);
            for value in [&info.surface_prop, &info.base_texture, &info.base_texture2].into_iter().flatten() {
                probes.string(value, class);
            }
        }
    }
}
