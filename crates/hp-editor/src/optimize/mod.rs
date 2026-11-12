use glam::DVec3;
use hp_geom::Aabb;

use crate::selection::ObjRef;

pub mod areaportals;
pub mod detail;
pub mod hints;
pub mod lights;
#[doc(hidden)]
pub mod sample;
pub mod sealing;
pub mod vis;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Impact {
    Low,
    Medium,
    High,
}

impl Impact {
    pub fn id(self) -> &'static str {
        match self {
            Impact::Low => "low",
            Impact::Medium => "medium",
            Impact::High => "high",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Topic {
    Visleaves,
    Detail,
    Hints,
    Areaportals,
    Lights,
}

impl Topic {
    pub fn id(self) -> &'static str {
        match self {
            Topic::Visleaves => "visleaves",
            Topic::Detail => "func_detail",
            Topic::Hints => "hints",
            Topic::Areaportals => "areaportals",
            Topic::Lights => "lights",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub topic: Topic,
    pub code: &'static str,
    pub impact: Impact,
    pub score: f64,
    pub objects: Vec<ObjRef>,
    pub points: Vec<DVec3>,
    pub boxes: Vec<Aabb>,
    pub args: Vec<String>,
    pub message: String,
    pub fix: Option<&'static str>,
}

impl Finding {
    pub fn new(topic: Topic, code: &'static str, impact: Impact, message: String) -> Finding {
        Finding {
            topic,
            code,
            impact,
            score: 0.0,
            objects: Vec::new(),
            points: Vec::new(),
            boxes: Vec::new(),
            args: Vec::new(),
            message,
            fix: None,
        }
    }

    pub fn score(mut self, score: f64) -> Finding {
        self.score = if score.is_finite() { score } else { 0.0 };
        self
    }

    pub fn objects(mut self, objects: Vec<ObjRef>) -> Finding {
        self.objects = objects;
        self
    }

    pub fn points(mut self, points: Vec<DVec3>) -> Finding {
        self.points = points;
        self
    }

    pub fn boxes(mut self, boxes: Vec<Aabb>) -> Finding {
        self.boxes = boxes;
        self
    }

    pub fn args(mut self, args: Vec<String>) -> Finding {
        self.args = args;
        self
    }

    pub fn fix(mut self, fix: &'static str) -> Finding {
        self.fix = Some(fix);
        self
    }

    pub fn arg(&self, i: usize) -> &str {
        self.args.get(i).map_or("", String::as_str)
    }

    pub fn rank(&self) -> f64 {
        let base = match self.impact {
            Impact::High => 2.0e9,
            Impact::Medium => 1.0e9,
            Impact::Low => 0.0,
        };
        base + self.score.clamp(0.0, 9.0e8)
    }

    pub fn focus(&self) -> Aabb {
        let mut b = Aabb::EMPTY;
        for p in &self.points {
            b.extend(*p);
        }
        for x in &self.boxes {
            if !x.is_empty() {
                b = b.union(x);
            }
        }
        b
    }
}

pub fn sort_findings(list: &mut [Finding]) {
    list.sort_by(|a, b| b.rank().total_cmp(&a.rank()).then_with(|| a.code.cmp(b.code)));
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OptimizeReport {
    pub vis: Option<vis::VisReport>,
    pub findings: Vec<Finding>,
    pub compile_error: Option<String>,
    pub leaked: bool,
    pub revision: u64,
    pub ms: u64,
}

impl OptimizeReport {
    pub fn count(&self, impact: Impact) -> usize {
        self.findings.iter().filter(|f| f.impact == impact).count()
    }
}

pub(crate) fn fmt_point(p: DVec3) -> String {
    format!("{:.0} {:.0} {:.0}", p.x, p.y, p.z)
}

const CELL: f64 = 256.0;
const MAX_CELLS: i64 = 4096;

pub struct SolidIndex {
    solids: Vec<(Aabb, Vec<hp_geom::Plane>, crate::Id)>,
    cells: std::collections::HashMap<(i64, i64, i64), Vec<u32>>,
    big: Vec<u32>,
}

fn cell_of(p: DVec3) -> (i64, i64, i64) {
    let c = |v: f64| (v / CELL).floor().clamp(-1.0e6, 1.0e6) as i64;
    (c(p.x), c(p.y), c(p.z))
}

impl SolidIndex {
    pub fn new(doc: &crate::Document, world_only: bool) -> SolidIndex {
        let mut idx = SolidIndex { solids: Vec::new(), cells: std::collections::HashMap::new(), big: Vec::new() };
        for n in doc.scene.solids.values() {
            if world_only && n.owner != crate::Owner::World {
                continue;
            }
            if !n.geom.valid || n.has_disp() || !doc.is_visible(ObjRef::Solid(n.solid.id)) {
                continue;
            }
            let sealing =
                n.solid.sides.iter().all(|s| crate::limits::checks::MaterialFacts::guess(&s.material).seals());
            if !sealing {
                continue;
            }
            let i = idx.solids.len() as u32;
            let b = n.geom.aabb;
            idx.solids.push((b, n.geom.faces.iter().map(|f| f.plane).collect(), n.solid.id));
            let (a, z) = (cell_of(b.min), cell_of(b.max));
            let count = (z.0 - a.0 + 1) * (z.1 - a.1 + 1) * (z.2 - a.2 + 1);
            if count > MAX_CELLS {
                idx.big.push(i);
                continue;
            }
            for x in a.0..=z.0 {
                for y in a.1..=z.1 {
                    for w in a.2..=z.2 {
                        idx.cells.entry((x, y, w)).or_default().push(i);
                    }
                }
            }
        }
        idx
    }

    pub fn contains(&self, p: DVec3) -> Option<crate::Id> {
        if !p.is_finite() {
            return None;
        }
        let list = self.cells.get(&cell_of(p)).map(Vec::as_slice).unwrap_or(&[]);
        list.iter().chain(self.big.iter()).find_map(|&i| {
            let (b, planes, id) = &self.solids[i as usize];
            (b.contains_point(p) && planes.iter().all(|pl| pl.distance(p) < -0.01)).then_some(*id)
        })
    }
}
