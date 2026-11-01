use std::path::PathBuf;
use std::time::Instant;

use hp_compile::{CompileEvent, CompilePlan, LogLocation, Severity as LogSeverity, StepKind};

use crate::util::contains_ci;

pub(crate) const MAX_LOG_LINES: usize = 250_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StepStatus {
    Pending,
    Running,
    Done,
    Failed,
    Skipped,
}

#[derive(Clone, Debug)]
pub(crate) struct StepView {
    pub index: usize,
    pub kind: StepKind,
    pub status: StepStatus,
    pub fraction: f32,
    pub phase: Option<String>,
    pub started: Option<Instant>,
    pub duration_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub command_line: String,
}

#[derive(Clone, Debug)]
pub(crate) struct LogLine {
    pub step: Option<usize>,
    pub text: String,
    pub severity: LogSeverity,
    pub location: Option<LogLocation>,
    pub knowledge: Option<&'static hp_compile::knowledge::Knowledge>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Summary {
    pub success: bool,
    pub cancelled: bool,
    pub leaked: bool,
    pub total_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Effect {
    StepFinished { kind: StepKind, success: bool, duration_ms: u64 },
    Leak(PathBuf),
    Finished(Summary),
}

#[derive(Clone, Debug)]
pub(crate) struct RunState {
    pub map_name: String,
    pub vmf: Option<PathBuf>,
    pub steps: Vec<StepView>,
    pub log: Vec<LogLine>,
    pub started: Instant,
    pub summary: Option<Summary>,
    pub warnings: usize,
    pub errors: usize,
    pub leak_file: Option<PathBuf>,
    pub leak_entity: Option<LogLocation>,
    pub doc_serial: Option<u64>,
    pub restored: Vec<usize>,
    pub portalclusters: Option<u32>,
    pub numportals: Option<u32>,
    truncated: bool,
}

impl RunState {
    pub(crate) fn preparing(map_name: &str) -> Self {
        Self {
            map_name: map_name.to_owned(),
            vmf: None,
            steps: Vec::new(),
            log: Vec::new(),
            started: Instant::now(),
            summary: None,
            warnings: 0,
            errors: 0,
            leak_file: None,
            leak_entity: None,
            doc_serial: None,
            restored: Vec::new(),
            portalclusters: None,
            numportals: None,
            truncated: false,
        }
    }

    pub(crate) fn set_plan(&mut self, plan: &CompilePlan) {
        self.steps = plan
            .enabled_steps()
            .map(|(index, s)| StepView {
                index,
                kind: s.kind,
                status: StepStatus::Pending,
                fraction: 0.0,
                phase: None,
                started: None,
                duration_ms: None,
                exit_code: None,
                command_line: s.command_line(),
            })
            .collect();
    }

    pub(crate) fn push_log(
        &mut self,
        step: Option<usize>,
        severity: LogSeverity,
        text: String,
        location: Option<LogLocation>,
    ) {
        match severity {
            LogSeverity::Warning => self.warnings += 1,
            LogSeverity::Error => self.errors += 1,
            _ => {}
        }
        if self.log.len() >= MAX_LOG_LINES {
            if !self.truncated {
                self.truncated = true;
                self.log.push(LogLine {
                    step,
                    text: hp_app::t!("compile.log_truncated").to_owned(),
                    severity: LogSeverity::Warning,
                    location: None,
                    knowledge: None,
                });
            }
            return;
        }
        let knowledge = if matches!(severity, LogSeverity::Warning | LogSeverity::Error) {
            hp_compile::knowledge::explain(&text)
        } else {
            None
        };
        self.log.push(LogLine { step, text, severity, location, knowledge });
    }

    fn step_mut(&mut self, index: usize) -> Option<&mut StepView> {
        self.steps.iter_mut().find(|s| s.index == index)
    }

    pub(crate) fn apply(&mut self, event: CompileEvent) -> Option<Effect> {
        match event {
            CompileEvent::StepStarted { index, kind, command_line } => {
                if let Some(s) = self.step_mut(index) {
                    s.status = StepStatus::Running;
                    s.started = Some(Instant::now());
                    s.fraction = 0.0;
                    s.phase = None;
                } else {
                    self.steps.push(StepView {
                        index,
                        kind,
                        status: StepStatus::Running,
                        fraction: 0.0,
                        phase: None,
                        started: Some(Instant::now()),
                        duration_ms: None,
                        exit_code: None,
                        command_line: command_line.clone(),
                    });
                }
                self.push_log(Some(index), LogSeverity::Info, format!("> {command_line}"), None);
                None
            }
            CompileEvent::Log { step, line, severity, location } => {
                if let Some((stat, n)) = hp_compile::history::parse_portal_stat(&line) {
                    if stat == "portalclusters" {
                        self.portalclusters = Some(n);
                    } else {
                        self.numportals = Some(n);
                    }
                }
                if self.leak_entity.is_none()
                    && hp_compile::is_leak_line(&line)
                    && location.as_ref().is_some_and(|l| l.entity_class.is_some() || l.position.is_some())
                {
                    self.leak_entity.clone_from(&location);
                }
                self.push_log(Some(step), severity, line, location);
                None
            }
            CompileEvent::Notice { step, notice } => {
                if matches!(notice, hp_compile::Notice::FromCache { .. }) {
                    self.restored.push(step);
                }
                self.push_log(Some(step), notice.severity(), super::notice_text(&notice), None);
                None
            }
            CompileEvent::StepSkipped { index, kind } => {
                match self.step_mut(index) {
                    Some(s) => s.status = StepStatus::Skipped,
                    None => self.steps.push(StepView {
                        index,
                        kind,
                        status: StepStatus::Skipped,
                        fraction: 0.0,
                        phase: None,
                        started: None,
                        duration_ms: None,
                        exit_code: None,
                        command_line: String::new(),
                    }),
                }
                None
            }
            CompileEvent::Progress { step, fraction, phase } => {
                if let Some(s) = self.step_mut(step) {
                    s.fraction = fraction.clamp(0.0, 1.0);
                    if phase.is_some() {
                        s.phase = phase;
                    }
                }
                None
            }
            CompileEvent::StepFinished { index, success, exit_code, duration_ms } => {
                let kind = self.step_mut(index).map(|s| {
                    s.status = if success { StepStatus::Done } else { StepStatus::Failed };
                    s.duration_ms = Some(duration_ms);
                    s.exit_code = exit_code;
                    if success {
                        s.fraction = 1.0;
                    }
                    s.kind
                });
                kind.map(|kind| Effect::StepFinished { kind, success, duration_ms })
            }
            CompileEvent::LeakDetected { pointfile } => {
                self.leak_file = Some(pointfile.clone());
                Some(Effect::Leak(pointfile))
            }
            CompileEvent::Finished { success, cancelled, leaked, total_ms } => {
                for s in &mut self.steps {
                    match s.status {
                        StepStatus::Pending => s.status = StepStatus::Skipped,
                        StepStatus::Running => {
                            s.status = StepStatus::Failed;
                            s.duration_ms = s.started.map(|t| t.elapsed().as_millis() as u64);
                        }
                        _ => {}
                    }
                }
                let summary = Summary { success, cancelled, leaked, total_ms };
                self.summary = Some(summary);
                Some(Effect::Finished(summary))
            }
        }
    }

    pub(crate) fn abort(&mut self, cancelled: bool) -> Summary {
        let summary =
            Summary { success: false, cancelled, leaked: false, total_ms: self.started.elapsed().as_millis() as u64 };
        for s in &mut self.steps {
            if matches!(s.status, StepStatus::Pending | StepStatus::Running) {
                s.status = StepStatus::Skipped;
            }
        }
        self.summary = Some(summary);
        summary
    }

    pub(crate) fn overall_progress(&self) -> f32 {
        if self.steps.is_empty() {
            return 0.0;
        }
        let done: f32 = self
            .steps
            .iter()
            .map(|s| match s.status {
                StepStatus::Done | StepStatus::Failed | StepStatus::Skipped => 1.0,
                StepStatus::Running => s.fraction,
                StepStatus::Pending => 0.0,
            })
            .sum();
        done / self.steps.len() as f32
    }

    pub(crate) fn log_text(&self) -> String {
        let mut out = String::with_capacity(self.log.iter().map(|l| l.text.len() + 1).sum());
        for l in &self.log {
            out.push_str(&l.text);
            out.push('\n');
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LogFilter {
    pub info: bool,
    pub warnings: bool,
    pub errors: bool,
    pub search: String,
    pub step: Option<usize>,
}

impl Default for LogFilter {
    fn default() -> Self {
        Self { info: true, warnings: true, errors: true, search: String::new(), step: None }
    }
}

impl LogFilter {
    fn accepts(&self, line: &LogLine, needle_lower: &str) -> bool {
        let sev = match line.severity {
            LogSeverity::Info | LogSeverity::Success => self.info,
            LogSeverity::Warning => self.warnings,
            LogSeverity::Error => self.errors,
        };
        let step = self.step.is_none_or(|s| line.step == Some(s));
        sev && step && contains_ci(&line.text, needle_lower)
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct LogView {
    pub filter: LogFilter,
    applied: Option<(LogFilter, u64)>,
    pub rows: Vec<u32>,
    scanned: usize,
    pub row_height: f32,
}

impl LogView {
    pub(crate) fn update(&mut self, log: &[LogLine], run_id: u64) {
        let key = (self.filter.clone(), run_id);
        if self.applied.as_ref() != Some(&key) || self.scanned > log.len() {
            self.rows.clear();
            self.scanned = 0;
            self.applied = Some(key);
        }
        let needle = self.filter.search.trim().to_ascii_lowercase();
        for (i, line) in log.iter().enumerate().skip(self.scanned) {
            if self.filter.accepts(line, &needle) {
                self.rows.push(i as u32);
            }
        }
        self.scanned = log.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_compile::Step;

    pub(crate) fn plan() -> CompilePlan {
        let mut plan = CompilePlan::new("C:/maps/gm_x.vmf");
        plan.steps.push(Step::new(StepKind::Bsp, "hammerpro.exe", vec!["a".into()]));
        plan.steps.push(Step::new(StepKind::Rad, "hammerpro.exe", Vec::new()));
        let mut rad = Step::new(StepKind::Rad, "hammerpro.exe", Vec::new());
        rad.enabled = false;
        plan.steps.push(rad);
        plan
    }

    #[test]
    fn events_drive_steps() {
        let mut run = RunState::preparing("gm_x");
        run.set_plan(&plan());
        assert_eq!(run.steps.len(), 2, "disabled steps are not listed");
        assert!(
            run.apply(CompileEvent::StepStarted { index: 0, kind: StepKind::Bsp, command_line: "hammerpro a".into() })
                .is_none()
        );
        assert_eq!(run.steps[0].status, StepStatus::Running);
        run.apply(CompileEvent::Progress { step: 0, fraction: 0.5, phase: Some("ProcessWorld".into()) });
        assert_eq!(run.steps[0].fraction, 0.5);
        assert_eq!(run.steps[0].phase.as_deref(), Some("ProcessWorld"));
        run.apply(CompileEvent::Progress { step: 0, fraction: 2.0, phase: None });
        assert_eq!(run.steps[0].fraction, 1.0);
        assert_eq!(run.steps[0].phase.as_deref(), Some("ProcessWorld"));
        run.apply(CompileEvent::Log {
            step: 0,
            line: "Brush 2: WARNING, microbrush".into(),
            severity: LogSeverity::Warning,
            location: Some(LogLocation { brush_id: Some(2), ..LogLocation::default() }),
        });
        assert_eq!(run.warnings, 1);
        let fx =
            run.apply(CompileEvent::StepFinished { index: 0, success: true, exit_code: Some(0), duration_ms: 1200 });
        assert_eq!(fx, Some(Effect::StepFinished { kind: StepKind::Bsp, success: true, duration_ms: 1200 }));
        assert_eq!(run.steps[0].status, StepStatus::Done);
        assert!((run.overall_progress() - 0.5).abs() < 1e-6);
        let fx = run.apply(CompileEvent::LeakDetected { pointfile: PathBuf::from("C:/maps/gm_x.lin") });
        assert_eq!(fx, Some(Effect::Leak(PathBuf::from("C:/maps/gm_x.lin"))));
        let fx = run.apply(CompileEvent::Finished { success: false, cancelled: false, leaked: true, total_ms: 1500 });
        assert!(matches!(fx, Some(Effect::Finished(Summary { leaked: true, success: false, .. }))));
        assert_eq!(run.steps[1].status, StepStatus::Skipped);
        assert!(run.log_text().contains("microbrush"));
    }

    #[test]
    fn log_filtering_is_incremental() {
        let mut run = RunState::preparing("m");
        run.push_log(None, LogSeverity::Info, "hello world".into(), None);
        run.push_log(None, LogSeverity::Warning, "careful".into(), None);
        run.push_log(None, LogSeverity::Error, "broken".into(), None);
        let mut view = LogView::default();
        view.update(&run.log, 1);
        assert_eq!(view.rows, vec![0, 1, 2]);
        run.push_log(None, LogSeverity::Success, "done".into(), None);
        view.update(&run.log, 1);
        assert_eq!(view.rows, vec![0, 1, 2, 3]);
        view.filter.info = false;
        view.update(&run.log, 1);
        assert_eq!(view.rows, vec![1, 2]);
        view.filter.info = true;
        view.filter.search = "BRO".into();
        view.update(&run.log, 1);
        assert_eq!(view.rows, vec![2]);
        view.filter.search.clear();
        run.push_log(Some(3), LogSeverity::Info, "from step 3".into(), None);
        view.filter.step = Some(3);
        view.update(&run.log, 1);
        assert_eq!(view.rows, vec![4]);
        view.filter.step = None;
        let run2 = RunState::preparing("m");
        view.update(&run2.log, 2);
        assert!(view.rows.is_empty());
        assert_eq!(run.errors, 1);
    }

    #[test]
    fn notices_and_skipped_steps() {
        crate::register_translations();
        let mut run = RunState::preparing("gm_x");
        run.set_plan(&plan());
        run.apply(CompileEvent::StepSkipped { index: 1, kind: StepKind::Rad });
        assert_eq!(run.steps[1].status, StepStatus::Skipped);
        run.apply(CompileEvent::Notice { step: 1, notice: hp_compile::Notice::Cancelled });
        assert_eq!(run.warnings, 1);
        let line = run.log.last().expect("line");
        assert_eq!(line.severity, LogSeverity::Warning);
        assert_eq!(line.step, Some(1));
        assert!(!line.text.starts_with("compile."), "translated: {}", line.text);
        run.apply(CompileEvent::StepSkipped { index: 7, kind: StepKind::Custom });
        assert!(run.steps.iter().any(|s| s.index == 7 && s.status == StepStatus::Skipped));
    }

    #[test]
    fn abort_marks_steps() {
        let mut run = RunState::preparing("m");
        run.set_plan(&plan());
        let s = run.abort(true);
        assert!(s.cancelled && !s.success);
        assert!(run.steps.iter().all(|s| s.status == StepStatus::Skipped));
    }
}
