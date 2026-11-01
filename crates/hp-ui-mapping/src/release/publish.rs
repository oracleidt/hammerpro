use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use hp_compile::{CompileEvent, Severity as LogSeverity, StepAction, StepKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PublishKind {
    Create,
    Update(u64),
}

pub(crate) fn gmpublish_exe(gmod_root: &Path) -> Option<PathBuf> {
    let bin = gmod_root.join("bin");
    [
        bin.join("win64").join("gmpublish.exe"),
        bin.join("gmpublish.exe"),
        bin.join("gmpublish_linux"),
        bin.join("gmpublish_osx"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

pub(crate) fn args(kind: PublishKind, gma: &Path, icon: &Path, note: &str) -> Vec<String> {
    let path = |p: &Path| p.to_string_lossy().into_owned();
    match kind {
        PublishKind::Create => vec!["create".into(), "-addon".into(), path(gma), "-icon".into(), path(icon)],
        PublishKind::Update(id) => {
            let mut a = vec!["update".into(), "-addon".into(), path(gma), "-id".into(), id.to_string()];
            if icon.is_file() {
                a.push("-icon".into());
                a.push(path(icon));
            }
            let note: String = note.chars().filter(|c| !c.is_control() || *c == '\n').take(8000).collect();
            if !note.trim().is_empty() {
                a.push("-changes".into());
                a.push(note);
            }
            a
        }
    }
}

pub(crate) fn parse_id(text: &str) -> Option<u64> {
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(at) = lower.find("uid:").or_else(|| lower.find("id:")) {
            let digits: String =
                line[at..].chars().skip_while(|c| !c.is_ascii_digit()).take_while(char::is_ascii_digit).collect();
            if let Ok(id) = digits.parse::<u64>()
                && id > 0
            {
                return Some(id);
            }
        }
    }
    None
}

pub(crate) enum PublishEvent {
    Log(LogSeverity, String),
    Done { success: bool, id: Option<u64> },
}

pub(crate) fn run(
    exe: PathBuf,
    args: Vec<String>,
    vmf: PathBuf,
    tx: Sender<PublishEvent>,
    cancel: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    let send = |e: PublishEvent| {
        let _ = tx.send(e);
        wake();
    };
    if hp_compile::sandbox::is_sandboxed() {
        send(PublishEvent::Log(LogSeverity::Error, hp_compile::sandbox::REFUSED.to_owned()));
        send(PublishEvent::Done { success: false, id: None });
        return;
    }
    let mut plan = hp_compile::CompilePlan::new(&vmf);
    let workdir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    plan.steps.push(hp_compile::Step {
        kind: StepKind::Custom,
        enabled: true,
        exe,
        args,
        continue_on_error: false,
        action: StepAction::Run,
        workdir: None,
    });
    let handle = hp_compile::start(plan, &workdir);
    let mut output = String::new();
    let mut success = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            handle.cancel();
        }
        match handle.events.recv_timeout(Duration::from_millis(200)) {
            Ok(CompileEvent::Log { line, severity, .. }) => {
                if output.len() < 1 << 20 {
                    output.push_str(&line);
                    output.push('\n');
                }
                send(PublishEvent::Log(severity, line));
            }
            Ok(CompileEvent::Finished { success: s, .. }) => {
                success = s;
                break;
            }
            Ok(_) => {}
            Err(e) if e.is_timeout() => {}
            Err(_) => break,
        }
    }
    handle.wait();
    send(PublishEvent::Done { success, id: parse_id(&output) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines_and_ids() {
        let a = args(PublishKind::Create, Path::new("a.gma"), Path::new("i.jpg"), "");
        assert_eq!(a, ["create", "-addon", "a.gma", "-icon", "i.jpg"]);
        let u = args(PublishKind::Update(42), Path::new("a.gma"), Path::new("missing.jpg"), "Fixed\u{7}stuff");
        assert_eq!(u, ["update", "-addon", "a.gma", "-id", "42", "-changes", "Fixedstuff"]);
        assert_eq!(parse_id("Uploading...\nDone! UID: 3141592\n"), Some(3_141_592));
        assert_eq!(parse_id("nothing"), None);
        assert!(gmpublish_exe(Path::new("/definitely/not/here")).is_none());
    }
}
