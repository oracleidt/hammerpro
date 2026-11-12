use egui::{
    Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use hp_app::{t, tf};
use hp_fgd::ResolvedFgd;

const ROW_H: f32 = 20.0;
const LIST_H: f32 = 300.0;
const POPUP_W: f32 = 300.0;
const MAX_RECENT_SHOWN: usize = 6;

pub(crate) fn filter_classes(classes: &[String], query: &str) -> Vec<u32> {
    let words: Vec<String> = query.split_whitespace().map(str::to_ascii_lowercase).collect();
    if words.is_empty() {
        return (0..classes.len().min(u32::MAX as usize) as u32).collect();
    }
    let mut prefix = Vec::new();
    let mut other = Vec::new();
    let mut lower = String::new();
    for (i, name) in classes.iter().enumerate() {
        let Ok(i) = u32::try_from(i) else { break };
        lower.clear();
        lower.extend(name.chars().map(|c| c.to_ascii_lowercase()));
        if !words.iter().all(|w| lower.contains(w.as_str())) {
            continue;
        }
        if words.first().is_some_and(|w| lower.starts_with(w.as_str())) {
            prefix.push(i);
        } else {
            other.push(i);
        }
    }
    prefix.extend(other);
    prefix
}

pub(crate) struct PickerSource<'a> {
    pub classes: &'a [String],
    pub recent: &'a [String],
    pub fixed: &'a [(String, String)],
    pub fgd: Option<&'a ResolvedFgd>,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FilterKey {
    generation: u64,
    len: usize,
    search: String,
}

#[derive(Debug, Default)]
struct PickerState {
    search: String,
    filtered: Vec<u32>,
    key: Option<FilterKey>,
    cursor: usize,
    reveal_cursor: bool,
    focus_search: bool,
    scroll_offset: f32,
    view_height: f32,
}

#[derive(Debug, Default)]
pub(crate) struct ClassPicker {
    pub(crate) open: bool,
    st: PickerState,
}

impl ClassPicker {
    #[cfg(test)]
    pub(crate) fn search_mut(&mut self) -> &mut String {
        &mut self.st.search
    }

    #[cfg(test)]
    pub(crate) fn filtered_len(&self) -> usize {
        self.st.filtered.len()
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut Ui,
        id_salt: &str,
        button_text: &str,
        tooltip: &str,
        src: &PickerSource<'_>,
    ) -> Option<String> {
        let button = egui::Button::new(RichText::new(format!("{button_text}  ⏷"))).min_size(Vec2::new(170.0, 0.0));
        let button = ui.add(button).on_hover_text(tooltip);
        if button.clicked() {
            self.open = !self.open;
            if self.open {
                self.st.focus_search = true;
                self.st.reveal_cursor = true;
            }
        }
        let popup_id = ui.make_persistent_id(id_salt).with("class_popup");
        let st = &mut self.st;
        let mut picked = None;
        egui::Popup::from_response(&button)
            .id(popup_id)
            .open_bool(&mut self.open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width(POPUP_W)
            .show(|ui| {
                ui.set_min_width(POPUP_W);
                picked = st.contents(ui, src);
                if picked.is_some() {
                    ui.close();
                }
            });
        picked
    }
}

impl PickerState {
    fn refresh(&mut self, src: &PickerSource<'_>) {
        let fresh = self
            .key
            .as_ref()
            .is_some_and(|k| k.generation == src.generation && k.len == src.classes.len() && k.search == self.search);
        if fresh {
            return;
        }
        self.filtered = filter_classes(src.classes, &self.search);
        self.cursor = 0;
        self.reveal_cursor = true;
        self.key = Some(FilterKey { generation: src.generation, len: src.classes.len(), search: self.search.clone() });
    }

    fn contents(&mut self, ui: &mut Ui, src: &PickerSource<'_>) -> Option<String> {
        self.refresh(src);
        let mut picked = None;

        let (down, up) = ui.input_mut(|i| {
            (i.consume_key(Modifiers::NONE, Key::ArrowDown), i.consume_key(Modifiers::NONE, Key::ArrowUp))
        });
        let n = self.filtered.len();
        if n > 0 {
            if down {
                self.cursor = (self.cursor + 1).min(n - 1);
                self.reveal_cursor = true;
            }
            if up {
                self.cursor = self.cursor.saturating_sub(1);
                self.reveal_cursor = true;
            }
        }

        let edit = egui::TextEdit::singleline(&mut self.search)
            .hint_text(t!("create.picker.search"))
            .desired_width(f32::INFINITY);
        let resp = ui.add(edit);
        if self.focus_search {
            resp.request_focus();
            self.focus_search = false;
        }
        if resp.changed() {
            self.refresh(src);
        }
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));

        let typed = self.search.trim().to_owned();
        let exact = !typed.is_empty() && src.classes.iter().any(|c| c.eq_ignore_ascii_case(&typed));
        if enter {
            if let Some(name) = self.filtered.get(self.cursor).and_then(|&i| src.classes.get(i as usize)) {
                picked = Some(name.clone());
            } else if !typed.is_empty() {
                picked = Some(typed.clone());
            }
        }

        if !src.fixed.is_empty() {
            for (value, label) in src.fixed {
                if row(ui, label, None, false, None).clicked() {
                    picked = Some(value.clone());
                }
            }
            ui.separator();
        }

        if !typed.is_empty() && !exact && is_valid_class_name(&typed) {
            let label = tf!("create.picker.use_typed", typed);
            if row(ui, &label, None, false, Some(t!("create.picker.use_typed_tip"))).clicked() {
                picked = Some(typed.clone());
            }
        }

        if self.search.trim().is_empty() && !src.recent.is_empty() {
            ui.label(RichText::new(t!("create.picker.recent")).small().weak());
            for name in src.recent.iter().take(MAX_RECENT_SHOWN) {
                let (desc, color) = describe(src.fgd, name);
                if row(ui, name, color, false, desc).clicked() {
                    picked = Some(name.clone());
                }
            }
            ui.separator();
        }

        if src.classes.is_empty() {
            ui.label(RichText::new(t!("create.picker.no_fgd")).weak());
        } else if self.filtered.is_empty() {
            ui.label(RichText::new(t!("create.picker.no_match")).weak());
        } else {
            ui.label(RichText::new(tf!("create.picker.count", self.filtered.len())).small().weak());
            let spacing = ui.spacing().item_spacing.y;
            let mut area = ScrollArea::vertical().id_salt("class_list").max_height(LIST_H).auto_shrink([false, true]);
            if self.reveal_cursor {
                let top = self.cursor as f32 * (ROW_H + spacing);
                let bottom = top + ROW_H;
                if top < self.scroll_offset {
                    area = area.vertical_scroll_offset(top);
                } else if self.view_height > 0.0 && bottom > self.scroll_offset + self.view_height {
                    area = area.vertical_scroll_offset(bottom - self.view_height);
                }
                self.reveal_cursor = false;
            }
            let filtered = &self.filtered;
            let cursor = self.cursor;
            let out = area.show_rows(ui, ROW_H, filtered.len(), |ui, range| {
                let mut clicked = None;
                for idx in range {
                    let Some(name) = filtered.get(idx).and_then(|&i| src.classes.get(i as usize)) else { continue };
                    let (desc, color) = describe(src.fgd, name);
                    if row(ui, name, color, idx == cursor, desc).clicked() {
                        clicked = Some(name.clone());
                    }
                }
                clicked
            });
            self.scroll_offset = out.state.clamped_offset().y;
            self.view_height = out.inner_rect.height();
            if let Some(c) = out.inner {
                picked = Some(c);
            }
        }
        if picked.is_some() {
            self.search.clear();
            self.key = None;
        }
        picked
    }
}

pub(crate) fn is_valid_class_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn describe<'a>(fgd: Option<&'a ResolvedFgd>, name: &str) -> (Option<&'a str>, Option<Color32>) {
    let Some(class) = fgd.and_then(|f| f.class(name)) else { return (None, None) };
    let desc = class.description.trim();
    let color = class.color().map(|[r, g, b]| Color32::from_rgb(r, g, b));
    ((!desc.is_empty()).then_some(desc), color)
}

fn row(ui: &mut Ui, label: &str, color: Option<Color32>, active: bool, tooltip: Option<&str>) -> egui::Response {
    let width = ui.available_width().max(POPUP_W - 24.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, ROW_H), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let visuals = ui.visuals();
        if active {
            painter.rect_filled(rect, 3.0, hp_app::theme::accent_bg());
        } else if resp.hovered() {
            painter.rect_filled(rect, 3.0, visuals.widgets.hovered.weak_bg_fill);
        }
        let swatch = Rect::from_center_size(Pos2::new(rect.left() + 10.0, rect.center().y), Vec2::splat(8.0));
        match color {
            Some(c) => {
                painter.rect_filled(swatch, 2.0, c);
            }
            None => {
                painter.rect_stroke(swatch, 2.0, Stroke::new(1.0, visuals.weak_text_color()), StrokeKind::Inside);
            }
        }
        let text_color = if active || resp.hovered() { visuals.strong_text_color() } else { visuals.text_color() };
        painter.with_clip_rect(hp_app::widgets::clip_x(rect, painter.clip_rect())).text(
            Pos2::new(rect.left() + 22.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(13.0),
            text_color,
        );
    }
    match tooltip {
        Some(tip) => resp.on_hover_text(tip),
        None => resp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn filtering_ranks_prefix_matches_first() {
        let classes =
            names(&["env_fire", "info_player_start", "light", "light_spot", "point_spotlight", "prop_physics"]);
        let pick = |q: &str| -> Vec<&str> {
            filter_classes(&classes, q).iter().map(|&i| classes[i as usize].as_str()).collect()
        };
        assert_eq!(pick("").len(), classes.len());
        assert_eq!(pick("light"), vec!["light", "light_spot", "point_spotlight"]);
        assert_eq!(pick("SPOT"), vec!["light_spot", "point_spotlight"]);
        assert_eq!(pick("spot light"), vec!["light_spot", "point_spotlight"]);
        assert!(pick("zzz").is_empty());
    }

    #[test]
    fn class_name_validation() {
        assert!(is_valid_class_name("gmod_button"));
        assert!(is_valid_class_name("sent_ball2"));
        assert!(!is_valid_class_name(""));
        assert!(!is_valid_class_name("bad name"));
        assert!(!is_valid_class_name("x\"y"));
    }
}
