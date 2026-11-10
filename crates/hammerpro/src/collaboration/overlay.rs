use std::hash::{Hash, Hasher};
use std::time::Instant;

use egui::{Align2, Color32, Painter, Pos2, Stroke, vec2};
use glam::DVec3;
use hp_app::runtime::collaboration::Collaborator;
use hp_app::view::ViewInfo;
use hp_app::{AppState, t, tf};
use hp_editor::ObjRef;
use hp_render::{Overlay, OverlayLine};
use hp_ui_viewports::layers::OverlayLayer;
use hp_ui_viewports::paint::text_pill;

use super::people::{MARKER_LIFETIME, object_of};

pub(crate) const MAX_LINES: usize = 60_000;
const DETAILED_LINES: usize = 48;
const BOX_MARGIN: f64 = 3.0;
const CONFLICT_MARGIN: f64 = 6.0;
const HEADER: f32 = 20.0;

pub(crate) struct TeamLayer;

fn shown(state: &AppState) -> bool {
    let runtime = &state.runtime.collaboration;
    runtime.connected && runtime.active_map
}

fn rgba(color: [u8; 3], alpha: f32) -> [f32; 4] {
    [f32::from(color[0]) / 255.0, f32::from(color[1]) / 255.0, f32::from(color[2]) / 255.0, alpha]
}

fn color32(color: [u8; 3], alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(color[0], color[1], color[2], (alpha.clamp(0.0, 1.0) * 255.0) as u8)
}

fn line(out: &mut Overlay, a: DVec3, b: DVec3, color: [f32; 4]) {
    out.lines_on_top.push(OverlayLine { a: a.as_vec3().to_array(), b: b.as_vec3().to_array(), color });
}

fn push_box(out: &mut Overlay, min: DVec3, max: DVec3, color: [f32; 4]) {
    let corner = |index: usize| {
        DVec3::new(
            if index & 1 == 0 { min.x } else { max.x },
            if index & 2 == 0 { min.y } else { max.y },
            if index & 4 == 0 { min.z } else { max.z },
        )
    };
    for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
        line(out, corner(a), corner(b), color);
    }
}

fn push_solid(state: &AppState, out: &mut Overlay, id: u32, color: [f32; 4], budget: &mut usize) {
    let Some(node) = state.doc.scene().solid(id) else { return };
    let edges: usize = node.geom.faces.iter().map(|face| face.vertices.len()).sum();
    if edges > *budget {
        if *budget >= 12 && !node.bounds.is_empty() {
            push_box(out, node.bounds.min, node.bounds.max, color);
            *budget -= 12;
        }
        return;
    }
    *budget -= edges;
    for face in &node.geom.faces {
        let count = face.vertices.len();
        for index in 0..count {
            line(out, face.vertices[index], face.vertices[(index + 1) % count], color);
        }
    }
}

fn push_object(state: &AppState, out: &mut Overlay, key: &str, color: [f32; 4], budget: &mut usize) {
    match object_of(key) {
        Some(ObjRef::Solid(id)) => push_solid(state, out, id, color, budget),
        Some(ObjRef::Entity(id)) => {
            let solids = state.doc.scene().entity_solid_ids(id);
            if solids.is_empty() {
                let bounds = |entity: &hp_vmf::Entity| state.entity_bounds(entity);
                if *budget >= 12
                    && let Some(aabb) =
                        state.doc.scene().object_bounds(ObjRef::Entity(id), &bounds).filter(|aabb| !aabb.is_empty())
                {
                    push_box(out, aabb.min, aabb.max, color);
                    *budget -= 12;
                }
            }
            for solid in solids {
                push_solid(state, out, solid, color, budget);
            }
        }
        None => {}
    }
}

fn object_box(state: &AppState, key: &str) -> Option<(DVec3, DVec3)> {
    let bounds = |entity: &hp_vmf::Entity| state.entity_bounds(entity);
    let object = object_of(key)?;
    let aabb = state.doc.scene().object_bounds(object, &bounds).filter(|aabb| !aabb.is_empty())?;
    Some((aabb.min, aabb.max))
}

fn others(state: &AppState) -> impl Iterator<Item = &Collaborator> {
    let local = state.runtime.collaboration.local_id;
    state.runtime.collaboration.participants.iter().filter(move |peer| peer.id != local)
}

pub(crate) fn build(state: &AppState, out: &mut Overlay) {
    let runtime = &state.runtime.collaboration;
    let mut budget = MAX_LINES / 4;
    for highlight in &runtime.highlights {
        let margin = if highlight.conflict { CONFLICT_MARGIN } else { BOX_MARGIN };
        let color = rgba(highlight.color, 1.0);
        for key in &highlight.keys {
            if budget < 12 {
                break;
            }
            if let Some((min, max)) = object_box(state, key) {
                push_box(out, min - DVec3::splat(margin), max + DVec3::splat(margin), color);
                budget -= 12;
            }
        }
    }
    let peers: Vec<&Collaborator> = others(state).filter(|peer| !peer.selected.is_empty()).collect();
    let share = (MAX_LINES - MAX_LINES / 4 + budget) / peers.len().max(1);
    for peer in peers {
        let color = rgba(peer.color, 0.95);
        let mut own = share;
        let detailed = peer.selected.len() * DETAILED_LINES <= own;
        let mut rest: Option<(DVec3, DVec3)> = None;
        for key in &peer.selected {
            if own >= 24 {
                if detailed {
                    push_object(state, out, key, color, &mut own);
                } else if let Some((min, max)) = object_box(state, key) {
                    push_box(out, min, max, color);
                    own -= 12;
                }
            } else if let Some((min, max)) = object_box(state, key) {
                rest = Some(rest.map_or((min, max), |(low, high)| (low.min(min), high.max(max))));
            }
        }
        if let Some((min, max)) = rest {
            push_box(out, min - DVec3::splat(BOX_MARGIN), max + DVec3::splat(BOX_MARGIN), color);
        }
    }
}

fn camera_lines(view: &ViewInfo, peer: &Collaborator, out: &mut Overlay) {
    let position = DVec3::from_array(peer.position);
    let Some(forward) = DVec3::from_array(peer.direction).try_normalize() else { return };
    if !position.is_finite() {
        return;
    }
    let size = if view.is_2d() { 14.0 * view.units_per_point() } else { 20.0 };
    let side = forward.cross(DVec3::Z).try_normalize().unwrap_or(DVec3::X);
    let up = side.cross(forward).normalize_or(DVec3::Z);
    let center = position + forward * size * 1.4;
    let corners = [
        center + (side + up * 0.6) * size * 0.7,
        center + (side - up * 0.6) * size * 0.7,
        center + (-side - up * 0.6) * size * 0.7,
        center + (-side + up * 0.6) * size * 0.7,
    ];
    let color = rgba(peer.color, 1.0);
    for (index, corner) in corners.iter().enumerate() {
        line(out, position, *corner, color);
        line(out, *corner, corners[(index + 1) % 4], color);
    }
    line(out, corners[0] + up * size * 0.2, corners[3] + up * size * 0.2, color);
}

fn arrow_to(painter: &Painter, view: &ViewInfo, target: DVec3, color: Color32, label: String) {
    let rect = view.rect.shrink(28.0);
    let camera = view.cam3d;
    let local = target - camera.pos;
    let x = local.dot(camera.right());
    let y = local.dot(camera.up());
    let direction = vec2(x as f32, -(y as f32));
    let direction = if direction.length() < 1e-3 { vec2(0.0, 1.0) } else { direction.normalized() };
    let center = rect.center();
    let scale =
        (rect.width() * 0.5 / direction.x.abs().max(1e-3)).min(rect.height() * 0.5 / direction.y.abs().max(1e-3));
    let tip = center + direction * scale;
    let back = tip - direction * 14.0;
    let normal = vec2(-direction.y, direction.x) * 7.0;
    painter.add(egui::Shape::convex_polygon(vec![tip, back + normal, back - normal], color, Stroke::NONE));
    let anchor = if direction.x > 0.3 {
        Align2::RIGHT_CENTER
    } else if direction.x < -0.3 {
        Align2::LEFT_CENTER
    } else if direction.y > 0.0 {
        Align2::CENTER_BOTTOM
    } else {
        Align2::CENTER_TOP
    };
    text_pill(painter, back - direction * 6.0, anchor, label, color);
}

impl OverlayLayer for TeamLayer {
    fn name(&self) -> &'static str {
        "team_mapping"
    }

    fn retained_version(&self, state: &AppState, _view: &ViewInfo) -> Option<u64> {
        if !shown(state) {
            return None;
        }
        let runtime = &state.runtime.collaboration;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        runtime.overlay_serial.hash(&mut hasher);
        if !runtime.highlights.is_empty() || others(state).any(|peer| !peer.selected.is_empty()) {
            state.doc.revision().hash(&mut hasher);
        }
        Some(hasher.finish())
    }

    fn retained(&self, state: &AppState, _view: &ViewInfo, out: &mut Overlay) {
        if shown(state) {
            build(state, out);
        }
    }

    fn add(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if !shown(state) {
            return;
        }
        for peer in others(state) {
            camera_lines(view, peer, out);
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &Painter) {
        if !shown(state) {
            return;
        }
        let runtime = &state.runtime.collaboration;
        let painter = painter.with_clip_rect(view.rect);
        for peer in others(state) {
            let color = color32(peer.color, 1.0);
            if let Some(screen) = view.world_to_screen(DVec3::from_array(peer.position))
                && view.rect.contains(screen)
            {
                let mut text = format!("{} | {} ms", peer.name, peer.latency_ms);
                if peer.editing {
                    text = format!("{text} | {}", t!("team.editing"));
                }
                text_pill(&painter, screen + vec2(8.0, -10.0), Align2::LEFT_BOTTOM, text, color);
            }
            if let Some(center) = peer.selection_center
                && let Some(screen) = view.world_to_screen(DVec3::from_array(center))
                && view.rect.contains(screen)
            {
                text_pill(&painter, screen + vec2(0.0, -14.0), Align2::CENTER_BOTTOM, peer.name.clone(), color);
            }
        }
        for highlight in runtime.highlights.iter().filter(|highlight| highlight.conflict) {
            if let Some((min, max)) = highlight.keys.first().and_then(|key| object_box(state, key))
                && let Some(screen) = view.world_to_screen((min + max) * 0.5)
                && view.rect.contains(screen)
            {
                text_pill(
                    &painter,
                    screen,
                    Align2::CENTER_CENTER,
                    highlight.label.clone(),
                    color32(highlight.color, 1.0),
                );
            }
        }
        let now = Instant::now();
        let lifetime = MARKER_LIFETIME.as_secs_f32();
        for marker in &runtime.markers {
            let age = now.duration_since(marker.at).as_secs_f32();
            let fade = (1.0 - (age - lifetime + 1.5).max(0.0) / 1.5).clamp(0.0, 1.0);
            let color = color32(marker.color, fade);
            let position = DVec3::from_array(marker.position);
            let visible = view.world_to_screen(position).filter(|screen| view.rect.contains(*screen));
            match visible {
                Some(screen) => {
                    let pulse = (age * 1.5).fract();
                    painter.circle_stroke(
                        screen,
                        8.0 + pulse * 26.0,
                        Stroke::new(2.5, color32(marker.color, fade * (1.0 - pulse))),
                    );
                    painter.circle_filled(screen, 5.0, color);
                    painter.line_segment([screen, screen + vec2(0.0, -26.0)], Stroke::new(2.0, color));
                    text_pill(&painter, screen + vec2(0.0, -28.0), Align2::CENTER_BOTTOM, marker.name.clone(), color);
                }
                None if !view.is_2d() => arrow_to(&painter, view, position, color, marker.name.clone()),
                None => {}
            }
        }
        if !view.is_2d()
            && let Some(peer) = runtime.following.and_then(|id| runtime.participant(id))
        {
            let top = Pos2::new(view.rect.center().x, view.rect.top() + HEADER + 30.0);
            text_pill(
                &painter,
                top,
                Align2::CENTER_TOP,
                tf!("team.following_pill", peer.name),
                color32(peer.color, 1.0),
            );
        }
        let focused = state.ui.focused_view.unwrap_or(0) == view.index;
        if focused && let Some(lock) = runtime.locks.first() {
            let bottom = Pos2::new(view.rect.center().x, view.rect.bottom() - 34.0);
            text_pill(
                &painter,
                bottom,
                Align2::CENTER_BOTTOM,
                tf!("team.lock_pill", lock.name, lock.objects),
                color32(lock.color, 1.0),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use hp_app::runtime::collaboration::{Collaborator, Highlight, Marker};

    use super::*;

    fn state_with_peer(selected: Vec<String>) -> AppState {
        let mut state = hp_app::testing::test_state();
        let runtime = &mut state.runtime.collaboration;
        runtime.connected = true;
        runtime.active_map = true;
        runtime.local_id = 1;
        runtime.participants = vec![
            Collaborator { id: 1, name: "Host".into(), color: [255, 159, 28], ..Collaborator::default() },
            Collaborator {
                id: 2,
                name: "Guest".into(),
                color: [61, 184, 255],
                position: [0.0, 0.0, 64.0],
                direction: [1.0, 0.0, 0.0],
                selected,
                ..Collaborator::default()
            },
        ];
        state
    }

    fn view(kind: hp_app::ViewKind) -> ViewInfo {
        ViewInfo {
            index: 0,
            kind,
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
            cam3d: hp_app::view::Camera3d::default(),
            cam2d: hp_app::view::Camera2d::default(),
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    #[test]
    fn other_selections_are_outlined_in_their_colour_and_only_while_connected() {
        let solid = *hp_app::testing::test_state().doc.scene().solids().keys().next().unwrap();
        let mut state = state_with_peer(vec![format!("solid:{solid}")]);
        let layer = TeamLayer;
        for kind in [hp_app::ViewKind::Perspective, hp_app::ViewKind::Top] {
            let mut out = Overlay::default();
            layer.retained(&state, &view(kind), &mut out);
            assert!(!out.lines_on_top.is_empty());
            assert!(out.lines_on_top.iter().all(|line| line.color == rgba([61, 184, 255], 0.95)));
            let mut frame = Overlay::default();
            layer.add(&state, &view(kind), &mut frame);
            assert_eq!(frame.lines_on_top.len(), 9, "camera of the other participant");
        }
        let version = layer.retained_version(&state, &view(hp_app::ViewKind::Perspective));
        assert!(version.is_some());
        state.runtime.collaboration.overlay_serial += 1;
        assert_ne!(layer.retained_version(&state, &view(hp_app::ViewKind::Perspective)), version);
        state.runtime.collaboration.connected = false;
        assert_eq!(layer.retained_version(&state, &view(hp_app::ViewKind::Perspective)), None);
        let mut out = Overlay::default();
        layer.retained(&state, &view(hp_app::ViewKind::Perspective), &mut out);
        layer.add(&state, &view(hp_app::ViewKind::Perspective), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn huge_selections_and_highlights_stay_within_the_line_budget() {
        let mut state = state_with_peer(Vec::new());
        let solid = *state.doc.scene().solids().keys().next().unwrap();
        state.runtime.collaboration.participants[1].selected = vec![format!("solid:{solid}"); 20_000];
        state.runtime.collaboration.highlights.push(Highlight {
            author: 2,
            label: "Conflict".into(),
            color: [255, 72, 48],
            keys: vec![format!("solid:{solid}")],
            until: Instant::now() + Duration::from_secs(5),
            conflict: true,
        });
        let mut out = Overlay::default();
        build(&state, &mut out);
        assert!(out.lines_on_top.len() <= MAX_LINES + 12);
        assert_eq!(out.lines_on_top.iter().filter(|line| line.color == rgba([255, 72, 48], 1.0)).count(), 12);
        let peer = out.lines_on_top.iter().filter(|line| line.color == rgba([61, 184, 255], 0.95)).count();
        assert!(peer >= MAX_LINES / 2, "a huge selection still gets most of the budget: {peer}");
    }

    #[test]
    fn labels_markers_and_pills_paint_without_touching_the_map() {
        let mut state = state_with_peer(Vec::new());
        state.runtime.collaboration.markers.push(Marker {
            author: 2,
            name: "Guest".into(),
            color: [61, 184, 255],
            position: [10_000.0, 0.0, 0.0],
            at: Instant::now(),
        });
        state.runtime.collaboration.following = Some(2);
        state.runtime.collaboration.locks.push(hp_app::runtime::collaboration::Lock {
            participant: 2,
            name: "Guest".into(),
            color: [61, 184, 255],
            objects: 3,
        });
        let revision = state.doc.revision();
        let context = egui::Context::default();
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().clone();
            for kind in [hp_app::ViewKind::Perspective, hp_app::ViewKind::Top] {
                TeamLayer.paint(&state, &view(kind), &painter);
            }
        });
        assert!(output.shapes.len() >= 4, "{} shapes", output.shapes.len());
        assert_eq!(state.doc.revision(), revision);
    }
}
