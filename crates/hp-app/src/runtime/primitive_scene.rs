use std::collections::BTreeSet;

use super::primitives::{PrimitiveClass, PrimitiveStats, ResourceProbes};

fn editor(probes: &mut ResourceProbes, editor: &hp_vmf::EditorInfo, class: PrimitiveClass) {
    probes.vec(&editor.visgroup_ids, class);
    probes.pairs(&editor.extra_props, class);
    for value in [&editor.logical_pos, &editor.comments].into_iter().flatten() {
        probes.string(value, class);
    }
}

fn solid(probes: &mut ResourceProbes, solid: &hp_vmf::Solid) {
    let class = PrimitiveClass::World;
    probes.vec(&solid.sides, class);
    editor(probes, &solid.editor, class);
    probes.pairs(&solid.extra_props, class);
    probes.blocks(&solid.extra_blocks, class);
    for side in &solid.sides {
        probes.string(&side.material, class);
        probes.pairs(&side.extra_props, class);
        probes.blocks(&side.extra_blocks, class);
        if let Some(vertices) = &side.vertices_plus {
            probes.vec(vertices, class);
        }
        if let Some(disp) = &side.disp {
            probes.point((&**disp as *const hp_vmf::DispInfo) as usize, class);
            probes.vec(&disp.normals, class);
            probes.vec(&disp.distances, class);
            probes.vec(&disp.offsets, class);
            probes.vec(&disp.offset_normals, class);
            probes.vec(&disp.alphas, class);
            probes.vec(&disp.triangle_tags, class);
            probes.vec(&disp.allowed_verts, class);
            probes.pairs(&disp.extra_props, class);
            probes.blocks(&disp.extra_blocks, class);
        }
    }
}

fn entity_class(class: &str) -> PrimitiveClass {
    if class.starts_with("prop_") {
        PrimitiveClass::Props
    } else if class.starts_with("light")
        || matches!(
            class,
            "shadow_control" | "env_tonemap_controller" | "env_projectedtexture" | "info_lighting" | "info_lightorigin"
        )
    {
        PrimitiveClass::Lighting
    } else {
        PrimitiveClass::Shared
    }
}

fn entity(probes: &mut ResourceProbes, entity: &hp_vmf::Entity, class: PrimitiveClass) {
    probes.string(&entity.classname, class);
    probes.pairs(&entity.props, class);
    probes.vec(&entity.connections, class);
    for connection in &entity.connections {
        probes.string(&connection.output, class);
        probes.string(&connection.target, class);
        probes.string(&connection.input, class);
        probes.string(&connection.param, class);
    }
    probes.vec(&entity.solids, PrimitiveClass::World);
    for brush in &entity.solids {
        solid(probes, brush);
    }
    probes.vec(&entity.groups, class);
    for group in &entity.groups {
        editor(probes, &group.editor, class);
    }
    editor(probes, &entity.editor, class);
    probes.blocks(&entity.extra_blocks, class);
}

pub fn collect(doc: &hp_editor::Document, probes: &mut ResourceProbes) -> PrimitiveStats {
    let scene = doc.scene();
    let mut materials = BTreeSet::new();
    for node in scene.solids().values() {
        let class = PrimitiveClass::World;
        probes.point((node as *const hp_editor::SolidNode) as usize, class);
        solid(probes, &node.solid);
        probes.hold(&node.geom, class);
        probes.vec(&node.geom.faces, class);
        for face in &node.geom.faces {
            probes.vec(&face.vertices, class);
        }
        probes.vec(&node.disp, class);
        for mesh in node.disp.iter().flatten() {
            probes.hold(mesh, class);
            probes.vec(&mesh.positions, class);
            probes.vec(&mesh.normals, class);
            probes.vec(&mesh.alphas, class);
            probes.vec(&mesh.base_positions, class);
            probes.vec(&mesh.indices, class);
        }
        for side in &node.solid.sides {
            materials.insert(hp_vmt::normalize_texture_path(&side.material));
        }
    }
    let mut lights = 0;
    for node in scene.entities().values() {
        let class_name = node.entity.classname.to_ascii_lowercase();
        let class = entity_class(&class_name);
        if matches!(
            class_name.as_str(),
            "light" | "light_spot" | "light_environment" | "light_dynamic" | "env_projectedtexture"
        ) {
            lights += 1;
        }
        probes.point((node as *const hp_editor::EntityNode) as usize, class);
        entity(probes, &node.entity, class);
    }
    entity(probes, scene.world(), PrimitiveClass::Shared);
    for group in scene.groups().values() {
        editor(probes, &group.group.editor, PrimitiveClass::Shared);
    }
    PrimitiveStats {
        solids: scene.solids().len(),
        world_materials: materials.len(),
        lights,
        revision: doc.revision(),
        ..Default::default()
    }
}
