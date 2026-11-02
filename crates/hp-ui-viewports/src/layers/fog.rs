use egui::{Align2, Color32, Pos2, Rect, vec2};
use glam::DVec3;
use hp_app::AppState;
use hp_app::i18n::TrTable;
use hp_app::runtime::fog::{self, Note};
use hp_app::tf;
use hp_app::view::ViewInfo;
use hp_editor::{Document, Id, ObjRef};
use hp_render::{Overlay, OverlayLine};

use super::OverlayLayer;
use crate::paint::{PILL_BG, TEXT, TEXT_WEAK, small_font};

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(FogLayer::default())]
}

pub(crate) const TR: TrTable = &[
    (
        "viewport.fog_global_off",
        "Fog preview: \"{0}\" has Fog Enable at No, no fog",
        "Aperçu du brouillard : \"{0}\" a Fog Enable à No, pas de brouillard",
    ),
    ("viewport.fog_unnamed", "env_fog_controller #{0}", "env_fog_controller n°{0}"),
];

const MAX_LABELS: usize = 24;
const FAR_LABEL: f64 = 6000.0;
const WARN: Color32 = Color32::from_rgb(255, 190, 70);
const LINK_FOG: [f32; 4] = [0.55, 0.75, 1.0, 1.0];
const LINK_POST: [f32; 4] = [1.0, 0.6, 0.9, 1.0];
const LINK_BAD: [f32; 4] = [1.0, 0.7, 0.25, 1.0];

#[derive(Clone, Debug)]
struct Entry {
    id: Id,
    min: DVec3,
    max: DVec3,
    title: String,
    swatch: [f32; 3],
    short: Vec<Note>,
    full: Vec<Note>,
    links: Vec<(Id, DVec3, [f32; 4])>,
}

impl Entry {
    fn center(&self) -> DVec3 {
        (self.min + self.max) * 0.5
    }

    fn anchor(&self) -> DVec3 {
        let c = self.center();
        DVec3::new(c.x, c.y, self.max.z)
    }
}

#[derive(Default)]
struct FogLayer {
    stamp: Option<u64>,
    on: bool,
    entries: Vec<Entry>,
    global_warning: Option<String>,
}

fn color32(c: [f32; 3]) -> Color32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(b(c[0]), b(c[1]), b(c[2]))
}

fn origin_of(doc: &Document, id: Id) -> Option<DVec3> {
    let scene = doc.scene();
    if let Some(b) = scene.entity_solid_bounds(id) {
        return Some(b.center());
    }
    scene.entity(id)?.entity.origin().filter(|p| p.is_finite())
}

fn volume_shown(doc: &Document, id: Id) -> bool {
    doc.scene().entity_solid_ids(id).iter().any(|s| doc.is_solid_visible(*s))
}

fn link_color(l: &fog::Link) -> [f32; 4] {
    if matches!(l, fog::Link::Found { .. }) { LINK_POST } else { LINK_BAD }
}

fn build_entries(doc: &Document) -> Vec<Entry> {
    let mut out = Vec::new();
    for id in fog::fog_volume_ids(doc) {
        if !volume_shown(doc, id) {
            continue;
        }
        let Some(v) = fog::resolve_volume(doc, id) else { continue };
        let Some((min, max)) = v.bounds else { continue };
        let name = doc.scene().entity(id).and_then(|n| n.entity.get("targetname")).map(str::trim).unwrap_or_default();
        let mut title = fog::FOG_VOLUME.to_owned();
        if !name.is_empty() {
            title.push_str(&format!(" \"{name}\""));
        }
        if let fog::FogSource::Controller { name, .. } = &v.source {
            title.push_str(&format!(" > {name}"));
        }
        let mut links = Vec::new();
        let fog_color = if matches!(v.source, fog::FogSource::Controller { .. }) { LINK_FOG } else { LINK_BAD };
        for (target, color) in [
            (v.source.controller(), fog_color),
            (v.post_process.target(), link_color(&v.post_process)),
            (v.color_correction.target(), link_color(&v.color_correction)),
        ] {
            if let Some(t) = target
                && let Some(p) = origin_of(doc, t)
            {
                links.push((t, p, color));
            }
        }
        out.push(Entry {
            id,
            min,
            max,
            title,
            swatch: v.preview.color,
            short: fog::volume_label_notes(&v),
            full: fog::volume_notes(&v),
            links,
        });
    }
    out
}

fn global_warning(state: &AppState) -> Option<String> {
    if !state.settings.render.fog_preview {
        return None;
    }
    let (id, e) = fog::global_controller(&state.doc, state.runtime.render.fog_controller)?;
    if fog::fog_enabled(e) {
        return None;
    }
    let name = e.get("targetname").map(str::trim).filter(|n| !n.is_empty());
    let name = name.map_or_else(|| tf!("viewport.fog_unnamed", id), str::to_owned);
    Some(tf!("viewport.fog_global_off", name))
}

fn selected(doc: &Document, id: Id) -> bool {
    doc.selection().contains(ObjRef::Entity(id))
        || doc.scene().entity_solid_ids(id).iter().any(|s| doc.is_solid_selected(*s))
}

fn f3(v: DVec3) -> [f32; 3] {
    v.as_vec3().to_array()
}

fn box_edges(min: DVec3, max: DVec3, color: [f32; 4], out: &mut Vec<OverlayLine>) {
    let c = |i: usize| {
        DVec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        )
    };
    for i in 0..8usize {
        for bit in [1usize, 2, 4] {
            if i & bit == 0 {
                out.push(OverlayLine { a: f3(c(i)), b: f3(c(i | bit)), color });
            }
        }
    }
}

fn label_size(painter: &egui::Painter, e: &Entry, full: bool) -> (Vec<std::sync::Arc<egui::Galley>>, egui::Vec2) {
    let font = small_font();
    let notes = if full { &e.full } else { &e.short };
    let mut galleys = vec![painter.layout_no_wrap(e.title.clone(), font.clone(), TEXT)];
    galleys.extend(
        notes
            .iter()
            .map(|n| painter.layout_no_wrap(n.text.clone(), font.clone(), if n.warning { WARN } else { TEXT_WEAK })),
    );
    let width =
        galleys.iter().enumerate().map(|(i, g)| g.size().x + if i == 0 { 16.0 } else { 0.0 }).fold(0.0, f32::max);
    let height = 15.0 * galleys.len() as f32 + 5.0;
    (galleys, vec2(width + 12.0, height))
}

fn paint_label(painter: &egui::Painter, rect: Rect, e: &Entry, galleys: Vec<std::sync::Arc<egui::Galley>>) {
    painter.rect_filled(rect, 4, PILL_BG);
    let mut y = rect.top() + 3.0;
    let sw = Rect::from_min_size(egui::pos2(rect.left() + 6.0, y + 2.5), vec2(10.0, 10.0));
    painter.rect_filled(sw, 2, color32(e.swatch));
    for (i, g) in galleys.into_iter().enumerate() {
        let x = if i == 0 { sw.right() + 6.0 } else { rect.left() + 6.0 };
        painter.galley(egui::pos2(x, y), g, TEXT);
        y += 15.0;
    }
}

fn place(bounds: Rect, anchor: Pos2, size: egui::Vec2, placed: &[Rect]) -> Option<Rect> {
    let fit = |r: Rect| {
        let dx = (bounds.left() - r.left()).max(0.0) + (bounds.right() - r.right()).min(0.0);
        let dy = (bounds.top() - r.top()).max(0.0) + (bounds.bottom() - r.bottom()).min(0.0);
        r.translate(vec2(dx, dy))
    };
    let above = Align2::CENTER_BOTTOM.anchor_size(anchor - vec2(0.0, 6.0), size);
    let below = Align2::CENTER_TOP.anchor_size(anchor + vec2(0.0, 6.0), size);
    let step = size.y + 2.0;
    (0..4)
        .flat_map(|k| [above.translate(vec2(0.0, -step * k as f32)), below.translate(vec2(0.0, step * k as f32))])
        .map(fit)
        .find(|r| bounds.contains_rect(*r) && !placed.iter().any(|p| p.intersects(*r)))
}

impl OverlayLayer for FogLayer {
    fn name(&self) -> &'static str {
        "fog_volumes"
    }

    fn prepare(&mut self, state: &AppState) {
        self.on = state.settings.render.fog_volume_preview;
        self.global_warning = global_warning(state);
        if !self.on {
            self.entries.clear();
            self.stamp = None;
            return;
        }
        let stamp = state.doc.revision();
        if self.stamp != Some(stamp) {
            self.entries = build_entries(&state.doc);
            self.stamp = Some(stamp);
        }
    }

    fn add(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let player = state.runtime.lab.player.active;
        for e in &self.entries {
            let sel = selected(&state.doc, e.id);
            if !view.is_2d() && !sel && !player {
                let [r, g, b] = e.swatch;
                let bright = |v: f32| (v * 0.6 + 0.4).min(1.0);
                box_edges(e.min, e.max, [bright(r), bright(g), bright(b), 0.9], &mut out.lines);
            }
            for (target, p, color) in &e.links {
                if sel || selected(&state.doc, *target) {
                    out.lines_on_top.push(OverlayLine { a: f3(e.center()), b: f3(*p), color: *color });
                }
            }
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        if view.is_2d() || state.runtime.lab.player.active {
            return;
        }
        if let Some(w) = &self.global_warning {
            crate::paint::text_pill(
                painter,
                view.rect.left_bottom() + vec2(64.0, -10.0),
                Align2::LEFT_BOTTOM,
                w.clone(),
                WARN,
            );
        }
        let eye = view.cam3d.pos;
        let forward = view.cam3d.forward();
        let mut shown: Vec<(bool, f64, &Entry, Pos2)> = self
            .entries
            .iter()
            .filter_map(|e| {
                let anchor = e.anchor();
                if (anchor - eye).dot(forward) <= 1.0 {
                    return None;
                }
                let sel = selected(&state.doc, e.id);
                let dist = anchor.distance(eye);
                if dist > FAR_LABEL && !sel {
                    return None;
                }
                let p = view.world_to_screen(anchor)?;
                view.rect.expand(40.0).contains(p).then_some((sel, dist, e, p))
            })
            .collect();
        shown.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.total_cmp(&b.1)));
        shown.truncate(MAX_LABELS);
        let clip = painter.with_clip_rect(view.rect.shrink(1.0));
        let mut placed: Vec<Rect> = Vec::new();
        for (sel, _, e, p) in shown {
            let (galleys, size) = label_size(&clip, e, sel);
            let rect = match place(view.rect.shrink(4.0), p, size, &placed) {
                Some(r) => r,
                None if sel => Align2::LEFT_TOP.anchor_size(view.rect.left_top() + vec2(4.0, 26.0), size),
                None => continue,
            };
            placed.push(rect);
            paint_label(&clip, rect, e, galleys);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_vmf::{Entity, Map, Side, Solid};

    fn fog_doc(state: &mut AppState) -> Id {
        let (lo, hi) = (DVec3::new(-64.0, -64.0, 0.0), DVec3::new(64.0, 64.0, 128.0));
        let p = DVec3::new;
        let planes = [
            [p(lo.x, hi.y, hi.z), p(hi.x, hi.y, hi.z), p(hi.x, lo.y, hi.z)],
            [p(lo.x, lo.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, hi.y, lo.z)],
            [p(lo.x, hi.y, hi.z), p(lo.x, lo.y, hi.z), p(lo.x, lo.y, lo.z)],
            [p(hi.x, hi.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, lo.y, hi.z)],
            [p(hi.x, hi.y, hi.z), p(lo.x, hi.y, hi.z), p(lo.x, hi.y, lo.z)],
            [p(hi.x, lo.y, lo.z), p(lo.x, lo.y, lo.z), p(lo.x, lo.y, hi.z)],
        ];
        let sides = planes
            .iter()
            .enumerate()
            .map(|(k, plane)| Side {
                id: 100 + k as Id,
                plane: *plane,
                material: "tools/toolsfog".to_owned(),
                ..Default::default()
            })
            .collect();
        let mut m = Map::new_empty();
        let mut v = Entity { id: 10, classname: "fog_volume".to_owned(), ..Default::default() };
        v.solids.push(Solid { id: 11, sides, ..Default::default() });
        m.entities.push(v);
        state.doc = Document::from_map(m);
        10
    }

    #[test]
    fn labels_follow_the_keyvalues() {
        hp_app::register_translations();
        let mut state = hp_app::testing::test_state();
        let id = fog_doc(&mut state);
        let mut layer = FogLayer::default();
        layer.prepare(&state);
        assert_eq!(layer.entries.len(), 1);
        assert!(layer.entries[0].short[0].warning, "a fresh volume says what is missing");
        assert!(layer.entries[0].full[0].warning);
        assert!(layer.entries[0].links.is_empty());
        let mut lines = Vec::new();
        box_edges(layer.entries[0].min, layer.entries[0].max, [1.0; 4], &mut lines);
        assert_eq!(lines.len(), 12);
        state
            .doc
            .create_point_entity(
                "env_fog_controller",
                DVec3::new(0.0, 0.0, 256.0),
                &[("targetname".to_owned(), "mist".to_owned()), ("fogenable".to_owned(), "1".to_owned())],
            )
            .expect("controller");
        state.doc.edit_entity(id, |e| e.set("FogName", "mist")).expect("edit");
        layer.prepare(&state);
        let e = &layer.entries[0];
        assert!(e.short.is_empty() && !e.full[0].warning, "{:?}", e.full);
        assert!(e.title.ends_with("> mist"), "{}", e.title);
        assert_eq!(e.links.len(), 1);
        assert_eq!(e.links[0].2, LINK_FOG);
        state.settings.render.fog_volume_preview = false;
        layer.prepare(&state);
        assert!(layer.entries.is_empty());
    }

    #[test]
    fn labels_do_not_overlap_and_stay_in_the_view() {
        let bounds = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(400.0, 300.0));
        let size = vec2(120.0, 40.0);
        let a = place(bounds, egui::pos2(200.0, 100.0), size, &[]).expect("first");
        assert_eq!(a.bottom(), 94.0);
        let b = place(bounds, egui::pos2(210.0, 100.0), size, &[a]).expect("second");
        assert!(!b.intersects(a) && bounds.contains_rect(b));
        let top = place(bounds, egui::pos2(390.0, 2.0), size, &[]).expect("clamped");
        assert!(bounds.contains_rect(top));
        assert!(place(bounds, egui::pos2(200.0, 150.0), vec2(500.0, 40.0), &[]).is_none(), "wider than the view");
    }

    #[test]
    fn the_global_preview_warns_when_fog_is_off() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.settings.render.fog_preview = true;
        assert!(global_warning(&state).is_none(), "no controller");
        let c = state
            .doc
            .create_point_entity("env_fog_controller", DVec3::ZERO, &[("targetname".to_owned(), "f".to_owned())])
            .expect("controller");
        assert!(global_warning(&state).is_some_and(|w| w.contains("\"f\"")));
        state.doc.edit_entity(c, |e| e.set("fogenable", "1")).expect("edit");
        assert!(global_warning(&state).is_none());
    }
}
