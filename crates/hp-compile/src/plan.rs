use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::flags;
use crate::launch::{DEFAULT_GAME_ARGS, DEFAULT_STEAM_URL, LaunchMethod, has_flag, launch_step};
use crate::notice::Notice;
use crate::tools::{BSPC_ARG, RAD_ARG, ToolPaths, strip_trailing_separators};
use crate::{CompileError, MapNameProblem, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StepKind {
    Bsp,
    Vis,
    Rad,
    Pack,
    CopyToMaps,
    LaunchGame,
    Custom,
}

#[derive(Clone, Default)]
pub struct DeferredFiles(Arc<Mutex<Option<DeferredReceiver>>>);

type DeferredReceiver = crossbeam_channel::Receiver<Vec<(String, PathBuf)>>;

#[derive(Debug)]
pub struct DeferredSender(crossbeam_channel::Sender<Vec<(String, PathBuf)>>);

impl DeferredFiles {
    pub fn new() -> (DeferredFiles, DeferredSender) {
        let (tx, rx) = crossbeam_channel::bounded(1);
        (DeferredFiles(Arc::new(Mutex::new(Some(rx)))), DeferredSender(tx))
    }

    pub(crate) fn wait(&self, timeout: std::time::Duration) -> std::result::Result<Option<Vec<(String, PathBuf)>>, ()> {
        let Ok(mut slot) = self.0.lock() else { return Err(()) };
        let Some(rx) = slot.as_ref() else { return Err(()) };
        match rx.recv_timeout(timeout) {
            Ok(files) => {
                *slot = None;
                Ok(Some(files))
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => Ok(None),
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                *slot = None;
                Err(())
            }
        }
    }
}

impl DeferredSender {
    pub fn send(self, files: Vec<(String, PathBuf)>) {
        let _ = self.0.send(files);
    }
}

impl std::fmt::Debug for DeferredFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeferredFiles")
    }
}

impl PartialEq for DeferredFiles {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Serialize for DeferredFiles {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_unit()
    }
}

impl<'de> Deserialize<'de> for DeferredFiles {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        <()>::deserialize(d)?;
        Ok(DeferredFiles(Arc::new(Mutex::new(None))))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StepAction {
    Run,
    Pack { files: Vec<(String, PathBuf)> },
    PackDeferred { files: DeferredFiles },
    Copy { from: PathBuf, to: PathBuf },
    Spawn,
    Delete { path: PathBuf },
    Rename { from: PathBuf, to: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub kind: StepKind,
    pub enabled: bool,
    pub exe: PathBuf,
    pub args: Vec<String>,
    pub continue_on_error: bool,
    pub action: StepAction,
    #[serde(default)]
    pub workdir: Option<PathBuf>,
}

impl Step {
    pub fn new(kind: StepKind, exe: impl Into<PathBuf>, args: Vec<String>) -> Step {
        let action = if kind == StepKind::LaunchGame { StepAction::Spawn } else { StepAction::Run };
        Step { kind, enabled: true, exe: exe.into(), args, continue_on_error: false, action, workdir: None }
    }

    pub fn command_line(&self) -> String {
        command_line_string(self)
    }

    pub fn copy(kind: StepKind, from: PathBuf, to: PathBuf) -> Step {
        Step {
            kind,
            enabled: true,
            exe: PathBuf::new(),
            args: Vec::new(),
            continue_on_error: false,
            action: StepAction::Copy { from, to },
            workdir: None,
        }
    }

    pub fn pack(bsp: &Path, action: StepAction) -> Step {
        Step {
            kind: StepKind::Pack,
            enabled: true,
            exe: PathBuf::new(),
            args: vec![bsp.to_string_lossy().into_owned()],
            continue_on_error: false,
            action,
            workdir: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum BspMode {
    Off,
    #[default]
    Full,
    OnlyEnts,
    OnlyProps,
    EntsAndProps,
}

impl BspMode {
    pub fn is_partial(self) -> bool {
        matches!(self, BspMode::OnlyEnts | BspMode::OnlyProps | BspMode::EntsAndProps)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum VisMode {
    Off,
    Fast,
    #[default]
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum RadMode {
    Off,
    Draft,
    #[default]
    Normal,
    Final,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StepModes {
    pub bsp: BspMode,
    pub vis: VisMode,
    pub rad: RadMode,
}

impl StepModes {
    pub const fn new(bsp: BspMode, vis: VisMode, rad: RadMode) -> Self {
        StepModes { bsp, vis, rad }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Preset {
    #[serde(alias = "Fast")]
    Draft,
    #[default]
    Normal,
    Final,
    Custom,
}

impl Preset {
    pub const ALL: [Preset; 4] = [Preset::Draft, Preset::Normal, Preset::Final, Preset::Custom];

    pub fn modes(self) -> StepModes {
        match self {
            Preset::Draft => StepModes::new(BspMode::Full, VisMode::Fast, RadMode::Draft),
            Preset::Normal | Preset::Custom => StepModes::new(BspMode::Full, VisMode::Full, RadMode::Normal),
            Preset::Final => StepModes::new(BspMode::Full, VisMode::Full, RadMode::Final),
        }
    }

    pub fn of_modes(modes: StepModes) -> Preset {
        [Preset::Draft, Preset::Normal, Preset::Final]
            .into_iter()
            .find(|p| p.modes() == modes)
            .unwrap_or(Preset::Custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Lighting {
    #[default]
    Ldr,
    Hdr,
    Both,
}

impl Lighting {
    pub fn flag(self) -> &'static str {
        match self {
            Lighting::Ldr => "-ldr",
            Lighting::Hdr => "-hdr",
            Lighting::Both => "-both",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanOptions {
    pub lighting: Lighting,
    pub threads: Option<u32>,
    pub extra_bsp_args: String,
    pub extra_rad_args: String,
    pub pack_files: Vec<(String, PathBuf)>,
    #[serde(skip)]
    pub deferred_pack: Option<DeferredFiles>,
    pub copy_to_maps: bool,
    pub launch_game: bool,
    pub launch_method: LaunchMethod,
    pub game_args: String,
    pub steam_url: String,
    pub compile_game_dir: Option<PathBuf>,
    pub content_args: Vec<String>,
    pub modes: Option<StepModes>,
    pub stop_on_leak: bool,
    pub bsp_flags: Vec<flags::FlagChoice>,
    pub rad_flags: Vec<flags::FlagChoice>,
    pub static_prop_lighting: bool,
    pub static_prop_shadows: bool,
    pub target: Option<String>,
}

impl Default for PlanOptions {
    fn default() -> Self {
        PlanOptions {
            lighting: Lighting::default(),
            threads: None,
            extra_bsp_args: String::new(),
            extra_rad_args: String::new(),
            pack_files: Vec::new(),
            deferred_pack: None,
            copy_to_maps: true,
            launch_game: false,
            launch_method: LaunchMethod::default(),
            game_args: DEFAULT_GAME_ARGS.to_owned(),
            steam_url: DEFAULT_STEAM_URL.to_owned(),
            compile_game_dir: None,
            content_args: Vec::new(),
            modes: None,
            stop_on_leak: false,
            bsp_flags: Vec::new(),
            rad_flags: Vec::new(),
            static_prop_lighting: true,
            static_prop_shadows: true,
            target: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanNotice {
    pub step: usize,
    pub notice: Notice,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompilePlan {
    pub vmf_path: PathBuf,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub notices: Vec<PlanNotice>,
    #[serde(default)]
    pub stop_on_leak: bool,
    #[serde(default)]
    pub modes: StepModes,
    #[serde(default)]
    pub cache: Option<crate::cache::PlanCache>,
}

impl CompilePlan {
    pub fn new(vmf_path: impl Into<PathBuf>) -> CompilePlan {
        CompilePlan {
            vmf_path: vmf_path.into(),
            steps: Vec::new(),
            notices: Vec::new(),
            stop_on_leak: false,
            modes: StepModes::default(),
            cache: None,
        }
    }

    pub fn preset(preset: Preset, tools: &ToolPaths, vmf_path: &Path, opts: &PlanOptions) -> Result<CompilePlan> {
        let name = validated_map_name(vmf_path)?;
        let vmf_buf = absolute(vmf_path);
        let vmf_path = vmf_buf.as_path();
        let map_arg = vmf_path.with_extension("").to_string_lossy().into_owned();
        let game_dir = strip_trailing_separators(&absolute(&tools.game_dir));
        let compile_game_dir = match &opts.compile_game_dir {
            Some(dir) if !dir.as_os_str().is_empty() => {
                strip_trailing_separators(&absolute(dir)).to_string_lossy().into_owned()
            }
            _ => game_dir.to_string_lossy().into_owned(),
        };
        let bsp = vmf_path.with_extension("bsp");
        let mut notices = Vec::new();

        let mut modes = opts.modes.unwrap_or_else(|| preset.modes());
        if modes.bsp.is_partial() && !bsp.is_file() {
            notices.push((StepKind::Bsp, Notice::NoPreviousBsp { bsp: bsp.clone() }));
            let base = preset.modes();
            modes = StepModes {
                bsp: BspMode::Full,
                vis: if base.vis == VisMode::Off { VisMode::Fast } else { base.vis },
                rad: if base.rad == RadMode::Off { RadMode::Draft } else { base.rad },
            };
        }
        if modes.bsp != BspMode::Full {
            modes.vis = VisMode::Off;
        }

        let threads: Vec<String> =
            opts.threads.map(|t| vec!["-threads".to_owned(), t.max(1).to_string()]).unwrap_or_default();
        let target = opts.target.as_deref().map(str::trim).filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case("gmod"));
        let mut steps = Vec::with_capacity(5);
        if modes.bsp != BspMode::Off {
            let mut args = vec![BSPC_ARG.to_owned()];
            if let Some(root) = game_dir.parent().filter(|p| !p.as_os_str().is_empty() && target.is_none()) {
                args.push("-gmod".to_owned());
                args.push(root.to_string_lossy().into_owned());
            }
            let (flags, extra) = flags::merge_flags(
                StepKind::Bsp,
                &flags::bsp_flags(modes.bsp, modes.vis, opts.stop_on_leak),
                &opts.bsp_flags,
                &split_args(&opts.extra_bsp_args),
            );
            args.extend(flags);
            args.extend(threads.iter().cloned());
            args.extend(extra);
            push_content_args(&mut args, &opts.content_args);
            if let Some(t) = target {
                args.push("-target".to_owned());
                args.push(t.to_owned());
            }
            args.push("-game".to_owned());
            args.push(compile_game_dir.clone());
            args.push(map_arg.clone());
            steps.push(Step::new(StepKind::Bsp, &tools.compiler, args));
        }
        if modes.rad != RadMode::Off {
            let mut args = vec![RAD_ARG.to_owned(), flags::rad_lighting(modes.rad, opts.lighting).flag().to_owned()];
            let (flags, extra) = rad_step_flags(modes.rad, opts);
            args.extend(flags);
            args.extend(threads);
            args.extend(extra);
            push_content_args(&mut args, &opts.content_args);
            if let Some(t) = target {
                args.push("-target".to_owned());
                args.push(t.to_owned());
            }
            if let Some(root) = game_dir.parent().filter(|p| !p.as_os_str().is_empty() && target.is_none()) {
                args.push("-gmod".to_owned());
                args.push(root.to_string_lossy().into_owned());
            }
            args.push("-game".to_owned());
            args.push(compile_game_dir.clone());
            args.push(map_arg);
            steps.push(Step::new(StepKind::Rad, &tools.compiler, args));
        }

        let pack_action = match &opts.deferred_pack {
            Some(files) => Some(StepAction::PackDeferred { files: files.clone() }),
            None if !opts.pack_files.is_empty() => Some(StepAction::Pack {
                files: opts.pack_files.iter().map(|(inner, source)| (inner.clone(), absolute(source))).collect(),
            }),
            None => None,
        };
        if let Some(action) = pack_action {
            steps.push(Step::pack(&bsp, action));
        }

        if opts.copy_to_maps {
            steps.push(Step::copy(
                StepKind::CopyToMaps,
                bsp.clone(),
                absolute(&tools.maps_dir).join(format!("{name}.bsp")),
            ));
        }

        if opts.launch_game {
            let mut step = launch_step(tools, &name, opts);
            let exe_launch = opts.launch_method == LaunchMethod::Exe && tools.game_exe.is_some();
            if modes.bsp.is_partial() && exe_launch && !has_flag(&step.args, crate::launch::STALE_ZIP_FLAG) {
                let at = step.args.iter().position(|a| a.starts_with('+')).unwrap_or(step.args.len());
                step.args.insert(at, crate::launch::STALE_ZIP_FLAG.to_owned());
            }
            steps.push(step);
        }

        let notices = notices
            .into_iter()
            .map(|(kind, notice)| PlanNotice { step: steps.iter().position(|s| s.kind == kind).unwrap_or(0), notice })
            .collect();
        Ok(CompilePlan {
            vmf_path: vmf_path.to_path_buf(),
            steps,
            notices,
            stop_on_leak: opts.stop_on_leak,
            modes,
            cache: None,
        })
    }

    pub fn map_name(&self) -> Option<&str> {
        self.vmf_path.file_stem().and_then(|s| s.to_str())
    }

    pub fn bsp_path(&self) -> PathBuf {
        self.vmf_path.with_extension("bsp")
    }

    pub fn lin_path(&self) -> PathBuf {
        self.vmf_path.with_extension("lin")
    }

    pub fn prt_path(&self) -> PathBuf {
        self.vmf_path.with_extension("prt")
    }

    pub fn enabled_steps(&self) -> impl Iterator<Item = (usize, &Step)> {
        self.steps.iter().enumerate().filter(|(_, s)| s.enabled)
    }

    pub fn step_mut(&mut self, kind: StepKind) -> Option<&mut Step> {
        self.steps.iter_mut().find(|s| s.kind == kind)
    }

    pub fn is_partial(&self) -> bool {
        self.modes.bsp.is_partial()
    }

    pub fn with_cache(mut self, input: u64, dir: PathBuf, map_key: &str, max_bytes: u64) -> CompilePlan {
        if self.is_partial() || self.modes.bsp != BspMode::Full {
            return self;
        }
        let map_arg = self.vmf_path.with_extension("").to_string_lossy().into_owned();
        let mut key = input;
        let mut steps = Vec::new();
        for (i, step) in self.steps.iter().enumerate() {
            let stage = match step.kind {
                StepKind::Bsp => "bsp",
                StepKind::Rad => "rad",
                _ => continue,
            };
            let game = step.args.windows(2).find(|w| w[0] == "-game").map(|w| w[1].clone()).unwrap_or_default();
            key = crate::cache::stage_key(key, &step.exe, &crate::cache::key_args(&step.args, &map_arg, &game));
            steps.push((i, stage.to_owned(), key));
        }
        let map_key: String =
            map_key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
        self.cache = Some(crate::cache::PlanCache { dir, map_key, steps, max_bytes });
        self
    }
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn validated_map_name(vmf_path: &Path) -> Result<String> {
    let invalid = || CompileError::InvalidMapPath(vmf_path.to_path_buf());
    vmf_path.parent().ok_or_else(invalid)?;
    let name = vmf_path.file_stem().and_then(|s| s.to_str()).filter(|s| !s.is_empty()).ok_or_else(invalid)?;
    let reject = |problem| CompileError::InvalidMapName { name: name.to_owned(), problem };
    if name.contains('.') {
        return Err(reject(MapNameProblem::Dot));
    }
    if name.chars().any(char::is_whitespace) {
        return Err(reject(MapNameProblem::Whitespace));
    }
    Ok(name.to_owned())
}

pub fn rad_step_flags(mode: RadMode, opts: &PlanOptions) -> (Vec<String>, Vec<String>) {
    let mut choices = opts.rad_flags.clone();
    if !opts.static_prop_lighting {
        choices.push(flags::FlagChoice {
            flag: flags::STATIC_PROP_LIGHTING.to_owned(),
            on: false,
            value: String::new(),
        });
    }
    let (mut flags, mut extra) =
        flags::merge_flags(StepKind::Rad, &flags::rad_flags(mode), &choices, &split_args(&opts.extra_rad_args));
    if !opts.static_prop_shadows {
        extra.retain(|a| !a.eq_ignore_ascii_case(flags::NO_STATIC_PROP_SHADOWS));
        flags.push(flags::NO_STATIC_PROP_SHADOWS.to_owned());
    }
    (flags, extra)
}

fn push_content_args(args: &mut Vec<String>, content: &[String]) {
    let mut i = 0;
    while i < content.len() {
        let flag = &content[i];
        let with_value = flag.eq_ignore_ascii_case("-insert_search_path");
        let part = &content[i..(i + 1 + usize::from(with_value)).min(content.len())];
        i += part.len();
        let has = if with_value {
            args.windows(part.len()).any(|w| w[0].eq_ignore_ascii_case(flag) && w[1..] == part[1..])
        } else {
            args.iter().any(|a| a.eq_ignore_ascii_case(flag))
        };
        if !has {
            args.extend(part.iter().cloned());
        }
    }
}

pub fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut in_arg = false;
    let mut quoted = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
                in_arg = true;
            }
            '"' => {
                quoted = !quoted;
                in_arg = true;
            }
            c if c.is_whitespace() && !quoted => {
                if in_arg {
                    args.push(std::mem::take(&mut cur));
                    in_arg = false;
                }
            }
            c => {
                cur.push(c);
                in_arg = true;
            }
        }
    }
    if in_arg {
        args.push(cur);
    }
    args
}

pub fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.chars().any(|c| c.is_whitespace() || c == '"') {
        return arg.to_owned();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    let mut backslashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
                continue;
            }
            _ => {}
        }
        if c != '\\' {
            out.extend(std::iter::repeat_n('\\', backslashes));
            backslashes = 0;
            out.push(c);
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    out
}

pub fn command_line_string(step: &Step) -> String {
    match &step.action {
        StepAction::Pack { .. } | StepAction::PackDeferred { .. } if step.exe.as_os_str().is_empty() => {
            let mut out = "pack".to_owned();
            for arg in &step.args {
                out.push(' ');
                out.push_str(&quote_arg(arg));
            }
            return out;
        }
        StepAction::Copy { from, to } => {
            return format!("copy {} {}", quote_arg(&from.to_string_lossy()), quote_arg(&to.to_string_lossy()));
        }
        StepAction::Delete { path } => return format!("del {}", quote_arg(&path.to_string_lossy())),
        StepAction::Rename { from, to } => {
            return format!("move {} {}", quote_arg(&from.to_string_lossy()), quote_arg(&to.to_string_lossy()));
        }
        _ => {}
    }
    let mut out = quote_arg(&step.exe.to_string_lossy());
    for arg in &step.args {
        out.push(' ');
        out.push_str(&quote_arg(arg));
    }
    out
}

#[cfg(test)]
mod tests;
