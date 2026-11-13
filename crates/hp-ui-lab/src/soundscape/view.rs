use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::Align2;
use glam::{DVec2, DVec3};
use hp_app::view::{ViewInfo, ViewKind};
use hp_app::{AppState, t, tf};
use hp_render::{Overlay, OverlayLine, OverlayPoint, OverlayTri};
use hp_soundscape::rules::{Emitter, EmitterKind, decide};
use hp_ui_viewports::layers::OverlayLayer;
use hp_ui_viewports::paint::{overlay_color, text_pill};

use super::world::{self, AMBIENT_COLOR, Ambient, Trigger, color_for};

const SEGMENTS: usize = 64;
const MAX_CELLS: usize = 120 * 120;
const BUDGET: Duration = Duration::from_millis(4);
const EAR: f64 = 64.0;
const FLOOR_REACH: f64 = 4096.0;
const INFINITE_SPAN: f64 = 4096.0;
const HERE_EVERY: Duration = Duration::from_millis(200);

fn f(v: DVec3) -> [f32; 3] {
    v.as_vec3().to_array()
}

fn with_alpha(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Cell {
    floor: f64,
    owner: Option<usize>,
}

#[derive(Default)]
struct AreaMap {
    key: Option<(u64, i64)>,
    min: DVec2,
    cell: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Option<Cell>>,
    next: usize,
    version: u64,
}

impl AreaMap {
    fn reset(&mut self, key: (u64, i64), emitters: &[Emitter]) {
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        for e in emitters.iter().filter(|e| e.enabled && !e.soundscape.is_empty()) {
            let r = if e.infinite() { INFINITE_SPAN } else { e.radius.max(0.0) };
            let o = DVec2::new(e.origin[0], e.origin[1]);
            lo = lo.min(o - DVec2::splat(r));
            hi = hi.max(o + DVec2::splat(r));
        }
        self.key = Some(key);
        self.next = 0;
        self.version = self.version.wrapping_add(1);
        if !lo.is_finite() || !hi.is_finite() {
            self.cells.clear();
            self.nx = 0;
            self.ny = 0;
            return;
        }
        let span = (hi - lo).max(DVec2::splat(1.0)).min(DVec2::splat(65536.0));
        let mut cell = 32.0_f64;
        while ((span.x / cell).ceil() * (span.y / cell).ceil()) as usize > MAX_CELLS {
            cell *= 2.0;
        }
        self.cell = cell;
        self.min = (lo / cell).floor() * cell;
        self.nx = ((hi.x - self.min.x) / cell).ceil().max(1.0) as usize;
        self.ny = ((hi.y - self.min.y) / cell).ceil().max(1.0) as usize;
        self.cells = vec![None; self.nx * self.ny];
    }

    fn done(&self) -> bool {
        self.next >= self.cells.len()
    }

    fn step(&mut self, state: &AppState, emitters: &[Emitter], triggers: &[Trigger], slice: f64) {
        if self.done() {
            return;
        }
        let began = Instant::now();
        let sight = world::sight_fn(state);
        while self.next < self.cells.len() {
            let i = self.next;
            self.next += 1;
            let (x, y) = (i % self.nx, i / self.nx);
            let c = self.min + DVec2::new((x as f64 + 0.5) * self.cell, (y as f64 + 0.5) * self.cell);
            let top = DVec3::new(c.x, c.y, slice);
            let Some(floor) = world::floor_below(state, top, FLOOR_REACH) else { continue };
            let ear = DVec3::new(c.x, c.y, floor + EAR);
            let d = decide(emitters, ear.to_array(), None, |e, to| {
                sight(DVec3::from_array(e.origin), DVec3::from_array(to))
            });
            let feet = DVec3::new(c.x, c.y, floor);
            let owner = d.active.or_else(|| trigger_owner(emitters, triggers, feet));
            self.cells[i] = Some(Cell { floor, owner });
            if i.is_multiple_of(16) && began.elapsed() > BUDGET {
                break;
            }
        }
        self.version = self.version.wrapping_add(1);
    }
}

fn trigger_owner(emitters: &[Emitter], triggers: &[Trigger], feet: DVec3) -> Option<usize> {
    let (min, max) = world::player_box(feet, false);
    triggers
        .iter()
        .filter(|t| world::box_touches(t, min, max))
        .find_map(|t| t.triggerable.and_then(|id| emitters.iter().position(|e| e.id == id)))
}

#[derive(Clone, Debug, PartialEq)]
struct Here {
    name: String,
    id: u32,
    kept: bool,
    trigger: bool,
}

type LayerData = (u64, Arc<Vec<Emitter>>, Arc<Vec<Ambient>>, Arc<Vec<Trigger>>);

#[derive(Default)]
pub struct SoundscapeLayer {
    data: Option<LayerData>,
    area: AreaMap,
    here: Option<Here>,
    here_prev: Option<u32>,
    here_at: Option<Instant>,
    on: bool,
}

pub fn ear_of(state: &AppState) -> DVec3 {
    let p = &state.runtime.lab.player;
    if p.active { hp_ui_viewports::player::eye(p) } else { state.cameras.cam3d.pos }
}

fn circle(out: &mut Vec<OverlayLine>, center: DVec3, radius: f64, a: DVec3, b: DVec3, color: [f32; 4]) {
    let mut prev = center + a * radius;
    for i in 1..=SEGMENTS {
        let t = i as f64 / SEGMENTS as f64 * std::f64::consts::TAU;
        let p = center + (a * t.cos() + b * t.sin()) * radius;
        out.push(OverlayLine { a: f(prev), b: f(p), color });
        prev = p;
    }
}

fn view_axes(view: &ViewInfo) -> Option<(DVec3, DVec3)> {
    view.kind.axes_2d().map(|(r, u, _)| {
        let mut a = DVec3::ZERO;
        let mut b = DVec3::ZERO;
        a[r] = 1.0;
        b[u] = 1.0;
        (a, b)
    })
}

fn sphere(out: &mut Overlay, view: &ViewInfo, center: DVec3, radius: f64, color: [f32; 4]) {
    if radius <= 0.0 || !radius.is_finite() || !center.is_finite() {
        return;
    }
    match view_axes(view) {
        Some((a, b)) => circle(&mut out.lines, center, radius, a, b, color),
        None => {
            circle(&mut out.lines, center, radius, DVec3::X, DVec3::Y, color);
            let faint = with_alpha(color, color[3] * 0.45);
            circle(&mut out.lines, center, radius, DVec3::X, DVec3::Z, faint);
            circle(&mut out.lines, center, radius, DVec3::Y, DVec3::Z, faint);
        }
    }
}

impl SoundscapeLayer {
    fn refresh(&mut self, state: &AppState) {
        let rev = state.doc.revision();
        if self.data.as_ref().is_none_or(|d| d.0 != rev) {
            self.data = Some((
                rev,
                Arc::new(world::collect_emitters(state)),
                Arc::new(world::collect_ambients(state)),
                Arc::new(world::collect_triggers(state)),
            ));
        }
    }

    fn update_here(&mut self, state: &AppState, emitters: &[Emitter], triggers: &[Trigger]) {
        if state.runtime.lab.player.active && state.settings.lab.soundscape.hearing {
            self.here = state.runtime.lab.soundscape.active.as_ref().map(|a| Here {
                name: a.name.clone(),
                id: a.entity,
                kept: a.kept,
                trigger: a.by_trigger,
            });
            return;
        }
        if self.here_at.is_some_and(|t| t.elapsed() < HERE_EVERY) {
            return;
        }
        self.here_at = Some(Instant::now());
        let ear = ear_of(state);
        let previous = self.here_prev.and_then(|id| emitters.iter().position(|e| e.id == id));
        let sight = world::sight_fn(state);
        let d = decide(emitters, ear.to_array(), previous, |e, to| {
            sight(DVec3::from_array(e.origin), DVec3::from_array(to))
        });
        let feet = ear - DVec3::Z * hp_ui_viewports::player::EYE;
        let by_trigger =
            if d.kept_previous || d.active.is_none() { trigger_owner(emitters, triggers, feet) } else { None };
        let chosen = by_trigger.or(d.active).and_then(|i| emitters.get(i));
        self.here_prev = chosen.map(|e| e.id);
        self.here = chosen.map(|e| Here {
            name: e.soundscape.clone(),
            id: e.id,
            kept: d.kept_previous && by_trigger.is_none(),
            trigger: by_trigger.is_some(),
        });
    }
}

impl OverlayLayer for SoundscapeLayer {
    fn name(&self) -> &'static str {
        "soundscapes"
    }

    fn prepare(&mut self, state: &AppState) {
        let s = state.settings.lab.soundscape;
        self.on = s.view;
        if !s.view {
            if self.data.is_some() {
                self.data = None;
                self.area = AreaMap::default();
                self.here = None;
                self.here_prev = None;
            }
            return;
        }
        self.refresh(state);
        let Some((rev, emitters, _, triggers)) = self.data.clone() else { return };
        self.update_here(state, &emitters, &triggers);
        if !s.areas {
            if self.area.key.is_some() {
                self.area = AreaMap { version: self.area.version.wrapping_add(1), ..AreaMap::default() };
            }
            return;
        }
        let slice = ear_of(state).z;
        let key = (rev, (slice / 64.0).round() as i64);
        if self.area.key != Some(key) {
            self.area.reset(key, &emitters);
        }
        self.area.step(state, &emitters, &triggers, key.1 as f64 * 64.0);
    }

    fn add(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if !self.on {
            return;
        }
        let Some((_, emitters, ambients, triggers)) = &self.data else { return };
        let selected = |id: u32| state.doc.selection().objects.contains(&hp_editor::ObjRef::Entity(id));
        for e in emitters.iter() {
            let o = DVec3::from_array(e.origin);
            let base = color_for(&e.soundscape);
            let alpha = if !e.enabled {
                0.35
            } else if selected(e.id) {
                1.0
            } else {
                0.8
            };
            let color = with_alpha(base, alpha);
            if e.infinite() {
                hp_ui_viewports::overlay::cross_marker(out, view, o, 14.0, color);
            } else {
                sphere(out, view, o, e.radius, color);
                hp_ui_viewports::overlay::cross_marker(out, view, o, 6.0, color);
            }
            for (_, _, at) in &e.positions {
                if let Some(p) = at.map(DVec3::from_array) {
                    out.lines_on_top.push(OverlayLine { a: f(o), b: f(p), color: with_alpha(base, 0.7) });
                    out.points.push(OverlayPoint { pos: f(p), color: base, size_px: 7.0 });
                }
            }
            if let Some(main) = e.main.and_then(|m| emitters.iter().find(|x| x.id == m)) {
                out.lines_on_top.push(OverlayLine {
                    a: f(o),
                    b: f(DVec3::from_array(main.origin)),
                    color: with_alpha(base, 0.4),
                });
            }
        }
        for t in triggers.iter() {
            if t.bounds.is_empty() {
                continue;
            }
            let target = t.triggerable.and_then(|id| emitters.iter().find(|e| e.id == id));
            let base = target.map_or([0.6, 0.6, 0.6, 1.0], |e| color_for(&e.soundscape));
            let color = with_alpha(base, if t.enabled { 0.9 } else { 0.35 });
            hp_ui_viewports::overlay::box_lines_on_top(out, t.bounds.min, t.bounds.max, color);
            if let Some(e) = target {
                let c = (t.bounds.min + t.bounds.max) * 0.5;
                out.lines_on_top.push(OverlayLine {
                    a: f(c),
                    b: f(DVec3::from_array(e.origin)),
                    color: with_alpha(base, 0.5),
                });
            }
        }
        for a in ambients.iter() {
            let o = DVec3::from_array(a.origin);
            let alpha = if a.start_silent {
                0.35
            } else if selected(a.id) {
                1.0
            } else {
                0.7
            };
            let color = with_alpha(AMBIENT_COLOR, alpha);
            if a.everywhere {
                hp_ui_viewports::overlay::cross_marker(out, view, o, 10.0, color);
            } else {
                sphere(out, view, o, a.radius, color);
            }
        }
    }

    fn retained_version(&self, state: &AppState, view: &ViewInfo) -> Option<u64> {
        let tiles = self.on && state.settings.lab.soundscape.areas;
        let kind = match view.kind {
            ViewKind::Perspective => 1,
            ViewKind::Top => 2,
            _ => 3,
        };
        let rev = self.data.as_ref().map_or(0, |d| d.0);
        Some(if tiles {
            self.area.version.wrapping_mul(31).wrapping_add(rev.wrapping_mul(7)).wrapping_add(kind)
        } else {
            kind
        })
    }

    fn retained(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if !self.on
            || !state.settings.lab.soundscape.areas
            || !matches!(view.kind, ViewKind::Perspective | ViewKind::Top)
        {
            return;
        }
        let Some((_, emitters, _, _)) = &self.data else { return };
        let a = &self.area;
        let inset = a.cell * 0.06;
        let fill = if view.is_2d() { 0.30 } else { 0.16 };
        for (i, cell) in a.cells.iter().enumerate() {
            let Some(c) = cell else { continue };
            let color = match c.owner.and_then(|o| emitters.get(o)) {
                Some(e) => with_alpha(color_for(&e.soundscape), fill),
                None => [0.55, 0.55, 0.55, fill * 0.4],
            };
            let (x, y) = (i % a.nx, i / a.nx);
            let lo = a.min + DVec2::new(x as f64 * a.cell + inset, y as f64 * a.cell + inset);
            let hi = lo + DVec2::splat(a.cell - 2.0 * inset);
            let z = c.floor + 2.0;
            let p = |u: f64, v: f64| f(DVec3::new(u, v, z));
            out.triangles.push(OverlayTri { p: [p(lo.x, lo.y), p(hi.x, lo.y), p(hi.x, hi.y)], color });
            out.triangles.push(OverlayTri { p: [p(lo.x, lo.y), p(hi.x, hi.y), p(lo.x, hi.y)], color });
        }
    }

    fn paint(&self, _state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        if !self.on {
            return;
        }
        let Some((_, emitters, ambients, triggers)) = &self.data else { return };
        let area = view.rect.expand(40.0);
        let near = |p: DVec3| view.is_2d() || (p - view.cam3d.pos).length() < 6000.0;
        for e in emitters.iter() {
            let o = DVec3::from_array(e.origin);
            let Some(pos) = view.world_to_screen(o).filter(|p| area.contains(*p) && near(o)) else { continue };
            let name =
                if e.soundscape.is_empty() { t!("soundscape.view.no_name").to_owned() } else { e.soundscape.clone() };
            let radius =
                if e.infinite() { t!("soundscape.view.everywhere").to_owned() } else { format!("r {:.0}", e.radius) };
            let mut text = format!("{name}  {radius}");
            match e.kind {
                EmitterKind::Proxy => text.push_str(&tf!(
                    "soundscape.view.proxy_of",
                    e.main.map_or_else(|| "?".to_owned(), |m| m.to_string())
                )),
                EmitterKind::Triggerable => text.push_str(t!("soundscape.view.triggerable")),
                EmitterKind::Main => {}
            }
            if !e.enabled {
                text.push_str(t!("soundscape.view.disabled"));
            }
            text_pill(
                painter,
                pos + egui::vec2(0.0, -8.0),
                Align2::CENTER_BOTTOM,
                text,
                overlay_color(color_for(&e.soundscape)),
            );
            for (i, _, at) in &e.positions {
                if let Some(p) = at.map(DVec3::from_array)
                    && let Some(sp) = view.world_to_screen(p).filter(|s| area.contains(*s) && near(p))
                {
                    text_pill(
                        painter,
                        sp + egui::vec2(0.0, 6.0),
                        Align2::CENTER_TOP,
                        tf!("soundscape.view.position", i),
                        overlay_color(color_for(&e.soundscape)),
                    );
                }
            }
        }
        for t in triggers.iter() {
            if t.bounds.is_empty() {
                continue;
            }
            let c = (t.bounds.min + t.bounds.max) * 0.5;
            let Some(pos) = view.world_to_screen(c).filter(|p| area.contains(*p) && near(c)) else { continue };
            let target = t.triggerable.and_then(|id| emitters.iter().find(|e| e.id == id));
            let (text, color) = match target {
                Some(e) => (tf!("soundscape.view.trigger", e.soundscape), overlay_color(color_for(&e.soundscape))),
                None => (tf!("soundscape.view.trigger_missing", t.soundscape), hp_ui_viewports::paint::TEXT_WEAK),
            };
            let text = if t.enabled { text } else { format!("{text}{}", t!("soundscape.view.disabled")) };
            text_pill(painter, pos, Align2::CENTER_CENTER, text, color);
        }
        for a in ambients.iter() {
            let o = DVec3::from_array(a.origin);
            let Some(pos) = view.world_to_screen(o).filter(|p| area.contains(*p) && near(o)) else { continue };
            let file = a.message.rsplit(['/', '\\']).next().unwrap_or(&a.message);
            let mut text = if a.everywhere {
                format!("{file}  {}", t!("soundscape.view.everywhere"))
            } else {
                format!("{file}  r {:.0}", a.radius)
            };
            if a.start_silent {
                text.push_str(t!("soundscape.view.silent"));
            }
            text_pill(painter, pos + egui::vec2(0.0, 10.0), Align2::CENTER_TOP, text, overlay_color(AMBIENT_COLOR));
        }
        if !view.is_2d() {
            let text = match &self.here {
                Some(h) if h.trigger => tf!("soundscape.view.here_trigger", h.name, h.id),
                Some(h) if h.kept => tf!("soundscape.view.here_kept", h.name, h.id),
                Some(h) => tf!("soundscape.view.here", h.name, h.id),
                None => t!("soundscape.view.here_none").to_owned(),
            };
            let color = self.here.as_ref().map_or(hp_ui_viewports::paint::TEXT, |h| overlay_color(color_for(&h.name)));
            text_pill(painter, view.rect.left_bottom() + egui::vec2(8.0, -8.0), Align2::LEFT_BOTTOM, text, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emitter(x: f64, radius: f64) -> Emitter {
        Emitter {
            id: 1,
            kind: EmitterKind::Main,
            origin: [x, 0.0, 0.0],
            radius,
            soundscape: "a".to_owned(),
            enabled: true,
            targetname: String::new(),
            positions: Vec::new(),
            main: None,
        }
    }

    #[test]
    fn the_area_grid_covers_every_radius_within_its_cell_budget() {
        let mut a = AreaMap::default();
        a.reset((1, 0), &[emitter(0.0, 512.0), emitter(4000.0, 256.0)]);
        assert!(a.nx * a.ny <= MAX_CELLS);
        assert!(a.min.x <= -512.0 && a.min.x + a.nx as f64 * a.cell >= 4256.0);
        assert!(a.min.y <= -512.0 && a.min.y + a.ny as f64 * a.cell >= 512.0);
        let mut far = AreaMap::default();
        far.reset((1, 0), &[emitter(0.0, -1.0)]);
        assert!(far.nx * far.ny <= MAX_CELLS && far.cell >= 64.0);
        let mut none = AreaMap::default();
        none.reset((1, 0), &[]);
        assert!(none.cells.is_empty() && none.done());
    }

    #[test]
    fn the_layer_draws_radii_and_labels_only_when_turned_on() {
        let mut state = hp_app::testing::test_state();
        let kv = vec![("soundscape".to_owned(), "gm_t.a".to_owned()), ("radius".to_owned(), "300".to_owned())];
        state.doc.create_point_entity("env_soundscape", DVec3::new(0.0, 0.0, 4000.0), &kv).unwrap();
        let amb = vec![("message".to_owned(), "ambient/wind.wav".to_owned()), ("radius".to_owned(), "600".to_owned())];
        state.doc.create_point_entity("ambient_generic", DVec3::new(100.0, 0.0, 4000.0), &amb).unwrap();
        let mut layer = SoundscapeLayer::default();
        let view = ViewInfo {
            index: 0,
            kind: ViewKind::Top,
            rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0)),
            cam3d: hp_app::view::Camera3d::default(),
            cam2d: hp_app::view::Camera2d::default(),
            pixels_per_point: 1.0,
            grid: 16.0,
        };
        layer.prepare(&state);
        let mut out = Overlay::default();
        layer.add(&state, &view, &mut out);
        assert!(out.is_empty(), "off by default");
        state.settings.lab.soundscape.view = true;
        layer.prepare(&state);
        layer.add(&state, &view, &mut out);
        assert!(out.lines.len() >= 2 * SEGMENTS, "a circle for the soundscape and one for the ambient_generic");
        let mut tiles = Overlay::default();
        for _ in 0..200 {
            layer.prepare(&state);
            if layer.area.done() {
                break;
            }
        }
        assert!(layer.area.done());
        layer.retained(&state, &view, &mut tiles);
        let v1 = layer.retained_version(&state, &view);
        state.settings.lab.soundscape.areas = false;
        layer.prepare(&state);
        assert_ne!(layer.retained_version(&state, &view), v1);
        assert!(layer.here.is_none() || layer.here.as_ref().is_some_and(|h| h.name == "gm_t.a"));
    }
}
