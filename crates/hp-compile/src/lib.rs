pub mod cache;
pub mod cmdseq;
mod cubemaps;
mod decode;
pub mod flags;
pub mod history;
pub mod incremental;
pub mod knowledge;
pub mod launch;
mod log;
mod notice;
mod pack;
mod plan;
mod pointfile;
pub mod power;
mod progress;
mod runner;
pub mod sandbox;
pub mod stable_hash;
pub mod stats;
pub mod template;
pub mod valve;
mod tools;

#[cfg(windows)]
mod job;

#[cfg(test)]
mod test_util;

pub use flags::FlagChoice;
pub use launch::{DEFAULT_GAME_ARGS, DEFAULT_STEAM_URL, LaunchMethod, game_args_for, launch_step};
pub use log::{LogLocation, Severity, describe_exit_code, is_leak_line, parse_log_line};
pub use notice::{CrashKind, Notice, crash_kind, format_bytes, format_exit_code};
pub use plan::{
    BspMode, CompilePlan, DeferredFiles, DeferredSender, Lighting, PlanNotice, PlanOptions, Preset, RadMode, Step,
    StepAction, StepKind, StepModes, VisMode, command_line_string, quote_arg, rad_step_flags, split_args,
    validated_map_name,
};
pub use pointfile::{find_pointfile, parse_pointfile, read_pointfile};
pub use runner::{CompileEvent, CompileHandle, start, start_with_waker};
pub use tools::{BSPC_ARG, RAD_ARG, ToolPaths, default_threads, pack_files_from_dir};

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}:{line}: {message}")]
    Pointfile { path: PathBuf, line: usize, message: String },
    #[error("invalid map path: {0}")]
    InvalidMapPath(PathBuf),
    #[error("invalid map name {name:?}: {problem}")]
    InvalidMapName { name: String, problem: MapNameProblem },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MapNameProblem {
    Dot,
    Whitespace,
}

impl std::fmt::Display for MapNameProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            MapNameProblem::Dot => "map names cannot contain '.' (the compiled file names would be cut at the dot)",
            MapNameProblem::Whitespace => "map names cannot contain spaces (the game cannot load them)",
        })
    }
}

pub type Result<T> = std::result::Result<T, CompileError>;
