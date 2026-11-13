use crate::gamewire::{MODEL_CYAN, WORLD_GREEN, WORLD_PINK};
use crate::gpu::{ColorVertex, EdgeVertex, GpuVertex, InstanceGpu, PointInstance, SpriteGpu};

pub(crate) const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub(crate) const LDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
pub(crate) const DISPLAY_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

const EDGE_DEPTH_BIAS: f64 = 4.0e-4;

macro_rules! shader_source {
    ($($file:literal),*) => {
        concat!(include_str!("shaders/common.wgsl"), $("\n", include_str!(concat!("shaders/", $file))),*)
    };
}

pub(crate) const MESH_WGSL: &str = shader_source!("material.wgsl", "parallax.wgsl", "lighting.wgsl", "mesh.wgsl");
pub(crate) const LINES_WGSL: &str = shader_source!("lines.wgsl");
pub(crate) const SPRITE_WGSL: &str = shader_source!("material.wgsl", "parallax.wgsl", "sprite.wgsl");
pub(crate) const GRID_WGSL: &str = shader_source!("grid.wgsl");
pub(crate) const SKY_WGSL: &str = shader_source!("sky.wgsl");
pub(crate) const SUN_GLOW_WGSL: &str = shader_source!("material.wgsl", "parallax.wgsl", "sun_glow.wgsl");
pub(crate) const DETAIL_WGSL: &str = shader_source!("material.wgsl", "parallax.wgsl", "detail.wgsl");

#[derive(Clone)]
pub(crate) struct Shared {
    pub view_bgl: wgpu::BindGroupLayout,
    pub material_bgl: wgpu::BindGroupLayout,
    pub wide_bgl: wgpu::BindGroupLayout,
    pub sky_bgl: wgpu::BindGroupLayout,
    pub(crate) layout_view: wgpu::PipelineLayout,
    pub(crate) layout_material: wgpu::PipelineLayout,
    layout_wide: wgpu::PipelineLayout,
    layout_sky: wgpu::PipelineLayout,
    pub(crate) mesh: wgpu::ShaderModule,
    pub(crate) lines: wgpu::ShaderModule,
    sprite: wgpu::ShaderModule,
    grid: wgpu::ShaderModule,
    sky: wgpu::ShaderModule,
}

fn storage_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn depth_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

impl Shared {
    pub fn new(device: &wgpu::Device) -> Self {
        let vf = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let view_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hp-render view"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: vf,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage_entry(1, vf),
                sampler_entry(2),
                sampler_entry(3),
                storage_entry(4, wgpu::ShaderStages::FRAGMENT),
                storage_entry(5, wgpu::ShaderStages::FRAGMENT),
                storage_entry(6, wgpu::ShaderStages::FRAGMENT),
                depth_entry(7),
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                storage_entry(9, vf),
                storage_entry(10, wgpu::ShaderStages::VERTEX),
                wgpu::BindGroupLayoutEntry {
                    binding: 11,
                    visibility: vf,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 12,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::CubeArray,
                        multisampled: false,
                    },
                    count: None,
                },
                storage_entry(13, wgpu::ShaderStages::FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 14,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                depth_entry(15),
                depth_entry(16),
                depth_entry(17),
                storage_entry(18, wgpu::ShaderStages::FRAGMENT),
                storage_entry(19, wgpu::ShaderStages::FRAGMENT),
                depth_entry(21),
                depth_entry(22),
                wgpu::BindGroupLayoutEntry {
                    binding: 20,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let material_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hp-render material"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture_entry(3),
                texture_entry(4),
                texture_entry(5),
                texture_entry(6),
                texture_entry(7),
                texture_entry(8),
                texture_entry(9),
                sampler_entry(10),
                sampler_entry(11),
            ],
        });
        let wide_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hp-render wide lines"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let sky_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hp-render sky"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                texture_entry(2),
                texture_entry(3),
                texture_entry(4),
                texture_entry(5),
            ],
        });
        let layout_view = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hp-render layout view"),
            bind_group_layouts: &[Some(&view_bgl)],
            immediate_size: 0,
        });
        let layout_wide = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hp-render layout wide lines"),
            bind_group_layouts: &[Some(&view_bgl), Some(&wide_bgl)],
            immediate_size: 0,
        });
        let layout_sky = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hp-render layout sky"),
            bind_group_layouts: &[Some(&view_bgl), Some(&sky_bgl)],
            immediate_size: 0,
        });
        let layout_material = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hp-render layout material"),
            bind_group_layouts: &[Some(&view_bgl), Some(&material_bgl)],
            immediate_size: 0,
        });
        let module = |label: &str, src: &'static str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(src.into()),
            })
        };
        Self {
            view_bgl,
            material_bgl,
            wide_bgl,
            sky_bgl,
            layout_view,
            layout_material,
            layout_wide,
            layout_sky,
            mesh: module("hp-render mesh", MESH_WGSL),
            lines: module("hp-render lines", LINES_WGSL),
            sprite: module("hp-render sprite", SPRITE_WGSL),
            grid: module("hp-render grid", GRID_WGSL),
            sky: module("hp-render sky", SKY_WGSL),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum MeshPass {
    Opaque = 0,
    AlphaTest = 1,
    Translucent = 2,
    Additive = 3,
    Modulate = 4,
}

pub(crate) const MESH_PASSES: usize = 5;

pub(crate) struct PipelineSet {
    pub chunk: [[wgpu::RenderPipeline; 2]; MESH_PASSES],
    pub chunk_faded: [[wgpu::RenderPipeline; 2]; 2],
    pub prepass: [wgpu::RenderPipeline; 2],
    pub chunk_equal: [wgpu::RenderPipeline; 2],
    pub model_prepass: [wgpu::RenderPipeline; 2],
    pub model_equal: [wgpu::RenderPipeline; 2],
    pub model: [[wgpu::RenderPipeline; 2]; MESH_PASSES],
    pub model_faded: [[wgpu::RenderPipeline; 2]; 2],
    pub edge_depth: wgpu::RenderPipeline,
    pub edge_unselected: wgpu::RenderPipeline,
    pub edge_selected: wgpu::RenderPipeline,
    pub edge_always: wgpu::RenderPipeline,
    pub edge_invalid: wgpu::RenderPipeline,
    pub box_depth: wgpu::RenderPipeline,
    pub box_far: wgpu::RenderPipeline,
    pub box_unselected: wgpu::RenderPipeline,
    pub box_selected: wgpu::RenderPipeline,
    pub model_edges: [wgpu::RenderPipeline; 2],
    pub game_wire: GameWirePipelines,
    pub collision: wgpu::RenderPipeline,
    pub collision_hull: wgpu::RenderPipeline,
    pub box_no_collision: wgpu::RenderPipeline,
    pub dots: wgpu::RenderPipeline,
    pub marks: [wgpu::RenderPipeline; 3],
    pub fill: wgpu::RenderPipeline,
    pub sprite: wgpu::RenderPipeline,
    pub overlay_lines: [wgpu::RenderPipeline; 2],
    pub overlay_tris: [wgpu::RenderPipeline; 2],
    pub wide_lines: [wgpu::RenderPipeline; 2],
    pub points: wgpu::RenderPipeline,
    pub grid: wgpu::RenderPipeline,
    pub sky: wgpu::RenderPipeline,
}

pub(crate) struct GameWirePipelines {
    pub world_tris: Option<[[wgpu::RenderPipeline; 2]; 2]>,
    pub world_outlines: [wgpu::RenderPipeline; 2],
    pub entity_faces: [[wgpu::RenderPipeline; 2]; 2],
    pub models: Option<wgpu::RenderPipeline>,
    pub model_edges: wgpu::RenderPipeline,
}

pub(crate) const CHUNK_ATTRS: [wgpu::VertexAttribute; 6] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 20, shader_location: 2 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 24, shader_location: 3 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 28, shader_location: 13 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 32, shader_location: 14 },
];

pub(crate) const MODEL_ATTRS: [wgpu::VertexAttribute; 5] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 20, shader_location: 2 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 24, shader_location: 3 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 28, shader_location: 13 },
];

pub(crate) const MODEL_MESH_ATTRS: [wgpu::VertexAttribute; 4] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 20, shader_location: 2 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32x2, offset: 24, shader_location: 13 },
];

pub(crate) const INSTANCE_ATTRS: [wgpu::VertexAttribute; 10] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: 4 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 5 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 6 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 7 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 64, shader_location: 8 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 80, shader_location: 9 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 92, shader_location: 10 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 96, shader_location: 11 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 108, shader_location: 12 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 116, shader_location: 14 },
];

pub(crate) const INSTANCE_MESH_ATTRS: [wgpu::VertexAttribute; 11] = [
    INSTANCE_ATTRS[0],
    INSTANCE_ATTRS[1],
    INSTANCE_ATTRS[2],
    INSTANCE_ATTRS[3],
    INSTANCE_ATTRS[4],
    INSTANCE_ATTRS[5],
    INSTANCE_ATTRS[6],
    INSTANCE_ATTRS[7],
    INSTANCE_ATTRS[8],
    INSTANCE_ATTRS[9],
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 120, shader_location: 3 },
];

pub(crate) fn model_mesh_buffers() -> [Option<wgpu::VertexBufferLayout<'static>>; 3] {
    [
        Some(vertex_layout(size_of::<GpuVertex>(), wgpu::VertexStepMode::Vertex, &MODEL_MESH_ATTRS)),
        Some(vertex_layout(size_of::<InstanceGpu>(), wgpu::VertexStepMode::Instance, &INSTANCE_MESH_ATTRS)),
        Some(vertex_layout(size_of::<u32>(), wgpu::VertexStepMode::Vertex, &TANGENT_ATTRS)),
    ]
}

pub(crate) const TANGENT_ATTRS: [wgpu::VertexAttribute; 1] =
    [wgpu::VertexAttribute { format: wgpu::VertexFormat::Snorm8x4, offset: 0, shader_location: 15 }];

pub(crate) const EDGE_ATTRS: [wgpu::VertexAttribute; 2] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 12, shader_location: 1 },
];

pub(crate) const COLOR_ATTRS: [wgpu::VertexAttribute; 2] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Unorm8x4, offset: 12, shader_location: 1 },
];

const SEGMENT_ATTRS: [wgpu::VertexAttribute; 3] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Unorm8x4, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 16, shader_location: 2 },
];

const POINT_ATTRS: [wgpu::VertexAttribute; 3] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Unorm8x4, offset: 16, shader_location: 2 },
];

const SPRITE_ATTRS: [wgpu::VertexAttribute; 4] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Unorm8x4, offset: 16, shader_location: 2 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 20, shader_location: 3 },
];

pub(crate) fn vertex_layout(
    stride: usize,
    step: wgpu::VertexStepMode,
    attrs: &[wgpu::VertexAttribute],
) -> wgpu::VertexBufferLayout<'_> {
    wgpu::VertexBufferLayout { array_stride: stride as u64, step_mode: step, attributes: attrs }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Blend {
    Replace,
    Alpha,
    Additive,
    Modulate,
    Premultiplied,
}

impl Blend {
    fn state(self) -> Option<wgpu::BlendState> {
        match self {
            Blend::Replace => None,
            Blend::Alpha => Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            Blend::Additive => Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Zero,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            Blend::Premultiplied => Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            Blend::Modulate => Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Dst,
                    dst_factor: wgpu::BlendFactor::Src,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Zero,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
        }
    }
}

pub(crate) struct Desc<'a> {
    pub label: &'a str,
    pub module: &'a wgpu::ShaderModule,
    pub vs: &'a str,
    pub fs: &'a str,
    pub layout: &'a wgpu::PipelineLayout,
    pub buffers: &'a [Option<wgpu::VertexBufferLayout<'a>>],
    pub topology: wgpu::PrimitiveTopology,
    pub cull: Option<wgpu::Face>,
    pub depth_test: bool,
    pub depth_write: bool,
    pub blend: Blend,
    pub constants: &'a [(&'a str, f64)],
}

pub(crate) fn build(device: &wgpu::Device, samples: u32, d: Desc<'_>) -> wgpu::RenderPipeline {
    build_with(device, samples, d, wgpu::PolygonMode::Fill)
}

pub(crate) fn build_with(
    device: &wgpu::Device,
    samples: u32,
    d: Desc<'_>,
    polygon_mode: wgpu::PolygonMode,
) -> wgpu::RenderPipeline {
    build_depth(device, samples, d, polygon_mode, None)
}

fn build_depth(
    device: &wgpu::Device,
    samples: u32,
    d: Desc<'_>,
    polygon_mode: wgpu::PolygonMode,
    depth_compare: Option<wgpu::CompareFunction>,
) -> wgpu::RenderPipeline {
    build_inner(device, samples, d, polygon_mode, depth_compare, true)
}

pub(crate) fn build_color_only(device: &wgpu::Device, d: Desc<'_>) -> wgpu::RenderPipeline {
    build_inner(device, 1, d, wgpu::PolygonMode::Fill, None, false)
}

fn build_inner(
    device: &wgpu::Device,
    samples: u32,
    d: Desc<'_>,
    polygon_mode: wgpu::PolygonMode,
    depth_compare: Option<wgpu::CompareFunction>,
    depth: bool,
) -> wgpu::RenderPipeline {
    let bias = if polygon_mode == wgpu::PolygonMode::Line && d.depth_test {
        wgpu::DepthBiasState { constant: 8, slope_scale: 1.0, clamp: 0.0 }
    } else {
        wgpu::DepthBiasState::default()
    };
    crate::pipeline_cache::with(device, |cache| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(d.label),
            layout: Some(d.layout),
            vertex: wgpu::VertexState {
                module: d.module,
                entry_point: Some(d.vs),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: d.constants,
                    zero_initialize_workgroup_memory: false,
                },
                buffers: d.buffers,
            },
            primitive: wgpu::PrimitiveState {
                topology: d.topology,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: d.cull,
                unclipped_depth: false,
                polygon_mode,
                conservative: false,
            },
            depth_stencil: depth.then_some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(d.depth_write),
                depth_compare: depth_compare.or(Some(if d.depth_test {
                    wgpu::CompareFunction::GreaterEqual
                } else {
                    wgpu::CompareFunction::Always
                })),
                stencil: wgpu::StencilState::default(),
                bias,
            }),
            multisample: wgpu::MultisampleState { count: samples, mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState {
                module: d.module,
                entry_point: Some(d.fs),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: d.constants,
                    zero_initialize_workgroup_memory: false,
                },
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: d.blend.state(),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache,
        })
    })
}

type Buffers<'a> = [Option<wgpu::VertexBufferLayout<'a>>];

impl PipelineSet {
    pub fn new(device: &wgpu::Device, shared: &Shared, samples: u32) -> Self {
        let chunk_vb = [Some(vertex_layout(size_of::<GpuVertex>(), wgpu::VertexStepMode::Vertex, &CHUNK_ATTRS))];
        let model_vb = [
            Some(vertex_layout(size_of::<GpuVertex>(), wgpu::VertexStepMode::Vertex, &MODEL_ATTRS)),
            Some(vertex_layout(size_of::<InstanceGpu>(), wgpu::VertexStepMode::Instance, &INSTANCE_ATTRS)),
        ];
        let model_mesh_vb = model_mesh_buffers();
        let edge_vb = [Some(vertex_layout(size_of::<EdgeVertex>(), wgpu::VertexStepMode::Vertex, &EDGE_ATTRS))];
        let dot_vb = [Some(vertex_layout(size_of::<EdgeVertex>(), wgpu::VertexStepMode::Instance, &EDGE_ATTRS))];
        let box_vb = [Some(vertex_layout(size_of::<InstanceGpu>(), wgpu::VertexStepMode::Instance, &INSTANCE_ATTRS))];
        let color_vb = [Some(vertex_layout(size_of::<ColorVertex>(), wgpu::VertexStepMode::Vertex, &COLOR_ATTRS))];
        let segment_vb =
            [Some(vertex_layout(2 * size_of::<ColorVertex>(), wgpu::VertexStepMode::Instance, &SEGMENT_ATTRS))];
        let point_vb = [Some(vertex_layout(size_of::<PointInstance>(), wgpu::VertexStepMode::Instance, &POINT_ATTRS))];
        let sprite_vb = [Some(vertex_layout(size_of::<SpriteGpu>(), wgpu::VertexStepMode::Instance, &SPRITE_ATTRS))];

        let mesh = |vs: &str, buffers: &Buffers<'_>, pass: MeshPass, two_sided: bool, equal: bool| {
            let (blend, depth_write, mode) = match pass {
                MeshPass::Opaque | MeshPass::AlphaTest => (Blend::Replace, true, 0.0),
                MeshPass::Translucent => (Blend::Alpha, false, 1.0),
                MeshPass::Additive => (Blend::Additive, false, 2.0),
                MeshPass::Modulate => (Blend::Modulate, false, 3.0),
            };
            let alpha_test = if pass == MeshPass::AlphaTest { 1.0 } else { 0.0 };
            let tint_filter = if matches!(pass, MeshPass::Opaque | MeshPass::AlphaTest) { 1.0 } else { 0.0 };
            build_depth(
                device,
                samples,
                Desc {
                    label: "hp-render mesh",
                    module: &shared.mesh,
                    vs,
                    fs: "fs_mesh",
                    layout: &shared.layout_material,
                    buffers,
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull: if two_sided { None } else { Some(wgpu::Face::Back) },
                    depth_test: true,
                    depth_write: depth_write && !equal,
                    blend,
                    constants: &[("ALPHA_TEST", alpha_test), ("BLEND_MODE", mode), ("TINT_FILTER", tint_filter)],
                },
                wgpu::PolygonMode::Fill,
                equal.then_some(wgpu::CompareFunction::Equal),
            )
        };
        let faded = |vs: &str, buffers: &Buffers<'_>, alpha_test: bool, two_sided: bool| {
            build(
                device,
                samples,
                Desc {
                    label: "hp-render faded mesh",
                    module: &shared.mesh,
                    vs,
                    fs: "fs_mesh",
                    layout: &shared.layout_material,
                    buffers,
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull: if two_sided { None } else { Some(wgpu::Face::Back) },
                    depth_test: true,
                    depth_write: false,
                    blend: Blend::Alpha,
                    constants: &[
                        ("ALPHA_TEST", if alpha_test { 1.0 } else { 0.0 }),
                        ("BLEND_MODE", 1.0),
                        ("TINT_FILTER", 2.0),
                        ("TINT_OPACITY", 1.0),
                    ],
                },
            )
        };
        let passes =
            [MeshPass::Opaque, MeshPass::AlphaTest, MeshPass::Translucent, MeshPass::Additive, MeshPass::Modulate];
        let pair = |a: &(dyn Fn() -> wgpu::RenderPipeline + Sync), b: &(dyn Fn() -> wgpu::RenderPipeline + Sync)| {
            let (a, b) = rayon::join(a, b);
            [a, b]
        };
        let ((chunk, model), (chunk_faded, model_faded)) = rayon::join(
            || {
                rayon::join(
                    || {
                        passes.map(|p| {
                            pair(&|| mesh("vs_chunk", &chunk_vb, p, false, false), &|| {
                                mesh("vs_chunk", &chunk_vb, p, true, false)
                            })
                        })
                    },
                    || {
                        passes.map(|p| {
                            pair(&|| mesh("vs_model", &model_mesh_vb, p, false, false), &|| {
                                mesh("vs_model", &model_mesh_vb, p, true, false)
                            })
                        })
                    },
                )
            },
            || {
                rayon::join(
                    || {
                        [false, true].map(|alpha| {
                            pair(&|| faded("vs_chunk", &chunk_vb, alpha, false), &|| {
                                faded("vs_chunk", &chunk_vb, alpha, true)
                            })
                        })
                    },
                    || {
                        [false, true].map(|alpha| {
                            pair(&|| faded("vs_model", &model_mesh_vb, alpha, false), &|| {
                                faded("vs_model", &model_mesh_vb, alpha, true)
                            })
                        })
                    },
                )
            },
        );

        let lines = |vs: &str, buffers: &Buffers<'_>, constants: &[(&str, f64)], depth: bool| {
            build(
                device,
                samples,
                Desc {
                    label: "hp-render lines",
                    module: &shared.lines,
                    vs,
                    fs: "fs_color",
                    layout: &shared.layout_view,
                    buffers,
                    topology: wgpu::PrimitiveTopology::LineList,
                    cull: None,
                    depth_test: depth,
                    depth_write: false,
                    blend: Blend::Alpha,
                    constants,
                },
            )
        };
        let bias = |depth: bool| if depth { EDGE_DEPTH_BIAS } else { 0.0 };
        let edge = |filter: f64, depth: bool| {
            lines("vs_edge", &edge_vb, &[("FILTER", filter), ("DEPTH_BIAS", bias(depth))], depth)
        };
        let boxes = |filter: f64, depth: bool, box_filter: f64| {
            let constants = [("FILTER", filter), ("DEPTH_BIAS", bias(depth)), ("BOX_FILTER", box_filter)];
            lines("vs_box", &box_vb, &constants, depth)
        };
        let quads = |label: &str,
                     vs: &str,
                     layout: &wgpu::PipelineLayout,
                     buffers: &Buffers<'_>,
                     depth_bias: f64,
                     depth: bool| {
            build(
                device,
                samples,
                Desc {
                    label,
                    module: &shared.lines,
                    vs,
                    fs: "fs_color",
                    layout,
                    buffers,
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull: None,
                    depth_test: depth,
                    depth_write: false,
                    blend: Blend::Alpha,
                    constants: &[("FILTER", 0.0), ("DEPTH_BIAS", depth_bias)],
                },
            )
        };
        let view_layout = &shared.layout_view;
        let tris = |depth: bool| {
            let b = if depth { EDGE_DEPTH_BIAS * 0.5 } else { 0.0 };
            quads("hp-render overlay tris", "vs_color", view_layout, &color_vb, b, depth)
        };
        let wide = |depth: bool| {
            quads("hp-render wide lines", "vs_wide", &shared.layout_wide, &segment_vb, bias(depth), depth)
        };
        let model_edge =
            |filter: f64| lines("vs_model_edge", &model_vb, &[("FILTER", filter), ("DEPTH_BIAS", 0.0)], false);
        let wire_rgb = |c: [u8; 3]| f64::from(u32::from(c[0]) | u32::from(c[1]) << 8 | u32::from(c[2]) << 16);
        let game_wire = |label, vs, buffers: &Buffers<'_>, topology, depth: bool, rgb: [u8; 3], mode, set: f64| {
            let constants = [("WIRE_RGB", wire_rgb(rgb)), ("WIRE_SET", set)];
            let d = Desc {
                label,
                module: &shared.mesh,
                vs,
                fs: "fs_game_wire",
                layout: &shared.layout_view,
                buffers,
                topology,
                cull: (mode == wgpu::PolygonMode::Line).then_some(wgpu::Face::Back),
                depth_test: depth,
                depth_write: false,
                blend: Blend::Replace,
                constants: &constants,
            };
            build_with(device, samples, d, mode)
        };
        let lines_mode = device.features().contains(wgpu::Features::POLYGON_MODE_LINE);
        let tri = wgpu::PrimitiveTopology::TriangleList;
        let line_list = wgpu::PrimitiveTopology::LineList;
        let line = wgpu::PolygonMode::Line;
        let fill = wgpu::PolygonMode::Fill;
        let set_tris = |set: f64| {
            [
                game_wire("hp-render game wire world", "vs_chunk_wire", &chunk_vb, tri, false, WORLD_PINK, line, set),
                game_wire("hp-render game wire world z", "vs_chunk_wire", &chunk_vb, tri, true, WORLD_GREEN, line, set),
            ]
        };
        let outlines = |set: f64| {
            game_wire(
                "hp-render game wire outlines",
                "vs_chunk_outline",
                &chunk_vb,
                line_list,
                false,
                WORLD_PINK,
                fill,
                set,
            )
        };
        let entity_faces = |alpha_test: bool, two_sided: bool| {
            build(
                device,
                samples,
                Desc {
                    label: "hp-render game wire entity faces",
                    module: &shared.mesh,
                    vs: "vs_chunk",
                    fs: "fs_mesh",
                    layout: &shared.layout_material,
                    buffers: &chunk_vb,
                    topology: tri,
                    cull: if two_sided { None } else { Some(wgpu::Face::Back) },
                    depth_test: true,
                    depth_write: true,
                    blend: Blend::Replace,
                    constants: &[
                        ("ALPHA_TEST", if alpha_test { 1.0 } else { 0.0 }),
                        ("BLEND_MODE", 0.0),
                        ("TINT_FILTER", 1.0),
                        ("ENTITY_ONLY", 1.0),
                    ],
                },
            )
        };
        let game_wire = GameWirePipelines {
            world_tris: lines_mode.then(|| [set_tris(1.0), set_tris(2.0)]),
            world_outlines: [outlines(1.0), outlines(2.0)],
            entity_faces: [
                [entity_faces(false, false), entity_faces(false, true)],
                [entity_faces(true, false), entity_faces(true, true)],
            ],
            models: lines_mode.then(|| {
                game_wire("hp-render game wire models", "vs_model_lod", &model_vb, tri, false, MODEL_CYAN, line, 0.0)
            }),
            model_edges: game_wire(
                "hp-render game wire model edges",
                "vs_model_lod",
                &model_vb,
                line_list,
                false,
                MODEL_CYAN,
                fill,
                0.0,
            ),
        };
        let prepass_pipe = |vs: &str, buffers: &Buffers<'_>, two_sided: bool| {
            crate::pipeline_cache::with(device, |cache| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("hp-render depth prepass"),
                    layout: Some(&shared.layout_view),
                    vertex: wgpu::VertexState {
                        module: &shared.mesh,
                        entry_point: Some(vs),
                        compilation_options: wgpu::PipelineCompilationOptions {
                            constants: &[("ALPHA_TEST", 0.0), ("BLEND_MODE", 0.0), ("TINT_FILTER", 1.0)],
                            zero_initialize_workgroup_memory: false,
                        },
                        buffers,
                    },
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: if two_sided { None } else { Some(wgpu::Face::Back) },
                        unclipped_depth: false,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        conservative: false,
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: Some(true),
                        depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState { count: samples, mask: !0, alpha_to_coverage_enabled: false },
                    fragment: Some(wgpu::FragmentState {
                        module: &shared.mesh,
                        entry_point: Some("fs_depth"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: COLOR_FORMAT,
                            blend: None,
                            write_mask: wgpu::ColorWrites::empty(),
                        })],
                    }),
                    multiview_mask: None,
                    cache,
                })
            })
        };
        let prepass = [prepass_pipe("vs_chunk", &chunk_vb, false), prepass_pipe("vs_chunk", &chunk_vb, true)];
        let chunk_equal = [false, true].map(|two| mesh("vs_chunk", &chunk_vb, MeshPass::Opaque, two, true));
        let model_prepass =
            [prepass_pipe("vs_model", &model_mesh_vb, false), prepass_pipe("vs_model", &model_mesh_vb, true)];
        let model_equal = [false, true].map(|two| mesh("vs_model", &model_mesh_vb, MeshPass::Opaque, two, true));
        Self {
            chunk,
            chunk_faded,
            prepass,
            chunk_equal,
            model_prepass,
            model_equal,
            model,
            model_faded,
            edge_depth: edge(1.0, true),
            edge_unselected: edge(1.0, false),
            edge_selected: edge(2.0, false),
            edge_always: edge(3.0, true),
            edge_invalid: edge(4.0, false),
            box_depth: boxes(0.0, true, 0.0),
            box_far: boxes(1.0, true, 1.0),
            box_unselected: boxes(1.0, false, 0.0),
            box_selected: boxes(2.0, false, 0.0),
            model_edges: [model_edge(1.0), model_edge(2.0)],
            game_wire,
            collision: lines(
                "vs_model_edge",
                &model_vb,
                &[("FILTER", 0.0), ("DEPTH_BIAS", EDGE_DEPTH_BIAS), ("COLLISION", 1.0)],
                true,
            ),
            collision_hull: build(
                device,
                samples,
                Desc {
                    label: "hp-render collision hulls",
                    module: &shared.lines,
                    vs: "vs_collision_hull",
                    fs: "fs_color",
                    layout: &shared.layout_view,
                    buffers: &model_vb,
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull: None,
                    depth_test: true,
                    depth_write: false,
                    blend: Blend::Alpha,
                    constants: &[("FILTER", 0.0), ("DEPTH_BIAS", 0.0)],
                },
            ),
            box_no_collision: lines(
                "vs_box",
                &box_vb,
                &[("FILTER", 0.0), ("DEPTH_BIAS", EDGE_DEPTH_BIAS), ("BOX_FILTER", 0.0), ("NO_COLLISION", 1.0)],
                true,
            ),
            dots: quads("hp-render dots", "vs_dot", view_layout, &dot_vb, 0.0, false),
            marks: [
                quads("hp-render walk marks", "vs_mark", view_layout, &edge_vb, EDGE_DEPTH_BIAS * 0.5, true),
                quads("hp-render walk marks 2d", "vs_mark", view_layout, &edge_vb, 0.0, false),
                build(
                    device,
                    samples,
                    Desc {
                        label: "hp-render walk marks wireframe",
                        module: &shared.lines,
                        vs: "vs_mark",
                        fs: "fs_color",
                        layout: view_layout,
                        buffers: &edge_vb,
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        cull: None,
                        depth_test: true,
                        depth_write: true,
                        blend: Blend::Alpha,
                        constants: &[("FILTER", 0.0), ("DEPTH_BIAS", 0.0)],
                    },
                ),
            ],
            fill: build(
                device,
                samples,
                Desc {
                    label: "hp-render fill",
                    module: &shared.mesh,
                    vs: "vs_chunk",
                    fs: "fs_fill",
                    layout: &shared.layout_material,
                    buffers: &chunk_vb,
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull: Some(wgpu::Face::Back),
                    depth_test: false,
                    depth_write: false,
                    blend: Blend::Alpha,
                    constants: &[("SELECTED_ONLY", 1.0)],
                },
            ),
            sprite: build(
                device,
                samples,
                Desc {
                    label: "hp-render sprite",
                    module: &shared.sprite,
                    vs: "vs_sprite",
                    fs: "fs_sprite",
                    layout: &shared.layout_material,
                    buffers: &sprite_vb,
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull: None,
                    depth_test: true,
                    depth_write: false,
                    blend: Blend::Alpha,
                    constants: &[],
                },
            ),
            overlay_lines: [
                lines("vs_color", &color_vb, &[("FILTER", 0.0), ("DEPTH_BIAS", EDGE_DEPTH_BIAS)], true),
                lines("vs_color", &color_vb, &[("FILTER", 0.0), ("DEPTH_BIAS", 0.0)], false),
            ],
            overlay_tris: [tris(true), tris(false)],
            wide_lines: [wide(true), wide(false)],
            points: quads("hp-render points", "vs_point", view_layout, &point_vb, 0.0, false),
            grid: build(
                device,
                samples,
                Desc {
                    label: "hp-render grid",
                    module: &shared.grid,
                    vs: "vs_full",
                    fs: "fs_grid",
                    layout: view_layout,
                    buffers: &[],
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull: None,
                    depth_test: false,
                    depth_write: false,
                    blend: Blend::Alpha,
                    constants: &[],
                },
            ),
            sky: build(
                device,
                samples,
                Desc {
                    label: "hp-render sky",
                    module: &shared.sky,
                    vs: "vs_sky",
                    fs: "fs_sky",
                    layout: &shared.layout_sky,
                    buffers: &[],
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull: None,
                    depth_test: false,
                    depth_write: false,
                    blend: Blend::Replace,
                    constants: &[],
                },
            ),
        }
    }
}

pub(crate) struct ShadowPipelines {
    pub chunk: wgpu::RenderPipeline,
    pub chunk_alpha: wgpu::RenderPipeline,
    pub model: wgpu::RenderPipeline,
    pub model_alpha: wgpu::RenderPipeline,
    pub sky_cut: wgpu::RenderPipeline,
    pub sky_peel: wgpu::RenderPipeline,
}

impl ShadowPipelines {
    pub fn new(device: &wgpu::Device, shared: &Shared) -> Self {
        let chunk_vb = [Some(vertex_layout(size_of::<GpuVertex>(), wgpu::VertexStepMode::Vertex, &CHUNK_ATTRS))];
        let model_vb = model_mesh_buffers();
        let build = |vs: &str, fs: &str, alpha: bool, buffers: &[Option<wgpu::VertexBufferLayout<'_>>]| {
            let constants: &[(&str, f64)] = &[("ALPHA_TEST", 0.0), ("BLEND_MODE", 0.0)];
            let sky_faces = fs == "fs_sky_cut" || fs == "fs_sky_peel";
            crate::pipeline_cache::with(device, |cache| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("hp-render shadow"),
                    layout: Some(if alpha { &shared.layout_material } else { &shared.layout_view }),
                    vertex: wgpu::VertexState {
                        module: &shared.mesh,
                        entry_point: Some(vs),
                        compilation_options: wgpu::PipelineCompilationOptions {
                            constants,
                            zero_initialize_workgroup_memory: false,
                        },
                        buffers,
                    },
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        unclipped_depth: false,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        conservative: false,
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: Some(true),
                        depth_compare: Some(if sky_faces {
                            wgpu::CompareFunction::GreaterEqual
                        } else {
                            wgpu::CompareFunction::LessEqual
                        }),
                        stencil: wgpu::StencilState::default(),
                        bias: if sky_faces {
                            wgpu::DepthBiasState::default()
                        } else {
                            wgpu::DepthBiasState { constant: 0, slope_scale: 1.5, clamp: 0.0 }
                        },
                    }),
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shared.mesh,
                        entry_point: Some(fs),
                        compilation_options: wgpu::PipelineCompilationOptions {
                            constants,
                            zero_initialize_workgroup_memory: false,
                        },
                        targets: &[],
                    }),
                    multiview_mask: None,
                    cache,
                })
            })
        };
        let set = Self {
            chunk: build("vs_chunk", "fs_shadow_cut", false, &chunk_vb),
            chunk_alpha: build("vs_chunk", "fs_shadow_alpha", true, &chunk_vb),
            model: build("vs_model", "fs_shadow_cut", false, &model_vb),
            model_alpha: build("vs_model", "fs_shadow_alpha", true, &model_vb),
            sky_cut: build("vs_chunk", "fs_sky_cut", false, &chunk_vb),
            sky_peel: build("vs_chunk", "fs_sky_peel", false, &chunk_vb),
        };
        crate::pipeline_cache::save_if_dirty(device);
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shaders_parse_and_validate() {
        let modules = [
            ("mesh", MESH_WGSL),
            ("particles", crate::particles::PARTICLE_WGSL),
            ("lightmap", crate::lightmap::SHADER),
            ("compiled_light", crate::compiled_light::SHADER),
            ("lines", LINES_WGSL),
            ("sprite", SPRITE_WGSL),
            ("grid", GRID_WGSL),
            ("sky", SKY_WGSL),
        ];
        for (name, src) in modules {
            assert!(!src.contains('\u{2014}') && !src.contains('\u{2013}'), "{name} contains a dash character");
            assert!(src.contains("struct View"), "{name} misses the common block");
            let module = wgpu::naga::front::wgsl::parse_str(src)
                .unwrap_or_else(|error| panic!("{name}: {}", error.emit_to_string(src)));
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|error| panic!("{name}: {}", error.emit_to_string(src)));
        }
    }
}
