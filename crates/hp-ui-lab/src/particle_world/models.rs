use super::*;

type Key = (u64, u64, u64, usize);

#[derive(Clone, Debug, Default)]
pub(crate) struct Models {
    key: Option<Key>,
    pub data: hp_app::state::ParticleModelSnapshot,
}

fn key(state: &AppState) -> Key {
    let world = (state.runtime.lab.player.active && !state.runtime.lab.player.editor_world)
        .then_some(state.runtime.health.world.as_ref())
        .flatten();
    (
        state.sidecar.serial(),
        if world.is_some() { 0 } else { state.doc.revision() },
        state.assets.generation(),
        world.map_or(0, |world| Arc::as_ptr(world) as usize),
    )
}

impl Models {
    pub(crate) fn ready(&self, state: &AppState, objects: &[u32]) -> bool {
        self.key == Some(key(state)) && objects.iter().all(|id| !self.data.pending.contains(id))
    }
}

pub(super) fn prepare(state: &mut AppState, handle: &ParticleHandle, ctx: &egui::Context) {
    let Ok(mut shared) = handle.lock() else { return };
    if shared.model_requirement.is_empty() || !state.settings.lab.show_particles {
        return;
    }
    let compiled = state.runtime.lab.player.active && !state.runtime.lab.player.editor_world;
    let entities = if compiled {
        state
            .runtime
            .health
            .world
            .as_ref()
            .map(|world| {
                world
                    .particle_entities
                    .iter()
                    .enumerate()
                    .filter_map(|(index, entity)| {
                        let id = crate::particles::compiled_entity_id(index, entity);
                        shared.model_requirement.contains(&id).then(|| (id, entity.clone()))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        shared
            .model_requirement
            .iter()
            .filter_map(|id| state.doc.scene().entity(*id).map(|node| (*id, node.entity.clone())))
            .collect()
    };
    let data = state.assets.particle_model_snapshot(&entities, !compiled);
    if !data.pending.is_empty() {
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
    shared.models = Arc::new(Models { key: Some(key(state)), data });
}

#[cfg(test)]
mod tests {
    use hp_pcf::dmx::Value;
    use hp_pcf::editor::{Category, Document};
    use hp_ui_viewports::layers::OverlayLayer;

    use super::*;

    #[test]
    fn linked_model_effects_wait_for_their_snapshot_and_use_the_target_hitboxes() {
        let mut state = hp_app::testing::test_state();
        state.settings.lab.show_particles = true;
        let mut map = hp_vmf::Map::new_empty();
        let mut source = hp_vmf::Entity { id: 5, classname: "info_particle_system".into(), ..Default::default() };
        source.set("origin", "0 0 0");
        source.set("effect_name", "model_case");
        source.set("cpoint1", "model_target");
        let mut target = hp_vmf::Entity { id: 6, classname: "prop_dynamic".into(), ..Default::default() };
        target.set("origin", "100 0 0");
        target.set("targetname", "model_target");
        target.set("model", "models/hp_parity/hitboxes.mdl");
        map.entities.extend([source, target]);
        state.doc = hp_editor::Document::from_map(map);
        let mut definition = Document::new();
        let system = definition.create_system("model_case").unwrap();
        definition
            .add_operator_with_params(
                system,
                Category::Emitters,
                "emit_instantaneously",
                vec![("num_to_emit".into(), Value::Int(1))],
            )
            .unwrap();
        definition
            .add_operator_with_params(
                system,
                Category::Initializers,
                "Position on Model Random",
                vec![("control_point_number".into(), Value::Int(1)), ("model hitbox scale".into(), Value::Float(0.5))],
            )
            .unwrap();
        definition.add_operator(system, Category::Renderers, "render_animated_sprites").unwrap();
        let handle = crate::particles::new_handle();
        handle.lock().unwrap().sprite_layer = true;
        handle
            .lock()
            .unwrap()
            .library
            .insert("model_case".into(), Arc::new(hp_pcf::Plan::new(&definition.definitions()[0])));
        let mut layer = crate::particles::ParticleLayer::new(Arc::clone(&handle));
        layer.prepare(&state);
        assert_eq!(handle.lock().unwrap().model_requirement.iter().copied().collect::<Vec<_>>(), vec![6]);
        assert_eq!(handle.lock().unwrap().particles, 0);
        let mut data = hp_app::state::ParticleModelSnapshot::default();
        data.pending.insert(6);
        handle.lock().unwrap().models = Arc::new(Models { key: Some(key(&state)), data });
        layer.prepare(&state);
        assert_eq!(handle.lock().unwrap().particles, 0);
        let boxes: Arc<[hp_pcf::environment::ModelHitbox]> = vec![hp_pcf::environment::ModelHitbox {
            minimum: [-16.0; 3],
            maximum: [16.0; 3],
            transform: [[1.0, 0.0, 0.0, 300.0], [0.0, 1.0, 0.0, 20.0], [0.0, 0.0, 1.0, 64.0]],
        }]
        .into();
        let mut data = hp_app::state::ParticleModelSnapshot::default();
        data.models.insert(6, HashMap::from([("effects".into(), boxes)]));
        handle.lock().unwrap().models = Arc::new(Models { key: Some(key(&state)), data });
        layer.prepare(&state);
        let shared = handle.lock().unwrap();
        assert_eq!(shared.particles, 1);
        assert_eq!(shared.sprites[0].pos, [300.0, 20.0, 64.0]);
    }
}
