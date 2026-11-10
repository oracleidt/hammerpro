use std::path::{Path, PathBuf};

use hp_app::AppState;
use hp_app::settings::Settings;
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, R};

const MAX_REPORT_BYTES: u64 = 4 * 1024 * 1024;
const DEFAULT_LINES: u64 = 400;

domain! { "diagnostics";
    get_crash_reports: Read, "Crash reports",
        "The crash reports Hammer Pro wrote (panics, worker thread failures, a previous run that ended \
        without closing, a lost graphics device), newest first: file name, kind, size, time and the message \
        line. Give report (a file name from the list) to read one report: its text with the backtrace and \
        the last log lines of that run (lines limits the size). Only the files of the crash folder are read.",
        {
            "report": string("File name of one report from the list, e.g. crash-1731234567890.txt."),
            "lines": integer("Most lines of the report returned (default 400).", 1, 5000),
        }, [];
    get_app_log: Read, "Hammer Pro log",
        "The last lines of Hammer Pro's own log of this session (warnings, errors and notes of the editor, \
        not the compile log: use get_compile_log for that), oldest first. Filter with contains (case \
        insensitive) and limit the count with lines. The log file itself is in the logs folder shown.",
        {
            "contains": string("Only the lines containing this text (case insensitive)."),
            "lines": integer("Most lines returned, the newest kept (default 200).", 1, 200),
        }, [];
}

fn valid_report_name(name: &str) -> bool {
    let Some(stem) = name.strip_prefix("crash-").and_then(|n| n.strip_suffix(".txt")) else { return false };
    !stem.is_empty() && stem.len() <= 64 && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn kind_of(name: &str) -> &'static str {
    if name.contains("-worker") {
        "worker_thread"
    } else if name.contains("-unexpected") {
        "unexpected_end"
    } else {
        "crash"
    }
}

fn message_line(path: &Path) -> Option<String> {
    use std::io::{BufRead, BufReader};
    let file = std::fs::File::open(path).ok()?;
    BufReader::new(file)
        .lines()
        .take(40)
        .map_while(Result::ok)
        .find_map(|l| l.strip_prefix("message: ").map(|m| m.chars().take(300).collect()))
}

pub(super) fn list_reports(dir: &Path) -> Vec<(String, PathBuf, std::fs::Metadata)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let meta = e.metadata().ok().filter(std::fs::Metadata::is_file)?;
            valid_report_name(&name).then(|| (name, e.path(), meta))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out
}

fn crash_reports_in(dir: &Path, a: &Args<'_>) -> R<ToolResult> {
    let lines = a.opt_u64("lines", 1, 5000)?.unwrap_or(DEFAULT_LINES) as usize;
    if let Some(report) = a.opt_str("report", 128)? {
        let name = report.trim();
        if !valid_report_name(name) {
            return Err(format!("\"{name}\" is not a crash report name (crash-<number>.txt, from the list)"));
        }
        let path = dir.join(name);
        let meta = std::fs::metadata(&path).map_err(|_| format!("{name}: no such report"))?;
        if !meta.is_file() || meta.len() > MAX_REPORT_BYTES {
            return Err(format!("{name}: not a file, or larger than 4 MB"));
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
        let all: Vec<&str> = text.lines().collect();
        let shown = all.len().min(lines);
        return Ok(ToolResult::json(json!({
            "report": name,
            "kind": kind_of(name),
            "path": path.display().to_string(),
            "lines": all.len(),
            "truncated": shown < all.len(),
            "text": all[..shown].join("\n"),
        })));
    }
    let reports: Vec<Value> = list_reports(dir)
        .into_iter()
        .map(|(name, path, meta)| {
            let modified =
                meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs());
            json!({
                "report": name,
                "kind": kind_of(&name),
                "bytes": meta.len(),
                "modified_unix": modified,
                "message": message_line(&path),
            })
        })
        .collect();
    let last = crate::crash::last_report().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
    Ok(ToolResult::json(json!({
        "folder": dir.display().to_string(),
        "total": reports.len(),
        "reports": reports,
        "this_session": last,
    })))
}

fn get_crash_reports(_state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    crash_reports_in(&Settings::crash_dir(), a)
}

fn get_app_log(_state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let lines = a.opt_u64("lines", 1, 200)?.unwrap_or(200) as usize;
    let needle = a.opt_str("contains", 256)?.map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
    let all = crate::logger::recent_lines();
    let kept: Vec<&String> =
        all.iter().filter(|l| needle.as_ref().is_none_or(|n| l.to_lowercase().contains(n.as_str()))).collect();
    let start = kept.len().saturating_sub(lines);
    Ok(ToolResult::json(json!({
        "lines": kept[start..],
        "matching": kept.len(),
        "returned": kept.len() - start,
        "log_folder": Settings::log_dir().display().to_string(),
    })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::ok;
    use super::*;

    #[test]
    fn report_names_cannot_leave_the_crash_folder() {
        for good in ["crash-1731234567890.txt", "crash-17-worker.txt", "crash-5-unexpected-2.txt"] {
            assert!(valid_report_name(good), "{good}");
        }
        for bad in [
            "../crash-1.txt",
            "crash-1.txt/..",
            "crash-..\\x.txt",
            "C:/crash-1.txt",
            "crash-.txt",
            "pending.txt",
            "crash-1.txt.exe",
            "crash-1 .txt",
        ] {
            assert!(!valid_report_name(bad), "{bad}");
        }
        assert_eq!(kind_of("crash-1-worker.txt"), "worker_thread");
        assert_eq!(kind_of("crash-1-unexpected.txt"), "unexpected_end");
        assert_eq!(kind_of("crash-1.txt"), "crash");
    }

    #[test]
    fn lists_and_reads_reports() {
        let dir = std::env::temp_dir().join(format!("hp_mcp_crash_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("crash-100.txt"), "Hammer Pro crash report\nmessage: boom\n\nbacktrace:\na\nb\n")
            .unwrap();
        std::fs::write(dir.join("crash-200-worker.txt"), "message: worker failed\n").unwrap();
        std::fs::write(dir.join("pending.txt"), "x").unwrap();
        std::fs::write(dir.join("running-5.txt"), "x").unwrap();
        let run = |v: serde_json::Value| {
            let a = super::super::tools::args(&v, &["report", "lines"]).unwrap();
            crash_reports_in(&dir, &a)
        };
        let list = run(json!({})).unwrap();
        let v = list.structured.clone().unwrap();
        assert_eq!(v["total"], 2);
        assert_eq!(v["reports"][0]["report"], "crash-200-worker.txt");
        assert_eq!(v["reports"][0]["kind"], "worker_thread");
        assert_eq!(v["reports"][1]["message"], "boom");
        let one = run(json!({ "report": "crash-100.txt", "lines": 2 })).unwrap().structured.unwrap();
        assert_eq!(one["text"], "Hammer Pro crash report\nmessage: boom");
        assert_eq!(one["truncated"], true);
        assert!(run(json!({ "report": "../settings.toml" })).is_err());
        assert!(run(json!({ "report": "crash-999.txt" })).unwrap_err().contains("no such report"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn app_log_answers() {
        let _lang = hp_app::testing::lang_lock();
        let mut s = hp_app::testing::test_state();
        let v = ok(&mut s, "get_app_log", json!({ "lines": 5, "contains": "zzz-not-in-the-log" }));
        assert_eq!(v["returned"], 0);
        assert!(v["log_folder"].as_str().is_some());
        let v = ok(&mut s, "get_crash_reports", json!({}));
        assert!(v["folder"].as_str().is_some());
    }
}
