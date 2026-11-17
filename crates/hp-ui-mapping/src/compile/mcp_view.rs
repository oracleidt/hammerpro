use std::path::PathBuf;

use hp_compile::Severity as LogSeverity;

use super::CompileDialog;
use super::run::StepStatus;

#[derive(Clone, Debug)]
pub struct CompileStepView {
    pub kind: String,
    pub status: &'static str,
    pub fraction: f32,
    pub phase: Option<String>,
    pub duration_ms: Option<u64>,
    pub exit_code: Option<i32>,
}

#[derive(Clone, Debug, Default)]
pub struct CompileRunView {
    pub map: String,
    pub running: bool,
    pub steps: Vec<CompileStepView>,
    pub lines: Vec<CompileLineView>,
    pub total_lines: usize,
    pub warnings: usize,
    pub errors: usize,
    pub leak_file: Option<PathBuf>,
    pub result: Option<(bool, bool, bool, u64)>,
    pub no_gpu: bool,
}

#[derive(Clone, Debug)]
pub struct CompileLineView {
    pub severity: &'static str,
    pub step: Option<usize>,
    pub text: String,
    pub location: Option<hp_compile::LogLocation>,
    pub knowledge: Option<&'static hp_compile::knowledge::Knowledge>,
}

fn severity_name(s: LogSeverity) -> &'static str {
    match s {
        LogSeverity::Info => "info",
        LogSeverity::Success => "success",
        LogSeverity::Warning => "warning",
        _ => "error",
    }
}

impl CompileDialog {
    pub fn mcp_severity_name(s: LogSeverity) -> &'static str {
        severity_name(s)
    }

    pub fn mcp_select_preset(s: &mut hp_app::settings::CompileSettings, p: hp_app::settings::CompilePresetSetting) {
        super::plan::apply_preset(s, p);
    }

    pub fn mcp_settle_preset(s: &mut hp_app::settings::CompileSettings) {
        super::settings_ui::settle_preset(s);
    }

    pub fn mcp_raytracing_optimal(s: &mut hp_app::settings::CompileSettings) {
        super::raytrace_ui::apply_optimal(s);
    }

    pub fn mcp_builds_cubemaps(s: &hp_app::settings::CompileSettings) -> bool {
        super::plan::builds_cubemaps(s)
    }

    pub fn mcp_parallax_cubemaps(s: &hp_app::settings::CompileSettings) -> bool {
        super::plan::parallax_cubemaps_on(s)
    }

    pub fn mcp_view(&self, tail: usize, problems_only: bool, contains: Option<&str>) -> Option<CompileRunView> {
        let run = self.run.as_ref()?;
        let needle = contains.map(str::to_ascii_lowercase);
        let kept: Vec<CompileLineView> = run
            .log
            .iter()
            .filter(|l| {
                !problems_only || matches!(l.severity, LogSeverity::Warning) || severity_name(l.severity) == "error"
            })
            .filter(|l| needle.as_deref().is_none_or(|n| l.text.to_ascii_lowercase().contains(n)))
            .map(|l| CompileLineView {
                severity: severity_name(l.severity),
                step: l.step,
                text: l.text.clone(),
                location: l.location.clone(),
                knowledge: l.knowledge,
            })
            .collect();
        let skip = kept.len().saturating_sub(tail);
        Some(CompileRunView {
            map: run.map_name.clone(),
            running: self.is_running(),
            steps: run
                .steps
                .iter()
                .map(|s| CompileStepView {
                    kind: format!("{:?}", s.kind).to_ascii_lowercase(),
                    status: match s.status {
                        StepStatus::Pending => "pending",
                        StepStatus::Running => "running",
                        StepStatus::Done => "done",
                        StepStatus::Failed => "failed",
                        _ => "skipped",
                    },
                    fraction: s.fraction,
                    phase: s.phase.clone(),
                    duration_ms: s.duration_ms,
                    exit_code: s.exit_code,
                })
                .collect(),
            lines: kept.into_iter().skip(skip).collect(),
            total_lines: run.log.len(),
            warnings: run.warnings,
            errors: run.errors,
            leak_file: run.leak_file.clone(),
            result: run.summary.as_ref().map(|s| (s.success, s.cancelled, s.leaked, s.total_ms)),
            no_gpu: run.no_gpu,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn no_view_before_the_first_compile() {
        let d = super::CompileDialog::new();
        assert!(d.mcp_view(10, false, None).is_none());
    }
}
