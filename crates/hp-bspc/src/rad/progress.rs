use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Scene,
    Direct,
    Occlusion,
    BounceRays,
    Bounces,
    LeafAmbient,
    DetailProps,
    StaticProps,
    Write,
}

impl Stage {
    pub const ALL: [Stage; 9] = [
        Stage::Scene,
        Stage::Direct,
        Stage::Occlusion,
        Stage::BounceRays,
        Stage::Bounces,
        Stage::LeafAmbient,
        Stage::DetailProps,
        Stage::StaticProps,
        Stage::Write,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Stage::Scene => "scene",
            Stage::Direct => "direct lighting",
            Stage::Occlusion => "ambient occlusion",
            Stage::BounceRays => "bounce rays",
            Stage::Bounces => "bounced light",
            Stage::LeafAmbient => "leaf ambient",
            Stage::DetailProps => "detail props",
            Stage::StaticProps => "static props",
            Stage::Write => "writing",
        }
    }

    pub fn from_name(name: &str) -> Option<Stage> {
        Stage::ALL.iter().copied().find(|s| s.name() == name)
    }

    fn weight(self) -> f64 {
        match self {
            Stage::Scene => 0.03,
            Stage::Direct => 0.55,
            Stage::Occlusion => 0.04,
            Stage::BounceRays => 0.08,
            Stage::Bounces => 0.08,
            Stage::LeafAmbient => 0.08,
            Stage::DetailProps => 0.03,
            Stage::StaticProps => 0.25,
            Stage::Write => 0.02,
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Stage::Scene => "very large maps and many static prop triangles (-StaticPropPolys) make it slow",
            Stage::Direct => {
                "its time grows with the luxels (small lightmap scales), the lights each face sees and the sky of the \
                 light_environment over large outdoor areas; -fast or larger lightmap scales cut it"
            }
            Stage::Occlusion => "-aosamples and -aoradius set its cost; -noao turns it off",
            Stage::BounceRays => "its time grows with the luxels; -fast or -bounce 0 cut it",
            Stage::Bounces => "-bounce sets the number of passes",
            Stage::LeafAmbient => "maps with very many leaves take long here; -fastambient cuts it",
            Stage::DetailProps => "its time grows with the number of detail props and of lights",
            Stage::StaticProps => {
                "-StaticPropLighting lights every vertex of every static prop: many or detailed props take long; \
                 -fast, a smaller -StaticPropSampleScale or turning it off cut it"
            }
            Stage::Write => "the lightmaps and the pakfile are written",
        }
    }

    fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

pub struct Work {
    stage: AtomicUsize,
    done: AtomicU64,
    total: AtomicU64,
    plan: AtomicU32,
}

impl Default for Work {
    fn default() -> Self {
        Work::new()
    }
}

impl Work {
    pub fn new() -> Work {
        Work {
            stage: AtomicUsize::new(usize::MAX),
            done: AtomicU64::new(0),
            total: AtomicU64::new(1),
            plan: AtomicU32::new(0),
        }
    }

    pub fn plan(&self, stages: &[Stage]) {
        self.plan.store(stages.iter().fold(0, |m, s| m | s.bit()), Ordering::Release);
    }

    pub fn begin(&self, stage: Stage, total: u64) {
        self.done.store(0, Ordering::Relaxed);
        self.total.store(total.max(1), Ordering::Relaxed);
        self.plan.fetch_or(stage.bit(), Ordering::AcqRel);
        self.stage.store(stage as usize, Ordering::Release);
    }

    pub fn finish(&self) {
        self.stage.store(usize::MAX, Ordering::Release);
    }

    pub fn add(&self, n: u64) {
        self.done.fetch_add(n, Ordering::Relaxed);
    }

    fn snapshot(&self) -> Option<(Stage, u64, u64, u32)> {
        let stage = *Stage::ALL.get(self.stage.load(Ordering::Acquire))?;
        let total = self.total.load(Ordering::Relaxed).max(1);
        let done = self.done.load(Ordering::Relaxed).min(total);
        Some((stage, done, total, self.plan.load(Ordering::Acquire)))
    }
}

pub fn duration_text(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s} s")
    } else if s < 3600 {
        format!("{} min {:02} s", s / 60, s % 60)
    } else {
        format!("{} h {:02} min", s / 3600, (s % 3600) / 60)
    }
}

const FIRST_LINE: Duration = Duration::from_secs(1);
const MIN_GAP: Duration = Duration::from_secs(1);
const HEARTBEAT: Duration = Duration::from_secs(30);
const STALL: Duration = Duration::from_secs(60);
const STALL_REPEAT: Duration = Duration::from_secs(180);
const LONG_STAGE: Duration = Duration::from_secs(600);
const STEPS: f64 = 20.0;

pub struct Reporter {
    current: Option<Stage>,
    stage_start: Instant,
    last_print: Option<Instant>,
    last_step: i64,
    last_done: u64,
    last_change: Instant,
    stall_warned: Option<Instant>,
    long_warned: bool,
}

impl Default for Reporter {
    fn default() -> Self {
        Reporter::new()
    }
}

impl Reporter {
    pub fn new() -> Reporter {
        let now = Instant::now();
        Reporter {
            current: None,
            stage_start: now,
            last_print: None,
            last_step: -1,
            last_done: 0,
            last_change: now,
            stall_warned: None,
            long_warned: false,
        }
    }

    pub fn poll(&mut self, work: &Work, log: &mut dyn FnMut(&str)) {
        self.poll_at(work, Instant::now(), log);
    }

    fn poll_at(&mut self, work: &Work, now: Instant, log: &mut dyn FnMut(&str)) {
        let Some((stage, done, total, plan)) = work.snapshot() else { return };
        if self.current != Some(stage) {
            self.current = Some(stage);
            self.stage_start = now;
            self.last_print = None;
            self.last_step = -1;
            self.last_done = done;
            self.last_change = now;
            self.stall_warned = None;
            self.long_warned = false;
        }
        if done != self.last_done {
            self.last_done = done;
            self.last_change = now;
        }
        let elapsed = now.saturating_duration_since(self.stage_start);
        let fraction = done as f64 / total as f64;
        let step = (fraction * STEPS).floor() as i64;
        let since_print = self.last_print.map(|t| now.saturating_duration_since(t));
        let due = elapsed >= FIRST_LINE
            && match since_print {
                None => true,
                Some(gap) => (step > self.last_step && gap >= MIN_GAP) || gap >= HEARTBEAT,
            };
        if due {
            log(&progress_line(stage, plan, fraction, elapsed));
            self.last_print = Some(now);
            self.last_step = step;
        }
        let stalled = now.saturating_duration_since(self.last_change);
        if stalled >= STALL && self.stall_warned.is_none_or(|t| now.saturating_duration_since(t) >= STALL_REPEAT) {
            log(&format!(
                "Warning: the lighting stage \"{}\" has not advanced for {} ({:.0}% done): it is still working; {}",
                stage.name(),
                duration_text(stalled),
                fraction * 100.0,
                stage.hint()
            ));
            self.stall_warned = Some(now);
        }
        if elapsed >= LONG_STAGE && !self.long_warned {
            log(&format!(
                "Warning: the lighting stage \"{}\" has run for {}: {}",
                stage.name(),
                duration_text(elapsed),
                stage.hint()
            ));
            self.long_warned = true;
        }
    }
}

fn overall(stage: Stage, plan: u32, fraction: f64) -> (f64, usize, usize) {
    let planned: Vec<Stage> = Stage::ALL.iter().copied().filter(|s| plan & s.bit() != 0 || *s == stage).collect();
    let sum: f64 = planned.iter().map(|s| s.weight()).sum();
    let index = planned.iter().position(|s| *s == stage).unwrap_or(0);
    let before: f64 = planned[..index].iter().map(|s| s.weight()).sum();
    let value = if sum > 0.0 { (before + stage.weight() * fraction) / sum } else { fraction };
    (value.clamp(0.0, 1.0), index + 1, planned.len())
}

fn progress_line(stage: Stage, plan: u32, fraction: f64, elapsed: Duration) -> String {
    let (all, index, count) = overall(stage, plan, fraction);
    let left = if fraction >= 0.02 && elapsed >= Duration::from_secs(2) {
        let rest = elapsed.as_secs_f64() * (1.0 - fraction) / fraction;
        format!("about {} left", duration_text(Duration::from_secs_f64(rest.min(1.0e7))))
    } else {
        "estimating the time left".to_owned()
    };
    format!(
        "Lighting {:.0}%: {} {:.0}% (stage {index} of {count}), {} elapsed, {left}",
        (all * 100.0).floor(),
        stage.name(),
        (fraction * 100.0).floor(),
        duration_text(elapsed)
    )
}

pub fn watch<R: Send>(work: &Work, log: &mut dyn FnMut(&str), job: impl FnOnce(&mut dyn FnMut(&str)) -> R + Send) -> R {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::scope(|s| {
        let worker = std::thread::Builder::new()
            .name("hp-rad".to_owned())
            .spawn_scoped(s, move || {
                let mut send = |l: &str| {
                    let _ = tx.send(l.to_owned());
                };
                let r = job(&mut send);
                work.finish();
                r
            })
            .expect("cannot start the lighting thread");
        let mut reporter = Reporter::new();
        let mut polled = Instant::now();
        loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(line) => log(&line),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if polled.elapsed() >= Duration::from_millis(200) {
                reporter.poll(work, log);
                polled = Instant::now();
            }
        }
        match worker.join() {
            Ok(r) => r,
            Err(e) => std::panic::resume_unwind(e),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(duration_text(Duration::from_secs(41)), "41 s");
        assert_eq!(duration_text(Duration::from_secs(185)), "3 min 05 s");
        assert_eq!(duration_text(Duration::from_secs(3 * 3600 + 7 * 60 + 5)), "3 h 07 min");
    }

    #[test]
    fn overall_fraction_follows_the_plan() {
        let plan = Stage::Direct.bit() | Stage::LeafAmbient.bit() | Stage::Write.bit();
        let (a, i, n) = overall(Stage::Direct, plan, 0.0);
        assert_eq!((i, n), (1, 3));
        assert_eq!(a, 0.0);
        let (b, i, _) = overall(Stage::LeafAmbient, plan, 0.5);
        assert_eq!(i, 2);
        assert!(b > 0.8 && b < 1.0, "{b}");
        assert_eq!(overall(Stage::Write, plan, 1.0).0, 1.0);
        assert!(overall(Stage::Direct, plan, 0.5).0 < overall(Stage::Direct, plan, 0.6).0);
    }

    #[test]
    fn lines_steps_and_watchdog() {
        let work = Work::new();
        work.plan(&[Stage::Direct, Stage::Write]);
        let mut r = Reporter::new();
        let mut lines = Vec::new();
        let t0 = Instant::now();
        let at = |r: &mut Reporter, s: u64, lines: &mut Vec<String>| {
            r.poll_at(&work, t0 + Duration::from_secs(s), &mut |l| lines.push(l.to_owned()))
        };
        r.poll_at(&work, t0, &mut |l| lines.push(l.to_owned()));
        assert!(lines.is_empty(), "nothing runs yet");
        work.begin(Stage::Direct, 1000);
        at(&mut r, 0, &mut lines);
        assert!(lines.is_empty(), "no line in the first second");
        work.add(100);
        at(&mut r, 2, &mut lines);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("Lighting "), "{}", lines[0]);
        assert!(lines[0].contains("direct lighting 10% (stage 1 of 2), 2 s elapsed, about 18 s left"), "{}", lines[0]);
        at(&mut r, 3, &mut lines);
        assert_eq!(lines.len(), 1, "no new step, no line");
        work.add(60);
        at(&mut r, 4, &mut lines);
        assert_eq!(lines.len(), 2, "a 5% step prints");
        at(&mut r, 40, &mut lines);
        assert_eq!(lines.len(), 3, "the heartbeat prints");
        at(&mut r, 70, &mut lines);
        assert!(
            lines.iter().any(|l| l.starts_with("Warning: the lighting stage \"direct lighting\" has not advanced"))
        );
        let warnings = lines.iter().filter(|l| l.starts_with("Warning")).count();
        at(&mut r, 80, &mut lines);
        assert_eq!(lines.iter().filter(|l| l.starts_with("Warning")).count(), warnings, "warned once");
        at(&mut r, 700, &mut lines);
        assert!(lines.iter().any(|l| l.contains("has run for")));
        work.begin(Stage::Write, 1);
        work.add(1);
        at(&mut r, 702, &mut lines);
        at(&mut r, 704, &mut lines);
        assert!(lines.last().is_some_and(|l| l.starts_with("Lighting 100%: writing 100% (stage 2 of 2)")), "{lines:?}");
    }

    #[test]
    fn watch_forwards_lines_and_returns() {
        let work = Work::new();
        let mut lines = Vec::new();
        let r = watch(&work, &mut |l| lines.push(l.to_owned()), |log| {
            work.begin(Stage::Direct, 10);
            log("first");
            work.add(10);
            log("second");
            7
        });
        assert_eq!(r, 7);
        assert_eq!(lines, vec!["first".to_owned(), "second".to_owned()]);
    }
}
