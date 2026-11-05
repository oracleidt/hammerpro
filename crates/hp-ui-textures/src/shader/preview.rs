use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use hp_shader::{EvalOptions, Evaluation, FileLoader, Graph, ImageCache, ImageLoader, Probe};
use hp_vtf::RgbaImage;

pub struct EditorLoader {
    pub files: FileLoader,
    pub fs: Option<hp_fs::GameFs>,
}

impl EditorLoader {
    pub fn new(base_dirs: Vec<std::path::PathBuf>, fs: Option<hp_fs::GameFs>) -> Self {
        Self { files: FileLoader::new(base_dirs), fs }
    }
}

impl ImageLoader for EditorLoader {
    fn load(&self, source: &str, max_dim: u32) -> Result<RgbaImage, String> {
        if self.files.resolve(source).is_some() {
            return self.files.read(source, max_dim);
        }
        let fs = self.fs.as_ref().ok_or_else(|| format!("file not found: {source}"))?;
        let tex = fs.load_texture(source, Some(max_dim)).map_err(|e| format!("{source}: {e}"))?;
        RgbaImage::new(tex.width, tex.height, tex.rgba).map_err(|e| format!("{source}: {e}"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Channel {
    #[default]
    Color,
    Rgb,
    Alpha,
    Red,
    Green,
    Blue,
}

impl Channel {
    pub const ALL: [Channel; 6] =
        [Channel::Color, Channel::Rgb, Channel::Alpha, Channel::Red, Channel::Green, Channel::Blue];

    pub fn key(self) -> &'static str {
        match self {
            Channel::Color => "color",
            Channel::Rgb => "rgb",
            Channel::Alpha => "alpha",
            Channel::Red => "r",
            Channel::Green => "g",
            Channel::Blue => "b",
        }
    }
}

#[derive(Clone, PartialEq)]
struct Key {
    graph: Graph,
    probe: Option<Probe>,
    res: u32,
    thumbs: bool,
    generation: u64,
}

struct Running {
    rx: Receiver<(Evaluation, Duration)>,
    cancel: Arc<AtomicBool>,
    key: Key,
}

#[derive(Default)]
pub struct Preview {
    running: Option<Running>,
    done: Option<Key>,
    pub result: Option<Evaluation>,
    pub image: Option<egui::TextureHandle>,
    pub thumbs: HashMap<u32, egui::TextureHandle>,
    uploaded: Option<(u64, Channel)>,
    serial: u64,
    pub elapsed: Duration,
    pub generation: u64,
    pub cache: Arc<ImageCache>,
}

pub struct Params<'a> {
    pub graph: &'a Graph,
    pub probe: Option<&'a Probe>,
    pub res: u32,
    pub thumbs: bool,
    pub channel: Channel,
    pub loader: Arc<dyn ImageLoader>,
}

fn channel_pixels(img: &RgbaImage, channel: Channel) -> Vec<u8> {
    let mut out = img.pixels.clone();
    for p in out.chunks_exact_mut(4) {
        match channel {
            Channel::Color => {}
            Channel::Rgb => p[3] = 255,
            Channel::Alpha => {
                let a = p[3];
                p.copy_from_slice(&[a, a, a, 255]);
            }
            Channel::Red => p.copy_from_slice(&[p[0], p[0], p[0], 255]),
            Channel::Green => p.copy_from_slice(&[p[1], p[1], p[1], 255]),
            Channel::Blue => p.copy_from_slice(&[p[2], p[2], p[2], 255]),
        }
    }
    out
}

fn color_image(img: &RgbaImage, channel: Channel) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &channel_pixels(img, channel))
}

impl Preview {
    pub fn busy(&self) -> bool {
        self.running.is_some()
    }

    pub fn invalidate(&mut self) {
        self.generation += 1;
        self.cache.clear();
    }

    pub fn update(&mut self, ctx: &egui::Context, p: &Params<'_>) {
        if let Some(run) = &self.running {
            match run.rx.try_recv() {
                Ok((eval, took)) => {
                    let key = run.key.clone();
                    self.running = None;
                    if !eval.cancelled {
                        self.done = Some(key);
                        self.result = Some(eval);
                        self.serial += 1;
                        self.elapsed = took;
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.running = None,
            }
        }
        let mut content = p.graph.clone();
        for n in &mut content.nodes {
            n.pos = [0.0, 0.0];
        }
        let want =
            Key { graph: content, probe: p.probe.cloned(), res: p.res, thumbs: p.thumbs, generation: self.generation };
        match &self.running {
            Some(run) => {
                if run.key != want {
                    run.cancel.store(true, Ordering::Relaxed);
                }
                ctx.request_repaint_after(Duration::from_millis(16));
            }
            None => {
                if self.done.as_ref() != Some(&want) {
                    self.spawn(ctx, want, p.loader.clone());
                }
            }
        }
        self.upload(ctx, p.channel);
    }

    fn spawn(&mut self, ctx: &egui::Context, key: Key, loader: Arc<dyn ImageLoader>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let job = (key.clone(), cancel.clone(), self.cache.clone(), ctx.clone());
        let spawned = std::thread::Builder::new().name("hp-shader-preview".to_owned()).spawn(move || {
            let (key, cancel, cache, ctx) = job;
            let start = Instant::now();
            let (fw, fh) = hp_shader::auto_size(&key.graph, loader.as_ref(), &cache);
            let scale = key.res as f32 / fw.max(fh) as f32;
            let w = ((fw as f32 * scale).round() as u32).clamp(4, key.res);
            let h = ((fh as f32 * scale).round() as u32).clamp(4, key.res);
            let opts = EvalOptions {
                width: w,
                height: h,
                outputs: false,
                probe: key.probe.clone().or_else(|| {
                    key.graph.output_ids().first().map(|id| Probe { node: *id, output: "color".to_owned() })
                }),
                thumbs: if key.thumbs { 72 } else { 0 },
                cancel: Some(cancel),
            };
            let eval = hp_shader::evaluate(&key.graph, loader.as_ref(), &cache, &opts);
            let _ = tx.send((eval, start.elapsed()));
            ctx.request_repaint();
        });
        if spawned.is_ok() {
            self.running = Some(Running { rx, cancel, key });
        } else {
            self.done = Some(key);
        }
    }

    fn upload(&mut self, ctx: &egui::Context, channel: Channel) {
        let Some(result) = &self.result else {
            return;
        };
        if self.uploaded == Some((self.serial, channel)) {
            return;
        }
        let fresh = self.uploaded.is_none_or(|(s, _)| s != self.serial);
        self.uploaded = Some((self.serial, channel));
        match &result.probe {
            Some(img) => {
                let ci = color_image(img, channel);
                match &mut self.image {
                    Some(tex) => tex.set(ci, egui::TextureOptions::LINEAR),
                    None => {
                        self.image = Some(ctx.load_texture("hp_shader_preview", ci, egui::TextureOptions::LINEAR));
                    }
                }
            }
            None => self.image = None,
        }
        if fresh {
            self.thumbs.retain(|id, _| result.thumbs.contains_key(id));
            for (id, img) in &result.thumbs {
                let ci = color_image(img, Channel::Rgb);
                match self.thumbs.get_mut(id) {
                    Some(tex) => tex.set(ci, egui::TextureOptions::LINEAR),
                    None => {
                        let tex = ctx.load_texture(format!("hp_shader_thumb_{id}"), ci, egui::TextureOptions::LINEAR);
                        self.thumbs.insert(*id, tex);
                    }
                }
            }
        }
    }
}
