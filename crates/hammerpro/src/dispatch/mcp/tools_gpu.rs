use hp_app::AppState;
use hp_mcp::protocol::ToolResult;
use serde_json::json;

use super::semantic::*;
use super::tools::{Args, R};

domain! { "graphics memory";
    get_gpu_memory: Read, "Graphics memory",
        "How much graphics memory (VRAM) Hammer Pro uses and its budget: usage (from the driver when it reports \
        it, else an estimate), budget (render.vram_budget_mb, 0 means automatic: 90% of what the system gives \
        the process), pressure (normal, high above 85%, critical when over the budget or after an out of memory \
        error), the estimated split (geometry, textures, models, lightmaps, compiled world, other, maps parked \
        in background tabs), the texture size limit applied under pressure, what was freed so far, and \
        texture_streaming (render.texture_streaming): how many textures far from every view are held at a smaller \
        size, the graphics memory they use now against their full size, the texture budget (60% of the budget) \
        and the reduction level used to stay under it (1 when the budget is not reached).",
        {}, [];
    get_performance: Read, "Frame time and quality",
        "Frame time and quality of the 3D view: FPS, frame time, CPU time of the last 3D render (prepare, record, \
        submit), GPU time from timestamp queries when the graphics card supports them (shadows, scene, post \
        effects, water reflection; passes splits it into light_cull (the clustered light lists of the lighting \
        preview), sun_shadows (sun, sky light and Textured mode shadow maps), local_shadows (the point and spot \
        light shadow maps and projected textures), main (the 3D view itself), effects (water, fog volumes, sun \
        glow, refraction), present (tone mapping, bloom, display) and outline (selection outline), and \
        water_reflection_passes splits the water reflection the same way), working set RAM, the graphics card, whether it is integrated, the performance \
        mode (render.performance_mode: Auto, Off, Low or VeryLow), the quality level in force (full, low, very_low), \
        whether Auto lowered it and at how many FPS, render.render_scale and render.perf_hud (the on-screen readout). \
        Auto only lowers the quality after the 3D view stays under 20 FPS for several seconds while it draws continuously (frames spaced out on purpose while idle do not count). Drive the camera \
        (set_camera, wait_frames) before reading to measure a view. Idle diagnostics: window_focused, animations_running (animated materials, sprite sway and \
        light styles are drawn: false in the background unless render.animate_in_background is on, and after \
        render.animation_idle_pause_s without input), frame_index (frames drawn so far), \
        frames_from_events (frames drawn for input or window events), continuous_frames (frames drawn right after \
        a request for an immediate repaint) and repaint_causes (source locations that asked for a repaint, with \
        their counts since the start, most frequent first): an idle window draws no frame, so they stay still. frame_phases_ms splits the CPU time of the last \
        frame of the window (begin, panels, tool, sync, lighting, views, chrome, dialogs, end) and worst_frame_ms \
        holds the slowest frame since the previous get_performance call, average_frame_ms the average of the \
        frames_since_last_read frames drawn since that call. views_ms gives, for each of the four \
        views in the last frame, the CPU time spent building its overlay (layers, grid, tool), drawing it (or reusing \
        its last image when nothing changed: rendered false) and painting its labels, with its overlay item count.",
        {}, [];
    run_benchmark: Editor, "Benchmark the 3D view",
        "Turn the 3D camera through a full circle around its position over a number of frames (the camera is         put back afterwards) and measure the frame time of the real window: average FPS, frame time average,         95th and 99th percentile and worst, CPU render time (average, median and 99th percentile), GPU time per stage when the card supports timestamp         queries (total, median, 99th percentile, and per pass like get_performance gpu_ms.passes and water_reflection_passes), and the working set RAM at the start, end and peak. It starts at once; call wait_frames, then         get_performance: its benchmark field is running while it runs, then holds the result. vsync caps the         FPS at the refresh rate, so compare the CPU and GPU milliseconds as well.",
        {
            "frames": integer("Frames to draw (default 300).", 10, 3600),
            "trace": boolean("Also return every frame (frame time, CPU and GPU render time, GPU time per pass, CPU time of the views phase, of drawing the views, of their overlays and labels, and of the whole frame) in benchmark.trace.")
        }, [];
    restore_full_quality: Editor, "Restore full quality",
        "Go back to the full quality preview after Auto lowered it, and pause the automatic reduction until         render.performance_mode is changed. Same as the Restore full quality button over the 3D view.",
        {}, [];
    free_gpu_memory: Editor, "Free graphics memory",
        "Free graphics memory now: the maps parked in background tabs (they load again when shown), textures \
        and models no open map uses, and the thumbnail caches. With lower_textures, textures are also loaded \
        again at half their size limit (they come back to full size when memory is free for a while).",
        { "lower_textures": boolean("Also lower the texture size limit one step.") }, [];
}

fn mb(bytes: u64) -> u64 {
    bytes >> 20
}

fn get_gpu_memory(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let g = &state.runtime.gpu;
    let e = &g.estimate;
    Ok(ToolResult::json(json!({
        "usage_mb": mb(g.usage),
        "peak_usage_mb": mb(g.peak_usage),
        "budget_mb": mb(g.budget),
        "budget_is_automatic": g.budget_is_auto,
        "budget_setting_mb": state.settings.render.vram_budget_mb,
        "source": g.source.id(),
        "driver_budget_mb": mb(g.driver_budget),
        "dedicated_mb": mb(g.dedicated),
        "pressure": g.pressure.id(),
        "fraction": g.fraction(),
        "estimate_mb": {
            "geometry": mb(e.geometry),
            "textures": mb(e.textures),
            "models": mb(e.models),
            "lightmaps": mb(e.lightmaps),
            "compiled_world": mb(e.compiled),
            "other": mb(e.other),
            "background_maps": mb(e.background_maps),
            "total": mb(e.total()),
        },
        "background_maps_on_gpu": g.background_maps,
        "keep_background_maps_on_gpu": state.settings.render.keep_background_maps_on_gpu,
        "texture_limit": g.texture_limit,
        "max_texture_size": g.max_texture,
        "out_of_memory_errors": g.out_of_memory_events,
        "releases": g.releases,
        "freed": {
            "background_maps": g.released.background_maps,
            "textures": g.released.textures,
            "models": g.released.models,
            "thumbnails": g.released.thumbnails,
            "textures_loaded_again": g.released.reloading,
        },
        "last_release": g.last_release,
        "warning": g.warning,
        "texture_streaming": texture_streaming_json(state),
    })))
}

fn texture_streaming_json(state: &AppState) -> serde_json::Value {
    let t = state.assets.texture_streaming();
    json!({
        "enabled": state.settings.render.texture_streaming,
        "active": t.active,
        "textures": t.textures,
        "reduced": t.reduced,
        "loading": t.pending,
        "texture_mb": mb(t.bytes),
        "full_size_mb": mb(t.full_bytes),
        "saved_mb": mb(t.full_bytes.saturating_sub(t.bytes)),
        "budget_mb": mb(t.budget),
        "reduction_level": t.aggression,
    })
}

fn bench_json(p: &hp_app::runtime::PerfRuntime) -> serde_json::Value {
    match (&p.bench_result, p.bench_running) {
        (_, true) => json!({ "running": true }),
        (Some(r), false) => json!({
            "running": false,
            "frames": r.frames,
            "seconds": r.seconds,
            "avg_fps": r.avg_fps,
            "frame_ms": { "avg": r.frame_ms_avg, "median": r.frame_ms_median, "p95": r.frame_ms_p95, "p99": r.frame_ms_p99, "max": r.frame_ms_max },
            "cpu_ms_avg": r.cpu_ms_avg,
            "cpu_ms_median": r.cpu_ms_median,
            "cpu_ms_p99": r.cpu_ms_p99,
            "gpu_ms_p99": r.gpu_ms_p99,
            "draw_calls_avg": r.draw_calls_avg,
            "triangles_avg": r.triangles_avg,
            "model_instances_drawn_avg": r.model_instances_avg,
            "gpu_ms_median": r.gpu_ms_median,
            "cpu_phases_ms_avg": { "setup": r.cpu_phases_ms_avg[0], "water_reflection": r.cpu_phases_ms_avg[1], "light_clusters": r.cpu_phases_ms_avg[2], "shadow_prep": r.cpu_phases_ms_avg[3], "model_culling": r.cpu_phases_ms_avg[4], "prepare_total": r.cpu_phases_ms_avg[5], "record": r.cpu_phases_ms_avg[6], "submit": r.cpu_phases_ms_avg[7] },
            "gpu_ms_avg": { "total": r.gpu_ms_avg, "shadows": r.gpu_shadows_ms_avg, "scene": r.gpu_scene_ms_avg, "post": r.gpu_post_ms_avg, "water_reflection": r.gpu_reflection_ms_avg, "passes": passes_json(&r.gpu_passes_ms_avg), "water_reflection_passes": passes_json(&r.gpu_reflection_passes_ms_avg) },
            "working_set_mb": { "start": r.working_set_start_mb, "end": r.working_set_end_mb, "peak": r.working_set_peak_mb },
            "views_rendered": r.views_rendered,
            "trace": (!r.trace.is_empty()).then(|| r.trace.iter().map(|f| json!({
                "frame_ms": f.frame_ms,
                "cpu_ms": f.cpu_ms,
                "gpu_ms": f.gpu_ms,
                "views_ms": f.views_ms,
                "view_render_ms": f.view_render_ms,
                "view_overlay_ms": f.view_overlay_ms,
                "app_ms": f.app_ms,
                "gpu_passes": passes_json(&f.gpu_passes),
            })).collect::<Vec<_>>()),
        }),
        _ => serde_json::Value::Null,
    }
}

fn get_performance(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let r = &state.settings.render;
    let p = &state.runtime.render.perf;
    let s = &p.sample;
    let level = p.level(r.performance_mode);
    let out = json!({
        "fps": state.frame.fps,
        "frame_ms": s.frame_ms,
        "cpu_ms": {
            "total": s.prepare_ms + s.record_ms + s.submit_ms,
            "prepare": s.prepare_ms,
            "record": s.record_ms,
            "submit": s.submit_ms,
            "setup": s.setup_ms,
            "water_reflection": s.reflection_cpu_ms,
            "light_clusters": s.clusters_ms,
            "shadow_prep": s.shadow_prep_ms,
            "model_culling": s.models_ms,
        },
        "gpu_ms": if s.gpu_supported {
            json!({
                "total": s.gpu_total_ms + s.gpu_reflection_ms,
                "shadows": s.gpu_shadows_ms,
                "scene": s.gpu_scene_ms,
                "post": s.gpu_post_ms,
                "water_reflection": s.gpu_reflection_ms,
                "passes": passes_json(&s.gpu_passes),
                "water_reflection_passes": passes_json(&s.gpu_reflection_passes),
            })
        } else {
            serde_json::Value::Null
        },
        "gpu_timing_supported": s.gpu_supported,
        "working_set_mb": s.working_set >> 20,
        "vram_usage_mb": state.runtime.gpu.usage >> 20,
        "adapter": p.adapter,
        "integrated_gpu": p.integrated,
        "performance_mode": r.performance_mode.id(),
        "quality_level": level.id(),
        "reduced_automatically": p.reduced_automatically(r.performance_mode),
        "auto_locked_to_full": p.auto_locked,
        "auto_fps_measured": p.auto_fps,
        "auto_reductions": p.auto_events,
        "render_scale": r.scale(),
        "perf_hud": r.perf_hud,
        "primitive_stats_enabled": r.primitive_stats,
        "map_resource_loading": state.runtime.startup.resources,
        "primitive_stats": state.runtime.primitives.stats,
        "primitive_memory_classes": ["world", "props", "lighting", "shared"],
        "primitive_memory_source": "windows_heap_allocations",
        "benchmark": bench_json(p),
        "views_rendered": state.runtime.render.views_rendered,
        "views_reused": state.runtime.render.views_reused,
        "window_focused": state.runtime.render.focused,
        "animations_running": state.runtime.render.animation.is_some(),
        "frame_index": state.frame.index,
        "frames_from_events": p.frames_from_events,
        "continuous_frames": p.continuous_frames,
        "repaint_causes": p.repaint_causes.iter().map(|c| json!({ "at": c.at, "count": c.count })).collect::<Vec<_>>(),
        "frame_phases_ms": phases_json(&p.frame_phases),
        "views_ms": p.views.iter().map(|v| json!({
            "overlay": v.overlay_ms,
            "render": v.render_ms,
            "paint": v.paint_ms,
            "overlay_items": v.overlay_items,
            "rendered": v.rendered,
        })).collect::<Vec<_>>(),
        "worst_frame_ms": phases_json(&p.worst_frame),
        "average_frame_ms": phases_json(&p.phase_sum.divided(p.phase_frames)),
        "frames_since_last_read": p.phase_frames,
        "adaptive_lighting": state.gfx.as_ref().map(|g| {
            let a = g.renderer.adaptive_stats();
            json!({
                "enabled": a.enabled,
                "level": a.level,
                "max_level": a.max_level,
                "gpu_ms": a.gpu_ms,
                "target_ms": a.target_ms,
                "changes": a.changes,
                "gpu_timestamps": a.timestamps,
            })
        }),
    });
    let p = &mut state.runtime.render.perf;
    p.worst_frame = hp_app::runtime::FramePhases::default();
    p.phase_sum = hp_app::runtime::FramePhases::default();
    p.phase_frames = 0;
    Ok(ToolResult::json(out))
}

fn passes_json(p: &hp_app::runtime::GpuPasses) -> serde_json::Value {
    json!({
        "light_cull": p.light_cull_ms,
        "sun_shadows": p.sun_shadows_ms,
        "local_shadows": p.local_shadows_ms,
        "main": p.main_ms,
        "effects": p.effects_ms,
        "present": p.present_ms,
        "outline": p.outline_ms,
    })
}

fn phases_json(f: &hp_app::runtime::FramePhases) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    out.insert("total".to_owned(), json!(f.total_ms));
    for (name, ms) in hp_app::runtime::FRAME_PHASES.iter().zip(f.ms) {
        out.insert((*name).to_owned(), json!(ms));
    }
    serde_json::Value::Object(out)
}

fn run_benchmark(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let frames = a.opt_u64("frames", 10, 3600)?.unwrap_or(300) as u32;
    let trace = a.opt_bool("trace")?.unwrap_or(false);
    state.runtime.render.perf.bench_request = Some(hp_app::runtime::BenchRequest { frames, trace });
    state.runtime.render.perf.bench_running = true;
    Ok(ToolResult::json(json!({ "started": true, "frames": frames })))
}

fn restore_full_quality(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    state.runtime.render.perf.restore_request = true;
    Ok(ToolResult::json(json!({ "requested": true })))
}

fn free_gpu_memory(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let lower = a.opt_bool("lower_textures")?.unwrap_or(false);
    state.runtime.gpu.release_request = Some(lower);
    Ok(ToolResult::json(json!({ "requested": true, "lower_textures": lower })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn memory_report_and_release_request() {
        let mut s = hp_app::testing::test_state();
        s.runtime.gpu.usage = 3 << 30;
        s.runtime.gpu.budget = 4 << 30;
        let m = ok(&mut s, "get_gpu_memory", json!({}));
        assert_eq!(m["usage_mb"], 3072);
        assert_eq!(m["budget_mb"], 4096);
        assert_eq!(m["pressure"], "normal");
        assert!(m["estimate_mb"]["total"].is_number());
        assert_eq!(m["texture_streaming"]["enabled"], true);
        assert_eq!(m["texture_streaming"]["reduced"], 0);
        ok(&mut s, "free_gpu_memory", json!({ "lower_textures": true }));
        assert_eq!(s.runtime.gpu.release_request, Some(true));
        ok(&mut s, "free_gpu_memory", json!({}));
        assert_eq!(s.runtime.gpu.release_request, Some(false));
        err(&mut s, "free_gpu_memory", json!({ "lower_textures": 3 }));
    }
}
