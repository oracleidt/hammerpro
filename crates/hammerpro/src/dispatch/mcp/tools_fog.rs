use hp_app::AppState;
use hp_app::runtime::fog::{self, FogSource, Link, VolumeFog};
use hp_mcp::protocol::ToolResult;
use hp_render::Fog;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, R, v3};

const MAX_LISTED: usize = 256;

domain! { "fog";
    list_fog: Read, "Fog and fog volumes",
        "The fog the 3D views preview and every fog_volume. preview: render.fog_preview (the global fog of an \
        env_fog_controller in the 3D views and the player view), the controller it shows (chosen with \
        preview_fog, else the one the game uses: the last with the Master flag, or the first of the map) and its resolved fog (null when that controller has Fog Enable \
        at No), sky_camera (the sky_camera of the 3D skybox preview) and sky_fog (its own fog keys, drawn in the 3D skybox only and measured in skybox units, null when its fogenable is off or the fog preview is off), render.fog_volume_preview and render.fog_volume_texture. The preview works like the game: linear fog from start to end capped by max_density and mixed in gamma space, color2 blended by the view direction when blend is on, nothing drawn past farz (the sky or the fog color shows there) and the 2D sky never fogged. controllers: id, name, fog_enable, the \
        fog read from its keys (color and color2 as 0 to 255, dir, blend, start, end, max_density, farz: 0 when \
        off) and the fog_volume ids that use it. fog_volumes: id, name, fog_name, bounds (the box around its \
        brushes, like the game), status (controller, controller_disabled, missing_controller or no_fog_name), \
        controller, in_game (fog: the controller fog replaces the map fog while the eyes are inside; clears_fog: \
        Fog Enable at No, no fog inside; map_fog: no controller found, the map fog stays), start_disabled, the \
        fog the 3D view draws in it (preview_fog; default_look true when it is the default look because there \
        is no enabled controller), post_process and color_correction (PostProcessName and ColorCorrectionName: \
        unset, found, missing or wrong_class with the entity), and the notes the 3D view and the entity \
        properties show.",
        {}, [];
    preview_fog: Editor, "Preview the map fog",
        "Choose the env_fog_controller whose fog the 3D views preview (without id: the one the game uses, the last Master one or else the first) and \
        turn the global fog preview on or off (render.fog_preview). fog_volumes turns the fog_volume preview on \
        or off (render.fog_volume_preview) and volume_texture the brush material of the volumes \
        (render.fog_volume_texture).",
        {
            "id": integer("env_fog_controller entity id (default: the one the game uses, the last Master one or else the first).", 1, i64::from(u32::MAX)),
            "show": boolean("Show the global fog preview (default true)."),
            "fog_volumes": boolean("Fill the fog_volumes with their fog in the 3D views."),
            "volume_texture": boolean("Draw the brush material of the fog_volumes in the 3D views."),
        }, [];
}

fn rgb(c: [f32; 3]) -> Value {
    json!(c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
}

fn round(v: f32) -> f64 {
    (f64::from(v) * 1000.0).round() / 1000.0
}

fn fog_json(f: &Fog) -> Value {
    json!({
        "color": rgb(f.color),
        "color2": rgb(f.color2),
        "dir": f.dir.map(round),
        "blend": f.blend,
        "start": round(f.start),
        "end": round(f.end),
        "max_density": round(f.max_density),
        "farz": round(f.far_z),
    })
}

fn link_json(l: &Link) -> Value {
    let mut out = json!({ "status": l.kind(), "name": l.name(), "entity": l.target() });
    if let Link::WrongClass { class, .. } = l {
        out["class"] = json!(class);
    }
    out
}

fn name_of(state: &AppState, id: u32) -> String {
    state.doc.scene().entity(id).and_then(|n| n.entity.get("targetname")).unwrap_or_default().trim().to_owned()
}

fn volume_json(state: &AppState, v: &VolumeFog) -> Value {
    let fog_name =
        state.doc.scene().entity(v.id).and_then(|n| n.entity.get("FogName")).unwrap_or_default().trim().to_owned();
    let in_game = match v.source {
        FogSource::Controller { .. } => "fog",
        FogSource::Disabled { .. } => "clears_fog",
        _ => "map_fog",
    };
    let notes: Vec<Value> =
        fog::volume_notes(v).into_iter().map(|n| json!({ "warning": n.warning, "text": n.text })).collect();
    json!({
        "id": v.id,
        "name": name_of(state, v.id),
        "fog_name": fog_name,
        "bounds": v.bounds.map(|(min, max)| json!({ "min": v3(min), "max": v3(max) })),
        "status": v.source.kind(),
        "controller": v.source.controller(),
        "in_game": in_game,
        "start_disabled": v.start_disabled,
        "preview_fog": fog_json(&v.preview),
        "default_look": v.uses_default_look(),
        "post_process": link_json(&v.post_process),
        "color_correction": link_json(&v.color_correction),
        "notes": notes,
    })
}

fn list_fog(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let doc = &state.doc;
    let r = &state.settings.render;
    let chosen = state.runtime.render.fog_controller;
    let shown = fog::global_controller(doc, chosen).map(|(id, _)| id);
    let sky_camera = state
        .runtime
        .render
        .sky_camera
        .filter(|id| doc.scene().entity(*id).is_some_and(|n| hp_editor::sky::is_sky_camera(&n.entity)))
        .or_else(|| hp_editor::sky::game_sky_camera(doc));
    let sky_fog = sky_camera
        .filter(|_| r.fog_preview)
        .and_then(|id| doc.scene().entity(id))
        .and_then(|n| fog::sky_camera_fog(&n.entity))
        .map(|f| fog_json(&f));
    let mut controllers: Vec<(u32, Value)> = doc
        .scene()
        .entities()
        .iter()
        .filter(|(_, n)| fog::is_fog_controller(&n.entity.classname))
        .map(|(id, n)| {
            let e = &n.entity;
            (
                *id,
                json!({
                    "id": id,
                    "name": e.get("targetname").unwrap_or_default().trim(),
                    "fog_enable": fog::fog_enabled(e),
                    "fog": fog_json(&fog::controller_fog(e)),
                    "previewed": shown == Some(*id),
                    "used_by": fog::volumes_using(doc, *id),
                }),
            )
        })
        .collect();
    controllers.sort_by_key(|(id, _)| *id);
    let ids = fog::fog_volume_ids(doc);
    let volumes: Vec<Value> = ids
        .iter()
        .take(MAX_LISTED)
        .filter_map(|id| fog::resolve_volume(doc, *id))
        .map(|v| volume_json(state, &v))
        .collect();
    Ok(ToolResult::json(json!({
        "preview": {
            "fog_preview": r.fog_preview,
            "controller": shown,
            "chosen": chosen,
            "fog": if r.fog_preview { fog::global_fog(doc, chosen).map(|f| fog_json(&f)) } else { None },
            "sky_camera": sky_camera,
            "sky_fog": sky_fog,
            "fog_volume_preview": r.fog_volume_preview,
            "fog_volume_texture": r.fog_volume_texture,
        },
        "controllers": controllers.into_iter().take(MAX_LISTED).map(|(_, v)| v).collect::<Vec<_>>(),
        "fog_volumes": volumes,
        "fog_volume_total": ids.len(),
    })))
}

fn preview_fog(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = match a.get("id") {
        Some(_) => {
            let id = a.id("id")?;
            match state.doc.scene().entity(id) {
                Some(n) if fog::is_fog_controller(&n.entity.classname) => Some(id),
                Some(n) => return Err(format!("entity {id} is a {}, not an env_fog_controller", n.entity.classname)),
                None => return Err(format!("no entity {id}")),
            }
        }
        None => None,
    };
    let show = a.opt_bool("show")?.unwrap_or(true);
    state.runtime.render.fog_controller = id;
    let r = &mut state.settings.render;
    let before = (r.fog_preview, r.fog_volume_preview, r.fog_volume_texture);
    r.fog_preview = show;
    if let Some(on) = a.opt_bool("fog_volumes")? {
        r.fog_volume_preview = on;
    }
    if let Some(on) = a.opt_bool("volume_texture")? {
        r.fog_volume_texture = on;
    }
    if before != (r.fog_preview, r.fog_volume_preview, r.fog_volume_texture) {
        state.settings_dirty = true;
    }
    let shown = fog::global_controller(&state.doc, id).map(|(c, _)| c);
    let fog = if show { fog::global_fog(&state.doc, id).map(|f| fog_json(&f)) } else { None };
    let r = &state.settings.render;
    Ok(ToolResult::json(json!({
        "controller": shown,
        "default_controller": id.is_none(),
        "fog_preview": show,
        "fog": fog,
        "fog_volume_preview": r.fog_volume_preview,
        "fog_volume_texture": r.fog_volume_texture,
        "note": if show && shown.is_some() && fog.is_none() { Some("that env_fog_controller has Fog Enable at No: no fog") } else { None },
    })))
}

#[cfg(test)]
mod tests {
    use glam::DVec3;
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    fn kv(k: &str, v: &str) -> (String, String) {
        (k.to_owned(), v.to_owned())
    }

    #[test]
    fn fog_is_listed_and_previewed() {
        let mut s = hp_app::testing::test_state();
        hp_app::register_translations();
        let off = s
            .doc
            .create_point_entity("env_fog_controller", DVec3::ZERO, &[kv("targetname", "off")])
            .expect("controller");
        let on = s
            .doc
            .create_point_entity(
                "env_fog_controller",
                DVec3::new(0.0, 0.0, 64.0),
                &[kv("targetname", "mist"), kv("fogenable", "1"), kv("fogcolor", "255 0 0"), kv("farz", "3000")],
            )
            .expect("controller");
        ok(
            &mut s,
            "create_brush",
            json!({
                "min": [-64, -64, 0],
                "max": [64, 64, 128],
                "material": "tools/toolsfog",
                "entity_class": "fog_volume",
                "keyvalues": { "FogName": "mist", "ColorCorrectionName": "cc" },
            }),
        );
        let vol = hp_app::runtime::fog::fog_volume_ids(&s.doc)[0];
        let l = ok(&mut s, "list_fog", json!({}));
        assert_eq!(l["preview"]["fog_preview"], true);
        assert_eq!(l["preview"]["controller"], off);
        assert_eq!(l["preview"]["fog"], json!(null), "the first controller has its fog off");
        assert_eq!((l["preview"]["sky_camera"].clone(), l["preview"]["sky_fog"].clone()), (json!(null), json!(null)));
        assert_eq!(l["controllers"][1]["used_by"], json!([vol]));
        assert_eq!(l["controllers"][1]["fog"]["farz"], 3000.0);
        let v = &l["fog_volumes"][0];
        assert_eq!(
            (v["status"].clone(), v["controller"].clone(), v["in_game"].clone()),
            (json!("controller"), json!(on), json!("fog"))
        );
        assert_eq!(v["preview_fog"]["color"], json!([255, 0, 0]));
        assert_eq!(v["color_correction"]["status"], "missing");
        assert_eq!(v["bounds"]["max"], json!([64.0, 64.0, 128.0]));
        let p = ok(&mut s, "preview_fog", json!({ "id": on, "fog_volumes": false }));
        assert_eq!(p["fog"]["color"], json!([255, 0, 0]));
        assert!(!s.settings.render.fog_volume_preview);
        assert_eq!(s.runtime.render.fog_controller, Some(on));
        s.doc.edit_entity(on, |e| e.set("fogenable", "0")).expect("edit");
        let v = &ok(&mut s, "list_fog", json!({}))["fog_volumes"][0];
        assert_eq!(
            (v["status"].clone(), v["in_game"].clone(), v["default_look"].clone()),
            (json!("controller_disabled"), json!("clears_fog"), json!(true))
        );
        assert!(err(&mut s, "preview_fog", json!({ "id": vol })).contains("not an env_fog_controller"));
        let sky = s
            .doc
            .create_point_entity(
                "sky_camera",
                DVec3::new(0.0, 0.0, -2000.0),
                &[kv("fogenable", "1"), kv("fogcolor", "0 255 0"), kv("fogstart", "10"), kv("fogend", "90")],
            )
            .expect("sky camera");
        let p = &ok(&mut s, "list_fog", json!({}))["preview"];
        assert_eq!(p["sky_camera"], sky);
        assert_eq!((p["sky_fog"]["color"].clone(), p["sky_fog"]["end"].clone()), (json!([0, 255, 0]), json!(90.0)));
        ok(&mut s, "preview_fog", json!({ "show": false }));
        assert!(!s.settings.render.fog_preview && s.runtime.render.fog_controller.is_none());
    }
}
