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
        in background tabs), the texture size limit applied under pressure, and what was freed so far.",
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
    })))
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
        ok(&mut s, "free_gpu_memory", json!({ "lower_textures": true }));
        assert_eq!(s.runtime.gpu.release_request, Some(true));
        ok(&mut s, "free_gpu_memory", json!({}));
        assert_eq!(s.runtime.gpu.release_request, Some(false));
        err(&mut s, "free_gpu_memory", json!({ "lower_textures": 3 }));
    }
}
