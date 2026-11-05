use std::collections::HashMap;
use std::sync::Arc;

use hp_app::AppState;
use hp_colorgrade::{Contribution, Fader, Zone, ZoneReport};

use crate::media;

const CLASS_POINT: &str = "color_correction";
const CLASS_VOLUME: &str = "color_correction_volume";

#[derive(Clone, Debug, PartialEq)]
enum Applied {
    None,
    Preview(usize),
    Zones(Vec<(u32, u16)>, u64),
}

pub struct ZoneDriver {
    zones: Vec<Zone>,
    classes: Vec<&'static str>,
    zones_key: Option<(u64, u64)>,
    fader: Fader,
    luts: HashMap<String, Option<Arc<Vec<u8>>>>,
    luts_key: Option<(u64, u64)>,
    applied: Applied,
}

impl Default for ZoneDriver {
    fn default() -> Self {
        Self {
            zones: Vec::new(),
            classes: Vec::new(),
            zones_key: None,
            fader: Fader::default(),
            luts: HashMap::new(),
            luts_key: None,
            applied: Applied::None,
        }
    }
}

pub fn collect_zones(state: &AppState) -> Vec<(Zone, &'static str)> {
    let scene = state.doc.scene();
    let mut out = Vec::new();
    for (id, node) in scene.entities() {
        let e = &node.entity;
        let get = |k: &str| e.get(k).map(str::to_owned);
        if e.classname.eq_ignore_ascii_case(CLASS_POINT) {
            let origin = e.origin().unwrap_or_default();
            out.push((Zone::point(*id, origin.to_array(), &get), CLASS_POINT));
        } else if e.classname.eq_ignore_ascii_case(CLASS_VOLUME)
            && let Some(b) = scene.entity_solid_bounds(*id)
        {
            out.push((Zone::volume(*id, b.min.to_array(), b.max.to_array(), &get), CLASS_VOLUME));
        }
    }
    out.sort_by_key(|(z, _)| z.id);
    out
}

impl ZoneDriver {
    pub fn update(&mut self, state: &mut AppState, ctx: &egui::Context) {
        let preview = state.runtime.lab.sound_color.preview_lut.clone();
        if let Some(lut) = preview {
            let ptr = Arc::as_ptr(&lut) as usize;
            if self.applied != Applied::Preview(ptr) {
                if let Some(gfx) = state.gfx.as_mut() {
                    gfx.renderer.set_color_zone(Some(&lut));
                }
                self.applied = Applied::Preview(ptr);
                state.runtime.lab.sound_color.zone_applied = true;
            }
            return;
        }
        if !state.settings.lab.color_correction_preview {
            if self.applied != Applied::None {
                if let Some(gfx) = state.gfx.as_mut() {
                    gfx.renderer.set_color_zone(None);
                }
                self.applied = Applied::None;
                let rt = &mut state.runtime.lab.sound_color;
                rt.zone_applied = false;
                rt.zones.clear();
            }
            return;
        }
        self.refresh_zones(state);
        let eye = state.cameras.cam3d.pos.to_array();
        let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1);
        let serial = state.runtime.lab.sound_color.lut_serial;
        let luts_key = (state.assets.generation(), serial);
        if self.luts_key != Some(luts_key) {
            self.luts.clear();
            self.luts_key = Some(luts_key);
        }
        let mut contributions = Vec::new();
        let mut reports = Vec::new();
        for (zone, class) in self.zones.iter().zip(&self.classes) {
            let fade = self.fader.step(zone, eye, dt);
            let falloff = zone.falloff(eye);
            let weight = fade * zone.max_weight * falloff;
            if !self.luts.contains_key(&zone.filename) && !zone.filename.is_empty() {
                let bytes = media::read_lut(state, &zone.filename).map(Arc::new);
                self.luts.insert(zone.filename.clone(), bytes);
            }
            let loaded = self.luts.get(&zone.filename).is_some_and(Option::is_some);
            let status = if zone.filename.is_empty() {
                "no_file"
            } else if !loaded {
                "file_not_found"
            } else if !zone.enabled {
                "disabled"
            } else if !zone.inside(eye) {
                "outside_volume"
            } else if weight <= 0.0 {
                "out_of_range"
            } else {
                "active"
            };
            if loaded {
                contributions.push(Contribution {
                    id: zone.id,
                    filename: zone.filename.clone(),
                    weight,
                    exclusive: zone.exclusive,
                });
            }
            reports.push(ZoneReport {
                id: zone.id,
                name: zone.name.clone(),
                class: (*class).to_owned(),
                filename: zone.filename.clone(),
                distance: zone.distance(eye),
                falloff,
                fade,
                weight,
                exclusive: zone.exclusive,
                enabled: zone.enabled,
                inside: zone.inside(eye),
                applied: false,
                status: status.to_owned(),
            });
        }
        let live = hp_colorgrade::resolve(&contributions);
        for r in &mut reports {
            if live.iter().any(|c| c.id == r.id) {
                r.applied = true;
            } else if r.status == "active" {
                r.status = "excluded".to_owned();
            }
        }
        let key: Vec<(u32, u16)> =
            live.iter().map(|c| (c.id, (c.weight.clamp(0.0, 1.0) * 255.0).round() as u16)).collect();
        let want = Applied::Zones(key, luts_key.0 ^ luts_key.1.rotate_left(17));
        if self.applied != want {
            let blended = {
                let refs: Vec<(&[u8], f32)> = live
                    .iter()
                    .filter_map(|c| {
                        self.luts.get(&c.filename).and_then(|l| l.as_ref()).map(|l| (l.as_slice(), c.weight))
                    })
                    .collect();
                hp_colorgrade::blend_rgba(&refs)
            };
            if let Some(gfx) = state.gfx.as_mut() {
                gfx.renderer.set_color_zone(blended.as_deref());
            }
            self.applied = want;
        }
        let rt = &mut state.runtime.lab.sound_color;
        rt.zone_applied = !live.is_empty();
        rt.eye = eye;
        if rt.zones != reports {
            rt.zones = reports;
        }

        if !self.fader.settled(&self.zones, eye) {
            ctx.request_repaint();
        }
    }

    fn refresh_zones(&mut self, state: &AppState) {
        let key = (state.doc.revision(), state.assets.generation());
        if self.zones_key == Some(key) {
            return;
        }
        self.zones_key = Some(key);
        let (zones, classes): (Vec<Zone>, Vec<&'static str>) = collect_zones(state).into_iter().unzip();
        self.zones = zones;
        self.classes = classes;
        self.fader.retain(&self.zones);
    }
}
