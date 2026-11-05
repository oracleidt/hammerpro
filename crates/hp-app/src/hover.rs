use egui::emath::easing::cubic_out;
use egui::{Color32, Context, Id, LayerId, Plugin, Rect, Ui};

use crate::widgets::HOVER_SECONDS;

const MAX_TRACKED: usize = 48;
const MAX_WIDTH: f32 = 480.0;
const MAX_HEIGHT: f32 = 64.0;

#[derive(Clone, Copy)]
struct Tracked {
    id: Id,
    layer: LayerId,
    rect: Rect,
    clip: Rect,
}

#[derive(Default)]
pub struct HoverFx {
    tracked: Vec<Tracked>,
}

pub fn install(ctx: &Context) {
    ctx.add_plugin(HoverFx::default());
}

fn candidate(ctx: &Context) -> Option<(Tracked, bool)> {
    let hit = ctx.viewport(|v| v.hits.click)?;
    let size = hit.rect.size();
    let plain_button = hit.sense.senses_click()
        && !hit.sense.senses_drag()
        && hit.enabled
        && size.x <= MAX_WIDTH
        && size.y <= MAX_HEIGHT
        && size.x >= 8.0
        && size.y >= 8.0;
    if !plain_button {
        return None;
    }
    let down = ctx.input(|i| i.pointer.primary_down());
    Some((Tracked { id: hit.id, layer: hit.layer_id, rect: hit.rect, clip: hit.interact_rect }, down))
}

impl Plugin for HoverFx {
    fn debug_name(&self) -> &'static str {
        "hp_hover_fx"
    }

    fn on_end_pass(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let target = candidate(&ctx);
        if let Some((t, _)) = target {
            match self.tracked.iter_mut().find(|e| e.id == t.id) {
                Some(slot) => *slot = t,
                None => {
                    ctx.animate_bool_with_time_and_easing(t.id.with("hp_hover_in"), false, HOVER_SECONDS, cubic_out);
                    ctx.animate_bool_with_time_and_easing(t.id.with("hp_press_in"), false, HOVER_SECONDS, cubic_out);
                    if self.tracked.len() >= MAX_TRACKED {
                        self.tracked.remove(0);
                    }
                    self.tracked.push(t);
                }
            }
        }
        let dark = crate::theme::is_dark();
        let radius = crate::theme::RADIUS;
        self.tracked.retain(|e| {
            let hot = target.is_some_and(|(t, _)| t.id == e.id);
            let down = hot && target.is_some_and(|(_, d)| d);
            let hover = ctx.animate_bool_with_time_and_easing(e.id.with("hp_hover_in"), hot, HOVER_SECONDS, cubic_out);
            let press = ctx.animate_bool_with_time_and_easing(e.id.with("hp_press_in"), down, HOVER_SECONDS, cubic_out);
            if hover <= 0.0 && press <= 0.0 {
                return false;
            }
            let alpha = (hover * 14.0 + press * 22.0).round().clamp(0.0, 255.0) as u8;
            let tint = if dark { Color32::from_white_alpha(alpha) } else { Color32::from_black_alpha(alpha) };
            let painter = ctx.layer_painter(e.layer).with_clip_rect(e.clip);
            painter.rect_filled(e.rect, radius, tint);
            true
        });
    }
}
