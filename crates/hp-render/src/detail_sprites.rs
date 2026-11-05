use bytemuck::{Pod, Zeroable};

use crate::arena::GrowBuffer;
use crate::pipelines::{self, Blend, DETAIL_WGSL, Desc, Shared, vertex_layout};
use crate::types::{DetailSprite, DetailSpriteOrientation, MaterialId, ObjectId};
use crate::{FrameCounter, Renderer};

pub(crate) const SPRITE_GREEN: [u8; 3] = [0, 255, 0];

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct DetailGpu {
    origin: [f32; 3],
    orient: u32,
    right: [f32; 3],
    slot: u32,
    up: [f32; 3],
    color: u32,
    rect: [f32; 4],
    tex: [f32; 4],
}

const ATTRS: [wgpu::VertexAttribute; 8] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 16, shader_location: 2 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 28, shader_location: 3 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 32, shader_location: 4 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Unorm8x4, offset: 44, shader_location: 5 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 6 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 64, shader_location: 7 },
];

struct Pipes {
    samples: u32,
    textured: wgpu::RenderPipeline,
    wire_tris: Option<wgpu::RenderPipeline>,
    wire_lines: wgpu::RenderPipeline,
}

#[derive(Default)]
pub(crate) struct DetailSprites {
    buffer: Option<GrowBuffer>,
    count: u32,
    swaying: bool,
    material: MaterialId,
    refs: Vec<ObjectId>,
    module: Option<wgpu::ShaderModule>,
    pipes: Vec<Pipes>,
    fade: Option<[f32; 2]>,
}

const DEFAULT_FADE: [f32; 2] = [1200.0, 400.0];

fn finite3(v: [f32; 3]) -> [f32; 3] {
    if v.iter().all(|c| c.is_finite()) { v } else { [0.0; 3] }
}

impl DetailSprites {
    fn pipes(&mut self, device: &wgpu::Device, shared: &Shared, samples: u32) -> usize {
        if let Some(i) = self.pipes.iter().position(|p| p.samples == samples) {
            return i;
        }
        let [dist, fade] = self.fade.unwrap_or(DEFAULT_FADE).map(f64::from);
        let module = self.module.get_or_insert_with(|| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("hp-render detail sprites"),
                source: wgpu::ShaderSource::Wgsl(DETAIL_WGSL.into()),
            })
        });
        let vb = [Some(vertex_layout(size_of::<DetailGpu>(), wgpu::VertexStepMode::Instance, &ATTRS))];
        let rgb =
            f64::from(u32::from(SPRITE_GREEN[0]) | u32::from(SPRITE_GREEN[1]) << 8 | u32::from(SPRITE_GREEN[2]) << 16);
        let make = |label, layout, wire: bool, lines: bool, mode| {
            let constants = [
                ("WIRE_RGB", rgb),
                ("LINE_LIST", if lines { 1.0 } else { 0.0 }),
                ("DETAIL_DIST", dist),
                ("DETAIL_FADE", fade),
            ];
            pipelines::build_with(
                device,
                samples,
                Desc {
                    label,
                    module,
                    vs: "vs_detail",
                    fs: if wire { "fs_detail_wire" } else { "fs_detail" },
                    layout,
                    buffers: &vb,
                    topology: if lines {
                        wgpu::PrimitiveTopology::LineList
                    } else {
                        wgpu::PrimitiveTopology::TriangleList
                    },
                    cull: None,
                    depth_test: !wire,
                    depth_write: false,
                    blend: if wire { Blend::Replace } else { Blend::Alpha },
                    constants: &constants,
                },
                mode,
            )
        };
        let lines_mode = device.features().contains(wgpu::Features::POLYGON_MODE_LINE);
        let p = Pipes {
            samples,
            textured: make("hp-render detail sprites", &shared.layout_material, false, false, wgpu::PolygonMode::Fill),
            wire_tris: lines_mode
                .then(|| make("hp-render detail wire", &shared.layout_view, true, false, wgpu::PolygonMode::Line)),
            wire_lines: make("hp-render detail wire lines", &shared.layout_view, true, true, wgpu::PolygonMode::Fill),
        };
        self.pipes.push(p);
        self.pipes.len() - 1
    }
}

impl Renderer {
    pub fn set_detail_sprites(&mut self, material: MaterialId, sprites: &[DetailSprite]) {
        let limit = (self.max_buffer / size_of::<DetailGpu>() as u64) as usize;
        let sprites = &sprites[..sprites.len().min(limit)];
        let mut refs = Vec::with_capacity(sprites.len());
        let mut data = Vec::with_capacity(sprites.len());
        for s in sprites {
            let slot = self.scene.acquire_ref(s.object);
            refs.push(s.object);
            let rect = [s.ul[0], s.ul[1], s.lr[0], s.lr[1]];
            data.push(DetailGpu {
                origin: finite3(s.origin),
                orient: u32::from(s.sway) << 8
                    | match s.orientation {
                        DetailSpriteOrientation::Fixed => 0,
                        DetailSpriteOrientation::ScreenAligned => 1,
                        DetailSpriteOrientation::Vertical => 2,
                    },
                right: finite3(s.right),
                slot,
                up: finite3(s.up),
                color: u32::from_le_bytes(s.color),
                rect: if rect.iter().all(|v| v.is_finite()) { rect } else { [0.0; 4] },
                tex: if s.tex.iter().all(|v| v.is_finite()) { s.tex } else { [0.0; 4] },
            });
        }
        for id in std::mem::replace(&mut self.detail.refs, refs) {
            self.scene.release_ref(id);
        }
        self.detail.material = material;
        self.detail.swaying = sprites.iter().any(|s| s.sway > 0);
        self.detail.count = data.len() as u32;
        if !data.is_empty() {
            let bytes: &[u8] = bytemuck::cast_slice(&data);
            let buffer = self.detail.buffer.get_or_insert_with(|| {
                GrowBuffer::new(&self.device, "hp-render detail sprites", wgpu::BufferUsages::VERTEX, 1 << 12)
            });
            let _ = buffer.ensure(&self.device, &self.queue, bytes.len() as u64, self.max_buffer, false);
            if bytes.len() as u64 <= buffer.size {
                self.queue.write_buffer(&buffer.buffer, 0, bytes);
            } else {
                self.detail.count = 0;
            }
            let _ = self.scene.material_slot(material);
            self.sync_materials();
        }
        self.content_gen += 1;
    }

    pub fn set_detail_sprite_colors(&mut self, colors: &[(u32, [u8; 4])]) {
        let Some(buffer) = &self.detail.buffer else { return };
        let mut changed = false;
        for &(i, c) in colors {
            if i >= self.detail.count {
                continue;
            }
            let at = u64::from(i) * size_of::<DetailGpu>() as u64 + std::mem::offset_of!(DetailGpu, color) as u64;
            self.queue.write_buffer(&buffer.buffer, at, &c);
            changed = true;
        }
        if changed {
            self.content_gen += 1;
        }
    }

    pub fn detail_sprite_count(&self) -> u32 {
        self.detail.count
    }

    pub fn detail_sprites_sway(&self) -> bool {
        self.detail.count > 0 && self.detail.swaying
    }

    pub fn set_detail_fade(&mut self, fade: Option<[f32; 2]>) {
        let fade = fade.filter(|f| f.iter().all(|v| v.is_finite() && *v >= 0.0) && f[0] > 0.0);
        if fade.unwrap_or(DEFAULT_FADE) != self.detail.fade.unwrap_or(DEFAULT_FADE) {
            self.detail.pipes.clear();
            self.content_gen += 1;
        }
        self.detail.fade = fade;
    }

    pub(crate) fn ensure_detail_pipes(&mut self, samples: u32) {
        if self.detail.count > 0 {
            let _ = self.detail.pipes(&self.device, &self.shared, samples);
        }
    }

    pub(crate) fn draw_detail_sprites(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        samples: u32,
        wire: bool,
        n: &mut FrameCounter,
    ) {
        let d = &self.detail;
        let (Some(buffer), Some(p)) = (&d.buffer, d.pipes.iter().find(|p| p.samples == samples)) else { return };
        if d.count == 0 {
            return;
        }
        pass.set_vertex_buffer(0, buffer.buffer.slice(..));
        if wire {
            match &p.wire_tris {
                Some(tris) => {
                    pass.set_pipeline(tris);
                    pass.draw(0..6, 0..d.count);
                }
                None => {
                    pass.set_pipeline(&p.wire_lines);
                    pass.draw(0..10, 0..d.count);
                }
            }
        } else {
            let slot = self.scene.find_material(d.material);
            let Some(g) = slot.and_then(|s| self.mat_gpu.get(s as usize)) else { return };
            pass.set_pipeline(&p.textured);
            pass.set_bind_group(1, &g.bind_group, &[]);
            pass.draw(0..6, 0..d.count);
        }
        n.add(u64::from(d.count) * 2);
        n.counts.sprites += d.count;
    }
}
