use std::sync::{Arc, mpsc};
use std::time::Duration;

use egui::{RichText, Ui};
use hp_app::settings::ModelLodSettings;
use hp_app::{AppState, t, tf};
use hp_editor::prop_export::PropGeometry;

use super::{prop_lod_plan, prop_lods};

pub const GAME_SCREEN_HEIGHT: f64 = 1080.0;
pub const GAME_FOV: f64 = 75.0;
const DEBOUNCE: f64 = 0.25;

#[derive(Clone, Debug)]
pub struct LodLevelPreview {
    pub index: usize,
    pub switch: f64,
    pub triangles: usize,
    pub target: f64,
    pub geometry: Arc<PropGeometry>,
}

impl LodLevelPreview {
    pub fn percent_of(&self, full: usize) -> f64 {
        if full == 0 { 0.0 } else { self.triangles as f64 * 100.0 / full as f64 }
    }

    pub fn short_of_target(&self, full: usize) -> bool {
        self.index > 0 && self.percent_of(full) > self.target + 5.0
    }

    pub fn game_distance(&self) -> f64 {
        distance_for_metric(self.switch)
    }
}

pub fn levels(geometry: &Arc<PropGeometry>, lod: ModelLodSettings) -> Vec<LodLevelPreview> {
    let mut out = vec![LodLevelPreview {
        index: 0,
        switch: 0.0,
        triangles: geometry.tris.len(),
        target: 100.0,
        geometry: Arc::clone(geometry),
    }];
    let plan = prop_lod_plan(&geometry.tris, lod);
    for (level, planned) in
        prop_lods(&geometry.tris, lod).into_iter().zip(plan).filter(|(level, _)| !level.tris.is_empty())
    {
        let triangles = level.tris.len();
        out.push(LodLevelPreview {
            index: out.len(),
            switch: level.switch,
            triangles,
            target: planned.fraction * 100.0,
            geometry: Arc::new(PropGeometry {
                tris: level.tris,
                disps: Vec::new(),
                pieces: Vec::new(),
                origin: geometry.origin,
            }),
        });
    }
    out
}

fn tan_half_fov_y() -> f64 {
    (GAME_FOV.to_radians() * 0.5).tan() * 0.75
}

pub fn game_metric(distance: f64) -> f64 {
    if distance.is_finite() { 100.0 * distance.max(0.0) * tan_half_fov_y() / GAME_SCREEN_HEIGHT } else { 0.0 }
}

pub fn distance_for_metric(metric: f64) -> f64 {
    if metric.is_finite() { metric.max(0.0) * GAME_SCREEN_HEIGHT / (100.0 * tan_half_fov_y()) } else { 0.0 }
}

pub fn lod_for_metric(levels: &[LodLevelPreview], metric: f64) -> usize {
    let last = levels.len().saturating_sub(1);
    (0..last).find(|&index| levels[index + 1].switch > metric).unwrap_or(last)
}

type Key = (usize, ModelLodSettings);

#[derive(Default)]
pub(super) struct LodPreview {
    levels: Vec<LodLevelPreview>,
    built: Option<Key>,
    job: Option<(Key, mpsc::Receiver<Vec<LodLevelPreview>>)>,
    wanted: Option<(Key, f64)>,
}

impl LodPreview {
    pub fn update(&mut self, ctx: &egui::Context, geometry: &Arc<PropGeometry>, lod: ModelLodSettings) {
        let key = (Arc::as_ptr(geometry) as usize, lod.clamped());
        if let Some((job_key, receiver)) = &self.job {
            match receiver.try_recv() {
                Ok(levels) => {
                    self.built = Some(*job_key);
                    self.levels = levels;
                    self.job = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.job = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.levels.first().is_none_or(|first| !Arc::ptr_eq(&first.geometry, geometry)) {
            self.levels = levels(geometry, ModelLodSettings { enabled: false, ..lod });
            self.built = None;
        }
        if self.built == Some(key) {
            self.wanted = None;
            return;
        }
        let now = ctx.input(|i| i.time);
        if self.wanted.is_none_or(|(wanted, _)| wanted != key) {
            self.wanted = Some((key, now));
        }
        if self.job.is_some() {
            return;
        }
        let waited = self.wanted.map_or(0.0, |(_, since)| now - since);
        if self.built.is_none() || waited >= DEBOUNCE {
            let (tx, rx) = mpsc::channel();
            let geometry = Arc::clone(geometry);
            let repaint = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(levels(&geometry, lod));
                repaint.request_repaint();
            });
            self.job = Some((key, rx));
        } else {
            ctx.request_repaint_after(Duration::from_secs_f64((DEBOUNCE - waited).max(0.01)));
        }
    }

    pub fn busy(&self) -> bool {
        self.wanted.is_some()
    }

    pub fn levels(&self) -> &[LodLevelPreview] {
        &self.levels
    }

    pub fn shown(&self, state: &AppState, camera: f64) -> usize {
        if state.settings.lab.propper_lod_auto {
            lod_for_metric(&self.levels, game_metric(camera))
        } else {
            state.runtime.lab.propper_lod_view.min(self.levels.len().saturating_sub(1))
        }
    }

    pub fn bar(&self, ui: &mut Ui, state: &mut AppState, camera: f64) -> usize {
        let shown = self.shown(state, camera);
        let full = self.levels.first().map_or(0, |level| level.triangles);
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("propper.lod_preview"));
            for level in &self.levels {
                let tip = if level.index == 0 {
                    tf!("propper.lod_full_tip", level.triangles)
                } else {
                    tf!(
                        "propper.lod_level_tip",
                        level.triangles,
                        format!("{:.0}", level.percent_of(full)),
                        format!("{}", level.switch),
                        format!("{:.0}", level.game_distance())
                    )
                };
                if ui
                    .selectable_label(shown == level.index, format!("LOD {}", level.index))
                    .on_hover_text(tip)
                    .clicked()
                {
                    state.runtime.lab.propper_lod_view = level.index;
                    if state.settings.lab.propper_lod_auto {
                        state.settings.lab.propper_lod_auto = false;
                        state.settings_dirty = true;
                    }
                }
            }
            if ui
                .checkbox(&mut state.settings.lab.propper_lod_auto, t!("propper.lod_auto"))
                .on_hover_text(t!("propper.lod_auto_tip"))
                .changed()
            {
                state.settings_dirty = true;
            }
            if self.busy() {
                hp_app::widgets::paced_spinner(ui, 14.0);
            }
        });
        let level = &self.levels[shown.min(self.levels.len().saturating_sub(1))];
        let mut text = if level.index == 0 {
            tf!("propper.lod_full", level.triangles)
        } else {
            tf!(
                "propper.lod_summary",
                level.index,
                level.triangles,
                format!("{:.0}", level.percent_of(full)),
                format!("{}", level.switch)
            )
        };
        if level.short_of_target(full) {
            text.push(' ');
            text.push_str(&tf!("propper.lod_short", format!("{:.0}", level.target)));
        }
        if self.levels.len() == 1 && !state.settings.lab.model_lod.enabled {
            text.push(' ');
            text.push_str(t!("propper.lod_off"));
        }
        if state.settings.lab.propper_lod_auto {
            text.push(' ');
            text.push_str(&tf!("propper.lod_camera", format!("{camera:.0}"), level.index));
        }
        ui.label(RichText::new(text).small());
        state.runtime.lab.propper_lod_view = level.index;
        state.runtime.lab.propper_lod_levels =
            self.levels.iter().map(|level| (level.triangles, level.switch)).collect();
        state.runtime.lab.propper_lod_camera = Some(camera);
        level.index
    }

    pub fn geometry(&self, index: usize) -> Option<&Arc<PropGeometry>> {
        self.levels.get(index).map(|level| &level.geometry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;
    use hp_editor::prop_export::PropTri;

    fn sphere() -> Arc<PropGeometry> {
        let mut tris = Vec::new();
        let (rings, segments) = (24, 48);
        let point = |ring: usize, segment: usize| {
            let theta = std::f64::consts::PI * ring as f64 / rings as f64;
            let phi = std::f64::consts::TAU * segment as f64 / segments as f64;
            DVec3::new(theta.sin() * phi.cos(), theta.sin() * phi.sin(), theta.cos()) * 64.0
        };
        for ring in 0..rings {
            for segment in 0..segments {
                let quad = [
                    point(ring, segment),
                    point(ring + 1, segment),
                    point(ring + 1, segment + 1),
                    point(ring, segment + 1),
                ];
                for pos in [[quad[0], quad[1], quad[2]], [quad[0], quad[2], quad[3]]] {
                    let normal = pos.map(|p| p.normalize_or_zero());
                    tris.push(PropTri {
                        material: "dev/dev_measuregeneric01b".into(),
                        pos,
                        normal,
                        uv: [[0.0, 0.0]; 3],
                        disp: None,
                    });
                }
            }
        }
        Arc::new(PropGeometry { tris, disps: Vec::new(), pieces: Vec::new(), origin: DVec3::ZERO })
    }

    #[test]
    fn preview_levels_are_the_levels_that_get_compiled() {
        let geometry = sphere();
        let settings = ModelLodSettings { reduction: 0.5, levels: 3, ..Default::default() };
        let preview = levels(&geometry, settings);
        let compiled = prop_lods(&geometry.tris, settings);
        assert_eq!(preview.len(), compiled.len() + 1);
        assert_eq!(preview[0].triangles, geometry.tris.len());
        assert_eq!(preview[1].target, 50.0);
        for (level, lod) in preview[1..].iter().zip(&compiled) {
            assert_eq!(level.triangles, lod.tris.len());
            assert_eq!(level.geometry.tris, lod.tris);
            assert_eq!(level.switch, lod.switch);
            assert!(!level.short_of_target(geometry.tris.len()), "{} for {}", level.triangles, level.target);
        }
        assert!(preview.windows(2).all(|pair| pair[1].triangles < pair[0].triangles));
        let gentler = levels(&geometry, ModelLodSettings { reduction: 0.8, levels: 3, ..Default::default() });
        assert!(gentler[1].triangles > preview[1].triangles);
        assert_eq!(levels(&geometry, ModelLodSettings { enabled: false, ..settings }).len(), 1);
    }

    #[test]
    fn distance_picks_the_level_like_the_engine() {
        let geometry = sphere();
        let preview = levels(&geometry, ModelLodSettings::default());
        assert_eq!(lod_for_metric(&preview, 0.0), 0);
        for level in &preview[1..] {
            let at = level.game_distance();
            assert!((game_metric(at) - level.switch).abs() < 1e-6);
            assert_eq!(lod_for_metric(&preview, game_metric(at * 1.01)), level.index);
            assert_eq!(lod_for_metric(&preview, game_metric(at * 0.99)), level.index - 1);
        }
        assert_eq!(lod_for_metric(&preview, 1e12), preview.len() - 1);
        assert!((game_metric(1000.0) - 100.0 * 1000.0 * 0.575_45 / 1080.0).abs() < 0.01);
    }
}
