use std::time::Instant;

use hp_app::AppState;
use hp_editor::limits::portals::BspVis;
use hp_editor::optimize::cubemaps::{self, CubemapKnowledge, CubemapReport, MaterialClass, Suggestion};

const AUTO_REFRESH_MS: u64 = 400;
const SETTLE: std::time::Duration = std::time::Duration::from_millis(700);

pub(crate) fn material_class(fs: Option<&hp_fs::GameFs>, name: &str) -> Option<MaterialClass> {
    let Some(fs) = fs.filter(|f| !f.sources().is_empty()) else { return cubemaps::class_by_name(name) };
    let mat = fs.material(name)?;
    let mat = mat.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false);
    let env = mat.get("$envmap")?;
    if !env.trim().eq_ignore_ascii_case("env_cubemap") {
        return None;
    }
    Some(cubemaps::class_from_material(name, mat.is_water(), mat.translucent()))
}

#[derive(Clone, Copy)]
enum Channel {
    Rgb,
    Alpha,
}

fn texture_mean(fs: &hp_fs::GameFs, texture: &str, channel: Channel) -> Option<f64> {
    let name = hp_vmt::normalize_texture_path(texture);
    let data = fs.read(&format!("materials/{name}.vtf"))?;
    let info = hp_vtf::read_info(&data).ok()?;
    let mip = info.mip_count.saturating_sub(4);
    let img = hp_vtf::decode_slice_rgba8(&data, mip, 0, 0, 0).ok()?;
    let n = img.pixels.len() / 4;
    if n == 0 {
        return None;
    }
    let sum: f64 = img
        .pixels
        .chunks_exact(4)
        .map(|p| match channel {
            Channel::Rgb => 0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]),
            Channel::Alpha => f64::from(p[3]),
        })
        .sum();
    Some(sum / (n as f64 * 255.0))
}

static STRENGTHS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, f64>>> =
    std::sync::OnceLock::new();

pub(crate) fn material_strength(fs: Option<&hp_fs::GameFs>, name: &str) -> f64 {
    let Some(fs) = fs.filter(|f| !f.sources().is_empty()) else { return 1.0 };
    let key = hp_vmt::normalize_material_name(name);
    let cache = STRENGTHS.get_or_init(Default::default);
    if let Some(v) = cache.lock().ok().and_then(|c| c.get(&key).copied()) {
        return v;
    }
    let v = measure_strength(fs, name);
    if let Ok(mut c) = cache.lock() {
        c.insert(key, v);
    }
    v
}

fn measure_strength(fs: &hp_fs::GameFs, name: &str) -> f64 {
    let Some(mat) = fs.material(name) else { return 1.0 };
    let mat = mat.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false);
    let flag = |k: &str| mat.get(k).is_some_and(|v| v.trim() != "0" && !v.trim().is_empty());
    let tint = mat.get("$envmaptint").and_then(cubemaps::parse_tint).unwrap_or(1.0);
    let mask = if let Some(t) = mat.texture("$envmapmask") {
        texture_mean(fs, &t, Channel::Rgb)
    } else if flag("$basealphaenvmapmask") {
        mat.texture("$basetexture").and_then(|t| texture_mean(fs, &t, Channel::Alpha))
    } else if flag("$normalmapalphaenvmapmask") {
        mat.texture("$bumpmap").and_then(|t| texture_mean(fs, &t, Channel::Alpha))
    } else {
        None
    };
    (tint * mask.unwrap_or(1.0)).clamp(0.0, 4.0)
}

pub(crate) fn vis_of(state: &AppState) -> Option<&BspVis> {
    let rt = &state.runtime.optimize;
    if !rt.stale(state.doc.scene_revision())
        && let Some(v) = rt.vis.as_deref()
    {
        return Some(v);
    }
    state.runtime.health.vis.as_deref()
}

pub(crate) fn analyze_with(state: &AppState, vis: Option<&BspVis>) -> CubemapReport {
    let fs = state.assets.fs();
    let class = |m: &str| material_class(fs, m);
    let strength = |m: &str| material_strength(fs, m);
    let k = CubemapKnowledge { material: &class, strength: &strength };
    cubemaps::analyze(&state.doc, &k, vis)
}

pub(crate) fn refresh(state: &mut AppState) -> std::sync::Arc<CubemapReport> {
    let t = Instant::now();
    let report = analyze_with(state, vis_of(state));
    let ms = t.elapsed().as_millis() as u64;
    let rt = &mut state.runtime.cubemaps;
    rt.store(report, ms);
    rt.vis_version = state.runtime.optimize.version;
    rt.request = false;
    rt.report.clone().unwrap_or_default()
}

pub(crate) fn chosen(state: &AppState, indices: Option<&[usize]>, size: Option<i32>) -> Vec<Suggestion> {
    let rt = &state.runtime.cubemaps;
    rt.pending()
        .into_iter()
        .filter(|(i, _)| indices.is_none_or(|ix| ix.contains(i)))
        .map(|(_, s)| {
            let mut s = s.clone();
            if s.is_new() {
                s.size = size.or(rt.size).unwrap_or(s.size).clamp(0, 12);
            }
            s
        })
        .collect()
}

pub(crate) fn tick(state: &mut AppState, last_edit: Option<Instant>) -> bool {
    let rt = &state.runtime.cubemaps;
    let active = rt.show || rt.view;
    if rt.wait_vis && !state.runtime.optimize.running && state.runtime.optimize.version != rt.vis_version {
        state.runtime.cubemaps.wait_vis = false;
        state.runtime.cubemaps.request = true;
    }
    let rt = &state.runtime.cubemaps;
    let settled = last_edit.is_none_or(|t| t.elapsed() >= SETTLE);
    let auto = active
        && rt.report.is_some()
        && rt.ms <= AUTO_REFRESH_MS
        && rt.stale(state.doc.scene_revision())
        && settled
        && !state.doc.in_transaction();
    if (rt.request || auto) && !state.doc.in_transaction() {
        refresh(state);
        return true;
    }
    active && rt.stale(state.doc.scene_revision()) && !settled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_classify_without_a_game() {
        assert_eq!(material_class(None, "glass/glasswindow001a"), Some(MaterialClass::Glass));
        assert_eq!(material_class(None, "concrete/concretefloor001a"), None);
        assert_eq!(material_strength(None, "glass/glasswindow001a"), 1.0);
    }

    #[test]
    fn refresh_stores_a_report_and_choices_use_the_size() {
        let mut state = hp_app::testing::test_state();
        let r = refresh(&mut state);
        assert_eq!(state.runtime.cubemaps.report.as_ref().map(|x| x.revision), Some(r.revision));
        assert!(!state.runtime.cubemaps.request);
        state.runtime.cubemaps.request = true;
        assert!(tick(&mut state, None));
        assert!(chosen(&state, None, Some(7)).iter().all(|s| !s.is_new() || s.size == 7));
    }
}
