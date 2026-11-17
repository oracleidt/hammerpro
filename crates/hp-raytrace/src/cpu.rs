use std::time::Instant;

use glam::Vec3A;
use rayon::prelude::*;

use crate::baker::{Backend, Baker, Control, Output, Phase, Progress, Refinement, Stop, SurfaceOut};
use crate::kernel::{Acc, Ctx, Packed, Tuning};
use crate::sampling::luminance;
use crate::scene::Scene;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Direct,
    Indirect,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Work {
    pub luxel: u32,
    pub surface: u32,
    pub start: u32,
    pub count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Stat {
    pub n: f32,
    pub sum: f32,
    pub sq: f32,
    pub ao: f32,
}

pub(crate) struct State {
    pub finished: bool,
    pub packed: Packed,
    pub lux_offset: Vec<u32>,
    pub lux_width: Vec<u8>,
    pub direct: Vec<Vec3A>,
    pub indirect: Vec<Vec3A>,
    pub dstat: Vec<Stat>,
    pub istat: Vec<Stat>,
    pub cache_ready: bool,
    pub cache_fresh: bool,
    pub target: u32,
    pub clamp: f32,
    pub rays: u64,
}

impl State {
    pub fn new(scene: &Scene) -> State {
        let packed = Packed::new(scene);
        let n = scene.luxels.len();
        let mut lux_offset = vec![0u32; n];
        let mut lux_width = vec![0u8; n];
        let mut at = 0u32;
        for s in packed.surfaces.iter() {
            let width = (packed.passes * s.slots as usize * s.normals as usize) as u32;
            for i in 0..s.luxels() {
                let l = s.first_luxel as usize + i;
                if l < n {
                    lux_offset[l] = at;
                    lux_width[l] = width as u8;
                    at += width;
                }
            }
        }
        State {
            finished: false,
            packed,
            lux_offset,
            lux_width,
            direct: vec![Vec3A::ZERO; at as usize],
            indirect: vec![Vec3A::ZERO; at as usize],
            dstat: vec![Stat::default(); n],
            istat: vec![Stat::default(); n],
            cache_ready: false,
            cache_fresh: false,
            target: 1,
            clamp: 0.0,
            rays: 0,
        }
    }
}

pub(crate) fn tuning(b: &Baker) -> Tuning {
    Tuning {
        seed: b.settings.seed,
        bounces: b.settings.params.bounces,
        sky_samples: b.settings.params.sky_samples,
        style_bounce: b.settings.style_bounce,
        ao_radius: b.settings.ao_radius,
        contact_ao: b.settings.params.contact_ao,
        clamp: b.state.clamp,
    }
}

fn run_cpu(scene: &Scene, state: &mut State, tune: Tuning, kind: Kind, work: &[Work]) {
    let ctx = Ctx { scene, packed: &state.packed, tuning: tune };
    let results: Vec<(Acc, Stat)> = work
        .par_iter()
        .map(|w| {
            let mut total = Acc::default();
            let mut st = Stat::default();
            let si = w.surface as usize;
            let s = &ctx.packed.surfaces[si];
            let width = ctx.packed.passes * s.slots as usize * s.normals as usize;
            let probe = (ctx.packed.passes - 1) * s.slots as usize * s.normals as usize;
            for k in w.start..w.start + w.count {
                let mut a = Acc::default();
                let ao = match kind {
                    Kind::Direct => ctx.direct_sample(si, w.luxel, k, &mut a),
                    Kind::Indirect => {
                        ctx.indirect_sample(si, w.luxel, k, &mut a);
                        1.0
                    }
                };
                let l = luminance(a.v[probe]);
                st.n += 1.0;
                st.sum += l;
                st.sq += l * l;
                st.ao += ao;
                for i in 0..width {
                    total.v[i] += a.v[i];
                }
            }
            (total, st)
        })
        .collect();
    for (w, (acc, st)) in work.iter().zip(results) {
        let l = w.luxel as usize;
        let off = state.lux_offset[l] as usize;
        let width = state.lux_width[l] as usize;
        let (buf, stats) = match kind {
            Kind::Direct => (&mut state.direct, &mut state.dstat),
            Kind::Indirect => (&mut state.indirect, &mut state.istat),
        };
        for i in 0..width {
            buf[off + i] += acc.v[i];
        }
        let s = &mut stats[l];
        s.n += st.n;
        s.sum += st.sum;
        s.sq += st.sq;
        s.ao += st.ao;
    }
}

pub(crate) fn run(b: &mut Baker, kind: Kind, work: &[Work]) -> Result<(), String> {
    if work.is_empty() {
        return Ok(());
    }
    let tune = tuning(b);
    match &mut b.backend {
        Backend::Gpu(g) => g.run(&b.scene, &mut b.state, tune, kind, work),
        Backend::Cpu => {
            let scene = b.scene.clone();
            let threads = b.settings.threads;
            let state = &mut b.state;
            with_low_priority(threads, || run_cpu(&scene, state, tune, kind, work));
            Ok(())
        }
    }
}

fn with_low_priority<R: Send>(threads: Option<usize>, f: impl FnOnce() -> R + Send) -> R {
    thread_local! {
        static LOWERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    let pool = POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads.unwrap_or(0))
            .thread_name(|i| format!("hp-raytrace-{i}"))
            .start_handler(|_| {
                LOWERED.with(|l| {
                    if !l.get() {
                        lower_priority();
                        l.set(true);
                    }
                })
            })
            .build()
            .ok()
    });
    match pool {
        Some(p) => p.install(f),
        None => f(),
    }
}

static POOL: std::sync::OnceLock<Option<rayon::ThreadPool>> = std::sync::OnceLock::new();

#[cfg(windows)]
#[allow(unsafe_code)]
fn lower_priority() {
    use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

#[cfg(not(windows))]
fn lower_priority() {}

pub(crate) fn sync(b: &mut Baker, kind: Kind) -> Result<(), String> {
    if let Backend::Gpu(g) = &mut b.backend {
        g.read_back(&mut b.state, kind)?;
    }
    Ok(())
}

fn rel_error(s: &Stat) -> f32 {
    if s.n < 2.0 {
        return f32::INFINITY;
    }
    let mean = s.sum / s.n;
    let var = (s.sq / s.n - mean * mean).max(0.0);
    let se = (var / s.n).sqrt();
    se / mean.abs().max(0.5)
}

pub(crate) fn build_cache(b: &mut Baker) {
    let st = &mut b.state;
    let passes = st.packed.passes;
    let mut cache = std::mem::take(&mut st.packed.cache);
    let mut valid = vec![0u8; b.scene.luxels.len()];
    let mut lums: Vec<f32> = Vec::new();
    for s in &st.packed.surfaces {
        let nn = s.normals as usize;
        let ns = s.slots as usize;
        let lux = s.luxels();
        for i in 0..lux {
            let l = s.first_luxel as usize + i;
            let n = st.dstat[l].n;
            if n <= 0.0 || b.scene.luxels[l].points == 0 {
                continue;
            }
            valid[l] = 1;
            let off = st.lux_offset[l] as usize;
            for pass in 0..passes {
                for slot in 0..ns {
                    let v = st.direct[off + (pass * ns + slot) * nn] / n;
                    cache[s.cache as usize + (pass * ns + slot) * lux + i] = v.to_array();
                    if pass + 1 == passes {
                        lums.push(luminance(v));
                    }
                }
            }
        }
    }
    st.packed.cache = cache;
    st.packed.cache_valid = valid;
    st.clamp = if lums.is_empty() {
        0.0
    } else {
        let k = ((lums.len() as f64) * 0.995) as usize;
        let k = k.min(lums.len() - 1);
        lums.select_nth_unstable_by(k, f32::total_cmp);
        lums[k].max(1.0) * 1.5
    };
    st.cache_ready = true;
}

fn max_count(b: &Baker) -> u32 {
    match &b.backend {
        Backend::Gpu(_) => 4,
        Backend::Cpu => 64,
    }
}

const MIN_GPU_BATCH: usize = 16_384;

fn item_cap(b: &Baker) -> usize {
    match &b.backend {
        Backend::Gpu(g) => g.max_items(),
        Backend::Cpu => usize::MAX,
    }
}

fn budget(b: &Baker, kind: Kind) -> usize {
    let p = b.settings.params;
    let base = if p.chunk_luxels > 0 { p.chunk_luxels as usize * max_count(b) as usize } else { 0 };
    if base > 0 {
        return base;
    }
    if let Backend::Gpu(g) = &b.backend {
        let cap = g.max_items() * max_count(b) as usize;
        let spu = g.seconds_per_unit(kind);
        let target = g.submit_seconds();
        let want = if spu > 0.0 { (target / spu) as usize } else { 1 << 15 };
        return want.clamp(MIN_GPU_BATCH, cap.max(MIN_GPU_BATCH));
    }
    let target = if b.on_gpu() { 1usize << 21 } else { 1usize << 16 };
    let lights = (b.scene.light_lists.len() / b.scene.surfaces.len().max(1)).max(1);
    let cost = match kind {
        Kind::Direct => lights + p.sky_samples as usize + 1,
        Kind::Indirect => (p.bounces as usize).min(4) * 2 + 1,
    };
    (target / cost).max(256)
}

fn schedule(b: &Baker, kind: Kind, target: u32, focus: Option<&[u32]>) -> Vec<Work> {
    let st = &b.state;
    let stats = match kind {
        Kind::Direct => &st.dstat,
        Kind::Indirect => &st.istat,
    };
    let p = b.settings.params;
    let mut out = Vec::new();
    let mut push = |si: usize, s: &crate::kernel::SurfInfo| {
        for i in 0..s.luxels() {
            let l = s.first_luxel as usize + i;
            if b.scene.luxels[l].points == 0 {
                continue;
            }
            let n = stats[l].n as u32;
            let want = if n < p.min_samples.min(target) {
                p.min_samples.min(target) - n
            } else if n < p.max_samples && n < target && rel_error(&stats[l]) > p.adaptive_threshold {
                (n.max(1)).min(p.max_samples - n).min(target - n)
            } else {
                0
            };
            if want > 0 {
                out.push(Work { luxel: l as u32, surface: si as u32, start: n, count: want });
            }
        }
    };
    match focus {
        Some(list) => {
            for &si in list {
                if let Some(s) = st.packed.surfaces.get(si as usize) {
                    push(si as usize, s);
                }
            }
        }
        None => {
            for (si, s) in st.packed.surfaces.iter().enumerate() {
                push(si, s);
            }
        }
    }
    out
}

fn split(work: Vec<Work>, per_batch: usize, max_count: u32, max_items: usize) -> Vec<Vec<Work>> {
    let mut items = Vec::with_capacity(work.len());
    for w in work {
        let mut start = w.start;
        let end = w.start + w.count;
        while start < end {
            let c = (end - start).min(max_count);
            items.push(Work { start, count: c, ..w });
            start += c;
        }
    }
    items.sort_by_key(|w| (w.start, w.luxel));
    let mut out: Vec<Vec<Work>> = Vec::new();
    let mut cur: Vec<Work> = Vec::new();
    let mut cost = 0usize;
    let mut last_start = u32::MAX;
    let mut seen = std::collections::HashSet::new();
    for w in items {
        if w.start != last_start {
            last_start = w.start;
        }
        if cost + w.count as usize > per_batch || cur.len() >= max_items || !seen.insert(w.luxel) {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            seen.clear();
            seen.insert(w.luxel);
            cost = 0;
        }
        cost += w.count as usize;
        cur.push(w);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

struct Meter {
    started: Instant,
    total: f64,
    done: f64,
}

impl Meter {
    fn report(&self, ctl: &Control<'_>, phase: Phase, base: f32, span: f32) {
        let f = if self.total > 0.0 { (self.done / self.total).min(1.0) as f32 } else { 1.0 };
        let el = self.started.elapsed().as_secs_f64();
        let left = (f > 0.02).then(|| el / f as f64 * (1.0 - f as f64));
        (ctl.progress)(Progress { phase, fraction: base + span * f, seconds_left: left });
    }
}

fn pass(
    b: &mut Baker,
    kind: Kind,
    target: u32,
    ctl: &Control<'_>,
    phase: Phase,
    base: f32,
    span: f32,
) -> Result<u64, Stop> {
    let mut samples = 0u64;
    let started = Instant::now();
    let mut meter = Meter { started, total: 0.0, done: 0.0 };
    let max_rounds = 16;
    for _ in 0..max_rounds {
        let work = schedule(b, kind, target, None);
        if work.is_empty() {
            break;
        }
        let total: u64 = work.iter().map(|w| u64::from(w.count)).sum();
        meter.total = meter.done + total as f64;
        let per = budget(b, kind);
        for batch in split(work, per, max_count(b), item_cap(b)) {
            if (ctl.cancelled)() {
                return Err(Stop::Cancelled);
            }
            run(b, kind, &batch)?;
            let n: u64 = batch.iter().map(|w| u64::from(w.count)).sum();
            samples += n;
            meter.done += n as f64;
            meter.report(ctl, phase, base, span);
        }
        sync(b, kind)?;
    }
    Ok(samples)
}

pub(crate) fn bake(b: &mut Baker, ctl: &Control<'_>, log: &mut dyn FnMut(&str)) -> Result<Output, Stop> {
    let started = Instant::now();
    let p = b.settings.params;
    let ds = pass(b, Kind::Direct, p.max_samples, ctl, Phase::Direct, 0.0, if p.bounces > 0 { 0.45 } else { 0.9 })?;
    log(&format!(
        "ray tracing: direct light {} samples ({:.1} per luxel) in {:.1} s",
        ds,
        ds as f64 / b.scene.luxels.len().max(1) as f64,
        started.elapsed().as_secs_f64()
    ));
    refresh_cache(b)?;
    let mut is = 0u64;
    if p.bounces > 0 {
        let t = Instant::now();
        is = pass(b, Kind::Indirect, p.max_samples, ctl, Phase::Indirect, 0.45, 0.45)?;
        log(&format!(
            "ray tracing: global illumination {} paths ({:.1} per luxel, up to {} bounces) in {:.1} s",
            is,
            is as f64 / b.scene.luxels.len().max(1) as f64,
            p.bounces,
            t.elapsed().as_secs_f64()
        ));
    }
    if let Backend::Gpu(g) = &b.backend {
        log(&format!("ray tracing: {}", g.timing()));
    }
    (ctl.progress)(Progress { phase: Phase::Denoise, fraction: 0.92, seconds_left: None });
    b.state.finished = true;
    let mut out = output(b, true);
    out.direct_samples = ds;
    out.indirect_samples = is;
    out.seconds = started.elapsed().as_secs_f64();
    (ctl.progress)(Progress { phase: Phase::Denoise, fraction: 1.0, seconds_left: Some(0.0) });
    Ok(out)
}

pub(crate) fn refine(
    b: &mut Baker,
    ctl: &Control<'_>,
    seconds: f32,
    focus: Option<&[u32]>,
) -> Result<Refinement, Stop> {
    let started = Instant::now();
    let p = b.settings.params;
    let mut touched: Vec<u32> = Vec::new();
    let budget_s = f64::from(seconds.max(0.01));
    loop {
        let direct_done = |b: &Baker, t: u32| schedule(b, Kind::Direct, t, focus).is_empty();
        let indirect_done = |b: &Baker, t: u32| {
            p.bounces == 0 || (b.state.cache_fresh && schedule(b, Kind::Indirect, t, focus).is_empty())
        };
        while b.state.target < p.max_samples && direct_done(b, b.state.target) && indirect_done(b, b.state.target) {
            b.state.target = (b.state.target * 2).min(p.max_samples);
        }
        let target = b.state.target;
        let (kind, work) = if !direct_done(b, target) {
            (Kind::Direct, schedule(b, Kind::Direct, target, focus))
        } else if p.bounces > 0 {
            if !b.state.cache_fresh {
                refresh_cache(b)?;
            }
            (Kind::Indirect, schedule(b, Kind::Indirect, target, focus))
        } else {
            break;
        };
        if work.is_empty() {
            break;
        }
        let per = budget(b, kind);
        let left = (budget_s - started.elapsed().as_secs_f64()).max(0.0);
        let spu = match &b.backend {
            Backend::Gpu(g) => g.seconds_per_unit(kind),
            Backend::Cpu => 0.0,
        };
        let mut work = work;
        if spu > 0.0 {
            let fits = ((left / spu) * 1.5) as u64 + per as u64;
            let mut total = 0u64;
            let keep = work.iter().take_while(|w| {
                total += u64::from(w.count);
                total <= fits
            });
            let n = keep.count().max(1);
            work.truncate(n);
        }
        let batches = split(work, per, max_count(b).min(16), item_cap(b));
        for batch in batches {
            if (ctl.cancelled)() {
                return Err(Stop::Cancelled);
            }
            run(b, kind, &batch)?;
            touched.extend(batch.iter().map(|w| w.surface));
            if started.elapsed().as_secs_f64() > budget_s {
                break;
            }
        }
        sync(b, kind)?;
        if kind == Kind::Direct {
            b.state.cache_fresh = false;
        }
        if started.elapsed().as_secs_f64() > budget_s {
            break;
        }
    }
    if p.bounces > 0 && !b.state.cache_fresh && b.state.dstat.iter().any(|s| s.n > 0.0) {
        refresh_cache(b)?;
    }
    touched.sort_unstable();
    touched.dedup();
    let finished = schedule(b, Kind::Direct, p.max_samples, focus).is_empty()
        && (p.bounces == 0 || schedule(b, Kind::Indirect, p.max_samples, focus).is_empty());
    if finished && focus.is_none() {
        b.state.finished = true;
    }
    let lux = b.scene.luxels.len().max(1) as f64;
    let spl = b.state.dstat.iter().map(|s| f64::from(s.n)).sum::<f64>() / lux;
    let progress = if finished { 1.0 } else { (spl / f64::from(p.max_samples.max(1))).min(1.0) as f32 };
    (ctl.progress)(Progress { phase: Phase::Direct, fraction: progress, seconds_left: None });
    Ok(Refinement {
        surfaces: touched,
        progress,
        samples_per_luxel: spl as f32,
        finished,
        seconds: started.elapsed().as_secs_f64(),
    })
}

fn refresh_cache(b: &mut Baker) -> Result<(), String> {
    sync(b, Kind::Direct)?;
    build_cache(b);
    if let Backend::Gpu(g) = &mut b.backend {
        g.upload_cache(&b.state)?;
    }
    b.state.cache_fresh = true;
    Ok(())
}

pub(crate) fn output(b: &Baker, denoise: bool) -> Output {
    let strength = if denoise { b.settings.params.denoise } else { 0.0 };
    let surfaces: Vec<SurfaceOut> =
        (0..b.state.packed.surfaces.len()).into_par_iter().map(|si| surface_out(b, si, strength)).collect();
    Output { surfaces, rays: b.state.rays, ..Output::default() }
}

pub(crate) fn output_some(b: &Baker, list: &[u32], denoise: bool) -> Vec<(u32, SurfaceOut)> {
    let strength = if denoise { b.settings.params.denoise } else { 0.0 };
    list.par_iter()
        .filter(|&&si| (si as usize) < b.state.packed.surfaces.len())
        .map(|&si| (si, surface_out(b, si as usize, strength)))
        .collect()
}

fn surface_out(b: &Baker, si: usize, strength: f32) -> SurfaceOut {
    let st = &b.state;
    let passes = st.packed.passes;
    let s = &st.packed.surfaces[si];
    let nn = s.normals as usize;
    let ns = s.slots as usize;
    let lux = s.luxels();
    let len = passes * ns * nn * lux;
    let mut direct = vec![[0.0f32; 3]; len];
    let mut indirect = vec![[0.0f32; 3]; len];
    let mut ao = vec![1.0f32; lux];
    let mut dvar = vec![0.0f32; lux];
    let mut ivar = vec![0.0f32; lux];
    let mut n_total = 0.0f32;
    for i in 0..lux {
        let l = s.first_luxel as usize + i;
        let off = st.lux_offset[l] as usize;
        let (dn, inn) = (st.dstat[l].n, st.istat[l].n);
        n_total += dn;
        if dn > 0.0 {
            ao[i] = st.dstat[l].ao / dn;
            dvar[i] = variance(&st.dstat[l]);
        }
        if inn > 0.0 {
            ivar[i] = variance(&st.istat[l]);
        }
        for pass in 0..passes {
            for slot in 0..ns {
                for n in 0..nn {
                    let src = off + (pass * ns + slot) * nn + n;
                    let dst = ((pass * ns + slot) * nn + n) * lux + i;
                    if dn > 0.0 {
                        direct[dst] = (st.direct[src] / dn).to_array();
                    }
                    if inn > 0.0 {
                        indirect[dst] = (st.indirect[src] / inn).to_array();
                    }
                }
            }
        }
    }
    if strength > 0.0 {
        let geo = crate::denoise::Geometry::of(&b.scene, si);
        crate::denoise::denoise(&geo, &mut indirect, &ivar, strength, false);
        crate::denoise::denoise(&geo, &mut direct, &dvar, strength, true);
        let ao_var = vec![0.02f32; lux];
        let mut ao3: Vec<[f32; 3]> = ao.iter().map(|a| [*a; 3]).collect();
        let ao_geo = crate::denoise::Geometry { channels: 1, probe: 0, ..geo };
        crate::denoise::denoise(&ao_geo, &mut ao3, &ao_var, strength, false);
        for (a, v) in ao.iter_mut().zip(&ao3) {
            *a = v[0];
        }
    }
    SurfaceOut { direct, indirect, ao, samples: n_total / lux.max(1) as f32 }
}

fn variance(s: &Stat) -> f32 {
    if s.n < 2.0 {
        return 0.0;
    }
    let mean = s.sum / s.n;
    ((s.sq / s.n - mean * mean).max(0.0)) / s.n
}
