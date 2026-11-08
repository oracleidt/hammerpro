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

fn solid_node(probes: &mut ResourceProbes, node: &hp_editor::SolidNode) {
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
}

pub fn collect(doc: &hp_editor::Document, probes: &mut ResourceProbes) -> PrimitiveStats {
    collect_with(doc, probes, &super::instances::InstanceCache::shared())
}

fn collect_with(
    doc: &hp_editor::Document,
    probes: &mut ResourceProbes,
    cache: &super::instances::InstanceCache,
) -> PrimitiveStats {
    let scene = doc.scene();
    let mut materials = BTreeSet::new();
    for node in scene.solids().values() {
        solid_node(probes, node);
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
    let mut solids = scene.solids().len();
    let mut instances_loading = 0;
    let mut instances_missing = 0;
    let mut files = BTreeSet::new();
    for root in scene.entities().values().filter(|node| hp_editor::instances::is_instance(&node.entity)) {
        let expansion = super::instances::expand_one(cache, &root.entity, doc.path(), None, false);
        instances_loading += usize::from(expansion.is_pending());
        instances_missing +=
            expansion.issues.iter().filter(|issue| **issue != hp_editor::instances::InstanceIssue::Pending).count();
        for piece in expansion.pieces {
            let Some(file) = cache.file(&piece.path) else {
                instances_loading += 1;
                continue;
            };
            solids += file.solids.len();
            for brush in &file.solids {
                for side in &brush.node.solid.sides {
                    materials.insert(hp_vmt::normalize_texture_path(&side.material));
                }
            }
            for source in &piece.map.entities {
                let class_name = piece.fixup.replace(&source.classname).to_ascii_lowercase();
                if matches!(
                    class_name.as_str(),
                    "light" | "light_spot" | "light_environment" | "light_dynamic" | "env_projectedtexture"
                ) {
                    lights += 1;
                }
            }
            if !files.insert(std::sync::Arc::as_ptr(&file) as usize) {
                continue;
            }
            probes.hold(&file, PrimitiveClass::Shared);
            probes.hold(&file.map, PrimitiveClass::Shared);
            probes.vec(&file.solids, PrimitiveClass::World);
            for brush in &file.solids {
                solid_node(probes, &brush.node);
            }
            entity(probes, &file.map.world, PrimitiveClass::Shared);
            probes.vec(&file.map.entities, PrimitiveClass::Shared);
            for source in &file.map.entities {
                entity(probes, source, entity_class(&source.classname.to_ascii_lowercase()));
            }
        }
    }
    PrimitiveStats {
        solids,
        instances_loading,
        instances_missing,
        world_materials: materials.len(),
        lights,
        revision: doc.revision(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn settle(
        doc: &hp_editor::Document,
        cache: &super::super::instances::InstanceCache,
    ) -> (PrimitiveStats, ResourceProbes) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let mut probes = ResourceProbes::default();
            let stats = collect_with(doc, &mut probes, cache);
            if stats.instances_loading == 0 {
                return (stats, probes);
            }
            assert!(Instant::now() < deadline);
            cache.wait_idle(Duration::from_millis(100));
        }
    }

    #[test]
    fn nested_placements_count_individually_and_shared_geometry_is_probed_once() {
        let dir = std::env::temp_dir().join(format!(
            "hp-primitive-instances-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let leaf = r#"
world
{
	"id" "1"
	"classname" "worldspawn"
	solid
	{
		"id" "2"
		side { "id" "1" "plane" "(0 16 16) (16 16 16) (16 0 16)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
		side { "id" "2" "plane" "(0 0 0) (16 0 0) (16 16 0)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" }
		side { "id" "3" "plane" "(16 16 16) (0 16 16) (0 16 0)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
		side { "id" "4" "plane" "(16 0 0) (0 0 0) (0 0 16)" "material" "DEV/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
		side { "id" "5" "plane" "(16 16 0) (16 0 0) (16 0 16)" "material" "DEV/A" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
		side { "id" "6" "plane" "(0 16 16) (0 0 16) (0 0 0)" "material" "DEV/A" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" }
	}
}
entity
{
	"id" "3"
	"classname" "func_instance_parms"
	"parm1" "$color color255 255 0 0"
}
entity { "id" "4" "classname" "light" "origin" "0 0 8" }
"#;
        std::fs::write(dir.join("leaf.vmf"), leaf).unwrap();
        std::fs::write(
            dir.join("middle.vmf"),
            r#"world { "id" "1" "classname" "worldspawn" }
entity { "id" "2" "classname" "func_instance" "file" "leaf.vmf" }
entity { "id" "3" "classname" "func_instance" "file" "leaf.vmf" "origin" "64 0 0" }"#,
        )
        .unwrap();
        let main = r#"world { "id" "1" "classname" "worldspawn" }
entity { "id" "2" "classname" "func_instance" "file" "middle.vmf" }
"#;
        let cache = super::super::instances::InstanceCache::new();
        let mut doc = hp_editor::Document::from_map(hp_vmf::parse(main).unwrap());
        doc.set_path(Some(dir.join("main.vmf")));
        let (one, one_probes) = settle(&doc, &cache);
        assert_eq!((one.solids, one.world_materials, one.lights, one.instances_missing), (2, 1, 2, 0));
        let twice = format!(
            "{main}{}",
            r#"entity { "id" "3" "classname" "func_instance" "file" "middle.vmf" "origin" "256 0 0" }"#
        );
        let mut doc = hp_editor::Document::from_map(hp_vmf::parse(&twice).unwrap());
        doc.set_path(Some(dir.join("main.vmf")));
        let (two, two_probes) = settle(&doc, &cache);
        assert_eq!((two.solids, two.world_materials, two.lights), (4, 1, 4));
        let world = |probes: &ResourceProbes| {
            probes
                .points
                .iter()
                .filter(|probe| probe.class == PrimitiveClass::World)
                .map(|probe| probe.address)
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(world(&one_probes), world(&two_probes));
        drop((one_probes, two_probes));
        let missing = format!("{twice}{}", r#"entity { "id" "4" "classname" "func_instance" "file" "missing.vmf" }"#);
        let mut doc = hp_editor::Document::from_map(hp_vmf::parse(&missing).unwrap());
        doc.set_path(Some(dir.join("main.vmf")));
        let (stats, probes) = settle(&doc, &cache);
        assert_eq!((stats.solids, stats.lights, stats.instances_missing), (4, 4, 1));
        drop(probes);
        cache.clear();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
