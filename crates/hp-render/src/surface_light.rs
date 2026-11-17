use crate::Renderer;
use crate::compiled_light::CompiledLightPass;
use crate::hooks::HookContext;
use crate::lightmap::LightmapPass;
use crate::pipelines::DEPTH_FORMAT;

pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub(crate) fn primitive() -> wgpu::PrimitiveState {
    wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        strip_index_format: None,
        front_face: wgpu::FrontFace::Ccw,
        cull_mode: None,
        unclipped_depth: false,
        polygon_mode: wgpu::PolygonMode::Fill,
        conservative: false,
    }
}

pub(crate) fn depth_state() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::Greater),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}

pub(crate) fn target() -> wgpu::ColorTargetState {
    wgpu::ColorTargetState { format: FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL }
}

pub(crate) struct SurfaceTarget {
    pub size: [u32; 2],
    pub generation: u64,
    _color: wgpu::Texture,
    pub view: wgpu::TextureView,
    _depth: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    pub pass_bind: Option<(wgpu::BindGroup, (u64, u64))>,
}

impl SurfaceTarget {
    pub fn new(device: &wgpu::Device, size: [u32; 2], generation: u64) -> Self {
        let extent = wgpu::Extent3d { width: size[0].max(1), height: size[1].max(1), depth_or_array_layers: 1 };
        let make = |label: &str, format: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let att = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let color = make("hp-render surface light", FORMAT, att | wgpu::TextureUsages::TEXTURE_BINDING);
        let depth = make("hp-render surface light depth", DEPTH_FORMAT, att);
        let view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        Self { size, generation, _color: color, view, _depth: depth, depth_view, pass_bind: None }
    }

    pub fn bytes(&self) -> u64 {
        u64::from(self.size[0]) * u64::from(self.size[1]) * 12
    }
}

pub(crate) fn dummy(device: &wgpu::Device) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("hp-render surface light dummy"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

impl Renderer {
    pub(crate) fn surface_light_wanted(&self, is_ortho: bool, compiled: bool, reflecting: bool) -> bool {
        if is_ortho || reflecting || self.probing || self.scene.surface_decals == 0 {
            return false;
        }
        compiled || LightmapPass::surface_drawn(self, is_ortho)
    }

    pub(crate) fn record_surface_light(
        &self,
        cx: &HookContext<'_>,
        target: &SurfaceTarget,
        enc: &mut wgpu::CommandEncoder,
    ) {
        let Some((bind, _)) = target.pass_bind.as_ref() else { return };
        let cx = HookContext { view_bind_group: bind, samples: 1, ..*cx };
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("hp-render surface light"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        LightmapPass::draw_surface(&cx, &mut pass);
        CompiledLightPass::draw_surface_light(&cx, &mut pass);
    }
}
