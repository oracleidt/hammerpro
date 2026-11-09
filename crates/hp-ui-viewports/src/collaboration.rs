use egui::{Align2, Painter, Stroke};
use glam::DVec3;
use hp_app::{AppState, theme, view::ViewInfo};

pub fn paint(state: &AppState, view: &ViewInfo, painter: &Painter) {
    let runtime = &state.runtime.collaboration;
    if !runtime.connected || !runtime.active_map {
        return;
    }
    let colors = [theme::AXIS_Z, theme::VIEW_ACCENT, theme::AXIS_X, theme::AXIS_Y, theme::VIEW_WARNING];
    let painter = painter.with_clip_rect(view.rect);
    for peer in &runtime.participants {
        if peer.id == runtime.local_id {
            continue;
        }
        let color = colors[peer.id as usize % colors.len()];
        let position = DVec3::from_array(peer.position);
        if let Some(screen) = view.world_to_screen(position) {
            painter.circle_filled(screen, 5.0, color);
            if let Some(end) = view.world_to_screen(position + DVec3::from_array(peer.direction) * 32.0) {
                painter.line_segment([screen, end], Stroke::new(2.0, color));
            }
            crate::paint::text_pill(
                &painter,
                screen + egui::vec2(8.0, -10.0),
                Align2::LEFT_BOTTOM,
                format!("{} | {} ms", peer.name, peer.latency_ms),
                color,
            );
        }
        for selected in &peer.selected {
            let Some((kind, id)) = selected.split_once(':') else {
                continue;
            };
            let Ok(id) = id.parse::<u32>() else {
                continue;
            };
            if kind == "solid" {
                if let Some(node) = state.doc.scene().solids().get(&id) {
                    for face in &node.geom.faces {
                        for index in 0..face.vertices.len() {
                            if let (Some(a), Some(b)) = (
                                view.world_to_screen(face.vertices[index]),
                                view.world_to_screen(face.vertices[(index + 1) % face.vertices.len()]),
                            ) {
                                painter.line_segment([a, b], Stroke::new(2.0, color));
                            }
                        }
                    }
                }
            } else if kind == "entity"
                && let Some(position) = state
                    .doc
                    .scene()
                    .entities()
                    .get(&id)
                    .and_then(|node| node.entity.origin())
                    .and_then(|position| view.world_to_screen(position))
            {
                painter.circle_stroke(position, 14.0, Stroke::new(2.0, color));
                crate::paint::text_pill(
                    &painter,
                    position + egui::vec2(16.0, 0.0),
                    Align2::LEFT_CENTER,
                    peer.name.clone(),
                    color,
                );
            }
        }
    }
}
