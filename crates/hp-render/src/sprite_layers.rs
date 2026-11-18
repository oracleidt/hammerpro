use crate::arena::GrowBuffer;
use crate::{Group, ObjectId, Renderer, SpriteInstance};

pub const SPRITE_LAYER_PARTICLES: u32 = 1;

pub(crate) struct SpriteLayer {
    pub buffer: GrowBuffer,
    pub groups: Vec<Group>,
    pub refs: Vec<ObjectId>,
}

impl Renderer {
    pub fn set_sprite_layer(&mut self, layer: u32, sprites: &[SpriteInstance]) {
        if sprites.is_empty() {
            if let Some(old) = self.sprite_layers.remove(&layer) {
                for id in old.refs {
                    self.scene.release_ref(id);
                }
                self.bump_3d_content();
            }
            return;
        }
        let (data, refs, groups) = self.pack_sprites(sprites);
        let mut l = match self.sprite_layers.remove(&layer) {
            Some(l) => l,
            None => SpriteLayer {
                buffer: GrowBuffer::new(&self.device, "hp-render sprite layer", wgpu::BufferUsages::VERTEX, 1 << 12),
                groups: Vec::new(),
                refs: Vec::new(),
            },
        };
        for id in std::mem::replace(&mut l.refs, refs) {
            self.scene.release_ref(id);
        }
        let bytes: &[u8] = bytemuck::cast_slice(&data);
        let _ = l.buffer.ensure(&self.device, &self.queue, bytes.len() as u64, self.max_buffer, false);
        if !bytes.is_empty() && bytes.len() as u64 <= l.buffer.size {
            self.queue.write_buffer(&l.buffer.buffer, 0, bytes);
            l.groups = groups;
        } else {
            l.groups.clear();
        }
        self.sprite_layers.insert(layer, l);
        self.bump_3d_content();
    }

    pub fn sprite_layer_len(&self, layer: u32) -> usize {
        self.sprite_layers.get(&layer).map_or(0, |l| l.refs.len())
    }
}
