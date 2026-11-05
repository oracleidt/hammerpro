use hp_app::AppState;
use hp_mcp::protocol::ToolResult;
use serde_json::json;

use super::semantic::*;
use super::tools::{Args, R};

domain! { "detail";
    detail_preview: Editor, "Preview the %detailtype grass in the 3D views",
        "Turn the detail props preview on or off (setting render.detail_preview, toolbar button, action \
        toggle_detail_preview) and report it. With it on, every displacement and brush face (world brushes and \
        func_detail) whose material has the %detailtype key gets the detail objects of the matching group of the \
        map's detail.vbsp (worldspawn detailvbsp, else detail.vbsp, else scripts/detail.vbsp of the game content), \
        placed like VBSP does at compile time: the group density per area, the displacement vertex alpha choosing \
        between the groups by their alpha, minAngle and maxAngle against the surface normal, upright, Sprite entries \
        drawn as billboards from the worldspawn detailmaterial (default detail/detailsprites) with their sprite \
        rectangle, spritesize, spriterandomscale, sprite_shape cross or tri and sway (swaying in time, driven by \
        render.animate_materials), Model entries as static models. Objects are generated only on the surfaces \
        within the view distance of the 3D camera (1200 units, fading over the last 400, or the \
        env_detail_controller fademindist and fademaxdist), nearest first, at most 60000 sprites and 1500 models, \
        and are the same for the same map. Without enabled it only reports. The report: enabled, drawn (objects \
        are on screen), detail_vbsp (the file read and the number of types in it, or the error), sprite_material, \
        surfaces (faces and displacements using a %detailtype that the file defines), displacements, brush_faces, \
        surfaces_in_range, sprites, models, capped (the limits cut some), view_distance, fade_distance and types \
        (material, detail_type, surfaces, known: false when the file has no such group). The player view draws its \
        own compiled detail props and hides this preview.",
        {
            "enabled": boolean("Turn the preview on (true) or off (false); omit to only read it."),
        }, [];
}

fn detail_preview(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if let Some(on) = a.opt_bool("enabled")? {
        state.settings.render.detail_preview = on;
        if !on {
            state.runtime.render.detail_preview = Default::default();
        }
    }
    let on = state.settings.render.detail_preview;
    let r = &state.runtime.render.detail_preview;
    let note = if !on {
        "off"
    } else if state.runtime.lab.player.active {
        "hidden while the player view is active: it draws the compiled detail props"
    } else if r.dict_error.is_some() {
        "the detail.vbsp could not be read"
    } else if r.surfaces == 0 && r.types.is_empty() {
        "no visible face uses a material with a %detailtype (or the first frame is not drawn yet)"
    } else if r.surfaces == 0 {
        "the materials name %detailtype groups that the detail.vbsp does not define"
    } else {
        "ok"
    };
    Ok(ToolResult::json(json!({
        "enabled": on,
        "drawn": r.drawn,
        "detail_vbsp": { "file": r.dict_file, "types": r.dict_types, "error": r.dict_error },
        "sprite_material": r.sprite_material,
        "surfaces": r.surfaces,
        "displacements": r.displacements,
        "brush_faces": r.brush_faces,
        "surfaces_in_range": r.surfaces_in_range,
        "sprites": r.sprites,
        "models": r.models,
        "capped": r.capped,
        "view_distance": r.view_distance,
        "fade_distance": r.fade_distance,
        "types": r.types.iter().map(|t| json!({
            "material": t.material,
            "detail_type": t.detail_type,
            "surfaces": t.surfaces,
            "known": t.known,
        })).collect::<Vec<_>>(),
        "note": note,
    })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::ok;

    #[test]
    fn the_preview_is_switched_and_read() {
        let mut state = hp_app::testing::test_state();
        let off = ok(&mut state, "detail_preview", json!({}));
        assert_eq!(off["enabled"], false);
        assert_eq!(off["note"], "off");
        let on = ok(&mut state, "detail_preview", json!({ "enabled": true }));
        assert!(state.settings.render.detail_preview);
        assert_eq!(on["enabled"], true);
        assert_eq!(on["types"], json!([]));
        assert_eq!(on["drawn"], false);
        let back = ok(&mut state, "detail_preview", json!({ "enabled": false }));
        assert_eq!(back["enabled"], false);
        assert!(!state.settings.render.detail_preview);
    }
}
