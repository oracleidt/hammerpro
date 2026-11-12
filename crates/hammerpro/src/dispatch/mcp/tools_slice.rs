use glam::DVec3;
use hp_app::{AppState, ViewKind};
use hp_mcp::protocol::ToolResult;
use hp_tools_create::{MAX_SEGMENTS, Orientation, SliceLayout, orientation_axis, snap_position, view_basis};
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, MAX_NAME, R, bounds_json, edit, material_name, v3};

domain! { "slicing";
    slice_brushes: Destructive, "Slice brushes",
        "Cut solids into several blocks with evenly spaced parallel planes (the slice tool, Shift+Y, in the 3D \
        view: left to right or top to bottom). The planes are perpendicular to one axis of the active grid \
        (the world axes on a plain grid, the grid axes on a rotated or custom grid): orientation left_right \
        cuts across the horizontal axis nearest the screen right of the chosen view, top_bottom across the \
        vertical axis; axis x, y or z names the grid axis directly. segments is the number of cuts (1 to 64): \
        N cuts give N+1 blocks. position is where the cut goes along the axis (the plane distance n.p, the \
        coordinate on a plain grid), always kept inside the limits of the selection: the first plane is at \
        position, and the planes repeat the distance from the nearest face of the selection to position toward \
        the other face, so position 16 units from the edge with 3 segments cuts every 16 units; omit position \
        to cut at the center (an even split into N+1 equal blocks). Cuts that would leave a piece thinner \
        than 0.25 unit are dropped. snap rounds position to the grid step along the axis (default false). The \
        pieces are convex brushes that keep the original textures (the cut faces take the material of the \
        closest face, or cap_material); brush entity solids keep their entity. Displacements are kept on the pieces (exact where the cut follows the displacement grid, resampled up to power 4 elsewhere, seams sewn) unless the setting edit.keep_displacements_on_cut is false. One undo step. dry_run \
        answers the planes without changing the map. Answers the ids of the resulting solids.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "orientation": choice("left_right (default) or top_bottom, relative to the view.", &["left_right", "top_bottom"]),
            "axis": axis("Grid axis of the planes (instead of orientation)."),
            "view": choice("View the orientation is relative to (default perspective: the 3D camera).", &["perspective", "top", "front", "side"]),
            "world_axes": boolean("Use the world axes instead of the active grid axes (default false)."),
            "position": number("Where the cut goes along the axis (default: the center of the selection).", -262144.0, 262144.0),
            "segments": integer("Number of cuts, N cuts give N+1 blocks (default 1).", 1, i64::from(MAX_SEGMENTS)),
            "snap": boolean("Round position to the grid step (default false)."),
            "cap_material": string("Material of the cut faces (default: the closest face's)."),
            "dry_run": boolean("Only answer the planes, change nothing (default false)."),
        }, [];
}

fn slice_brushes(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let solids = target_solids(state, a, "solid_ids")?;
    let kind = match a.opt_enum("view", &["perspective", "top", "front", "side"])?.as_deref() {
        Some("top") => ViewKind::Top,
        Some("front") => ViewKind::Front,
        Some("side") => ViewKind::Side,
        _ => ViewKind::Perspective,
    };
    let frame = if a.opt_bool("world_axes")?.unwrap_or(false) {
        hp_app::grid::GridFrame::uniform(state.settings.edit.grid_size)
    } else {
        state.edit_grid_frame(kind)
    };
    let orientation = match a.opt_enum("orientation", &["left_right", "top_bottom"])?.as_deref() {
        Some("top_bottom") => Orientation::TopBottom,
        _ => Orientation::LeftRight,
    };
    let axis_index = match (a.opt_enum("axis", &["x", "y", "z"])?.as_deref(), a.get("orientation")) {
        (Some(_), Some(_)) => return Err("give axis or orientation, not both".to_owned()),
        (Some("x"), None) => 0,
        (Some("y"), None) => 1,
        (Some(_), None) => 2,
        (None, _) => {
            let (right, up) = view_basis(kind, state.cameras.cam3d.right());
            orientation_axis(&frame, orientation, right, up)
        }
    };
    let segments = a.opt_u64("segments", 1, u64::from(MAX_SEGMENTS))?.unwrap_or(1) as u32;
    let mut position = a.opt_f64("position", -262144.0, 262144.0)?;
    if a.opt_bool("snap")?.unwrap_or(false)
        && let Some(p) = position
    {
        position = Some(snap_position(&frame, axis_index, p));
    }
    let cap = a.opt_str("cap_material", MAX_NAME)?.map(material_name).transpose()?;
    let dry_run = a.opt_bool("dry_run")?.unwrap_or(false);
    let layout = SliceLayout::new(&state.doc, &frame, axis_index, solids.clone(), position, segments)
        .ok_or("the solids have no volume to cut")?;
    let planes = layout.planes();
    let axis_name = ["x", "y", "z"][layout.axis];
    let mut out = json!({
        "dry_run": dry_run,
        "axis": axis_name,
        "normal": v3(layout.normal + DVec3::ZERO),
        "extent": { "min": layout.lo, "max": layout.hi },
        "positions": layout.positions,
        "planes": planes.iter().map(|p| json!({ "normal": v3(p.normal + DVec3::ZERO), "distance": p.dist })).collect::<Vec<_>>(),
        "cuts": planes.len(),
        "blocks": layout.blocks(),
        "bounds": bounds_json(Some(layout.bounds)),
    });
    if planes.is_empty() {
        return Err("no cut fits inside the solids: move position away from the edge of the selection".to_owned());
    }
    if dry_run {
        return Ok(ToolResult::json(out));
    }
    let ids = edit(state, "slice_brushes", |doc| doc.slice_solids(&solids, &planes, cap.as_deref()))?;
    if let Value::Object(map) = &mut out {
        map.insert("solid_ids".to_owned(), json!(ids));
    }
    Ok(ToolResult::json(out))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    fn block(s: &mut hp_app::AppState, min: [i32; 3], max: [i32; 3]) -> u64 {
        let b = ok(s, "create_primitive", json!({ "min": min, "max": max }));
        b["solid_ids"][0].as_u64().unwrap()
    }

    #[test]
    fn slices_a_block_into_equal_parts() {
        let mut s = hp_app::testing::test_state();
        let id = block(&mut s, [0, 0, 0], [120, 64, 64]);
        let d = ok(&mut s, "slice_brushes", json!({ "solid_ids": [id], "axis": "x", "segments": 2, "dry_run": true }));
        assert_eq!(d["positions"], json!([40.0, 80.0]));
        assert_eq!(d["blocks"], 3);
        let before = s.doc.stats().solids;
        let r = ok(&mut s, "slice_brushes", json!({ "solid_ids": [id], "axis": "x", "segments": 2 }));
        assert_eq!(r["solid_ids"].as_array().unwrap().len(), 3);
        assert_eq!(s.doc.stats().solids, before + 2);
        let mut widths: Vec<f64> = r["solid_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| s.doc.scene().solid(i.as_u64().unwrap() as u32).unwrap().bounds.size().x)
            .collect();
        widths.sort_by(f64::total_cmp);
        assert_eq!(widths, vec![40.0, 40.0, 40.0]);
    }

    #[test]
    fn position_sets_the_pitch_and_stays_inside() {
        let mut s = hp_app::testing::test_state();
        let id = block(&mut s, [0, 0, 0], [100, 64, 64]);
        let d = ok(
            &mut s,
            "slice_brushes",
            json!({ "solid_ids": [id], "orientation": "left_right", "view": "top", "position": 16, "segments": 3, "dry_run": true }),
        );
        assert_eq!(d["axis"], "x");
        assert_eq!(d["positions"], json!([16.0, 32.0, 48.0]));
        let d = ok(
            &mut s,
            "slice_brushes",
            json!({ "solid_ids": [id], "axis": "x", "position": 84, "segments": 2, "dry_run": true }),
        );
        assert_eq!(d["positions"], json!([68.0, 84.0]));
    }

    #[test]
    fn nothing_outside_the_object() {
        let mut s = hp_app::testing::test_state();
        let id = block(&mut s, [0, 0, 0], [100, 64, 64]);
        err(&mut s, "slice_brushes", json!({ "solid_ids": [id], "axis": "x", "position": 0 }));
        err(&mut s, "slice_brushes", json!({ "solid_ids": [id], "axis": "x", "position": 5000 }));
        err(&mut s, "slice_brushes", json!({ "solid_ids": [id], "axis": "x", "orientation": "left_right" }));
        err(&mut s, "slice_brushes", json!({ "solid_ids": [id], "segments": 0 }));
        err(&mut s, "slice_brushes", json!({ "solid_ids": [id], "segments": 65 }));
    }

    #[test]
    fn top_bottom_cuts_across_z_and_is_one_undo_step() {
        let mut s = hp_app::testing::test_state();
        let id = block(&mut s, [0, 0, 0], [64, 64, 128]);
        let before = s.doc.stats().solids;
        let r = ok(
            &mut s,
            "slice_brushes",
            json!({ "solid_ids": [id], "orientation": "top_bottom", "segments": 3, "snap": true }),
        );
        assert_eq!(r["axis"], "z");
        assert_eq!(r["blocks"], 4);
        assert_eq!(s.doc.stats().solids, before + 3);
        ok(&mut s, "undo", json!({}));
        assert_eq!(s.doc.stats().solids, before);
    }

    #[test]
    fn several_brushes_are_cut_together() {
        let mut s = hp_app::testing::test_state();
        let a = block(&mut s, [0, 0, 0], [64, 64, 64]);
        let b = block(&mut s, [200, 0, 0], [264, 64, 64]);
        let before = s.doc.stats().solids;
        let r = ok(&mut s, "slice_brushes", json!({ "solid_ids": [a, b], "axis": "y", "segments": 1 }));
        assert_eq!(r["solid_ids"].as_array().unwrap().len(), 4);
        assert_eq!(s.doc.stats().solids, before + 2);
    }
}
