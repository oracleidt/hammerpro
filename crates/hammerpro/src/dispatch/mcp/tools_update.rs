use hp_app::runtime::UpdateStatus;
use hp_app::{AppState, t, tf};
use hp_mcp::protocol::ToolResult;
use serde_json::json;

use super::semantic::*;
use super::tools::{Args, R};

domain! { "updates";
    get_update_status: Read, "Update status",
        "The state of the auto update: installed version, status (idle, checking, up_to_date, available, \
        downloading, ready, failed), the newest release found (version, tag, title, release notes, page, \
        archive name, size and published SHA-256 checksum), the download progress, the last error, and the \
        preferences update.check_on_startup (check in the background at startup), update.show_dialog (show the update \
        window at all) and update.skipped_version (the version the user chose not to be asked about). The \
        preferences are changed with set_setting on the update.* keys.",
        {}, [];
    check_for_updates: Editor, "Check for updates",
        "Ask GitHub, in the background, whether a newer Hammer Pro release exists (same as Help > Check for \
        updates...). Answers at once; read the result with get_update_status a few seconds later. When one is \
        found, the update dialog opens for the user. Nothing is downloaded or installed.",
        {}, [];
    apply_update: Destructive, "Install the available update",
        "Download the available update, then close Hammer Pro, replace the program and start it again. It \
        interrupts the session (the user is asked to save unsaved maps first) and the connection to this MCP \
        server ends: run it only when the user explicitly asked to update now. Needs confirm: true, a found \
        update (check_for_updates, get_update_status shows status available) and no running compile. Ready made         builds exist for Windows only: on Linux and macOS it refuses and get_update_status gives the commands to         update the clone and build again (self_install false, source_update).",
        { "confirm": boolean("Must be true; only when the user explicitly asked to install the update now.") },
        ["confirm"];
}

fn info_json(state: &AppState) -> serde_json::Value {
    let rt = &state.runtime.update;
    json!({
        "current_version": rt.current,
        "status": rt.status.id(),
        "busy": rt.status.is_busy(),
        "latest": rt.latest.as_ref().map(|l| json!({
            "version": l.version,
            "tag": l.tag,
            "title": l.title,
            "notes": l.notes,
            "release_page": l.page_url,
            "archive": l.asset_name,
            "archive_bytes": l.asset_size,
            "archive_sha256": l.asset_sha256,
            "skipped": state.settings.update.skips(&l.tag),
        })),
        "latest_checked_version": (!rt.latest_seen.is_empty()).then(|| rt.latest_seen.clone()),
        "download": (rt.status == UpdateStatus::Downloading).then(|| json!({
            "done_bytes": rt.done_bytes,
            "total_bytes": rt.total_bytes,
            "fraction": rt.progress(),
        })),
        "error": rt.failure.as_ref().map(|f| tf!(f.key, f.detail)),
        "dialog_open": rt.dialog_open,
        "can_install": rt.can_install(),
        "self_install": hp_app::runtime::update::PREBUILT_PLATFORM,
        "platform": hp_app::runtime::update::platform_name(),
        "source_update": (!hp_app::runtime::update::PREBUILT_PLATFORM).then(|| json!({
            "message": tf!("update.source_body", hp_app::runtime::update::platform_name()),
            "commands": hp_app::runtime::update::SOURCE_UPDATE_COMMANDS.lines().collect::<Vec<_>>(),
        })),
        "check_on_startup": state.settings.update.check_on_startup,
        "show_dialog": state.settings.update.show_dialog,
        "skipped_version": state.settings.update.skipped_version,
        "update_address_overridden": rt.api_overridden,
    })
}

fn get_update_status(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    if state.runtime.update.current.is_empty() {
        state.runtime.update.current = hp_update::current_version().to_string();
    }
    Ok(ToolResult::json(info_json(state)))
}

fn check_for_updates(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let rt = &mut state.runtime.update;
    let started = !rt.status.is_busy();
    if started {
        rt.check_requested = Some(true);
    }
    Ok(ToolResult::json(json!({
        "started": started,
        "status": rt.status.id(),
        "next": "call get_update_status in a few seconds",
    })))
}

fn apply_update(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if a.opt_bool("confirm")? != Some(true) {
        return Err(
            "confirm must be true; the update closes Hammer Pro, so run it only when the user explicitly asked for it"
                .to_owned(),
        );
    }
    if hp_compile::sandbox::is_sandboxed() && !hp_update::api_url_overridden() {
        return Err(t!("update.busy_automated").to_owned());
    }
    let rt = &mut state.runtime.update;
    if !hp_app::runtime::update::PREBUILT_PLATFORM {
        return Err(tf!("update.err_source_only", hp_app::runtime::update::platform_name()));
    }
    if !rt.can_install() {
        return Err(match rt.status {
            UpdateStatus::Downloading => "the update is already downloading".to_owned(),
            UpdateStatus::Ready => "the update is downloaded and Hammer Pro is about to restart".to_owned(),
            _ if rt.latest.as_ref().is_some_and(|l| l.asset_name.is_none()) => t!("update.no_asset").to_owned(),
            _ => "no update is available: run check_for_updates, then get_update_status".to_owned(),
        });
    }
    if state.runtime.compile.progress.is_some() {
        return Err(t!("update.busy_compile").to_owned());
    }
    let version = state.runtime.update.latest.as_ref().map(|l| l.version.clone()).unwrap_or_default();
    state.runtime.update.install_requested = true;
    Ok(ToolResult::json(json!({
        "started": true,
        "version": version,
        "next": "Hammer Pro downloads the update, closes and restarts by itself; this connection ends",
    })))
}

#[cfg(test)]
mod tests {
    use hp_app::runtime::{UpdateFailure, UpdateInfo};
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};
    use super::*;

    fn available(s: &mut AppState, asset: bool) {
        let rt = &mut s.runtime.update;
        rt.status = UpdateStatus::Available;
        rt.latest = Some(UpdateInfo {
            tag: "v1.0.3".into(),
            version: "1.0.3".into(),
            title: "Hammer Pro 1.0.3".into(),
            notes: "- auto update".into(),
            page_url: "https://github.com/oracleidt/hammerpro/releases/tag/v1.0.3".into(),
            asset_name: asset.then(|| "HammerPro-1.0.3-windows.zip".to_owned()),
            asset_size: 1234,
            asset_sha256: Some("ab".repeat(32)),
        });
    }

    #[test]
    fn status_reports_the_state_and_the_preferences() {
        let _lang = hp_app::testing::lang_lock();
        let mut s = hp_app::testing::test_state();
        let v = ok(&mut s, "get_update_status", json!({}));
        assert_eq!(v["status"], "idle");
        assert_eq!(v["check_on_startup"], true);
        assert_eq!(v["show_dialog"], true);
        assert_eq!(v["skipped_version"], "");
        assert_eq!(v["can_install"], false);
        assert_eq!(v["current_version"], hp_update::current_version().to_string());
        assert!(v["latest"].is_null());
        available(&mut s, true);
        s.settings.update.skipped_version = "v1.0.3".into();
        let v = ok(&mut s, "get_update_status", json!({}));
        assert_eq!(v["status"], "available");
        assert_eq!(v["latest"]["version"], "1.0.3");
        assert_eq!(v["latest"]["notes"], "- auto update");
        assert_eq!(v["latest"]["skipped"], true);
        assert_eq!(v["latest"]["archive"], "HammerPro-1.0.3-windows.zip");
        assert_eq!(v["latest"]["archive_sha256"], "ab".repeat(32));
        assert_eq!(v["can_install"], hp_app::runtime::update::PREBUILT_PLATFORM);
        assert_eq!(v["self_install"], hp_app::runtime::update::PREBUILT_PLATFORM);
        assert_eq!(v["source_update"].is_null(), hp_app::runtime::update::PREBUILT_PLATFORM);
        s.runtime.update.status = UpdateStatus::Failed;
        s.runtime.update.failure = Some(UpdateFailure { key: "update.err_http", detail: "404".into() });
        let v = ok(&mut s, "get_update_status", json!({}));
        assert!(v["error"].as_str().unwrap().contains("404"));
    }

    #[test]
    fn checking_only_requests_a_check() {
        let _lang = hp_app::testing::lang_lock();
        let mut s = hp_app::testing::test_state();
        let v = ok(&mut s, "check_for_updates", json!({}));
        assert_eq!(v["started"], true);
        assert_eq!(s.runtime.update.check_requested, Some(true));
        s.runtime.update.check_requested = None;
        s.runtime.update.status = UpdateStatus::Checking;
        assert_eq!(ok(&mut s, "check_for_updates", json!({}))["started"], false);
        assert_eq!(s.runtime.update.check_requested, None);
        assert!(!s.runtime.update.install_requested);
    }

    #[test]
    fn applying_is_guarded() {
        let _lang = hp_app::testing::lang_lock();
        let mut s = hp_app::testing::test_state();
        let e = err(&mut s, "apply_update", json!({}));
        assert!(e.contains("confirm"), "{e}");
        let e = err(&mut s, "apply_update", json!({ "confirm": false }));
        assert!(e.contains("confirm must be true"), "{e}");
        let e = err(&mut s, "apply_update", json!({ "confirm": true }));
        if !hp_app::runtime::update::PREBUILT_PLATFORM {
            assert!(e.contains("git pull") && e.contains("Windows"), "{e}");
            assert!(!s.runtime.update.install_requested);
            return;
        }
        assert!(e.contains("no update is available"), "{e}");
        assert!(!s.runtime.update.install_requested);
        available(&mut s, false);
        let e = err(&mut s, "apply_update", json!({ "confirm": true }));
        assert!(e.contains("Windows archive"), "{e}");
        available(&mut s, true);
        s.runtime.compile.progress =
            Some(hp_app::runtime::CompileProgress { step: "bsp".into(), fraction: 0.5, elapsed_s: 1.0 });
        let e = err(&mut s, "apply_update", json!({ "confirm": true }));
        assert!(e.contains("compile"), "{e}");
        assert!(!s.runtime.update.install_requested);
        s.runtime.compile.progress = None;
        let v = ok(&mut s, "apply_update", json!({ "confirm": true }));
        assert_eq!(v["started"], true);
        assert_eq!(v["version"], "1.0.3");
        assert!(s.runtime.update.install_requested);
    }

    #[test]
    fn source_builds_get_clone_and_build_instructions() {
        hp_app::register_translations();
        let _lang = hp_app::testing::lang_lock();
        let mut s = hp_app::testing::test_state();
        available(&mut s, true);
        assert!(s.runtime.update.can_install_archive());
        let text = tf!("update.err_source_only", "Linux");
        assert!(text.contains("Windows") && text.contains("git pull") && text.contains("cargo build"), "{text}");
        let body = tf!("update.source_body", "macOS");
        assert!(body.contains("macOS") && body.contains("Windows only"), "{body}");
        assert!(hp_app::runtime::update::SOURCE_UPDATE_COMMANDS.starts_with("git pull"));
        if !hp_app::runtime::update::PREBUILT_PLATFORM {
            let v = ok(&mut s, "get_update_status", json!({}));
            assert_eq!(v["can_install"], false);
            assert!(v["source_update"]["commands"][0] == "git pull");
        }
    }
}
