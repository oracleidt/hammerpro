use std::borrow::Cow;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::camera::Camera;
use crate::occlusion::CandidateGpu;
use crate::pipelines::{COLOR_FORMAT, DEPTH_FORMAT, MESH_PASSES};
use crate::types::{OrthoAxis, RenderMode, ViewKind, ViewOptions, ViewParams};
use crate::{ChunkDraw, DrawCtx, FrameCounter, Group, Renderer};

pub(crate) type Ranges = Vec<(u32, u32)>;

static SERIAL: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_serial() -> u64 {
    SERIAL.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn scene_options(o: ViewOptions) -> ViewOptions {
    ViewOptions { time: 0.0, screen_fade: [0.0; 4], fade_modulate: false, light_cap: 0, ..o }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct SceneStamp {
    pub content: u64,
    pub epoch: u64,
    pub selection: (u64, u64),
}

impl Renderer {
    pub(crate) fn scene_stamp(&self) -> SceneStamp {
        SceneStamp {
            content: self.content_gen.wrapping_sub(self.state_only_bumps),
            epoch: self.draw_epoch,
            selection: (self.scene.selected_gen, self.scene.membership_gen),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct ListKey {
    pub stamp: SceneStamp,
    pub view_proj: [f32; 16],
    pub eye: [f32; 3],
    pub opts: ViewOptions,
    pub mode: RenderMode,
    pub flags: u8,
}

pub(crate) const LIST_SKY_HOLES: u8 = 1;
pub(crate) const LIST_WATER: u8 = 2;
pub(crate) const LIST_REFLECTING: u8 = 4;
pub(crate) const LIST_PROBING: u8 = 8;
pub(crate) const LIST_COMPILED: u8 = 16;

pub(crate) struct FrameLists {
    pub key: ListKey,
    pub serial: u64,
    pub lists: [Vec<ChunkDraw>; MESH_PASSES],
    pub counted: FrameCounter,
    pub animated: bool,
    edges: OnceLock<(Ranges, FrameCounter)>,
    selected_edges: OnceLock<Ranges>,
    marks: OnceLock<(Ranges, FrameCounter)>,
    faded: OnceLock<Vec<ChunkDraw>>,
    blended: OnceLock<Vec<ChunkDraw>>,
    args: OnceLock<Vec<u8>>,
    candidates: OnceLock<(Vec<ChunkDraw>, Vec<CandidateGpu>)>,
}

impl FrameLists {
    pub fn new(key: ListKey, lists: [Vec<ChunkDraw>; MESH_PASSES], counted: FrameCounter, animated: bool) -> Self {
        Self {
            key,
            serial: next_serial(),
            lists,
            counted,
            animated,
            edges: OnceLock::new(),
            selected_edges: OnceLock::new(),
            marks: OnceLock::new(),
            faded: OnceLock::new(),
            blended: OnceLock::new(),
            args: OnceLock::new(),
            candidates: OnceLock::new(),
        }
    }

    pub fn world_args(&self) -> &[u8] {
        self.args.get_or_init(|| {
            self.lists[0]
                .iter()
                .chain(&self.lists[1])
                .flat_map(|d| bytemuck::cast::<[u32; 5], [u8; 20]>([d.count, 1, d.first, 0, 0]))
                .collect()
        })
    }

    pub fn candidates(&self) -> (&[ChunkDraw], &[CandidateGpu]) {
        let (draws, gpu) = self.candidates.get_or_init(|| {
            let draws: Vec<ChunkDraw> = self.lists[0].iter().chain(&self.lists[1]).copied().collect();
            let gpu = draws.iter().map(ChunkDraw::candidate).collect();
            (draws, gpu)
        });
        (draws, gpu)
    }

    pub fn blended(&self) -> &[ChunkDraw] {
        self.blended.get_or_init(|| sorted_blended(&self.lists))
    }
}

pub(crate) fn sorted_blended(lists: &[Vec<ChunkDraw>; MESH_PASSES]) -> Vec<ChunkDraw> {
    let mut sorted: Vec<ChunkDraw> = Vec::with_capacity(lists[2].len() + lists[3].len());
    sorted.extend_from_slice(&lists[2]);
    sorted.extend_from_slice(&lists[3]);
    sorted.sort_by(|a, b| b.dist.total_cmp(&a.dist));
    sorted
}

impl<'a> DrawCtx<'a> {
    pub(crate) fn list_key(&self) -> ListKey {
        let r = self.r;
        let mut flags = 0;
        for (on, bit) in [
            (self.sky_holes, LIST_SKY_HOLES),
            (self.water.is_some(), LIST_WATER),
            (r.reflecting.is_some(), LIST_REFLECTING),
            (r.probing, LIST_PROBING),
            (r.compiled.present, LIST_COMPILED),
        ] {
            if on {
                flags |= bit;
            }
        }
        ListKey {
            stamp: r.scene_stamp(),
            view_proj: self.cam.view_proj.to_cols_array(),
            eye: self.cam.eye.to_array(),
            opts: scene_options(*self.opts),
            mode: self.params.mode,
            flags,
        }
    }

    pub(crate) fn build_frame(&self, key: ListKey) -> FrameLists {
        let mut counted = FrameCounter::default();
        let lists = self.collect_chunks(&mut counted);
        let animated = self.r.shows_animation(&lists);
        FrameLists::new(key, lists, counted, animated)
    }

    pub(crate) fn frame_lists(&self, cached: Option<FrameLists>) -> FrameLists {
        let key = self.list_key();
        match cached {
            Some(f) if f.key == key => f,
            _ => self.build_frame(key),
        }
    }

    pub(crate) fn edge_ranges(&self, n: &mut FrameCounter) -> Cow<'a, [(u32, u32)]> {
        match self.frame {
            Some(f) => {
                let (ranges, counted) = f.edges.get_or_init(|| {
                    let mut c = FrameCounter::default();
                    (self.visible_vertex_chunks(&self.r.scene.edge_chunks, &mut c), c)
                });
                n.absorb(counted);
                Cow::Borrowed(ranges)
            }
            None => Cow::Owned(self.visible_vertex_chunks(&self.r.scene.edge_chunks, n)),
        }
    }

    pub(crate) fn mark_ranges(&self, n: &mut FrameCounter) -> Cow<'a, [(u32, u32)]> {
        if !self.opts.walk_marks || self.r.scene.mark_chunks.is_empty() {
            return Cow::Borrowed(&[]);
        }
        match self.frame {
            Some(f) => {
                let (ranges, counted) = f.marks.get_or_init(|| {
                    let mut c = FrameCounter::default();
                    (self.visible_vertex_chunks(&self.r.scene.mark_chunks, &mut c), c)
                });
                n.absorb(counted);
                Cow::Borrowed(ranges)
            }
            None => Cow::Owned(self.visible_vertex_chunks(&self.r.scene.mark_chunks, n)),
        }
    }

    pub(crate) fn selected_ranges(&self) -> Cow<'a, [(u32, u32)]> {
        match self.frame {
            Some(f) => Cow::Borrowed(f.selected_edges.get_or_init(|| self.selected_edge_list())),
            None => Cow::Owned(self.selected_edge_list()),
        }
    }

    pub(crate) fn faded_list(&self) -> Cow<'a, [ChunkDraw]> {
        match self.frame {
            Some(f) => Cow::Borrowed(f.faded.get_or_init(|| self.faded_chunks())),
            None => Cow::Owned(self.faded_chunks()),
        }
    }

    pub(crate) fn blended_list(&self, lists: &[Vec<ChunkDraw>; MESH_PASSES]) -> Cow<'a, [ChunkDraw]> {
        match self.frame {
            Some(f) => Cow::Borrowed(f.blended()),
            None => Cow::Owned(sorted_blended(lists)),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Key2d {
    pub stamp: SceneStamp,
    pub scene_gen: u64,
    pub bind: (u64, u64, u64),
    pub view_gen: u64,
    pub samples: u32,
    pub size: [u32; 2],
    pub axis: OrthoAxis,
    pub ppu: f32,
    pub opts: ViewOptions,
    pub grid: bool,
    pub models: bool,
    pub sprites: bool,
    pub dots: bool,
    pub bg_ready: bool,
}

pub(crate) struct Bundle2d {
    pub key: Key2d,
    #[cfg(test)]
    pub serial: u64,
    pub center: [f32; 2],
    pub reach: [f32; 2],
    pub below: wgpu::RenderBundle,
    pub above: wgpu::RenderBundle,
    pub counted: FrameCounter,
    pub counted_above: FrameCounter,
}

pub(crate) const WINDOW_SCALE: u32 = 2;

pub(crate) fn window_camera(params: &ViewParams) -> Camera {
    let size = params.size.map(|s| s.max(1).saturating_mul(WINDOW_SCALE));
    Camera::new(&ViewParams { size, ..*params })
}

pub(crate) fn half_extent(cam: &Camera) -> [f32; 2] {
    let ppu = cam.ppu.max(1e-5);
    [cam.half_px[0] / ppu, cam.half_px[1] / ppu]
}

pub(crate) fn window_covers(center: [f32; 2], reach: [f32; 2], cam: &Camera) -> bool {
    let half = half_extent(cam);
    (0..2).all(|i| (cam.ortho_center[i] - center[i]).abs() + half[i] <= reach[i])
}

impl Bundle2d {
    pub fn reusable(&self, key: &Key2d, cam: &Camera) -> bool {
        self.key == *key && window_covers(self.center, self.reach, cam)
    }
}

pub(crate) fn bundle_encoder<'e>(device: &'e wgpu::Device, samples: u32, label: &str) -> wgpu::RenderBundleEncoder<'e> {
    device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
        label: Some(label),
        color_formats: &[Some(COLOR_FORMAT)],
        depth_stencil: Some(wgpu::RenderBundleDepthStencil {
            format: DEPTH_FORMAT,
            depth_read_only: false,
            stencil_read_only: true,
        }),
        sample_count: samples,
        multiview: None,
    })
}

impl<'a> DrawCtx<'a> {
    pub(crate) fn key_2d(&self, scene_gen: u64) -> Option<Key2d> {
        let ViewKind::Ortho { axis, .. } = self.params.kind else { return None };
        let dots = self.opts.vertex_dots_min_zoom;
        Some(Key2d {
            stamp: self.r.scene_stamp(),
            scene_gen,
            bind: self.view.bind_key,
            view_gen: self.view.generation,
            samples: self.view.samples,
            size: self.view.size,
            axis,
            ppu: self.cam.ppu,
            opts: scene_options(*self.opts),
            grid: self.params.grid.is_some(),
            models: self.params.show_models,
            sprites: self.params.show_sprites,
            dots: dots > 0.0 && self.cam.ppu >= dots,
            bg_ready: self.r.bg_pipes.is_some(),
        })
    }

    pub(crate) fn build_bundle_2d(&self, key: Key2d) -> Bundle2d {
        let window = window_camera(self.params);
        let wide = DrawCtx { cam: &window, frame: None, ..*self };
        let device = &self.r.device;
        let mut counted = FrameCounter::default();
        let mut enc = bundle_encoder(device, self.view.samples, "hp-render 2D scene");
        wgpu::util::RenderEncoder::set_bind_group(&mut enc, 0, Some(self.bind_group), &[]);
        wide.record_2d_static(&mut enc, &mut counted);
        let below = enc.finish(&wgpu::RenderBundleDescriptor { label: Some("hp-render 2D scene") });
        let mut counted_above = FrameCounter::default();
        let mut enc = bundle_encoder(device, self.view.samples, "hp-render 2D lines on top");
        wgpu::util::RenderEncoder::set_bind_group(&mut enc, 0, Some(self.bind_group), &[]);
        wide.draw_static_lines(&mut enc, true, &mut counted_above);
        let above = enc.finish(&wgpu::RenderBundleDescriptor { label: Some("hp-render 2D lines on top") });
        Bundle2d {
            key,
            #[cfg(test)]
            serial: next_serial(),
            center: window.ortho_center,
            reach: half_extent(&window),
            below,
            above,
            counted,
            counted_above,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct PackKey {
    pub stamp: SceneStamp,
    pub view_proj: [f32; 16],
    pub up: [f32; 3],
    pub height: f32,
    pub lods: bool,
    pub enabled: bool,
}

pub(crate) struct PackCache {
    pub key: PackKey,
    #[cfg(test)]
    pub serial: u64,
    pub buffer: Option<wgpu::Buffer>,
    pub len: u64,
    pub groups: Arc<[Group]>,
    pub skipped: bool,
}

#[cfg(test)]
#[path = "frame_cache_tests.rs"]
mod tests;
