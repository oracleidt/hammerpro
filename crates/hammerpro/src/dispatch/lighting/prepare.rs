use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(serde::Serialize, serde::Deserialize)]
struct Manifest {
    input: PathBuf,
    bsp: PathBuf,
    result: PathBuf,
    root: Option<PathBuf>,
    game_dir: Option<PathBuf>,
    mounts: Vec<hp_fs::SourceMount>,
    threads: usize,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Receipt {
    leaked: bool,
    warnings: Vec<String>,
}

pub(super) struct Output {
    pub bsp: Vec<u8>,
    pub leaked: bool,
    pub warnings: Vec<String>,
    pub peak: Option<super::memory::Usage>,
}

pub(super) enum Error {
    Cancelled,
    Failed(String),
}

struct Files(Vec<PathBuf>);

struct Process(std::process::Child);

impl std::ops::Deref for Process {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Process {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub(super) fn compile(
    map: hp_vmf::Map,
    fs: &hp_fs::GameFs,
    exe: &Path,
    threads: usize,
    job: u64,
    cancelled: impl Fn() -> bool,
    mut progress: impl FnMut(),
) -> Result<Output, Error> {
    let dir = std::env::temp_dir();
    let stem = format!("hammerpro-prepare-{}-{job}", std::process::id());
    let input = dir.join(format!("{stem}.vmf"));
    let bsp = dir.join(format!("{stem}.bsp"));
    let result = dir.join(format!("{stem}.result"));
    let request = dir.join(format!("{stem}.json"));
    let files = Files(vec![input.clone(), bsp.clone(), result.clone(), request.clone()]);
    let fail = |error: std::io::Error| Error::Failed(error.to_string());
    std::fs::write(&input, hp_vmf::write(&map)).map_err(fail)?;
    drop(map);
    let manifest = Manifest {
        input,
        bsp: bsp.clone(),
        result: result.clone(),
        root: fs.root().map(Path::to_path_buf),
        game_dir: fs.game_dir().map(Path::to_path_buf),
        mounts: fs.sources().iter().map(hp_fs::SourceInfo::mount).collect(),
        threads,
    };
    let file = std::fs::File::create(&request).map_err(fail)?;
    serde_json::to_writer(file, &manifest).map_err(|error| Error::Failed(error.to_string()))?;
    if cancelled() {
        return Err(Error::Cancelled);
    }
    let mut command = Command::new(exe);
    command
        .args([super::child::LIGHT_ARG, "prepare"])
        .arg(&request)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::{BELOW_NORMAL_PRIORITY_CLASS, CREATE_NO_WINDOW};
        command.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    }
    let mut child = Process(command.spawn().map_err(fail)?);
    let _input = child.stdin.take();
    let (tx, lines) = crossbeam_channel::unbounded();
    if let Some(stdout) = child.stdout.take() {
        let _ = std::thread::Builder::new().name("hp-prepare-out".into()).spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    let mut last = Vec::new();
    let mut report = Instant::now();
    let status = loop {
        if cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Cancelled);
        }
        if let Ok(line) = lines.recv_timeout(Duration::from_millis(50)) {
            log::debug!("preview geometry: {line}");
            last.push(line);
            if last.len() > 8 {
                last.remove(0);
            }
        }
        if report.elapsed() >= Duration::from_millis(200) {
            progress();
            report = Instant::now();
        }
        if let Some(status) = child.try_wait().map_err(fail)? {
            break status;
        }
    };
    if !status.success() {
        return Err(Error::Failed(
            last.into_iter()
                .rev()
                .find(|line| line.starts_with("Error:"))
                .unwrap_or_else(|| format!("the preview geometry process stopped ({status})")),
        ));
    }
    let peak = super::memory::of_child(&child);
    let receipt: Receipt = serde_json::from_reader(std::fs::File::open(&result).map_err(fail)?)
        .map_err(|error| Error::Failed(error.to_string()))?;
    let bytes = std::fs::read(bsp).map_err(fail)?;
    drop(files);
    Ok(Output { bsp: bytes, leaked: receipt.leaked, warnings: receipt.warnings, peak })
}

pub(super) fn run(args: &[String]) -> i32 {
    let result = (|| -> Result<(), String> {
        let [request] = args else { return Err("a geometry preparation request is required".into()) };
        super::memory::lower_process_memory_priority();
        super::child::watch_parent();
        let file = std::fs::File::open(request).map_err(|error| error.to_string())?;
        let manifest: Manifest = serde_json::from_reader(file).map_err(|error| error.to_string())?;
        let fs = hp_fs::GameFs::from_source_mounts(manifest.root, manifest.game_dir, &manifest.mounts)
            .map_err(|error| error.to_string())?;
        let map = hp_vmf::load(&manifest.input).map_err(|error| error.to_string())?;
        let compiled = super::bake::compile_for_child(map, &fs, manifest.threads)?;
        std::fs::write(manifest.bsp, compiled.bsp).map_err(|error| error.to_string())?;
        let receipt = Receipt { leaked: compiled.leaked, warnings: compiled.warnings };
        let file = std::fs::File::create(manifest.result).map_err(|error| error.to_string())?;
        serde_json::to_writer(file, &receipt).map_err(|error| error.to_string())?;
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(error) => {
            let _ = writeln!(std::io::stdout().lock(), "Error: {error}");
            1
        }
    }
}
