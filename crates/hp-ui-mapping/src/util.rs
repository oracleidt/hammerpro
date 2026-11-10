use glam::DVec3;
use hp_app::{Action, AppState};
use hp_editor::ObjRef;

pub(crate) fn contains_ci(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    let h = haystack.as_bytes();
    let n = needle_lower.as_bytes();
    if n.len() > h.len() {
        return false;
    }
    let first = n[0];
    let first_upper = first.to_ascii_uppercase();
    let rest = &n[1..];
    (0..=h.len() - n.len()).any(|i| {
        let b = h[i];
        (b == first || b == first_upper)
            && h[i + 1..i + n.len()].iter().zip(rest).all(|(a, c)| a.to_ascii_lowercase() == *c)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DocKey {
    revision: u64,
    path_hash: u64,
    solids: usize,
    entities: usize,
    next_id: hp_editor::Id,
}

impl DocKey {
    pub(crate) fn of(doc: &hp_editor::Document) -> Self {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        doc.path().hash(&mut h);
        Self {
            revision: doc.revision(),
            path_hash: h.finish(),
            solids: doc.scene().solids().len(),
            entities: doc.scene().entities().len(),
            next_id: doc.peek_next_object_id(),
        }
    }
}

pub(crate) fn normalize_material(name: &str) -> String {
    let mut s = name.trim().replace('\\', "/").to_ascii_lowercase();
    if let Some(rest) = s.strip_prefix("materials/") {
        s = rest.to_owned();
    }
    if let Some(rest) = s.strip_suffix(".vmt") {
        s = rest.to_owned();
    }
    s
}

fn tool_suffix(name: &str) -> Option<&str> {
    let n = name.trim_start();
    let first = *n.as_bytes().first()?;
    let n = if first.eq_ignore_ascii_case(&b'm') {
        strip_prefix_ci(n, "materials").and_then(|r| r.strip_prefix(['/', '\\']))?
    } else {
        n
    };
    strip_prefix_ci(n, "tools").and_then(|r| r.strip_prefix(['/', '\\']))
}

pub(crate) fn is_tool_material(name: &str) -> bool {
    tool_suffix(name).is_some()
}

pub(crate) fn is_nodraw_material(name: &str) -> bool {
    tool_suffix(name).is_some_and(|r| {
        let r = r.trim_end();
        r.strip_suffix(".vmt").unwrap_or(r).eq_ignore_ascii_case("toolsnodraw")
    })
}

pub(crate) fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &s[prefix.len()..])
}

pub(crate) fn fmt_duration_ms(ms: u64) -> String {
    if ms < 1000 {
        return format!("{ms} ms");
    }
    let secs = ms as f64 / 1000.0;
    if secs < 60.0 {
        return format!("{secs:.1} s");
    }
    let total = ms / 1000;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    if h > 0 { format!("{h} h {m:02} min") } else { format!("{m} min {s:02} s") }
}

const LOOK_DISTANCE: f64 = 256.0;

pub(crate) fn center_views_on(state: &mut AppState, p: DVec3) {
    if !p.is_finite() {
        return;
    }
    state.cameras.center_2d(p);
    let cam = &mut state.cameras.cam3d;
    cam.pos = p - cam.forward() * LOOK_DISTANCE;
}

pub(crate) fn select_and_center(state: &mut AppState, objs: &[ObjRef]) {
    state.doc.set_selection(objs);
    request_center(state);
}

pub(crate) fn request_center(state: &mut AppState) {
    state.request(Action::Center2dOnSelection);
    state.request(Action::Center3dOnSelection);
}

pub(crate) fn icon_title(icon: &str, text: &str) -> String {
    format!("{icon}  {text}")
}

pub(crate) fn centered_pos(ctx: &egui::Context, size: egui::Vec2) -> egui::Pos2 {
    let screen = ctx.content_rect();
    let pos = screen.center() - size * 0.5;
    egui::pos2(pos.x.max(screen.left() + 8.0), pos.y.max(screen.top() + 40.0))
}

fn focus_in_layer(ctx: &egui::Context, layer: egui::LayerId) -> bool {
    ctx.memory(|m| m.focused()).and_then(|id| ctx.read_response(id)).is_some_and(|r| r.layer_id == layer)
}

#[derive(Clone, Copy, Debug, Default)]
struct PassTrack {
    pass: u64,
    before: bool,
    now: bool,
}

fn true_recently(ctx: &egui::Context, id: egui::Id, now: bool) -> bool {
    let pass = ctx.cumulative_pass_nr();
    let track = ctx.data_mut(|d| {
        let t = d.get_temp_mut_or_default::<PassTrack>(id);
        if t.pass != pass {
            t.before = t.pass + 1 == pass && t.now;
            t.pass = pass;
        }
        t.now = now;
        *t
    });
    track.before || track.now
}

fn focus_inside_recently(ctx: &egui::Context, layer: egui::LayerId) -> bool {
    true_recently(ctx, egui::Id::unique(("hp_window_focus", layer.id)), focus_in_layer(ctx, layer))
}

fn text_focus_recently(ctx: &egui::Context) -> bool {
    true_recently(ctx, egui::Id::unique("hp_dialog_text_focus"), ctx.text_edit_focused())
}

fn popup_recently(ctx: &egui::Context) -> bool {
    true_recently(ctx, egui::Id::unique("hp_dialog_popup"), egui::Popup::is_any_open(ctx))
}

fn is_palette_window(layer: egui::LayerId) -> bool {
    layer.id == crate::dialogs::pointfile_window_id()
}

pub(crate) fn top_dialog_layer(ctx: &egui::Context) -> Option<egui::LayerId> {
    let palettes = hp_app::widgets::palette_windows(ctx);
    ctx.memory(|m| {
        let areas = m.areas();
        m.layer_ids()
            .filter(|l| {
                l.order == egui::Order::Middle
                    && areas.visible_last_frame(l)
                    && areas.parent_layer(*l).is_none()
                    && !is_palette_window(*l)
                    && !palettes.contains(&l.id)
            })
            .last()
    })
}

pub(crate) fn window_has_keyboard(ctx: &egui::Context, window: &egui::Response) -> bool {
    let focus = focus_inside_recently(ctx, window.layer_id);
    let pointer_layer = ctx.pointer_hover_pos().and_then(|p| ctx.layer_id_at(p));
    pointer_layer == Some(window.layer_id) || focus
}

pub(crate) fn escape_pressed(ctx: &egui::Context, window: &egui::Response) -> bool {
    let own_focus = focus_inside_recently(ctx, window.layer_id);
    let typing_elsewhere = text_focus_recently(ctx) && !own_focus;
    let popup = popup_recently(ctx);
    if !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return false;
    }
    let modal = ctx.memory(|m| m.top_modal_layer()).is_some();
    let takes =
        !popup && !modal && (own_focus || (!typing_elsewhere && top_dialog_layer(ctx) == Some(window.layer_id)));
    takes && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
}

pub(crate) fn viewport_escape_pressed(ctx: &egui::Context) -> bool {
    let viewport = ctx.viewport_id();
    let typing = true_recently(ctx, egui::Id::unique(("hp_viewport_text_focus", viewport)), ctx.text_edit_focused());
    let popup = true_recently(ctx, egui::Id::unique(("hp_viewport_popup", viewport)), egui::Popup::is_any_open(ctx));
    if !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return false;
    }
    let modal = ctx.memory(|m| m.top_modal_layer()).is_some();
    !typing && !popup && !modal && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
}

pub(crate) fn enter_pressed(ctx: &egui::Context, window: &egui::Response) -> bool {
    if !window_has_keyboard(ctx, window) {
        return false;
    }
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
}

pub(crate) fn plural(n: usize, one: &'static str, other: &'static str) -> &'static str {
    let singular = match hp_app::i18n::lang() {
        hp_app::i18n::Lang::Fr => n <= 1,
        hp_app::i18n::Lang::En => n == 1,
        hp_app::i18n::Lang::Ru => n % 10 == 1 && n % 100 != 11,
    };
    if singular { one } else { other }
}

pub(crate) fn focus_select_all(field: &egui::Response, text: &str) {
    field.request_focus();
    let ctx = &field.ctx;
    let mut st = egui::text_edit::TextEditState::load(ctx, field.id).unwrap_or_default();
    st.cursor.set_char_range(Some(egui::text::CCursorRange::select_all_str(text)));
    st.store(ctx, field.id);
}

pub(crate) fn dialog_bottom_bar(ui: &mut egui::Ui, id_salt: &'static str, bar: impl FnOnce(&mut egui::Ui)) {
    egui::Panel::bottom(id_salt).frame(egui::Frame::NONE).resizable(false).show_separator_line(false).show(ui, |ui| {
        ui.separator();
        ui.horizontal(bar);
    });
}

pub(crate) fn dialog_body<R>(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::CentralPanel::no_frame().show(ui, body).inner
}

pub(crate) fn wrap_group<R>(ui: &mut egui::Ui, salt: &'static str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let id = ui.scope_id().with(("hp_wrap_group", salt));
    let width = ui.ctx().data(|d| d.get_temp::<f32>(id)).unwrap_or(0.0);
    let size = egui::vec2(width, ui.spacing().interact_size.y);
    let r = ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center), add);
    let used = r.response.rect.width();
    if (used - width).abs() > 0.5 {
        ui.ctx().data_mut(|d| d.insert_temp(id, used));
        ui.ctx().request_discard("hp wrap group size");
    }
    r.inner
}

pub(crate) fn lazy_hover_text(r: egui::Response, text: impl FnOnce() -> String) -> egui::Response {
    if r.hovered() || r.ctx.has_accesskit_node(r.id) { r.on_hover_text(text()) } else { r }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Form {
    label_width: f32,
}

impl Form {
    pub(crate) fn new(ui: &egui::Ui, labels: &[&str], min: f32, max: f32) -> Self {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let painter = ui.painter();
        let widest = labels
            .iter()
            .map(|l| painter.layout_no_wrap((*l).to_owned(), font.clone(), egui::Color32::WHITE).size().x)
            .fold(0.0f32, f32::max);
        Self { label_width: (widest + 10.0).clamp(min, max) }
    }

    pub(crate) fn label_width(&self) -> f32 {
        self.label_width
    }

    pub(crate) fn row<R>(
        &self,
        ui: &mut egui::Ui,
        label: impl Into<egui::WidgetText>,
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> R {
        let height = ui.spacing().interact_size.y;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(self.label_width, height),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_size(egui::vec2(self.label_width, height));
                    ui.add(egui::Label::new(label).wrap());
                },
            );
            ui.vertical(|ui| ui.horizontal_wrapped(add).inner).inner
        })
        .inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_insensitive_contains() {
        assert!(contains_ci("Trigger_Multiple", "trigger"));
        assert!(contains_ci("abc", ""));
        assert!(!contains_ci("ab", "abc"));
        assert!(contains_ci("info_player_START", "start"));
        assert!(!contains_ci("light", "lamp"));
    }

    #[test]
    fn materials() {
        assert_eq!(normalize_material("Materials\\TOOLS\\ToolsNodraw.vmt"), "tools/toolsnodraw");
        assert!(is_tool_material("TOOLS/TOOLSTRIGGER"));
        assert!(is_tool_material("materials/tools/toolsclip"));
        assert!(!is_tool_material("concrete/tools_floor"));
        assert!(is_nodraw_material("TOOLS/toolsnodraw"));
        assert!(!is_nodraw_material("tools/toolsskip"));
        assert_eq!(strip_prefix_ci("ABCdef", "abc"), Some("def"));
        assert_eq!(strip_prefix_ci("ab", "abc"), None);
    }

    #[test]
    fn durations() {
        assert_eq!(fmt_duration_ms(850), "850 ms");
        assert_eq!(fmt_duration_ms(12_400), "12.4 s");
        assert_eq!(fmt_duration_ms(185_000), "3 min 05 s");
        assert_eq!(fmt_duration_ms(3_720_000), "1 h 02 min");
    }

    #[test]
    fn centering_moves_cameras() {
        let mut state = hp_app::testing::test_state();
        let p = DVec3::new(100.0, 200.0, 300.0);
        center_views_on(&mut state, p);
        assert_eq!(state.cameras.top.center, glam::DVec2::new(100.0, 200.0));
        let cam = state.cameras.cam3d;
        let ahead = cam.pos + cam.forward() * LOOK_DISTANCE;
        assert!((ahead - p).length() < 1e-6);
        center_views_on(&mut state, DVec3::NAN);
        assert_eq!(state.cameras.top.center, glam::DVec2::new(100.0, 200.0));
    }

    #[test]
    fn doc_keys_tell_documents_apart() {
        let state = hp_app::testing::test_state();
        let other = hp_editor::Document::new_empty();
        assert_ne!(state.doc.revision(), other.revision());
        assert_ne!(DocKey::of(&state.doc), DocKey::of(&other));
        assert_ne!(DocKey::of(&other), DocKey::of(&hp_editor::Document::new_empty()));
        assert_eq!(DocKey::of(&other), DocKey::of(&other));
        let mut changed = hp_editor::Document::new_empty();
        let before = DocKey::of(&changed);
        changed.set_view_setting("bSnapToGrid", "1");
        assert_ne!(DocKey::of(&changed), before);
    }

    #[test]
    fn tool_materials_fast_path() {
        assert!(is_tool_material("  tools\\toolsclip"));
        assert!(!is_tool_material("metal/metalfloor001a"));
        assert!(!is_tool_material("materials_other/tools/x"));
        assert!(!is_tool_material(""));
        assert!(is_nodraw_material("materials/tools/toolsnodraw.vmt"));
        assert!(!is_nodraw_material("tools/toolsnodraw2"));
    }

    fn escape() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[derive(Clone, Copy, Default)]
    struct Scene {
        small: bool,
        palette: bool,
        focus_big: bool,
        focus_small: bool,
        focus_panel: bool,
        open_popup: bool,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct Taken {
        big: bool,
        small: bool,
        left: bool,
    }

    const NOBODY: Taken = Taken { big: false, small: false, left: true };
    const BIG: Taken = Taken { big: true, small: false, left: false };
    const SMALL: Taken = Taken { big: false, small: true, left: false };

    fn run_windows(ctx: &egui::Context, events: Vec<egui::Event>, scene: Scene) -> Taken {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0))),
            events,
            ..Default::default()
        };
        let mut taken = Taken { big: false, small: false, left: false };
        let mut out = ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            let mut panel_text = String::from("p");
            let r = ui.text_edit_singleline(&mut panel_text);
            if scene.focus_panel {
                r.request_focus();
            }
            let popup_id = egui::Id::unique("test_popup");
            if scene.open_popup {
                egui::Popup::open_id(&ctx, popup_id);
            }
            egui::Popup::new(popup_id, ctx.clone(), egui::pos2(20.0, 600.0), egui::LayerId::background())
                .open_memory(None)
                .show(|ui| ui.label("menu"));
            let mut text = String::from("x");
            let big = egui::Window::new("big")
                .id(egui::Id::unique("test_big"))
                .fixed_pos([100.0, 100.0])
                .fixed_size([500.0, 400.0])
                .show(&ctx, |ui| {
                    let r = ui.text_edit_singleline(&mut text);
                    if scene.focus_big {
                        r.request_focus();
                    }
                });
            let small = if scene.small {
                egui::Window::new("small")
                    .id(egui::Id::unique("test_small"))
                    .fixed_pos([400.0, 150.0])
                    .fixed_size([200.0, 100.0])
                    .show(&ctx, |ui| {
                        let r = ui.text_edit_singleline(&mut text);
                        if scene.focus_small {
                            r.request_focus();
                        }
                    })
            } else {
                None
            };
            if scene.palette {
                egui::Window::new("navigator")
                    .id(crate::dialogs::pointfile_window_id())
                    .fixed_pos([700.0, 500.0])
                    .show(&ctx, |ui| ui.label("leak"));
            }
            if let Some(b) = big {
                taken.big = escape_pressed(&ctx, &b.response);
            }
            if let Some(s) = small {
                taken.small = escape_pressed(&ctx, &s.response);
            }
            taken.left = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        });
        out.textures_delta.clear();
        taken
    }

    fn settle(ctx: &egui::Context, scene: Scene) {
        for _ in 0..3 {
            run_windows(ctx, Vec::new(), scene);
        }
    }

    fn click(pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: egui::pos2(550.0, 110.0),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn escape_closes_the_top_dialog_wherever_the_pointer_is() {
        let ctx = egui::Context::default();
        let both = Scene { small: true, ..Scene::default() };
        settle(&ctx, both);
        run_windows(&ctx, vec![egui::Event::PointerMoved(egui::pos2(900.0, 700.0))], both);
        assert_eq!(run_windows(&ctx, vec![escape()], both), SMALL);
        run_windows(&ctx, vec![egui::Event::PointerMoved(egui::pos2(200.0, 140.0))], both);
        assert_eq!(run_windows(&ctx, vec![escape()], both), SMALL);
        settle(&ctx, Scene::default());
        assert_eq!(run_windows(&ctx, vec![escape()], Scene::default()), BIG);
        settle(&ctx, both);
        run_windows(&ctx, vec![egui::Event::PointerMoved(egui::pos2(550.0, 110.0))], both);
        run_windows(&ctx, vec![click(true)], both);
        run_windows(&ctx, vec![click(false)], both);
        run_windows(&ctx, vec![egui::Event::PointerMoved(egui::pos2(900.0, 700.0))], both);
        assert_eq!(run_windows(&ctx, vec![escape()], both), BIG);
    }

    #[test]
    fn palettes_do_not_hide_the_top_dialog() {
        let ctx = egui::Context::default();
        let scene = Scene { palette: true, ..Scene::default() };
        settle(&ctx, scene);
        assert_eq!(top_dialog_layer(&ctx).map(|l| l.id), Some(egui::Id::unique("test_big")));
        assert_eq!(run_windows(&ctx, vec![escape()], scene), BIG);
    }

    #[test]
    fn escape_leaves_text_fields_and_popups_first() {
        let ctx = egui::Context::default();
        let both = Scene { small: true, ..Scene::default() };
        settle(&ctx, both);
        run_windows(&ctx, Vec::new(), Scene { focus_big: true, ..both });
        run_windows(&ctx, Vec::new(), both);
        assert!(ctx.text_edit_focused());
        assert_eq!(run_windows(&ctx, vec![escape()], both), BIG);
        run_windows(&ctx, Vec::new(), Scene { focus_small: true, ..both });
        run_windows(&ctx, Vec::new(), both);
        assert_eq!(run_windows(&ctx, vec![escape()], both), SMALL);
        run_windows(&ctx, Vec::new(), Scene { focus_panel: true, ..both });
        run_windows(&ctx, Vec::new(), both);
        assert!(ctx.text_edit_focused());
        assert_eq!(run_windows(&ctx, vec![escape()], both), NOBODY);
        assert!(!ctx.text_edit_focused());
        run_windows(&ctx, Vec::new(), both);
        assert_eq!(run_windows(&ctx, vec![escape()], both), SMALL);
        run_windows(&ctx, Vec::new(), Scene { open_popup: true, ..both });
        run_windows(&ctx, Vec::new(), both);
        assert!(egui::Popup::is_any_open(&ctx));
        let taken = run_windows(&ctx, vec![escape()], both);
        assert!(!taken.big && !taken.small, "{taken:?}");
        assert!(!egui::Popup::is_any_open(&ctx));
        run_windows(&ctx, Vec::new(), both);
        assert_eq!(run_windows(&ctx, vec![escape()], both), SMALL);
    }

    #[test]
    fn enter_belongs_to_the_hovered_window() {
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let ctx = egui::Context::default();
        let run = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0))),
                events,
                ..Default::default()
            };
            let mut taken = false;
            let mut out = ctx.run_ui(input, |ui| {
                let ctx = ui.ctx().clone();
                let w = egui::Window::new("w")
                    .id(egui::Id::unique("test_enter"))
                    .fixed_pos([100.0, 100.0])
                    .fixed_size([300.0, 200.0])
                    .show(&ctx, |ui| ui.label("w"));
                if let Some(w) = w {
                    taken = enter_pressed(&ctx, &w.response);
                }
            });
            out.textures_delta.clear();
            taken
        };
        for _ in 0..3 {
            run(Vec::new());
        }
        run(vec![egui::Event::PointerMoved(egui::pos2(900.0, 700.0))]);
        assert!(!run(vec![enter.clone()]));
        run(vec![egui::Event::PointerMoved(egui::pos2(150.0, 150.0))]);
        assert!(run(vec![enter]));
    }

    #[test]
    fn plural_forms() {
        let _lang = crate::lang_lock();
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert_eq!(plural(0, "one", "other"), "other");
        assert_eq!(plural(1, "one", "other"), "one");
        assert_eq!(plural(2, "one", "other"), "other");
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        assert_eq!(plural(0, "one", "other"), "one");
        assert_eq!(plural(1, "one", "other"), "one");
        assert_eq!(plural(2, "one", "other"), "other");
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    }
}
