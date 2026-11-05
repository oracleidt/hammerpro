use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use hp_vtf::RgbaImage;
use rayon::prelude::*;

use crate::catalog::{NodeDef, PinTy, node_def};
use crate::graph::{Diagnostic, Graph, Node, Params, Severity};
use crate::ops::{self, NoiseSpec, V4, fv, lerp4, lum, smooth};
use crate::{DEFAULT_BAKE_SIZE, MAX_BAKE_SIZE};

pub trait ImageLoader: Send + Sync {
    fn load(&self, source: &str, max_dim: u32) -> Result<RgbaImage, String>;
}

#[derive(Clone, Debug, Default)]
pub struct FileLoader {
    pub base_dirs: Vec<PathBuf>,
}

impl FileLoader {
    pub fn new(base_dirs: Vec<PathBuf>) -> Self {
        Self { base_dirs }
    }

    pub fn resolve(&self, source: &str) -> Option<PathBuf> {
        let p = Path::new(source.trim());
        if p.is_absolute() {
            return p.is_file().then(|| p.to_path_buf());
        }
        self.base_dirs.iter().map(|d| d.join(p)).find(|c| c.is_file())
    }

    pub fn read(&self, source: &str, max_dim: u32) -> Result<RgbaImage, String> {
        let path = self.resolve(source).ok_or_else(|| format!("file not found: {source}"))?;
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        if hp_vtf::is_vtf(&bytes) {
            let opts =
                hp_vtf::DecodeOptions { mip: hp_vtf::MipSelect::MaxDimension(max_dim.max(1)), frame: 0, face: 0 };
            return hp_vtf::decode_rgba8(&bytes, &opts).map_err(|e| format!("{}: {e}", path.display()));
        }
        let img = hp_vtf::load_image_file(&bytes, &ext).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(downscale_to_fit(img, max_dim))
    }
}

impl ImageLoader for FileLoader {
    fn load(&self, source: &str, max_dim: u32) -> Result<RgbaImage, String> {
        self.read(source, max_dim)
    }
}

pub fn downscale_to_fit(mut img: RgbaImage, max_dim: u32) -> RgbaImage {
    let max_dim = max_dim.max(1);
    while img.width.max(img.height) > max_dim && img.width.max(img.height) > 1 {
        img = halve(&img);
    }
    img
}

fn halve(img: &RgbaImage) -> RgbaImage {
    let (w, h) = (img.width as usize, img.height as usize);
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut px = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        for x in 0..nw {
            let mut acc = [0u32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let sx = (x * 2 + dx).min(w - 1);
                let sy = (y * 2 + dy).min(h - 1);
                let i = (sy * w + sx) * 4;
                for (c, slot) in acc.iter_mut().enumerate() {
                    *slot += u32::from(img.pixels[i + c]);
                }
            }
            let o = (y * nw + x) * 4;
            for (c, slot) in acc.iter().enumerate() {
                px[o + c] = ((slot + 2) / 4) as u8;
            }
        }
    }
    RgbaImage { width: nw as u32, height: nh as u32, pixels: px }
}

pub struct Source {
    levels: Vec<RgbaImage>,
}

impl Source {
    pub fn new(img: RgbaImage) -> Self {
        let mut levels = vec![img];
        while let Some(last) = levels.last() {
            if last.width <= 1 && last.height <= 1 {
                break;
            }
            let next = halve(last);
            levels.push(next);
        }
        Self { levels }
    }

    pub fn width(&self) -> u32 {
        self.levels[0].width
    }

    pub fn height(&self) -> u32 {
        self.levels[0].height
    }

    fn texel(&self, level: usize, ix: i64, iy: i64, wrap: Wrap) -> V4 {
        let l = &self.levels[level];
        let (w, h) = (l.width as i64, l.height as i64);
        let (x, y) = (wrap.apply(ix, w), wrap.apply(iy, h));
        let i = ((y * w + x) * 4) as usize;
        let p = &l.pixels[i..i + 4];
        [f32::from(p[0]) / 255.0, f32::from(p[1]) / 255.0, f32::from(p[2]) / 255.0, f32::from(p[3]) / 255.0]
    }

    fn bilinear(&self, level: usize, u: f32, v: f32, wrap: Wrap) -> V4 {
        let l = &self.levels[level];
        let x = u * l.width as f32 - 0.5;
        let y = v * l.height as f32 - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (ix, iy) = (x0 as i64, y0 as i64);
        let a = lerp4(self.texel(level, ix, iy, wrap), self.texel(level, ix + 1, iy, wrap), fx);
        let b = lerp4(self.texel(level, ix, iy + 1, wrap), self.texel(level, ix + 1, iy + 1, wrap), fx);
        lerp4(a, b, fy)
    }

    fn nearest(&self, level: usize, u: f32, v: f32, wrap: Wrap) -> V4 {
        let l = &self.levels[level];
        self.texel(level, (u * l.width as f32).floor() as i64, (v * l.height as f32).floor() as i64, wrap)
    }

    pub fn sample(&self, u: f32, v: f32, lod: f32, wrap: Wrap, nearest: bool) -> V4 {
        let last = self.levels.len() - 1;
        let lod = if lod.is_finite() { lod.clamp(0.0, last as f32) } else { 0.0 };
        if nearest {
            return self.nearest(lod.round() as usize, u, v, wrap);
        }
        let l0 = lod.floor() as usize;
        let l1 = (l0 + 1).min(last);
        let a = self.bilinear(l0, u, v, wrap);
        if l1 == l0 {
            return a;
        }
        lerp4(a, self.bilinear(l1, u, v, wrap), lod - l0 as f32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrap {
    Repeat,
    Clamp,
    Mirror,
}

impl Wrap {
    fn parse(s: &str) -> Self {
        match s {
            "clamp" => Wrap::Clamp,
            "mirror" => Wrap::Mirror,
            _ => Wrap::Repeat,
        }
    }

    fn apply(self, i: i64, n: i64) -> i64 {
        match self {
            Wrap::Repeat => i.rem_euclid(n),
            Wrap::Clamp => i.clamp(0, n - 1),
            Wrap::Mirror => {
                let m = i.rem_euclid(2 * n);
                if m >= n { 2 * n - 1 - m } else { m }
            }
        }
    }
}

type CacheMap = HashMap<(String, u32), Result<Arc<Source>, String>>;

#[derive(Default)]
pub struct ImageCache {
    map: Mutex<CacheMap>,
}

impl ImageCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&self) {
        if let Ok(mut m) = self.map.lock() {
            m.clear();
        }
    }

    pub fn get(&self, loader: &dyn ImageLoader, source: &str, max_dim: u32) -> Result<Arc<Source>, String> {
        let key = (source.to_owned(), max_dim);
        if let Ok(m) = self.map.lock()
            && let Some(hit) = m.get(&key)
        {
            return hit.clone();
        }
        let loaded = loader.load(source, max_dim).and_then(|img| {
            if img.width == 0 || img.height == 0 || img.pixels.len() != img.width as usize * img.height as usize * 4 {
                Err(format!("{source}: empty image"))
            } else {
                Ok(Arc::new(Source::new(img)))
            }
        });
        if let Ok(mut m) = self.map.lock() {
            if m.len() > 64 {
                m.clear();
            }
            m.insert(key, loaded.clone());
        }
        loaded
    }
}

#[derive(Clone)]
enum Val {
    C(V4),
    B(Arc<Vec<V4>>),
}

impl Val {
    fn at(&self, i: usize) -> V4 {
        match self {
            Val::C(c) => *c,
            Val::B(b) => b[i],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe {
    pub node: u32,
    pub output: String,
}

#[derive(Clone, Debug, Default)]
pub struct EvalOptions {
    pub width: u32,
    pub height: u32,
    pub outputs: bool,
    pub probe: Option<Probe>,
    pub thumbs: u32,
    pub cancel: Option<Arc<AtomicBool>>,
}

#[derive(Clone, Debug)]
pub struct OutputImage {
    pub node: u32,
    pub target: String,
    pub suffix: String,
    pub format: String,
    pub mipmaps: bool,
    pub clamp: bool,
    pub image: RgbaImage,
}

#[derive(Clone, Debug, Default)]
pub struct Evaluation {
    pub width: u32,
    pub height: u32,
    pub outputs: Vec<OutputImage>,
    pub probe: Option<RgbaImage>,
    pub thumbs: HashMap<u32, RgbaImage>,
    pub diagnostics: Vec<Diagnostic>,
    pub cancelled: bool,
}

pub fn auto_size(graph: &Graph, loader: &dyn ImageLoader, cache: &ImageCache) -> (u32, u32) {
    let s = graph.settings;
    let clampd = |v: u32| v.clamp(4, MAX_BAKE_SIZE);
    if s.width > 0 && s.height > 0 {
        return (clampd(s.width), clampd(s.height));
    }
    let mut ids: Vec<&Node> = graph.nodes.iter().filter(|n| n.kind == "texture").collect();
    ids.sort_by_key(|n| n.id);
    for n in ids {
        let Some(def) = node_def(&n.kind) else { continue };
        let source = Params::new(n, def).s("source");
        if source.trim().is_empty() {
            continue;
        }
        if let Ok(src) = cache.get(loader, &source, MAX_BAKE_SIZE) {
            let (w, h) = (clampd(src.width()), clampd(src.height()));
            return match (s.width, s.height) {
                (0, 0) => (w, h),
                (sw, 0) => (clampd(sw), clampd((u64::from(sw) * u64::from(h) / u64::from(w.max(1))) as u32)),
                (0, sh) => (clampd((u64::from(sh) * u64::from(w) / u64::from(h.max(1))) as u32), clampd(sh)),
                (sw, sh) => (clampd(sw), clampd(sh)),
            };
        }
    }
    (DEFAULT_BAKE_SIZE, DEFAULT_BAKE_SIZE)
}

pub fn evaluate(graph: &Graph, loader: &dyn ImageLoader, cache: &ImageCache, opts: &EvalOptions) -> Evaluation {
    let mut diagnostics = Vec::new();
    let (mut w, mut h) = (opts.width, opts.height);
    if w == 0 || h == 0 {
        (w, h) = auto_size(graph, loader, cache);
    }
    let (cw, ch) = (w.clamp(1, MAX_BAKE_SIZE), h.clamp(1, MAX_BAKE_SIZE));
    if (cw, ch) != (w, h) {
        diagnostics.push(Diagnostic::new(Severity::Warning, None, "too_big", format!("{w}x{h} -> {cw}x{ch}")));
    }
    let eval = Eval {
        graph,
        loader,
        cache,
        w: cw as usize,
        h: ch as usize,
        uv: None,
        values: HashMap::new(),
        diagnostics,
        opts,
    };
    eval.run()
}

struct Eval<'a> {
    graph: &'a Graph,
    loader: &'a dyn ImageLoader,
    cache: &'a ImageCache,
    w: usize,
    h: usize,
    uv: Option<Val>,
    values: HashMap<u32, Vec<Val>>,
    diagnostics: Vec<Diagnostic>,
    opts: &'a EvalOptions,
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

fn pixel_row(val: &Val, w: usize, h: usize, f: impl Fn(V4) -> V4 + Sync) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    out.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let c = f(val.at(y * w + x));
            for k in 0..4 {
                row[x * 4 + k] = to_u8(c[k]);
            }
        }
    });
    out
}

impl Eval<'_> {
    fn cancelled(&self) -> bool {
        self.opts.cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed))
    }

    fn build(&self, f: impl Fn(usize, usize) -> V4 + Sync) -> Val {
        let w = self.w;
        let mut data = vec![[0.0f32; 4]; w * self.h];
        data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, p) in row.iter_mut().enumerate() {
                *p = f(x, y);
            }
        });
        Val::B(Arc::new(data))
    }

    fn map1(&self, a: &Val, f: impl Fn(V4) -> V4 + Sync) -> Val {
        if let Val::C(c) = a {
            return Val::C(f(*c));
        }
        let w = self.w;
        self.build(|x, y| f(a.at(y * w + x)))
    }

    fn map2(&self, a: &Val, b: &Val, f: impl Fn(V4, V4) -> V4 + Sync) -> Val {
        if let (Val::C(x), Val::C(y)) = (a, b) {
            return Val::C(f(*x, *y));
        }
        let w = self.w;
        self.build(|x, y| f(a.at(y * w + x), b.at(y * w + x)))
    }

    fn map3(&self, a: &Val, b: &Val, c: &Val, f: impl Fn(V4, V4, V4) -> V4 + Sync) -> Val {
        if let (Val::C(x), Val::C(y), Val::C(z)) = (a, b, c) {
            return Val::C(f(*x, *y, *z));
        }
        let w = self.w;
        self.build(|x, y| f(a.at(y * w + x), b.at(y * w + x), c.at(y * w + x)))
    }

    fn buffer(&self, v: &Val) -> Arc<Vec<V4>> {
        match v {
            Val::B(b) => b.clone(),
            Val::C(c) => Arc::new(vec![*c; self.w * self.h]),
        }
    }

    fn base_uv(&mut self) -> Val {
        if let Some(v) = &self.uv {
            return v.clone();
        }
        let (w, h) = (self.w, self.h);
        let v = self.build(|x, y| [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32, 0.0, 1.0]);
        self.uv = Some(v.clone());
        v
    }

    fn scale(&self) -> f32 {
        self.w.max(self.h) as f32 / 1024.0
    }

    fn source_for(&mut self, id: u32, source: &str) -> Option<Arc<Source>> {
        if source.trim().is_empty() {
            self.diagnostics.push(Diagnostic::new(Severity::Warning, Some(id), "no_source", ""));
            return None;
        }
        let max_dim = (self.w.max(self.h) as u32 * 2).clamp(64, 4096);
        match self.cache.get(self.loader, source, max_dim) {
            Ok(s) => Some(s),
            Err(e) => {
                self.diagnostics.push(Diagnostic::new(Severity::Warning, Some(id), "missing_texture", e));
                None
            }
        }
    }

    fn sample_texture(&self, uv: &Val, src: &Source, wrap: Wrap, nearest: bool) -> Val {
        if let Val::C(c) = uv {
            return Val::C(src.sample(c[0], c[1], 0.0, wrap, nearest));
        }
        let (w, h) = (self.w, self.h);
        let (sw, sh) = (src.width() as f32, src.height() as f32);
        self.build(|x, y| {
            let i = y * w + x;
            let p = uv.at(i);
            let nx = uv.at(if x + 1 < w { i + 1 } else { i.saturating_sub(1) });
            let ny = uv.at(if y + 1 < h { i + w } else { i.saturating_sub(w) });
            let fx = ((nx[0] - p[0]) * sw).hypot((nx[1] - p[1]) * sh);
            let fy = ((ny[0] - p[0]) * sw).hypot((ny[1] - p[1]) * sh);
            let fp = fx.max(fy);
            let lod = if fp > 1.0 { fp.log2() } else { 0.0 };
            src.sample(p[0], p[1], lod, wrap, nearest)
        })
    }

    fn needed(&self, id: u32) -> Vec<bool> {
        let n = self.graph.def_of(id).map_or(0, |d| d.outputs.len());
        let mut need = vec![false; n];
        let Some(def) = self.graph.def_of(id) else { return need };
        for l in self.graph.links.iter().filter(|l| l.from == id) {
            if let Some(i) = def.outputs.iter().position(|o| o.name == l.out) {
                need[i] = true;
            }
        }
        if let Some(p) = &self.opts.probe
            && p.node == id
            && let Some(i) = def.outputs.iter().position(|o| o.name == p.output)
        {
            need[i] = true;
        }
        if self.opts.thumbs > 0
            && let Some(first) = need.first_mut()
        {
            *first = true;
        }
        need
    }

    fn inputs(&mut self, node: &Node, def: &NodeDef) -> Vec<Val> {
        let mut out = Vec::with_capacity(def.inputs.len());
        for pin in def.inputs {
            let linked = self.graph.link_to(node.id, pin.name).and_then(|l| {
                let src_def = self.graph.def_of(l.from)?;
                let idx = src_def.outputs.iter().position(|o| o.name == l.out)?;
                self.values.get(&l.from).and_then(|v| v.get(idx)).cloned()
            });
            let v = match linked {
                Some(v) => v,
                None if pin.ty == PinTy::Uv => self.base_uv(),
                None => Val::C(self.graph.input_default(node.id, pin.name).unwrap_or(pin.default)),
            };
            out.push(v);
        }
        out
    }

    fn run(mut self) -> Evaluation {
        let graph = self.graph;
        let mut targets: Vec<u32> = Vec::new();
        if self.opts.thumbs > 0 {
            targets.extend(graph.nodes.iter().map(|n| n.id));
        } else {
            if self.opts.outputs {
                targets.extend(graph.output_ids());
            }
            if let Some(p) = &self.opts.probe {
                targets.push(p.node);
            }
        }
        let (order, broken) = graph.order_for(&targets);
        for l in broken {
            self.diagnostics.push(Diagnostic::new(
                Severity::Error,
                Some(l.to),
                "cycle",
                format!("{} -> {}", l.from, l.to),
            ));
        }
        let position: HashMap<u32, usize> = order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        let mut free_at: Vec<Vec<u32>> = vec![Vec::new(); order.len()];
        for (i, id) in order.iter().enumerate() {
            let last = graph
                .links
                .iter()
                .filter(|l| l.from == *id)
                .filter_map(|l| position.get(&l.to).copied())
                .max()
                .unwrap_or(i)
                .max(i);
            free_at[last].push(*id);
        }
        let mut result = Evaluation { width: self.w as u32, height: self.h as u32, ..Evaluation::default() };
        for (k, id) in order.iter().enumerate() {
            if self.cancelled() {
                result.cancelled = true;
                break;
            }
            let Some(node) = graph.node(*id) else { continue };
            let Some(def) = node_def(&node.kind) else { continue };
            let inputs = self.inputs(node, def);
            if def.is_output() {
                if self.opts.outputs {
                    let alpha_linked = graph.link_to(node.id, "alpha").is_some();
                    result.outputs.push(self.output_image(node, def, &inputs, alpha_linked));
                }
                if self.opts.probe.as_ref().is_some_and(|p| p.node == node.id) {
                    result.probe =
                        Some(self.output_image(node, def, &inputs, graph.link_to(node.id, "alpha").is_some()).image);
                }
            } else {
                let need = self.needed(node.id);
                let outs = self.run_node(node, def, &inputs, &need);
                if let Some(p) = &self.opts.probe
                    && p.node == node.id
                    && let Some(i) = def.outputs.iter().position(|o| o.name == p.output)
                    && let Some(v) = outs.get(i)
                {
                    let (w, h) = (self.w, self.h);
                    result.probe =
                        Some(RgbaImage { width: w as u32, height: h as u32, pixels: pixel_row(v, w, h, |c| c) });
                }
                if self.opts.thumbs > 0
                    && let Some(v) = outs.first()
                {
                    result.thumbs.insert(node.id, self.thumb(v, self.opts.thumbs as usize));
                }
                self.values.insert(node.id, outs);
            }
            for f in &free_at[k] {
                self.values.remove(f);
            }
        }
        result.diagnostics = std::mem::take(&mut self.diagnostics);
        result
    }

    fn thumb(&self, v: &Val, edge: usize) -> RgbaImage {
        let (w, h) = (self.w, self.h);
        let (tw, th) = (edge.min(w).max(1), edge.min(h).max(1));
        let mut px = vec![255u8; tw * th * 4];
        for ty in 0..th {
            for tx in 0..tw {
                let (x0, x1) = (tx * w / tw, ((tx + 1) * w / tw).max(tx * w / tw + 1).min(w));
                let (y0, y1) = (ty * h / th, ((ty + 1) * h / th).max(ty * h / th + 1).min(h));
                let (mut acc, mut n) = ([0.0f32; 3], 0.0);
                let step_x = ((x1 - x0) / 3).max(1);
                let step_y = ((y1 - y0) / 3).max(1);
                let mut y = y0;
                while y < y1 {
                    let mut x = x0;
                    while x < x1 {
                        let c = v.at(y * w + x);
                        for k in 0..3 {
                            acc[k] += c[k].clamp(0.0, 1.0);
                        }
                        n += 1.0;
                        x += step_x;
                    }
                    y += step_y;
                }
                let o = (ty * tw + tx) * 4;
                for k in 0..3 {
                    px[o + k] = to_u8(acc[k] / n);
                }
            }
        }
        RgbaImage { width: tw as u32, height: th as u32, pixels: px }
    }

    fn output_image(&self, node: &Node, def: &NodeDef, inputs: &[Val], alpha_linked: bool) -> OutputImage {
        let p = Params::new(node, def);
        let (w, h) = (self.w, self.h);
        let alpha = &inputs[1];
        let color = &inputs[0];
        let pixels = if alpha_linked {
            let mut out = vec![0u8; w * h * 4];
            out.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
                for x in 0..w {
                    let c = color.at(y * w + x);
                    let a = lum(alpha.at(y * w + x));
                    let px = [c[0], c[1], c[2], a];
                    for k in 0..4 {
                        row[x * 4 + k] = to_u8(px[k]);
                    }
                }
            });
            out
        } else {
            pixel_row(color, w, h, |c| c)
        };
        OutputImage {
            node: node.id,
            target: self.graph.output_target(node.id),
            suffix: self.graph.output_suffix(node.id),
            format: p.s("format"),
            mipmaps: p.b("mipmaps"),
            clamp: p.b("clamp"),
            image: RgbaImage { width: w as u32, height: h as u32, pixels },
        }
    }

    fn defaults_for(def: &NodeDef) -> Vec<Val> {
        def.outputs.iter().map(|o| Val::C(o.default)).collect()
    }

    fn run_node(&mut self, node: &Node, def: &NodeDef, ins: &[Val], need: &[bool]) -> Vec<Val> {
        let p = Params::new(node, def);
        let (w, h) = (self.w, self.h);
        let one = |v: Val| vec![v];
        match def.id {
            "texture" => {
                let source = p.s("source");
                let Some(src) = self.source_for(node.id, &source) else {
                    let g = Val::C([0.5, 0.5, 0.5, 1.0]);
                    return vec![g.clone(), g.clone(), g.clone(), g, Val::C(fv(1.0))];
                };
                let wrap = Wrap::parse(&p.s("wrap"));
                let nearest = p.s("filter") == "nearest";
                let rgba = self.sample_texture(&ins[0], &src, wrap, nearest);
                let mut outs = vec![rgba.clone()];
                for c in 0..4 {
                    outs.push(if need.get(c + 1).copied().unwrap_or(false) {
                        self.map1(&rgba, |v| fv(v[c]))
                    } else {
                        Val::C(fv(0.0))
                    });
                }
                outs
            }
            "color" => one(Val::C(p.color("color"))),
            "float" => one(Val::C(fv(p.f("value")))),
            "vec2" => {
                let v = p.v2("value");
                one(Val::C([v[0], v[1], 0.0, 1.0]))
            }
            "vec3" => one(Val::C([p.f("x"), p.f("y"), p.f("z"), 1.0])),
            "uv" => one(self.base_uv()),
            "tiling_offset" => {
                let (t, o) = (p.v2("tiling"), p.v2("offset"));
                one(self.map1(&ins[0], |c| [c[0] * t[0] + o[0], c[1] * t[1] + o[1], 0.0, 1.0]))
            }
            "rotate_uv" => {
                let (a, pv) = (p.f("angle").to_radians(), p.v2("pivot"));
                let (s, co) = a.sin_cos();
                one(self.map1(&ins[0], |c| {
                    let (dx, dy) = (c[0] - pv[0], c[1] - pv[1]);
                    [pv[0] + dx * co - dy * s, pv[1] + dx * s + dy * co, 0.0, 1.0]
                }))
            }
            "warp_uv" => {
                let k = p.f("strength");
                one(self.map2(&ins[0], &ins[1], |c, m| [c[0] + (m[0] - 0.5) * k, c[1] + (m[1] - 0.5) * k, 0.0, 1.0]))
            }
            "add" => one(self.map2(&ins[0], &ins[1], |a, b| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3]])),
            "subtract" => one(self.map2(&ins[0], &ins[1], |a, b| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3]])),
            "multiply" => one(self.map2(&ins[0], &ins[1], |a, b| [a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3]])),
            "divide" => {
                let d = |a: f32, b: f32| if b.abs() < 1e-6 { 0.0 } else { a / b };
                one(self.map2(&ins[0], &ins[1], |a, b| [d(a[0], b[0]), d(a[1], b[1]), d(a[2], b[2]), a[3]]))
            }
            "min" => one(self.map2(&ins[0], &ins[1], |a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2]), a[3]])),
            "max" => one(self.map2(&ins[0], &ins[1], |a, b| [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2]), a[3]])),
            "lerp" => one(self.map3(&ins[0], &ins[1], &ins[2], |a, b, t| lerp4(a, b, lum(t)))),
            "clamp" => one(self.map3(&ins[0], &ins[1], &ins[2], |v, lo, hi| {
                let (lo, hi) = (lum(lo), lum(hi));
                let (lo, hi) = (lo.min(hi), lo.max(hi));
                [v[0].clamp(lo, hi), v[1].clamp(lo, hi), v[2].clamp(lo, hi), v[3]]
            })),
            "saturate" => {
                one(self.map1(&ins[0], |v| [v[0].clamp(0.0, 1.0), v[1].clamp(0.0, 1.0), v[2].clamp(0.0, 1.0), v[3]]))
            }
            "power" => one(self.map2(&ins[0], &ins[1], |v, e| {
                let e = lum(e);
                [v[0].max(0.0).powf(e), v[1].max(0.0).powf(e), v[2].max(0.0).powf(e), v[3]]
            })),
            "abs" => one(self.map1(&ins[0], |v| [v[0].abs(), v[1].abs(), v[2].abs(), v[3]])),
            "one_minus" => one(self.map1(&ins[0], |v| [1.0 - v[0], 1.0 - v[1], 1.0 - v[2], v[3]])),
            "invert" => {
                let ia = p.b("invert_alpha");
                one(self.map1(&ins[0], |v| [1.0 - v[0], 1.0 - v[1], 1.0 - v[2], if ia { 1.0 - v[3] } else { v[3] }]))
            }
            "remap" => {
                let (i0, i1, o0, o1) = (p.f("in_min"), p.f("in_max"), p.f("out_min"), p.f("out_max"));
                let span = if (i1 - i0).abs() < 1e-6 { 1e-6 } else { i1 - i0 };
                let f = move |x: f32| o0 + (x - i0) / span * (o1 - o0);
                one(self.map1(&ins[0], |v| [f(v[0]), f(v[1]), f(v[2]), v[3]]))
            }
            "smoothstep" => {
                let (e0, e1) = (p.f("edge0"), p.f("edge1"));
                one(self.map1(&ins[0], |v| [smooth(e0, e1, v[0]), smooth(e0, e1, v[1]), smooth(e0, e1, v[2]), v[3]]))
            }
            "brightness_contrast" => {
                let b = p.f("brightness");
                let c = p.f("contrast").clamp(-0.99, 0.99);
                let f = (1.0 + c) / (1.0 - c);
                one(self.map1(&ins[0], |v| {
                    [(v[0] - 0.5) * f + 0.5 + b, (v[1] - 0.5) * f + 0.5 + b, (v[2] - 0.5) * f + 0.5 + b, v[3]]
                }))
            }
            "hsv" => {
                let (dh, ds, dv) = (p.f("hue") / 360.0, p.f("saturation"), p.f("value"));
                one(self.map1(&ins[0], |v| {
                    let [hh, s, val] = ops::rgb_to_hsv(v);
                    let [r, g, b] = ops::hsv_to_rgb(hh + dh, (s * ds).clamp(0.0, 1.0), val * dv);
                    [r, g, b, v[3]]
                }))
            }
            "tint" => {
                let k = p.f("amount");
                one(self.map2(&ins[0], &ins[1], |c, t| {
                    [
                        c[0] * (1.0 + (t[0] - 1.0) * k),
                        c[1] * (1.0 + (t[1] - 1.0) * k),
                        c[2] * (1.0 + (t[2] - 1.0) * k),
                        c[3],
                    ]
                }))
            }
            "grayscale" => {
                let (mode, k) = (p.s("mode"), p.f("amount"));
                one(self.map1(&ins[0], |c| {
                    let g = match mode.as_str() {
                        "average" => (c[0] + c[1] + c[2]) / 3.0,
                        "lightness" => (c[0].max(c[1]).max(c[2]) + c[0].min(c[1]).min(c[2])) * 0.5,
                        _ => lum(c),
                    };
                    [c[0] + (g - c[0]) * k, c[1] + (g - c[1]) * k, c[2] + (g - c[2]) * k, c[3]]
                }))
            }
            "posterize" => {
                let n = (p.i("levels") - 1).max(1) as f32;
                one(self.map1(&ins[0], |c| {
                    let q = |x: f32| (x.clamp(0.0, 1.0) * n).round() / n;
                    [q(c[0]), q(c[1]), q(c[2]), c[3]]
                }))
            }
            "levels" => {
                let (b, wt, g) = (p.f("black"), p.f("white"), p.f("gamma").max(0.01));
                let span = if (wt - b).abs() < 1e-4 { 1e-4 } else { wt - b };
                let inv = 1.0 / g;
                let f = move |x: f32| ((x - b) / span).clamp(0.0, 1.0).powf(inv);
                one(self.map1(&ins[0], |c| [f(c[0]), f(c[1]), f(c[2]), c[3]]))
            }
            "threshold" => {
                let (t, s) = (p.f("threshold"), p.f("softness"));
                one(self.map1(&ins[0], |c| {
                    let v = lum(c);
                    fv(if s <= 1e-4 { f32::from(u8::from(v >= t)) } else { smooth(t - s * 0.5, t + s * 0.5, v) })
                }))
            }
            "gradient_map" => {
                let stops = p.stops("gradient");
                let constant = p.s("mode") == "constant";
                one(self.map1(&ins[0], |c| ops::ramp(&stops, lum(c), constant)))
            }
            "blend" => {
                let mode = p.s("mode");
                one(self.map3(&ins[0], &ins[1], &ins[2], |b, l, o| {
                    let k = (lum(o) * l[3]).clamp(0.0, 1.0);
                    let mixed = [
                        ops::blend_channel(&mode, b[0], l[0]),
                        ops::blend_channel(&mode, b[1], l[1]),
                        ops::blend_channel(&mode, b[2], l[2]),
                    ];
                    [
                        b[0] + (mixed[0] - b[0]) * k,
                        b[1] + (mixed[1] - b[1]) * k,
                        b[2] + (mixed[2] - b[2]) * k,
                        b[3] + (1.0 - b[3]) * k,
                    ]
                }))
            }
            "blur" => {
                let r = p.f("radius") * self.scale();
                if r < 0.5 {
                    return one(ins[0].clone());
                }
                let src = self.buffer(&ins[0]);
                one(Val::B(Arc::new(ops::gaussian(&src, w, h, r))))
            }
            "sharpen" => {
                let (amount, r) = (p.f("amount"), p.f("radius") * self.scale());
                let src = self.buffer(&ins[0]);
                let blurred = ops::gaussian(&src, w, h, r.max(1.0));
                one(self.build(|x, y| {
                    let i = y * w + x;
                    let (c, b) = (src[i], blurred[i]);
                    [c[0] + (c[0] - b[0]) * amount, c[1] + (c[1] - b[1]) * amount, c[2] + (c[2] - b[2]) * amount, c[3]]
                }))
            }
            "edge_detect" => {
                let (s, r) = (p.f("strength"), p.f("radius") * self.scale());
                let src = self.buffer(&ins[0]);
                let vals: Vec<f32> = src.iter().map(|c| lum(*c)).collect();
                let d = (r.round() as i64).max(1);
                one(self.build(|x, y| {
                    let (gx, gy) = ops::sobel(&vals, w, h, x, y, d);
                    fv((gx.hypot(gy) * 0.25 * s).clamp(0.0, 1.0))
                }))
            }
            "height_to_normal" => {
                let k = p.f("strength") * (w as f32 / 1024.0) / 8.0;
                let directx = p.s("format") != "opengl";
                let src = self.buffer(&ins[0]);
                let vals: Vec<f32> = src.iter().map(|c| lum(*c)).collect();
                one(self.build(|x, y| {
                    let (gx, gy) = ops::sobel(&vals, w, h, x, y, 1);
                    let (nx, ny) = (-gx * k, if directx { -gy * k } else { gy * k });
                    let len = (nx * nx + ny * ny + 1.0).sqrt();
                    [nx / len * 0.5 + 0.5, ny / len * 0.5 + 0.5, 1.0 / len * 0.5 + 0.5, 1.0]
                }))
            }
            "normal_strength" => {
                let s = p.f("strength");
                one(self.map1(&ins[0], |c| {
                    let (nx, ny, nz) = ((c[0] * 2.0 - 1.0) * s, (c[1] * 2.0 - 1.0) * s, (c[2] * 2.0 - 1.0).max(0.001));
                    let len = (nx * nx + ny * ny + nz * nz).sqrt();
                    [nx / len * 0.5 + 0.5, ny / len * 0.5 + 0.5, nz / len * 0.5 + 0.5, c[3]]
                }))
            }
            "pixelate" => {
                let (cx, cy) = (p.i("cells_x").max(1) as f32, p.i("cells_y").max(1) as f32);
                let src = self.buffer(&ins[0]);
                one(self.build(|x, y| {
                    let u = ((x as f32 + 0.5) / w as f32 * cx).floor() + 0.5;
                    let v = ((y as f32 + 0.5) / h as f32 * cy).floor() + 0.5;
                    let sx = ((u / cx * w as f32) as usize).min(w - 1);
                    let sy = ((v / cy * h as f32) as usize).min(h - 1);
                    src[sy * w + sx]
                }))
            }
            "make_seamless" => {
                let width = p.f("width");
                let src = self.buffer(&ins[0]);
                one(self.build(|x, y| {
                    let a = src[y * w + x];
                    let b = src[((y + h / 2) % h) * w + (x + w / 2) % w];
                    let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
                    let d = (2.0 * u - 1.0).abs().max((2.0 * v - 1.0).abs());
                    lerp4(a, b, smooth(1.0 - 2.0 * width, 1.0, d))
                }))
            }
            "noise" => {
                let kind = p.s("kind");
                let vm = p.s("voronoi");
                let spec = NoiseSpec {
                    kind: &kind,
                    voronoi: &vm,
                    scale: p.i("scale"),
                    octaves: p.i("octaves"),
                    lacunarity: p.i("lacunarity"),
                    gain: p.f("gain"),
                    seed: p.i("seed") as u32,
                    jitter: p.f("jitter"),
                };
                one(self.map1(&ins[0], |c| fv(ops::noise(&spec, c[0], c[1]))))
            }
            "checker" => {
                let (tx, ty) = (p.i("tiles_x") as f32, p.i("tiles_y") as f32);
                let (ca, cb) = (p.color("color_a"), p.color("color_b"));
                let mask = self.map1(&ins[0], |c| {
                    let k = (c[0] * tx).floor() as i64 + (c[1] * ty).floor() as i64;
                    fv(if k.rem_euclid(2) == 0 { 1.0 } else { 0.0 })
                });
                let color = if need.first().copied().unwrap_or(true) {
                    self.map1(&mask, |m| if m[0] > 0.5 { ca } else { cb })
                } else {
                    Val::C(ca)
                };
                vec![color, mask]
            }
            "gradient" => {
                let kind = p.s("kind");
                let a = p.f("angle").to_radians();
                let (s, co) = a.sin_cos();
                one(self.map1(&ins[0], |c| {
                    let (dx, dy) = (c[0] - 0.5, c[1] - 0.5);
                    let t = match kind.as_str() {
                        "radial" => 1.0 - (dx * dx + dy * dy).sqrt() * 2.0,
                        "angular" => dy.atan2(dx) / std::f32::consts::TAU + 0.5,
                        "diamond" => 1.0 - (dx.abs() + dy.abs()) * 2.0,
                        _ => dx * co + dy * s + 0.5,
                    };
                    fv(t.clamp(0.0, 1.0))
                }))
            }
            "split" => (0..4)
                .map(|k| {
                    if need.get(k).copied().unwrap_or(false) {
                        self.map1(&ins[0], |c| fv(c[k]))
                    } else {
                        Val::C(fv(0.0))
                    }
                })
                .collect(),
            "merge" => one(self.gen_merge(ins)),
            "set_alpha" => one(self.map2(&ins[0], &ins[1], |c, a| [c[0], c[1], c[2], lum(a)])),
            _ => Self::defaults_for(def),
        }
    }

    fn gen_merge(&self, ins: &[Val]) -> Val {
        let w = self.w;
        if ins.iter().all(|v| matches!(v, Val::C(_))) {
            return Val::C([lum(ins[0].at(0)), lum(ins[1].at(0)), lum(ins[2].at(0)), lum(ins[3].at(0))]);
        }
        self.build(|x, y| {
            let i = y * w + x;
            [lum(ins[0].at(i)), lum(ins[1].at(i)), lum(ins[2].at(i)), lum(ins[3].at(i))]
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;

    struct Mem(HashMap<String, RgbaImage>);

    impl ImageLoader for Mem {
        fn load(&self, source: &str, max_dim: u32) -> Result<RgbaImage, String> {
            self.0.get(source).cloned().map(|i| downscale_to_fit(i, max_dim)).ok_or_else(|| format!("missing {source}"))
        }
    }

    fn solid(w: u32, h: u32, c: [u8; 4]) -> RgbaImage {
        RgbaImage { width: w, height: h, pixels: c.repeat((w * h) as usize) }
    }

    fn gradient_h(w: u32, h: u32) -> RgbaImage {
        let mut px = Vec::new();
        for _ in 0..h {
            for x in 0..w {
                let v = (x * 255 / (w - 1)) as u8;
                px.extend_from_slice(&[v, v, v, 255]);
            }
        }
        RgbaImage { width: w, height: h, pixels: px }
    }

    fn loader() -> Mem {
        let mut m = HashMap::new();
        m.insert("red.png".to_owned(), solid(8, 8, [200, 0, 0, 255]));
        m.insert("ramp.png".to_owned(), gradient_h(64, 64));
        Mem(m)
    }

    fn run(g: &Graph, size: u32) -> Evaluation {
        let opts = EvalOptions { width: size, height: size, outputs: true, ..EvalOptions::default() };
        evaluate(g, &loader(), &ImageCache::new(), &opts)
    }

    fn px(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width + x) * 4) as usize;
        [img.pixels[i], img.pixels[i + 1], img.pixels[i + 2], img.pixels[i + 3]]
    }

    #[test]
    fn texture_to_output() {
        let mut g = Graph::new();
        let t = g.add_with("texture", [0.0, 0.0], &[("source", json!("red.png"))]).unwrap();
        let o = g.add_node("output", [0.0, 0.0]).unwrap();
        g.connect(t, "rgba", o, "color").unwrap();
        let e = run(&g, 16);
        assert_eq!(e.outputs.len(), 1);
        assert_eq!(px(&e.outputs[0].image, 5, 5), [200, 0, 0, 255]);
        assert!(e.diagnostics.is_empty(), "{:?}", e.diagnostics);
    }

    #[test]
    fn missing_everything_still_produces_an_image() {
        let mut g = Graph::new();
        let t = g.add_with("texture", [0.0, 0.0], &[("source", json!("nope.png"))]).unwrap();
        let tint = g.add_node("tint", [0.0, 0.0]).unwrap();
        let o = g.add_node("output", [0.0, 0.0]).unwrap();
        g.connect(t, "rgba", tint, "color").unwrap();
        g.connect(tint, "color", o, "color").unwrap();
        let e = run(&g, 8);
        assert_eq!(e.outputs.len(), 1);
        assert!(e.diagnostics.iter().any(|d| d.code == "missing_texture" && d.node == Some(t)));
        let empty = Graph::new();
        let e = run(&empty, 8);
        assert!(e.outputs.is_empty());
        let mut lone = Graph::new();
        lone.add_node("output", [0.0, 0.0]).unwrap();
        let e = run(&lone, 4);
        assert_eq!(px(&e.outputs[0].image, 0, 0), [128, 128, 128, 255], "unconnected input uses its default");
    }

    #[test]
    fn math_and_alpha() {
        let mut g = Graph::new();
        let c = g.add_with("color", [0.0, 0.0], &[("color", json!([0.5, 0.25, 1.0, 0.5]))]).unwrap();
        let f = g.add_with("float", [0.0, 0.0], &[("value", json!(0.5))]).unwrap();
        let m = g.add_node("multiply", [0.0, 0.0]).unwrap();
        let o = g.add_node("output", [0.0, 0.0]).unwrap();
        g.connect(c, "color", m, "a").unwrap();
        g.connect(f, "value", m, "b").unwrap();
        g.connect(m, "result", o, "color").unwrap();
        let e = run(&g, 4);
        assert_eq!(px(&e.outputs[0].image, 1, 1), [64, 32, 128, 128]);
        g.connect(f, "value", o, "alpha").unwrap();
        let e = run(&g, 4);
        assert_eq!(px(&e.outputs[0].image, 1, 1)[3], 128);
    }

    #[test]
    fn uv_chain_tiles_texture() {
        let mut g = Graph::new();
        let uv = g.add_node("uv", [0.0, 0.0]).unwrap();
        let tile = g.add_with("tiling_offset", [0.0, 0.0], &[("tiling", json!([2.0, 1.0]))]).unwrap();
        let t =
            g.add_with("texture", [0.0, 0.0], &[("source", json!("ramp.png")), ("filter", json!("nearest"))]).unwrap();
        let o = g.add_node("output", [0.0, 0.0]).unwrap();
        g.connect(uv, "uv", tile, "uv").unwrap();
        g.connect(tile, "uv", t, "uv").unwrap();
        g.connect(t, "r", o, "color").unwrap();
        let e = run(&g, 64);
        let img = &e.outputs[0].image;
        let (a, b) = (px(img, 8, 0)[0], px(img, 40, 0)[0]);
        assert!((i32::from(a) - i32::from(b)).abs() < 12, "two copies side by side: {a} {b}");
        assert!(px(img, 30, 0)[0] > 200 && px(img, 34, 0)[0] < 60, "the ramp restarts at the middle");
    }

    #[test]
    fn height_to_normal_is_flat_on_flat_input() {
        let mut g = Graph::new();
        let c = g.add_node("float", [0.0, 0.0]).unwrap();
        let n = g.add_node("height_to_normal", [0.0, 0.0]).unwrap();
        let o = g.add_with("output", [0.0, 0.0], &[("target", json!("normalmap"))]).unwrap();
        g.connect(c, "value", n, "value").unwrap();
        g.connect(n, "color", o, "color").unwrap();
        let e = run(&g, 8);
        assert_eq!(e.outputs[0].target, "normalmap");
        let p = px(&e.outputs[0].image, 3, 3);
        assert!((i32::from(p[0]) - 128).abs() <= 1 && (i32::from(p[1]) - 128).abs() <= 1 && p[2] >= 254, "{p:?}");
    }

    #[test]
    fn height_ramp_tilts_the_normal() {
        let mut g = Graph::new();
        let t = g.add_with("texture", [0.0, 0.0], &[("source", json!("ramp.png"))]).unwrap();
        let n = g.add_with("height_to_normal", [0.0, 0.0], &[("strength", json!(100.0))]).unwrap();
        let o = g.add_node("output", [0.0, 0.0]).unwrap();
        g.connect(t, "rgba", n, "value").unwrap();
        g.connect(n, "color", o, "color").unwrap();
        let e = run(&g, 64);
        let p = px(&e.outputs[0].image, 32, 32);
        assert!(p[0] < 125, "the surface rises to the right so the normal leans left: {p:?}");
    }

    #[test]
    fn cycles_do_not_hang_or_panic() {
        let mut g = Graph::new();
        let a = g.add_node("blur", [0.0, 0.0]).unwrap();
        let b = g.add_node("blur", [0.0, 0.0]).unwrap();
        let o = g.add_node("output", [0.0, 0.0]).unwrap();
        g.links.push(crate::graph::Link { from: a, out: "color".into(), to: b, input: "color".into() });
        g.links.push(crate::graph::Link { from: b, out: "color".into(), to: a, input: "color".into() });
        g.links.push(crate::graph::Link { from: b, out: "color".into(), to: o, input: "color".into() });
        let e = run(&g, 8);
        assert_eq!(e.outputs.len(), 1);
        assert!(e.diagnostics.iter().any(|d| d.code == "cycle"));
    }

    #[test]
    fn probe_and_thumbs() {
        let mut g = Graph::new();
        let t = g.add_with("texture", [0.0, 0.0], &[("source", json!("red.png"))]).unwrap();
        let s = g.add_node("split", [0.0, 0.0]).unwrap();
        let o = g.add_node("output", [0.0, 0.0]).unwrap();
        g.connect(t, "rgba", s, "color").unwrap();
        g.connect(s, "r", o, "color").unwrap();
        let opts = EvalOptions {
            width: 16,
            height: 16,
            outputs: false,
            probe: Some(Probe { node: s, output: "r".into() }),
            thumbs: 8,
            cancel: None,
        };
        let e = evaluate(&g, &loader(), &ImageCache::new(), &opts);
        assert!(e.outputs.is_empty());
        let p = e.probe.unwrap();
        assert_eq!(px(&p, 0, 0), [200, 200, 200, 255]);
        assert_eq!(e.thumbs.len(), 2);
        assert_eq!(e.thumbs[&t].width, 8);
    }

    #[test]
    fn cancel_stops_work() {
        let mut g = Graph::new();
        let t = g.add_node("noise", [0.0, 0.0]).unwrap();
        let o = g.add_node("output", [0.0, 0.0]).unwrap();
        g.connect(t, "value", o, "color").unwrap();
        let flag = Arc::new(AtomicBool::new(true));
        let opts = EvalOptions { width: 32, height: 32, outputs: true, cancel: Some(flag), ..EvalOptions::default() };
        let e = evaluate(&g, &loader(), &ImageCache::new(), &opts);
        assert!(e.cancelled && e.outputs.is_empty());
    }

    #[test]
    fn seamless_matches_across_the_seam() {
        let mut g = Graph::new();
        let t = g.add_with("texture", [0.0, 0.0], &[("source", json!("ramp.png"))]).unwrap();
        let s = g.add_node("make_seamless", [0.0, 0.0]).unwrap();
        let o = g.add_node("output", [0.0, 0.0]).unwrap();
        g.connect(t, "rgba", s, "color").unwrap();
        g.connect(s, "color", o, "color").unwrap();
        let e = run(&g, 64);
        let img = &e.outputs[0].image;
        let (l, r) = (i32::from(px(img, 0, 20)[0]), i32::from(px(img, 63, 20)[0]));
        assert!((l - r).abs() < 10, "left and right edges meet: {l} {r}");
    }

    #[test]
    fn auto_size_follows_the_first_texture() {
        let mut g = Graph::new();
        g.add_with("texture", [0.0, 0.0], &[("source", json!("ramp.png"))]).unwrap();
        assert_eq!(auto_size(&g, &loader(), &ImageCache::new()), (64, 64));
        g.settings.width = 128;
        assert_eq!(auto_size(&g, &loader(), &ImageCache::new()), (128, 128));
        g.settings.width = 128;
        g.settings.height = 32;
        assert_eq!(auto_size(&g, &loader(), &ImageCache::new()), (128, 32));
        assert_eq!(auto_size(&Graph::new(), &loader(), &ImageCache::new()), (DEFAULT_BAKE_SIZE, DEFAULT_BAKE_SIZE));
    }

    #[test]
    fn every_node_runs_with_defaults() {
        for def in crate::catalog::NODES {
            let mut g = Graph::new();
            let id = g.add_node(def.id, [0.0, 0.0]).unwrap();
            if !def.is_output() {
                let o = g.add_node("output", [0.0, 0.0]).unwrap();
                if let Some(out) = def.outputs.first() {
                    g.connect(id, out.name, o, "color").unwrap();
                }
            }
            let e = run(&g, 16);
            assert_eq!(e.outputs.len(), 1, "{}", def.id);
            let opts = EvalOptions { width: 16, height: 16, thumbs: 4, ..EvalOptions::default() };
            let e = evaluate(&g, &loader(), &ImageCache::new(), &opts);
            assert!(e.thumbs.contains_key(&id) || def.outputs.is_empty(), "{}", def.id);
        }
    }
}
