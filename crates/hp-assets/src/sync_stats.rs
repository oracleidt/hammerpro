use std::time::Instant;

const WINDOW: u32 = 120;

pub(crate) struct Lap {
    at: Instant,
    stages: Vec<(&'static str, f32)>,
}

impl Lap {
    pub fn new() -> Self {
        Self { at: Instant::now(), stages: Vec::with_capacity(20) }
    }

    pub fn mark(&mut self, name: &'static str) {
        let now = Instant::now();
        self.stages.push((name, now.duration_since(self.at).as_secs_f32() * 1000.0));
        self.at = now;
    }

    pub fn done(self) -> Vec<(&'static str, f32)> {
        self.stages
    }
}

#[derive(Debug, Default)]
pub(crate) struct SyncStats {
    pub sum: Vec<(&'static str, f32, f32)>,
    pub frames: u32,
}

impl SyncStats {
    pub fn add(&mut self, stages: Vec<(&'static str, f32)>) {
        if self.frames >= WINDOW {
            self.sum.clear();
            self.frames = 0;
        }
        self.frames += 1;
        for (name, ms) in stages {
            match self.sum.iter_mut().find(|s| s.0 == name) {
                Some(s) => {
                    s.1 += ms;
                    s.2 = s.2.max(ms);
                }
                None => self.sum.push((name, ms, ms)),
            }
        }
    }

    pub fn averages(&self) -> Vec<(&'static str, f32, f32)> {
        let n = self.frames.max(1) as f32;
        self.sum.iter().map(|(name, total, max)| (*name, total / n, *max)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_average_over_frames() {
        let mut s = SyncStats::default();
        s.add(vec![("a", 1.0), ("b", 4.0)]);
        s.add(vec![("a", 3.0), ("b", 0.0)]);
        assert_eq!(s.averages(), vec![("a", 2.0, 3.0), ("b", 2.0, 4.0)]);
    }
}
