use hp_app::AppState;
use hp_app::runtime::collaboration::{CameraMode, CollaborationRequest, color_hex};
use hp_mcp::protocol::ToolResult;
use serde_json::json;

use super::semantic::*;
use super::tools::{Args, R};

domain! { "team";
    get_team_mapping_status: Read, "Team Mapping status",
        "The state of the Team Mapping session of this window: whether a session is active and connected, host \
        or guest, session name, status line, the invitation code (host only: it is a credential, give it only to \
        people the user wants to admit), shared revision, whether an edit waits for the host, the values kept only \
        on this PC because Team Mapping refuses to share them (entity, key and value), the transfer of \
        the shared map while joining (objects received of the total, bytes), traffic, the synchronization \
        timings (last and average round trip of an own edit, time to apply the last remote edit and its \
        maximum, time to capture the last local edit and its maximum), the counts of edits sent, received and \
        sent live while a stroke or drag was held, the size of the last sent edit, the number of objects kept \
        in the local recovery copy after conflicts, the participants (id, nickname, colour used for their camera, \
        selection and edits in the views, network round trip, upload and download per second, average edit \
        round trip, whether they edited in the last 2 seconds, number of selected objects, how many of your \
        selected objects they also select), the participant you follow, the recent chat lines and ping markers, \
        the reconnection state after a network loss and how many times the host sent the whole map again \
        because this connection fell behind (resyncs), and the last notices about script or command entities \
        another participant added or changed (script_notices). The live edit option is the setting \
        workflow.team_live_edits (set_setting).",
        {}, [];
    team_mapping_host: Editor, "Host a Team Mapping session",
        "Share the map of the active tab in a new Team Mapping session (Team Mapping > Host selected map). The \
        relay mode keeps IP addresses private; direct mode listens on a UDP port and guests need the host IP. \
        Answers at once: read get_team_mapping_status until connected is true, then give the invitation, the \
        session name and the password to the guests through a trusted channel. Only when the user asked to \
        share the map.",
        {
            "session": string("Session name: 1 to 48 ASCII letters, digits, spaces, _ or -."),
            "nickname": string("Host nickname: 1 to 12 ASCII letters, digits, _ or -."),
            "password": string("Session password, 8 to 256 characters."),
            "direct": boolean("Direct connection instead of the relay (default false)."),
            "port": integer("UDP listen port in direct mode (default 41840).", 1024, 65535)
        },
        ["session", "nickname", "password"];
    team_mapping_join: Editor, "Join a Team Mapping session",
        "Join a Team Mapping session in a new local tab (Team Mapping > Join). Needs the invitation code, the \
        exact session name and the password given by the host, and for a direct session the host IP:port. \
        Answers at once: read get_team_mapping_status, the map transfer progress is reported until connected \
        is true. Only when the user asked to join.",
        {
            "invitation": string("Invitation code starting with hpjoin1:."),
            "session": string("Exact session name."),
            "nickname": string("Your nickname: 1 to 12 ASCII letters, digits, _ or -."),
            "password": string("Session password."),
            "address": string("Host IP:port for a direct session; leave out for a relay session.")
        },
        ["invitation", "session", "nickname", "password"];
    team_mapping_save_recovery: Editor, "Save the Team Mapping recovery copy",
        "Write the local recovery copy (Team Mapping > Recover local copy) to a VMF file: the shared map with your \
        versions of the objects that lost a conflict or that the host refused (recovery_objects in \
        get_team_mapping_status). Written in the background; the count drops to 0 when it is saved.",
        { "path": string("Absolute path of the .vmf file to write.") },
        ["path"];
    team_mapping_chat: Editor, "Send a Team Mapping chat message",
        "Send a short message to every participant of the session (Team Mapping > Chat). It appears in their chat \
        and as a notification when their panel is closed. Only when the user asked to write to the team.",
        { "text": string("Message, 1 to 200 characters.") },
        ["text"];
    team_mapping_ping: Editor, "Ping a point for the team",
        "Show a pulsing marker with your name and colour at a point of the shared map to every participant for 8 \
        seconds (Ping button, or right click > Ping here for the team). Without a position, the point at the \
        centre of your 3D view is used.",
        { "position": vec3("World position [x, y, z]; default: the surface at the centre of the 3D view.") },
        [];
    team_mapping_camera: Editor, "Go to or follow a Team Mapping participant",
        "Move your own view to another participant: teleport (camera at their position and direction, 2D views \
        centred), look (turn toward them), follow (your camera keeps following theirs until you move it or call \
        stop), selection (frame the objects they have selected) or stop (stop following). Switches to the shared \
        map tab. Never changes the map.",
        {
            "participant": string("Participant id or nickname (get_team_mapping_status lists them)."),
            "mode": choice("teleport (default), look, follow, selection or stop.", &["teleport", "look", "follow", "selection", "stop"])
        },
        [];
    team_mapping_reconnect: Editor, "Reconnect to the Team Mapping session",
        "As a guest: reconnect now when the connection was lost (the editor retries by itself 6 times over about 1 \
        minute), try again after it gave up, or resynchronize a working session by downloading the shared map \
        again. Edits made while offline are sent again when nobody else changed the same objects, otherwise kept \
        in the recovery copy. stop: give up and keep the map only on this PC.",
        { "stop": boolean("Stop reconnecting and stay offline (default false).") },
        [];
    team_mapping_leave: Editor, "Leave the Team Mapping session",
        "Leave the current Team Mapping session (as host this ends it for everyone). The local copy of the map \
        stays open.",
        {}, [];
}

fn get_team_mapping_status(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let rt = &state.runtime.collaboration;
    let s = &rt.stats;
    let active = !s.session.is_empty();
    Ok(ToolResult::json(json!({
        "active": active,
        "connected": rt.connected,
        "host": active && s.host,
        "session": s.session,
        "status": s.status,
        "invitation": (!s.invitation.is_empty()).then(|| s.invitation.clone()),
        "revision": s.revision,
        "pending_edit": s.pending,
        "loading": s.loading,
        "join": { "objects": s.join_units, "total": s.join_total, "bytes": s.join_bytes, "milliseconds": s.join_ms },
        "traffic": { "sent_bytes": s.sent_bytes, "received_bytes": s.received_bytes },
        "timings_ms": {
            "round_trip": s.round_trip_ms,
            "round_trip_average": s.round_trip_average_ms,
            "remote_apply": s.apply_ms,
            "remote_apply_max": s.apply_max_ms,
            "local_capture": s.capture_ms,
            "local_capture_max": s.capture_max_ms,
        },
        "edits": { "sent": s.edits_sent, "received": s.edits_received, "live": s.live_edits, "last_bytes": s.last_edit_bytes },
        "recovery_objects": s.conflicts,
        "kept_only_on_this_pc": s.local_only,
        "held_objects": s.held,
        "live_edits": state.settings.workflow.team_live_edits,
        "participants": rt.participants.iter().map(|p| json!({
            "id": p.id,
            "name": p.name,
            "color": color_hex(p.color),
            "latency_ms": p.latency_ms,
            "upload_bytes_per_second": if p.id == rt.local_id { s.upload } else { p.upload },
            "download_bytes_per_second": if p.id == rt.local_id { s.download } else { p.download },
            "edit_round_trip_ms": if p.id == rt.local_id { s.round_trip_average_ms.round() as u32 } else { p.round_trip_ms },
            "editing": p.editing,
            "selected": p.selected.len(),
            "also_selected_by_you": rt.locks.iter().find(|lock| lock.participant == p.id).map_or(0, |lock| lock.objects),
            "position": p.position,
            "you": p.id == rt.local_id,
        })).collect::<Vec<_>>(),
        "following": rt.following,
        "chat": rt.chat.iter().rev().take(20).rev().map(|line| json!({
            "from": line.name,
            "participant": line.author,
            "text": line.text,
            "ping": line.marker,
            "seconds_ago": line.at.elapsed().as_secs(),
            "you": line.own,
        })).collect::<Vec<_>>(),
        "markers": rt.markers.iter().map(|marker| json!({
            "from": marker.name,
            "position": marker.position,
            "seconds_ago": marker.at.elapsed().as_secs_f64(),
        })).collect::<Vec<_>>(),
        "reconnecting": rt.reconnecting.as_ref().map(|r| json!({
            "attempt": r.attempt,
            "attempts": r.attempts,
            "connecting": r.connecting,
            "next_in_seconds": r.next.map(|next| next.saturating_duration_since(std::time::Instant::now()).as_secs_f64()),
            "gave_up": !r.connecting && r.next.is_none(),
            "error": r.error,
        })),
        "reconnects": s.reconnects,
        "offline_edits_resent": s.offline_resent,
        "resyncs": s.resyncs,
        "script_notices": s.script_notices,
    })))
}

fn participant(state: &AppState, a: &Args<'_>) -> R<u32> {
    let rt = &state.runtime.collaboration;
    let Some(value) = a.opt_str("participant", 32)? else {
        return Err("participant is required".to_owned());
    };
    let found = match value.trim().parse::<u32>() {
        Ok(id) => rt.participant(id).map(|p| p.id),
        Err(_) => rt.participants.iter().find(|p| p.name.eq_ignore_ascii_case(value.trim())).map(|p| p.id),
    };
    found.ok_or_else(|| format!("no participant \"{value}\" in this session"))
}

fn team_mapping_chat(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if !state.runtime.collaboration.connected {
        return Err("Team Mapping is not connected".to_owned());
    }
    let text = a.str("text", 1024)?.trim().to_owned();
    if text.is_empty() || text.chars().count() > 200 {
        return Err("text must have 1 to 200 characters".to_owned());
    }
    state.runtime.collaboration.requests.push(CollaborationRequest::Chat { text: text.clone() });
    Ok(ToolResult::json(json!({ "sent": true, "text": text })))
}

fn team_mapping_ping(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if !state.runtime.collaboration.connected {
        return Err("Team Mapping is not connected".to_owned());
    }
    let position = a.opt_vec3("position")?;
    if position.is_some_and(|p| !p.is_finite() || p.abs().max_element() > 1_000_000.0) {
        return Err("position must be finite and within 1000000 units".to_owned());
    }
    let position = position.map(|p| p.to_array());
    state.runtime.collaboration.requests.push(CollaborationRequest::Ping { position });
    Ok(ToolResult::json(json!({ "pinged": true, "position": position })))
}

fn team_mapping_camera(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if !state.runtime.collaboration.connected {
        return Err("Team Mapping is not connected".to_owned());
    }
    let mode = match a.opt_enum("mode", &["teleport", "look", "follow", "selection", "stop"])?.as_deref() {
        Some("look") => CameraMode::Look,
        Some("follow") => CameraMode::Follow,
        Some("selection") => CameraMode::Selection,
        Some("stop") => CameraMode::Stop,
        _ => CameraMode::Teleport,
    };
    let id = if mode == CameraMode::Stop {
        state.runtime.collaboration.following.unwrap_or(0)
    } else {
        participant(state, a)?
    };
    if mode != CameraMode::Stop && id == state.runtime.collaboration.local_id {
        return Err("choose another participant than yourself".to_owned());
    }
    state.runtime.collaboration.requests.push(CollaborationRequest::Camera { participant: id, mode });
    Ok(ToolResult::json(json!({ "participant": id, "mode": format!("{mode:?}").to_lowercase() })))
}

fn team_mapping_reconnect(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let stop = a.opt_bool("stop")?.unwrap_or(false);
    let rt = &state.runtime.collaboration;
    if stop {
        if rt.reconnecting.is_none() {
            return Err("no reconnection in progress".to_owned());
        }
        state.runtime.collaboration.requests.push(CollaborationRequest::StopReconnecting);
        return Ok(ToolResult::json(json!({ "stopped": true })));
    }
    if rt.reconnecting.is_none() && (!rt.connected || rt.stats.host) {
        return Err("only a connected guest or a guest who lost the connection can reconnect".to_owned());
    }
    state.runtime.collaboration.requests.push(CollaborationRequest::Reconnect);
    Ok(ToolResult::json(json!({ "started": true, "next": "call get_team_mapping_status until connected is true" })))
}

fn team_mapping_host(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if !state.runtime.collaboration.stats.session.is_empty() {
        return Err("a Team Mapping session is already active: call team_mapping_leave first".to_owned());
    }
    let request = CollaborationRequest::Host {
        session: a.str("session", 48)?.to_owned(),
        nickname: a.str("nickname", 12)?.to_owned(),
        password: a.str("password", 256)?.to_owned(),
        direct: a.opt_bool("direct")?.unwrap_or(false),
        port: a.opt_u64("port", 1024, 65535)?.unwrap_or(41840) as u16,
    };
    state.runtime.collaboration.requests.push(request);
    Ok(ToolResult::json(json!({ "started": true, "next": "call get_team_mapping_status until connected is true" })))
}

fn team_mapping_join(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if !state.runtime.collaboration.stats.session.is_empty() {
        return Err("a Team Mapping session is already active: call team_mapping_leave first".to_owned());
    }
    let request = CollaborationRequest::Join {
        invitation: a.str("invitation", 2048)?.to_owned(),
        session: a.str("session", 48)?.to_owned(),
        nickname: a.str("nickname", 12)?.to_owned(),
        password: a.str("password", 256)?.to_owned(),
        address: a.opt_str("address", 128)?.map(str::to_owned),
    };
    state.runtime.collaboration.requests.push(request);
    Ok(ToolResult::json(json!({ "started": true, "next": "call get_team_mapping_status until connected is true" })))
}

fn team_mapping_save_recovery(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let path = std::path::PathBuf::from(a.str("path", 1024)?);
    if !path.is_absolute() || !path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("vmf")) {
        return Err("path must be an absolute path to a .vmf file".to_owned());
    }
    if state.runtime.collaboration.stats.conflicts == 0 {
        return Err("there is no local copy to recover (recovery_objects is 0)".to_owned());
    }
    state.runtime.collaboration.requests.push(CollaborationRequest::SaveRecovery { path: path.clone() });
    Ok(ToolResult::json(json!({ "started": true, "path": path.to_string_lossy() })))
}

fn team_mapping_leave(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let active = !state.runtime.collaboration.stats.session.is_empty();
    state.runtime.collaboration.requests.push(CollaborationRequest::Leave);
    Ok(ToolResult::json(json!({ "left": active })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};
    use hp_app::runtime::collaboration::CollaborationRequest;

    #[test]
    fn team_mapping_tools_queue_requests_and_report_the_session() {
        let _lang = hp_app::testing::lang_lock();
        let mut s = hp_app::testing::test_state();
        let v = ok(&mut s, "get_team_mapping_status", json!({}));
        assert_eq!(v["active"], false);
        assert_eq!(v["live_edits"], true);
        ok(&mut s, "team_mapping_host", json!({ "session": "QA", "nickname": "Host", "password": "long-password" }));
        assert!(matches!(
            &s.runtime.collaboration.requests[0],
            CollaborationRequest::Host { direct: false, port: 41840, .. }
        ));
        s.runtime.collaboration.stats.session = "QA".into();
        s.runtime.collaboration.stats.host = true;
        s.runtime.collaboration.stats.invitation = "hpjoin1:x".into();
        s.runtime.collaboration.stats.round_trip_ms = 12.0;
        let v = ok(&mut s, "get_team_mapping_status", json!({}));
        assert_eq!(v["host"], true);
        assert_eq!(v["invitation"], "hpjoin1:x");
        assert_eq!(v["timings_ms"]["round_trip"], 12.0);
        err(
            &mut s,
            "team_mapping_join",
            json!({ "invitation": "x", "session": "QA", "nickname": "G", "password": "long-password" }),
        );
        ok(&mut s, "team_mapping_leave", json!({}));
        assert_eq!(s.runtime.collaboration.requests.last(), Some(&CollaborationRequest::Leave));
    }

    #[test]
    fn chat_ping_camera_and_reconnect_tools_check_the_session_and_queue_requests() {
        use hp_app::runtime::collaboration::{CameraMode, Collaborator};
        let _lang = hp_app::testing::lang_lock();
        let mut s = hp_app::testing::test_state();
        err(&mut s, "team_mapping_chat", json!({ "text": "hello" }));
        err(&mut s, "team_mapping_ping", json!({}));
        err(&mut s, "team_mapping_camera", json!({ "participant": "Ana" }));
        err(&mut s, "team_mapping_reconnect", json!({}));
        let rt = &mut s.runtime.collaboration;
        rt.connected = true;
        rt.local_id = 2;
        rt.stats.session = "QA".into();
        rt.participants = vec![
            Collaborator { id: 1, name: "Ana".into(), color: [255, 159, 28], upload: 300, ..Collaborator::default() },
            Collaborator { id: 2, name: "Bot".into(), color: [61, 184, 255], ..Collaborator::default() },
        ];
        ok(&mut s, "team_mapping_chat", json!({ "text": "  hello team  " }));
        err(&mut s, "team_mapping_chat", json!({ "text": "x".repeat(201) }));
        err(&mut s, "team_mapping_chat", json!({ "text": "   " }));
        ok(&mut s, "team_mapping_ping", json!({ "position": [1, 2, 3] }));
        ok(&mut s, "team_mapping_ping", json!({}));
        err(&mut s, "team_mapping_ping", json!({ "position": [1e9, 0, 0] }));
        ok(&mut s, "team_mapping_camera", json!({ "participant": "ana", "mode": "follow" }));
        ok(&mut s, "team_mapping_camera", json!({ "participant": "1" }));
        err(&mut s, "team_mapping_camera", json!({ "participant": "Bot" }));
        err(&mut s, "team_mapping_camera", json!({ "participant": "Zed" }));
        ok(&mut s, "team_mapping_reconnect", json!({}));
        err(&mut s, "team_mapping_reconnect", json!({ "stop": true }));
        assert_eq!(
            s.runtime.collaboration.requests,
            vec![
                CollaborationRequest::Chat { text: "hello team".into() },
                CollaborationRequest::Ping { position: Some([1.0, 2.0, 3.0]) },
                CollaborationRequest::Ping { position: None },
                CollaborationRequest::Camera { participant: 1, mode: CameraMode::Follow },
                CollaborationRequest::Camera { participant: 1, mode: CameraMode::Teleport },
                CollaborationRequest::Reconnect,
            ]
        );
        let v = ok(&mut s, "get_team_mapping_status", json!({}));
        assert_eq!(v["participants"][0]["color"], "#ff9f1c");
        assert_eq!(v["participants"][0]["upload_bytes_per_second"], 300);
        assert_eq!(v["participants"][1]["you"], true);
        assert!(v["reconnecting"].is_null());
    }
}
