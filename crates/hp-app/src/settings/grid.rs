use serde::{Deserialize, Serialize};

use super::workflow::clamp_grid;
use crate::view::ViewKind;

pub const MAX_CUSTOM_GRIDS: usize = 64;
pub const MAX_MAJOR_EVERY: u32 = 64;
pub const GRID_COORD_LIMIT: f64 = 32768.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GridMode {
    #[default]
    Global,
    PerView,
    Custom,
    Selection,
}

impl GridMode {
    pub const ALL: [GridMode; 4] = [GridMode::Global, GridMode::PerView, GridMode::Custom, GridMode::Selection];

    pub fn name(self) -> &'static str {
        match self {
            GridMode::Global => "global",
            GridMode::PerView => "per_view",
            GridMode::Custom => "custom",
            GridMode::Selection => "selection",
        }
    }

    pub fn from_name(name: &str) -> Option<GridMode> {
        let n = name.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        GridMode::ALL.into_iter().find(|m| m.name() == n || (n == "perview" && *m == GridMode::PerView))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewGrid {
    pub size: f64,
    pub origin: [f64; 3],
    pub major_every: u32,
    pub angle: f64,
}

impl Default for ViewGrid {
    fn default() -> Self {
        Self { size: 16.0, origin: [0.0; 3], major_every: 0, angle: 0.0 }
    }
}

impl ViewGrid {
    pub fn sanitized(&self) -> ViewGrid {
        ViewGrid {
            size: clamp_grid(self.size, 16.0),
            origin: self.origin.map(clean_coord),
            major_every: self.major_every.min(MAX_MAJOR_EVERY),
            angle: clean_angle(self.angle),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PerViewGrids {
    pub perspective: ViewGrid,
    pub top: ViewGrid,
    pub front: ViewGrid,
    pub side: ViewGrid,
}

impl PerViewGrids {
    pub fn get(&self, kind: ViewKind) -> &ViewGrid {
        match kind {
            ViewKind::Perspective => &self.perspective,
            ViewKind::Top => &self.top,
            ViewKind::Front => &self.front,
            ViewKind::Side => &self.side,
        }
    }

    pub fn get_mut(&mut self, kind: ViewKind) -> &mut ViewGrid {
        match kind {
            ViewKind::Perspective => &mut self.perspective,
            ViewKind::Top => &mut self.top,
            ViewKind::Front => &mut self.front,
            ViewKind::Side => &mut self.side,
        }
    }

    pub fn is_unset(&self) -> bool {
        [&self.perspective, &self.top, &self.front, &self.side]
            .into_iter()
            .all(|g| ViewGrid { angle: 0.0, ..g.clone() } == ViewGrid::default())
    }

    pub fn uniform(size: f64) -> PerViewGrids {
        let g = ViewGrid { size: clamp_grid(size, 16.0), ..ViewGrid::default() };
        PerViewGrids { perspective: g.clone(), top: g.clone(), front: g.clone(), side: g }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GridSpec {
    pub name: String,
    pub step: [f64; 3],
    pub origin: [f64; 3],
    pub angles: [f64; 3],
    pub major_every: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<[[f64; 3]; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
}

impl Default for GridSpec {
    fn default() -> Self {
        Self {
            name: String::new(),
            step: [16.0; 3],
            origin: [0.0; 3],
            angles: [0.0; 3],
            major_every: 0,
            bounds: None,
            color: None,
        }
    }
}

fn clean_coord(v: f64) -> f64 {
    if v.is_finite() { v.clamp(-GRID_COORD_LIMIT, GRID_COORD_LIMIT) + 0.0 } else { 0.0 }
}

fn clean_angle(v: f64) -> f64 {
    if v.is_finite() { (v % 360.0) + 0.0 } else { 0.0 }
}

impl GridSpec {
    pub fn uniform(name: &str, size: f64) -> GridSpec {
        GridSpec { name: name.to_owned(), step: [clamp_grid(size, 16.0); 3], ..GridSpec::default() }
    }

    pub fn sanitized(&self) -> GridSpec {
        let bounds = self.bounds.and_then(|[a, b]| {
            let a = a.map(clean_coord);
            let b = b.map(clean_coord);
            let min = [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])];
            let max = [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])];
            (0..3).all(|i| max[i] - min[i] > 1e-6).then_some([min, max])
        });
        GridSpec {
            name: hp_editor::clean_grid_name(&self.name),
            step: self.step.map(|s| clamp_grid(s, 16.0)),
            origin: self.origin.map(clean_coord),
            angles: self.angles.map(clean_angle),
            major_every: self.major_every.min(MAX_MAJOR_EVERY),
            bounds,
            color: self.color,
        }
    }

    pub fn is_uniform(&self) -> bool {
        self.step.iter().all(|s| (s - self.step[0]).abs() <= 1e-9 * s.abs().max(1.0))
    }

    pub fn to_block(&self) -> hp_kv::Block {
        let num = |v: f64| {
            let v = if v == 0.0 { 0.0 } else { v };
            format!("{v}")
        };
        let vec = |v: [f64; 3]| format!("{} {} {}", num(v[0]), num(v[1]), num(v[2]));
        let mut b = hp_kv::Block::new("grid");
        b.push_prop("name", self.name.clone());
        b.push_prop("step", vec(self.step));
        b.push_prop("origin", vec(self.origin));
        b.push_prop("angles", vec(self.angles));
        b.push_prop("major", self.major_every.to_string());
        if let Some([min, max]) = self.bounds {
            b.push_prop("mins", vec(min));
            b.push_prop("maxs", vec(max));
        }
        if let Some(c) = self.color {
            b.push_prop("color", format!("{} {} {}", c[0], c[1], c[2]));
        }
        b
    }

    pub fn from_block(b: &hp_kv::Block) -> Option<GridSpec> {
        let vec = |key: &str| -> Option<[f64; 3]> {
            let text = b.get(key)?;
            let parts: Vec<f64> = text.split_whitespace().filter_map(|p| p.parse::<f64>().ok()).collect();
            (parts.len() == 3).then(|| [parts[0], parts[1], parts[2]])
        };
        let defaults = GridSpec::default();
        let step = match vec("step") {
            Some(s) => s,
            None => [b.get("step").and_then(|t| t.trim().parse().ok()).unwrap_or(16.0); 3],
        };
        let color = b.get("color").and_then(|t| {
            let parts: Vec<u8> = t.split_whitespace().filter_map(|p| p.parse::<u8>().ok()).collect();
            (parts.len() == 3).then(|| [parts[0], parts[1], parts[2]])
        });
        let spec = GridSpec {
            name: b.get("name").unwrap_or_default().to_owned(),
            step,
            origin: vec("origin").unwrap_or(defaults.origin),
            angles: vec("angles").unwrap_or(defaults.angles),
            major_every: b.get("major").and_then(|t| t.trim().parse().ok()).unwrap_or(0),
            bounds: vec("mins").zip(vec("maxs")).map(|(a, b)| [a, b]),
            color,
        }
        .sanitized();
        (!spec.name.is_empty()).then_some(spec)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GridSettings {
    pub mode: GridMode,
    pub custom: String,
    pub per_view: PerViewGrids,
    pub presets: Vec<GridSpec>,
    pub object_grids: bool,
    pub plane_3d: bool,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            mode: GridMode::Global,
            custom: String::new(),
            per_view: PerViewGrids::default(),
            presets: Vec::new(),
            object_grids: true,
            plane_3d: true,
        }
    }
}

impl GridSettings {
    pub fn preset(&self, name: &str) -> Option<&GridSpec> {
        self.presets.iter().find(|g| g.name.eq_ignore_ascii_case(name.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::workflow::GRID_MAX;
    use super::*;

    #[test]
    fn old_files_load_as_global_and_round_trip() {
        let s: GridSettings = toml::from_str("").unwrap();
        assert_eq!(s, GridSettings::default());
        assert_eq!(s.mode, GridMode::Global);
        let mut s = GridSettings { mode: GridMode::PerView, ..GridSettings::default() };
        s.per_view.top = ViewGrid { size: 12.0, origin: [4.0, 0.0, 0.0], major_every: 4, angle: -30.0 };
        s.presets.push(GridSpec {
            name: "Stairs".into(),
            step: [12.0, 12.0, 8.0],
            angles: [0.0, 30.0, 0.0],
            bounds: Some([[0.0; 3], [256.0; 3]]),
            color: Some([255, 128, 0]),
            ..GridSpec::default()
        });
        s.presets.push(GridSpec::uniform("Fine", 1.5));
        let text = toml::to_string_pretty(&s).unwrap();
        let back: GridSettings = toml::from_str(&text).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.preset("stairs").unwrap().step, [12.0, 12.0, 8.0]);
    }

    #[test]
    fn modes_by_name() {
        for m in GridMode::ALL {
            assert_eq!(GridMode::from_name(m.name()), Some(m));
        }
        assert_eq!(GridMode::from_name("Per View"), Some(GridMode::PerView));
        assert_eq!(GridMode::from_name("perview"), Some(GridMode::PerView));
        assert_eq!(GridMode::from_name("nope"), None);
    }

    #[test]
    fn specs_are_sanitized_and_survive_the_vmf_block() {
        let bad = GridSpec {
            name: " a\"b ".into(),
            step: [0.0, f64::NAN, 1e9],
            origin: [f64::INFINITY, -1e9, -0.0],
            angles: [725.0, f64::NAN, -30.0],
            major_every: 1000,
            bounds: Some([[10.0, 10.0, 10.0], [10.0, 20.0, 20.0]]),
            color: None,
        }
        .sanitized();
        assert_eq!(bad.name, "ab");
        assert_eq!(bad.step, [16.0, 16.0, GRID_MAX]);
        assert_eq!(bad.origin, [0.0, -GRID_COORD_LIMIT, 0.0]);
        assert_eq!(bad.angles, [5.0, 0.0, -30.0]);
        assert_eq!(bad.major_every, MAX_MAJOR_EVERY);
        assert_eq!(bad.bounds, None, "a flat box is no limit");
        let spec = GridSpec {
            name: "Roof".into(),
            step: [12.0, 24.0, 1.5],
            origin: [8.0, -4.0, 64.0],
            angles: [15.0, 45.0, 0.0],
            major_every: 4,
            bounds: Some([[256.0, 0.0, 0.0], [-256.0, 512.0, 128.0]]),
            color: Some([10, 200, 30]),
        }
        .sanitized();
        assert_eq!(spec.bounds, Some([[-256.0, 0.0, 0.0], [256.0, 512.0, 128.0]]), "min and max sorted");
        let back = GridSpec::from_block(&spec.to_block()).unwrap();
        assert_eq!(back, spec);
        assert!(!spec.is_uniform() && GridSpec::uniform("u", 8.0).is_uniform());
        assert!(GridSpec::from_block(&GridSpec::default().to_block()).is_none(), "no name");
    }
}
