use hp_app::AppState;
use hp_collab::moderation::{MAX_BAN_SECONDS, MAX_INVITATION_MINUTES};
use hp_mcp::protocol::ToolResult;
use serde_json::json;

use super::semantic::*;
use super::tools::{AppFn, Args, Outcome, R};
use crate::collaboration::access::{Moderation, PolicyChange};

const ACTIONS: &[&str] = &["admit", "decline", "kick", "ban", "unban", "read_only", "editor"];

domain! { "team";
    get_team_session: Read, "Team Mapping session and security",
        "The Team Mapping session of this window: active, connected, your role (none, host, editor or viewer), \
        session name, connection mode (relay or direct), revision, status line, participants (id, name, role, \
        latency_ms), the access policy (locked: closed to new participants; approval: the host admits each new \
        participant; read_only_guests: new participants join as viewers; invitation_minutes: how long the \
        invitation admits new participants, 0 for the whole session; share_scripts: script and command entities \
        are shared), people waiting for approval (id, name, \
        waiting_seconds), bans (id, name, reason, remaining_seconds) and security counters (failed_passwords, \
        invalid_attempts, refused_joins, rate_limited: participants slowed down for sending too fast, resyncs: \
        times a participant that fell behind received the whole map again, password_lockout_seconds, \
        new_participants_seconds_left, admitted_devices). The invitation code and the password are never returned.",
        {}, [];
    set_team_access: Edit, "Team Mapping access policy",
        "Change the access policy of the hosted Team Mapping session, or the policy the next hosted session \
        starts with when none is active. Only the given fields change. locked closes the session to new \
        participants (already admitted ones can still reconnect); approval makes every new participant wait \
        until the host admits them (moderate_team_session admit or decline); read_only_guests makes new \
        participants viewers who cannot edit; invitation_minutes limits how long the invitation admits new \
        participants (0: the whole session); share_scripts shares lua_run, logic_script, point_servercommand, \
        point_clientcommand and the Command, RunScriptCode, RunScriptFile and RunPassedCode inputs (on by \
        default; off: the host keeps its own on its PC and refuses the ones guests add or change, with a precise \
        message). Guests cannot change it.",
        {
            "locked": boolean("Closed to new participants."),
            "approval": boolean("Ask the host before admitting a new participant."),
            "read_only_guests": boolean("New participants join read only."),
            "invitation_minutes": integer("Minutes the invitation admits new participants, 0 for the whole session.", 0, MAX_INVITATION_MINUTES as i64),
            "share_scripts": boolean("Share script and command entities and inputs (setting workflow.team_share_scripts, on by default)."),
        }, [];
    moderate_team_session: Destructive, "Moderate Team Mapping participants",
        "Host moderation of the Team Mapping session. id is a participant id or a waiting id from get_team_session \
        (a ban id for unban). admit or decline a person waiting for approval; kick removes a participant (with \
        approval or a closed session they must be admitted again); ban refuses the participant and their \
        installation (and IP in direct mode) for minutes (0: until the session ends) with an optional reason \
        shown to them; unban lifts a ban; read_only makes a participant a viewer whose edits are refused and \
        whose selection locks nothing; editor gives editing back.",
        {
            "action": choice("What to do.", ACTIONS),
            "id": integer("Participant, waiting or ban id from get_team_session.", 1, i64::from(u32::MAX)),
            "minutes": integer("Ban duration in minutes, 0 until the session ends (default 0).", 0, (MAX_BAN_SECONDS / 60) as i64),
            "reason": string("Ban reason shown to the participant (up to 256 characters)."),
        }, ["action", "id"];
}

fn get_team_session(_state: &mut AppState, _a: &Args<'_>) -> R<Outcome> {
    Ok(Outcome::AppFn(AppFn(Box::new(|app, _ctx| ToolResult::json(app.collaboration.team_status(&app.state))))))
}

fn set_team_access(_state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let change = PolicyChange {
        locked: a.opt_bool("locked")?,
        approval: a.opt_bool("approval")?,
        read_only_guests: a.opt_bool("read_only_guests")?,
        invitation_minutes: a
            .opt_u64("invitation_minutes", 0, u64::from(MAX_INVITATION_MINUTES))?
            .map(|minutes| minutes as u32),
        share_scripts: a.opt_bool("share_scripts")?,
    };
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
        match app.collaboration.change_policy(&mut app.state, change) {
            Ok(result) => {
                ToolResult::json(json!({ "result": result, "session": app.collaboration.team_status(&app.state) }))
            }
            Err(error) => ToolResult::error(format!("set_team_access: {error}")),
        }
    }))))
}

fn moderate_team_session(_state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let action = a.opt_enum("action", ACTIONS)?.ok_or("action is required")?;
    let id = a.opt_u64("id", 1, u64::from(u32::MAX))?.ok_or("id is required")?;
    let minutes = a.opt_u64("minutes", 0, MAX_BAN_SECONDS / 60)?.unwrap_or(0);
    let reason = a.opt_str("reason", 1024)?.unwrap_or_default().to_owned();
    if reason.chars().count() > 256 || reason.chars().any(char::is_control) {
        return Err("reason must be up to 256 characters without line breaks".into());
    }
    let participant = id as u32;
    let request = match action.as_str() {
        "admit" => Moderation::Admit(participant),
        "decline" => Moderation::Decline(participant),
        "kick" => Moderation::Kick(participant),
        "ban" => Moderation::Ban { participant, minutes, reason },
        "unban" => Moderation::Unban(id),
        "read_only" => Moderation::ReadOnly { participant, read_only: true },
        _ => Moderation::ReadOnly { participant, read_only: false },
    };
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| match app.collaboration.moderate(&app.state, request) {
        Ok(result) => ToolResult::json(json!({ "result": result, "action": action, "id": id })),
        Err(error) => ToolResult::error(format!("moderate_team_session: {error}")),
    }))))
}
