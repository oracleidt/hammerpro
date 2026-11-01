use hp_app::{AppState, t};
use hp_mcp::protocol::ToolResult;
use serde_json::json;

use super::semantic::*;
use super::tools::{Args, R};

domain! { "discord";
    get_discord_status: Read, "Discord presence status",
        "The state of the Discord Rich Presence: status (connected, connecting, discord_not_running, refused, \
        disabled, no_application_id, bad_application_id, off in automated runs) with its text as shown in \
        Settings > Discord, the application ID in use (Hammer Pro's own one unless the setting \
        discord.application_id replaces it) and the details and state lines sent to Discord. The options \
        themselves are the discord.* settings (get_settings, set_setting).",
        {}, [];
}

fn get_discord_status(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let s = &state.settings.discord;
    let rt = &state.runtime.discord;
    let shown = hp_ui_shell::discord_shown_status(&rt.status, s.enabled, &s.application_id);
    let id = hp_discord::application_id(&s.application_id);
    Ok(ToolResult::json(json!({
        "status": shown.id(),
        "text": t!(shown.label_key()),
        "detail": shown.detail(),
        "connected": shown.is_ok(),
        "enabled": s.enabled,
        "application_id": id,
        "builtin_application_id": s.application_id.trim().is_empty() && !hp_discord::DEFAULT_APPLICATION_ID.is_empty(),
        "activity": rt.activity_details.as_ref().map(|d| json!({ "details": d, "state": rt.activity_state })),
    })))
}

#[cfg(test)]
mod tests {
    use hp_app::runtime::DiscordStatus;
    use serde_json::json;

    use super::super::semantic_tests::ok;

    #[test]
    fn discord_status() {
        let _lang = hp_app::testing::lang_lock();
        let mut s = hp_app::testing::test_state();
        let v = ok(&mut s, "get_discord_status", json!({}));
        assert_eq!(v["status"], "off");
        s.runtime.discord.status = DiscordStatus::NotRunning;
        let v = ok(&mut s, "get_discord_status", json!({}));
        if hp_discord::DEFAULT_APPLICATION_ID.is_empty() {
            assert_eq!(v["status"], "no_application_id");
        } else {
            assert_eq!(v["status"], "discord_not_running");
            assert_eq!(v["builtin_application_id"], true);
            assert_eq!(v["application_id"], hp_discord::DEFAULT_APPLICATION_ID);
        }
        s.settings.discord.application_id = "abc".into();
        assert_eq!(ok(&mut s, "get_discord_status", json!({}))["status"], "bad_application_id");
        s.settings.discord.application_id = "1234567890123456789".into();
        s.runtime.discord.status = DiscordStatus::Refused("Invalid Client ID (4000)".into());
        let v = ok(&mut s, "get_discord_status", json!({}));
        assert_eq!(v["status"], "refused");
        assert_eq!(v["detail"], "Invalid Client ID (4000)");
        assert_eq!(v["builtin_application_id"], false);
        s.runtime.discord.status = DiscordStatus::Connected;
        assert_eq!(ok(&mut s, "get_discord_status", json!({}))["connected"], true);
        s.settings.discord.enabled = false;
        assert_eq!(ok(&mut s, "get_discord_status", json!({}))["status"], "disabled");
    }
}
