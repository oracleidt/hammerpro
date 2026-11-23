use hp_app::{t, tf};
use hp_compile::knowledge::{Fix, Knowledge};
use hp_compile::{CompileError, CrashKind, MapNameProblem, Notice, format_exit_code};

use crate::util::plural;

pub(crate) fn crash_text(kind: CrashKind) -> &'static str {
    match kind {
        CrashKind::AccessViolation => t!("compile.crash.access_violation"),
        CrashKind::DivisionByZero => t!("compile.crash.division_by_zero"),
        CrashKind::StackOverflow => t!("compile.crash.stack_overflow"),
        CrashKind::MemoryCorruption => t!("compile.crash.memory_corruption"),
        CrashKind::Aborted => t!("compile.crash.aborted"),
        CrashKind::OutOfMemory => t!("compile.crash.out_of_memory"),
        CrashKind::Breakpoint => t!("compile.crash.breakpoint"),
        CrashKind::MissingDll => t!("compile.crash.missing_dll"),
        CrashKind::WrongArchitecture => t!("compile.crash.wrong_architecture"),
        CrashKind::Interrupted => t!("compile.crash.interrupted"),
        CrashKind::IllegalInstruction => t!("compile.crash.illegal_instruction"),
    }
}

pub(crate) fn notice_text(n: &Notice) -> String {
    match n {
        Notice::ThreadFailed { error } => tf!("compile.notice.thread_failed", error),
        Notice::PipelineStopped => t!("compile.notice.pipeline_stopped").to_owned(),
        Notice::Leaked { pointfile } => tf!("compile.notice.leaked", pointfile.display()),
        Notice::LeakedNoPointfile => t!("compile.notice.leaked_no_pointfile").to_owned(),
        Notice::BspNotWritten { bsp } => tf!("compile.notice.bsp_not_written", bsp.display()),
        Notice::PackInvalidPath { path } => tf!("compile.notice.pack_invalid_path", format!("{path:?}")),
        Notice::PackMissingFile { file } => tf!("compile.notice.pack_missing_file", file.display()),
        Notice::NothingToPack => t!("compile.notice.nothing_to_pack").to_owned(),
        Notice::PackFailed { error } => tf!("compile.notice.pack_failed", error),
        Notice::Packing { count } => {
            tf!(plural(*count, "compile.notice.packing_one", "compile.notice.packing"), count)
        }
        Notice::Packed { count, bytes } => tf!(
            plural(*count, "compile.notice.packed_one", "compile.notice.packed"),
            count,
            hp_compile::format_bytes(*bytes)
        ),
        Notice::PackNotInBsp { files } => tf!(
            plural(files.len(), "compile.notice.pack_not_in_bsp_one", "compile.notice.pack_not_in_bsp"),
            files.len(),
            file_list(files)
        ),
        Notice::CopySourceMissing { from } => tf!("compile.notice.copy_source_missing", from.display()),
        Notice::CreateDirFailed { dir, error } => tf!("compile.notice.create_dir_failed", dir.display(), error),
        Notice::CopyFailed { from, to, error } => {
            tf!("compile.notice.copy_failed", from.display(), to.display(), error)
        }
        Notice::ReplaceFailed { to, error } => tf!("compile.notice.replace_failed", to.display(), error),
        Notice::Copied { from, to } => tf!("compile.notice.copied", from.display(), to.display()),
        Notice::CubemapsKept { count, bsp } => {
            tf!(
                plural(*count, "compile.notice.cubemaps_kept_one", "compile.notice.cubemaps_kept"),
                count,
                bsp.display()
            )
        }
        Notice::Started { exe } => tf!("compile.notice.started", exe.display()),
        Notice::StartFailed { exe, error } => tf!("compile.notice.start_failed", exe.display(), error),
        Notice::Cancelled => t!("compile.notice.cancelled").to_owned(),
        Notice::QueryFailed { error } => tf!("compile.notice.query_failed", error),
        Notice::Crashed { program, code, kind } => {
            tf!("compile.notice.crashed", program, format_exit_code(*code), crash_text(*kind))
        }
        Notice::ExitCode { program, code } => tf!("compile.notice.exit_code", program, code),
        Notice::Terminated { program } => tf!("compile.notice.terminated", program),
        Notice::NoPreviousBsp { bsp } => tf!("compile.notice.no_previous_bsp", bsp.display()),
        Notice::StoppedOnLeak => t!("compile.notice.stopped_on_leak").to_owned(),
        Notice::FromCache { from } => tf!("compile.notice.from_cache", from.display()),
        Notice::PackWaiting => t!("compile.notice.pack_waiting").to_owned(),
        Notice::PackListUnavailable => t!("compile.notice.pack_list_unavailable").to_owned(),
        Notice::FileOpFailed { path, error } => tf!("compile.notice.file_op_failed", path.display(), error),
        Notice::StepOutOfMemory { step, bytes } => {
            tf!("compile.notice.step_out_of_memory", super::step_name(*step), hp_compile::format_bytes(*bytes))
        }
        Notice::CrashedDuring { step, lines } => {
            let mut text = tf!("compile.notice.crashed_during", super::step_name(*step));
            for l in lines {
                text.push_str("\n  ");
                text.push_str(l);
            }
            text
        }
    }
}

pub(crate) fn knowledge_text(k: &Knowledge) -> String {
    let key = format!("compile.kb.{}", k.code);
    if hp_app::i18n::has_key(&key) { hp_app::i18n::tr(&key).to_owned() } else { k.english.to_owned() }
}

pub(crate) fn fix_label(f: Fix) -> &'static str {
    match f {
        Fix::GoToLocation => t!("compile.fix.goto"),
        Fix::OpenProblems => t!("compile.fix.problems"),
        Fix::OpenDocs(_) => t!("compile.fix.docs"),
        Fix::LoadLeak => t!("compile.fix.leak"),
    }
}

fn file_list(files: &[String]) -> String {
    const SHOWN: usize = 8;
    let head = files.iter().take(SHOWN).map(String::as_str).collect::<Vec<_>>().join(", ");
    if files.len() > SHOWN { tf!("compile.notice.more_files", head, files.len() - SHOWN) } else { head }
}

pub(crate) fn plan_error_text(e: &CompileError) -> String {
    match e {
        CompileError::InvalidMapName { name, problem: MapNameProblem::Dot } => tf!("compile.err_map_name_dot", name),
        CompileError::InvalidMapName { name, problem: MapNameProblem::Whitespace } => {
            tf!("compile.err_map_name_space", name)
        }
        CompileError::InvalidMapPath(p) => tf!("compile.err_map_path", p.display()),
        other => tf!("compile.err_plan", other),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn every_notice_is_translated() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let p = PathBuf::from("C:/maps/a.bsp");
        let notices = vec![
            Notice::ThreadFailed { error: "e".into() },
            Notice::PipelineStopped,
            Notice::Leaked { pointfile: p.clone() },
            Notice::LeakedNoPointfile,
            Notice::BspNotWritten { bsp: p.clone() },
            Notice::PackInvalidPath { path: "x".into() },
            Notice::PackMissingFile { file: p.clone() },
            Notice::NothingToPack,
            Notice::PackFailed { error: "e".into() },
            Notice::Packing { count: 1 },
            Notice::Packing { count: 2 },
            Notice::Packed { count: 1, bytes: 10 },
            Notice::Packed { count: 3, bytes: 4096 },
            Notice::PackNotInBsp { files: vec!["materials/a.vmt".into()] },
            Notice::PackNotInBsp { files: vec!["materials/a.vmt".into(), "materials/a.vtf".into()] },
            Notice::CopySourceMissing { from: p.clone() },
            Notice::CreateDirFailed { dir: p.clone(), error: "e".into() },
            Notice::CopyFailed { from: p.clone(), to: p.clone(), error: "e".into() },
            Notice::ReplaceFailed { to: p.clone(), error: "e".into() },
            Notice::Copied { from: p.clone(), to: p.clone() },
            Notice::CubemapsKept { count: 1, bsp: p.clone() },
            Notice::CubemapsKept { count: 4, bsp: p.clone() },
            Notice::Started { exe: p.clone() },
            Notice::StartFailed { exe: p.clone(), error: "e".into() },
            Notice::Cancelled,
            Notice::QueryFailed { error: "e".into() },
            Notice::Crashed { program: "hammerpro.exe".into(), code: -1_073_741_819, kind: CrashKind::AccessViolation },
            Notice::ExitCode { program: "hammerpro.exe".into(), code: 1 },
            Notice::StepOutOfMemory { step: hp_compile::StepKind::Rad, bytes: 5_001_064_080 },
            Notice::CrashedDuring { step: hp_compile::StepKind::Bsp, lines: vec!["a".into()] },
            Notice::Terminated { program: "hammerpro.exe".into() },
            Notice::NoPreviousBsp { bsp: p.clone() },
            Notice::StoppedOnLeak,
            Notice::FromCache { from: p.clone() },
            Notice::PackWaiting,
            Notice::PackListUnavailable,
            Notice::FileOpFailed { path: p.clone(), error: "e".into() },
        ];
        for lang in hp_app::i18n::Lang::ALL {
            hp_app::i18n::set_lang(lang);
            for n in &notices {
                let text = notice_text(n);
                assert!(!text.is_empty() && !text.starts_with("compile."), "{n:?}: {text}");
            }
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        assert_eq!(
            notice_text(&Notice::ExitCode { program: "hammerpro.exe".into(), code: 1 }),
            "hammerpro.exe a échoué avec le code de sortie 1."
        );
        assert!(
            notice_text(&Notice::Crashed {
                program: "hammerpro.exe".into(),
                code: -1_073_741_819,
                kind: CrashKind::AccessViolation
            })
            .contains("0xC0000005")
        );
        assert_eq!(notice_text(&Notice::Packing { count: 1 }), "Empaquetage de 1 fichier.");
        let missing = Notice::PackNotInBsp { files: vec!["materials/qa/metal.vmt".into()] };
        assert_eq!(
            notice_text(&missing),
            "Empaquetage : 1 fichier manque dans le BSP après l'empaquetage, les joueurs ne l'auront pas : materials/qa/metal.vmt"
        );
        for code in hp_compile::knowledge::codes() {
            assert!(hp_app::i18n::has_key(&format!("compile.kb.{code}")), "{code}");
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert_eq!(notice_text(&Notice::Packing { count: 1 }), "Packing 1 file.");
        assert_eq!(notice_text(&Notice::Packing { count: 3 }), "Packing 3 files.");
        let many: Vec<String> = (0..10).map(|i| format!("materials/m{i}.vmt")).collect();
        let text = notice_text(&Notice::PackNotInBsp { files: many });
        assert!(text.starts_with("Pack: 10 files are missing from the BSP"), "{text}");
        assert!(text.ends_with("materials/m7.vmt (and 2 more)"), "{text}");
        for kind in [
            CrashKind::AccessViolation,
            CrashKind::DivisionByZero,
            CrashKind::StackOverflow,
            CrashKind::MemoryCorruption,
            CrashKind::Aborted,
            CrashKind::OutOfMemory,
            CrashKind::Breakpoint,
            CrashKind::MissingDll,
            CrashKind::WrongArchitecture,
            CrashKind::Interrupted,
            CrashKind::IllegalInstruction,
        ] {
            assert!(!crash_text(kind).starts_with("compile."), "{kind:?}");
        }
    }

    #[test]
    fn plan_errors_are_translated() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        let e = CompileError::InvalidMapName { name: "my map".into(), problem: MapNameProblem::Whitespace };
        let text = plan_error_text(&e);
        assert!(text.contains("my map") && text.contains("espaces"), "{text}");
        let e = CompileError::InvalidMapName { name: "a.v2".into(), problem: MapNameProblem::Dot };
        assert!(plan_error_text(&e).contains("point"));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert!(plan_error_text(&CompileError::InvalidMapPath(PathBuf::new())).starts_with("Invalid map path"));
    }
}
