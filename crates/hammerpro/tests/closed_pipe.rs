use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn exe() -> &'static str {
    env!("CARGO_BIN_EXE_hammerpro")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("closed_pipe_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tiny_map(dir: &Path) -> PathBuf {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../hp-bspc/tests/fixtures/golden_brush_entity.vmf");
    let map = dir.join("tiny.vmf");
    std::fs::copy(fixture, &map).unwrap();
    map
}

fn run_open(args: &[&str]) -> Output {
    Command::new(exe()).args(args).stdin(Stdio::null()).output().unwrap()
}

fn run_closed(args: &[&str]) -> Output {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let out = Command::new(exe())
        .args(args)
        .stdin(Stdio::null())
        .stdout(writer.try_clone().unwrap())
        .stderr(writer)
        .output()
        .unwrap();
    assert!(out.status.code().is_some(), "{args:?} ended without an exit code: {:?}", out.status);
    out
}

fn same_ending(args: &[&str]) {
    let open = run_open(args);
    let text = String::from_utf8_lossy(&open.stdout).into_owned() + &String::from_utf8_lossy(&open.stderr);
    assert!(!text.is_empty(), "{args:?} printed nothing, the test would prove nothing");
    assert!(!text.contains("panicked"), "{args:?} panicked with open pipes:\n{text}");
    let closed = run_closed(args);
    assert_ne!(closed.status.code(), Some(101), "{args:?} panicked on a closed pipe");
    assert_eq!(closed.status.code(), open.status.code(), "{args:?} ended differently on a closed pipe");
}

#[test]
fn cli_paths_survive_closed_stdout_and_stderr() {
    same_ending(&["--bspc", "-no-such-option-at-all"]);
    same_ending(&["--rad", "-no-such-option-at-all"]);
    same_ending(&["--light-preview"]);
    let a = scratch("bspc_a");
    let b = scratch("bspc_b");
    let map_a = tiny_map(&a);
    let map_b = tiny_map(&b);
    let open = run_open(&["--bspc", map_a.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&open.stdout).into_owned();
    assert!(text.lines().count() > 3, "the compile printed too little:\n{text}");
    let closed = run_closed(&["--bspc", map_b.to_str().unwrap()]);
    assert_eq!(closed.status.code(), open.status.code(), "the compile ended differently on a closed pipe");
    assert_eq!(a.join("tiny.bsp").is_file(), b.join("tiny.bsp").is_file(), "the compile did its work");
    let open = run_open(&["--rad", map_a.to_str().unwrap()]);
    let closed = run_closed(&["--rad", map_b.to_str().unwrap()]);
    assert_eq!(closed.status.code(), open.status.code(), "the lighting ended differently on a closed pipe");
    let _ = std::fs::remove_dir_all(&a);
    let _ = std::fs::remove_dir_all(&b);
}
