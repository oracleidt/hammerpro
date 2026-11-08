use std::collections::BTreeMap;

use glam::Vec3;

use crate::arena::GrowBuffer;
use crate::gpu::{InstanceGpu, LightGpu};
use crate::scene::Scene;
use crate::sprite_layers::SpriteLayer;
use crate::{Group, ModelInstance, ObjectId, Renderer};

pub struct ParkedScene {
    scene: Scene,
    arenas: [GrowBuffer; 3],
    states: GrowBuffer,
    instances: GrowBuffer,
    instance_count: u32,
    instance_groups: Vec<Group>,
    instance_refs: Vec<ObjectId>,
    instance_bounds: Option<(Vec3, Vec3)>,
    instance_boxes: Vec<(Vec3, Vec3)>,
    instance_data: Vec<InstanceGpu>,
    instance_list: Vec<ModelInstance>,
    translucent_instances: bool,
    world_sprites: Vec<crate::WorldSprite>,
    sprites: GrowBuffer,
    sprite_groups: Vec<Group>,
    sprite_refs: Vec<ObjectId>,
    sprite_layers: BTreeMap<u32, SpriteLayer>,
    lights_buf: GrowBuffer,
    lights_cpu: Vec<LightGpu>,
    dir_lights: u32,
    lights_left_out: u32,
    sun: crate::SunParams,
    ambient: [f32; 3],
}

impl ParkedScene {
    pub fn objects(&self) -> usize {
        self.scene.objects.len()
    }

    pub fn model_instances(&self) -> usize {
        self.instance_list.len()
    }

    pub fn gpu_bytes(&self) -> u64 {
        self.arenas.iter().map(|a| a.size).sum::<u64>()
            + self.states.size
            + self.instances.size
            + self.sprites.size
            + self.lights_buf.size
            + self.sprite_layers.values().map(|l| l.buffer.size).sum::<u64>()
    }

    pub fn drop_in_background(self) {
        if self.scene.objects.len() < 4096 {
            return;
        }
        let spawned = std::thread::Builder::new().name("hp-render-drop".into()).spawn(move || drop(self));
        if let Err(e) = spawned {
            log::warn!("hp-render: cannot start a thread to free a scene: {e}");
        }
    }
}

impl Renderer {
    pub fn park_scene(&mut self) -> ParkedScene {
        let empty = self.empty_scene(self.scene.fresh());
        self.swap_scene(empty)
    }

    pub fn restore_scene(&mut self, mut parked: ParkedScene) -> ParkedScene {
        parked.scene.adopt_materials(&self.scene);
        self.swap_scene(parked)
    }

    fn swap_scene(&mut self, incoming: ParkedScene) -> ParkedScene {
        self.set_particle_models(&[]);
        self.particle_draw_batches.clear();
        let ParkedScene {
            scene,
            arenas,
            states,
            instances,
            instance_count,
            instance_groups,
            instance_refs,
            instance_bounds,
            instance_boxes,
            instance_data,
            instance_list,
            translucent_instances,
            world_sprites,
            sprites,
            sprite_groups,
            sprite_refs,
            sprite_layers,
            lights_buf,
            lights_cpu,
            dir_lights,
            lights_left_out,
            sun,
            ambient,
        } = incoming;
        self.game_outlines = None;
        self.set_detail_sprites(0, &[]);
        let out = ParkedScene {
            scene: std::mem::replace(&mut self.scene, scene),
            arenas: std::mem::replace(&mut self.arenas, arenas),
            states: std::mem::replace(&mut self.states, states),
            instances: std::mem::replace(&mut self.instances, instances),
            instance_count: std::mem::replace(&mut self.instance_count, instance_count),
            instance_groups: std::mem::replace(&mut self.instance_groups, instance_groups),
            instance_refs: std::mem::replace(&mut self.instance_refs, instance_refs),
            instance_bounds: std::mem::replace(&mut self.instance_bounds, instance_bounds),
            instance_boxes: std::mem::replace(&mut self.instance_boxes, instance_boxes),
            instance_data: std::mem::replace(&mut self.instance_data, instance_data),
            instance_list: std::mem::replace(&mut self.instance_list, instance_list),
            translucent_instances: std::mem::replace(&mut self.translucent_instances, translucent_instances),
            world_sprites: std::mem::replace(&mut self.world_sprites, world_sprites),
            sprites: std::mem::replace(&mut self.sprites, sprites),
            sprite_groups: std::mem::replace(&mut self.sprite_groups, sprite_groups),
            sprite_refs: std::mem::replace(&mut self.sprite_refs, sprite_refs),
            sprite_layers: std::mem::replace(&mut self.sprite_layers, sprite_layers),
            lights_buf: std::mem::replace(&mut self.lights_buf, lights_buf),
            lights_cpu: std::mem::replace(&mut self.lights_cpu, lights_cpu),
            dir_lights: std::mem::replace(&mut self.dir_lights, dir_lights),
            lights_left_out: std::mem::replace(&mut self.lights_left_out, lights_left_out),
            sun: std::mem::replace(&mut self.sun, sun),
            ambient: std::mem::replace(&mut self.ambient, ambient),
        };
        self.sync_materials();
        self.scene_gen += 1;
        self.lights_gen += 1;
        self.shadow_epoch += 1;
        self.content_gen += 1;
        self.sel_chunks = ((0, 0), Vec::new());
        for v in self.views.values_mut() {
            v.occ = None;
        }
        self.lightmap.objects_cleared();
        out
    }

    fn empty_scene(&self, scene: Scene) -> ParkedScene {
        let d = &self.device;
        let vb = wgpu::BufferUsages::VERTEX;
        let storage = wgpu::BufferUsages::STORAGE;
        ParkedScene {
            scene,
            arenas: [
                GrowBuffer::new(d, "hp-render mesh vertices", vb, 1 << 20),
                GrowBuffer::new(d, "hp-render indices", wgpu::BufferUsages::INDEX, 1 << 20),
                GrowBuffer::new(d, "hp-render edge vertices", vb, 1 << 19),
            ],
            states: GrowBuffer::new(d, "hp-render object states", storage, 1 << 14),
            instances: GrowBuffer::new(d, "hp-render instances", vb, 1 << 12),
            instance_count: 0,
            instance_groups: Vec::new(),
            instance_refs: Vec::new(),
            instance_bounds: None,
            instance_boxes: Vec::new(),
            instance_data: Vec::new(),
            instance_list: Vec::new(),
            translucent_instances: false,
            world_sprites: Vec::new(),
            sprites: GrowBuffer::new(d, "hp-render sprites", vb, 1 << 12),
            sprite_groups: Vec::new(),
            sprite_refs: Vec::new(),
            sprite_layers: BTreeMap::new(),
            lights_buf: GrowBuffer::new(d, "hp-render lights", storage, size_of::<LightGpu>() as u64),
            lights_cpu: Vec::new(),
            dir_lights: 0,
            lights_left_out: 0,
            sun: crate::SunParams::default(),
            ambient: [0.0; 3],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parked_scenes_move_between_threads() {
        fn send<T: Send>() {}
        send::<ParkedScene>();
    }
}
