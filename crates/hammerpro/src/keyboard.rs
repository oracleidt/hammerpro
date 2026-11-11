use egui::{Event, Key, Modifiers};
use hp_app::{Action, AppState, Tool};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStart {
    pub text_focus: bool,
    pub any_focus: bool,
    pub popup: bool,
    pub modal: bool,
}

impl FrameStart {
    pub fn capture(ctx: &egui::Context) -> Self {
        Self {
            text_focus: ctx.text_edit_focused(),
            any_focus: ctx.memory(|m| m.focused()).is_some(),
            popup: egui::Popup::is_any_open(ctx),
            modal: ctx.memory(|m| m.top_modal_layer()).is_some(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AppCapture {
    pub dialog: bool,
    pub keyboard: bool,
    pub context_menu: bool,
}

impl AppCapture {
    pub fn any(self) -> bool {
        self.dialog || self.keyboard || self.context_menu
    }
}

fn is_symbol_key(key: Key) -> bool {
    matches!(
        key,
        Key::Colon
            | Key::Comma
            | Key::Backslash
            | Key::Slash
            | Key::Pipe
            | Key::Questionmark
            | Key::Exclamationmark
            | Key::OpenBracket
            | Key::CloseBracket
            | Key::OpenCurlyBracket
            | Key::CloseCurlyBracket
            | Key::Backtick
            | Key::Minus
            | Key::Period
            | Key::Plus
            | Key::Equals
            | Key::Semicolon
            | Key::Quote
    )
}

fn is_typing_key(key: Key) -> bool {
    let name = key.name();
    (name.len() == 1 && name.as_bytes()[0].is_ascii_alphanumeric())
        || is_symbol_key(key)
        || matches!(key, Key::Delete | Key::Backspace)
}

fn is_focus_key(key: Key) -> bool {
    matches!(
        key,
        Key::Tab
            | Key::ArrowUp
            | Key::ArrowDown
            | Key::ArrowLeft
            | Key::ArrowRight
            | Key::Enter
            | Key::Space
            | Key::Escape
    )
}

pub fn is_widget_key(event: &Event) -> bool {
    let Event::Key { key, pressed: true, modifiers, .. } = event else { return false };
    let ctrl_or_alt = modifiers.ctrl || modifiers.alt || modifiers.command || modifiers.mac_cmd;
    if !ctrl_or_alt {
        return is_typing_key(*key) || is_focus_key(*key);
    }
    modifiers.ctrl && modifiers.alt && is_symbol_key(*key)
}

fn is_focus_tab(event: &Event) -> bool {
    matches!(
        event,
        Event::Key { key: Key::Tab, modifiers, .. } if !modifiers.alt && !modifiers.ctrl && !modifiers.command && !modifiers.mac_cmd
    )
}

#[derive(Debug, Default)]
pub struct KeyRouter {
    start: FrameStart,
    start_capture: AppCapture,
    stashed_tabs: Vec<Event>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Routed {
    pub actions: Vec<Action>,
    pub paste_text: Option<String>,
}

impl KeyRouter {
    pub fn before_frame(
        &mut self,
        ctx: &egui::Context,
        raw: &mut egui::RawInput,
        keep_tabs: bool,
        capture: AppCapture,
    ) {
        self.start = FrameStart::capture(ctx);
        self.start_capture = capture;
        self.stashed_tabs.clear();
        if keep_tabs || self.start.modal || hp_app::dialog_keys::take_tabs(ctx, raw) || self.start.any_focus {
            return;
        }
        let mut kept = Vec::with_capacity(raw.events.len());
        for event in raw.events.drain(..) {
            if is_focus_tab(&event) {
                self.stashed_tabs.push(event);
            } else {
                kept.push(event);
            }
        }
        raw.events = kept;
    }

    pub fn blocked(&self, ctx: &egui::Context, capture: AppCapture) -> bool {
        capture.any()
            || self.start_capture.any()
            || self.start.text_focus
            || self.start.popup
            || self.start.modal
            || ctx.text_edit_focused()
            || egui::Popup::is_any_open(ctx)
    }

    pub fn widget_focus(&self, ctx: &egui::Context) -> bool {
        self.start.any_focus || ctx.memory(|m| m.focused()).is_some()
    }

    pub fn after_ui(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        tool: &mut dyn Tool,
        capture: AppCapture,
    ) -> Routed {
        hp_app::dialog_keys::finish_frame(ctx);
        let tabs = std::mem::take(&mut self.stashed_tabs);
        if self.blocked(ctx, capture) {
            return Routed::default();
        }
        if !tabs.is_empty() {
            ctx.input_mut(|i| i.events.extend(tabs));
        }
        if self.widget_focus(ctx) {
            ctx.input_mut(|i| i.events.retain(|e| !is_widget_key(e)));
        }
        if !egui::DragAndDrop::has_any_payload(ctx) {
            tool.key_input(state, ctx);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) && !tool.cancel(state) {
            state.doc.clear_selection();
        }
        let selected = text_selected(ctx);
        let (actions, paste_text) = ctx.input_mut(|i| {
            let paste = i.events.iter().find_map(|e| match e {
                Event::Paste(text) => Some(text.clone()),
                _ => None,
            });
            let kept: Vec<Event> = if selected {
                i.events.iter().filter(|e| matches!(e, Event::Copy | Event::Cut)).cloned().collect()
            } else {
                Vec::new()
            };
            let looking = state.runtime.lab.fly_texture.active;
            let actions = state
                .keys
                .take_triggered_where(&mut i.events, |a| looking || hp_ui_viewports::fly_texture::op_of(a).is_none());
            i.events.extend(kept);
            (actions, paste)
        });
        let actions = if selected {
            actions.into_iter().filter(|a| !matches!(a, Action::Copy | Action::Cut)).collect()
        } else {
            actions
        };
        let paste_text = paste_text.filter(|_| actions.contains(&Action::Paste));
        Routed { actions, paste_text }
    }
}

fn text_selected(ctx: &egui::Context) -> bool {
    ctx.plugin::<egui::text_selection::LabelSelectionState>().lock().has_selection()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_app::{ToolFlow, ToolId, ViewInput};

    #[derive(Default)]
    struct KeyTool {
        calls: usize,
        tabs: usize,
        arrows: usize,
    }

    impl Tool for KeyTool {
        fn id(&self) -> ToolId {
            ToolId::Select
        }
        fn view_input(&mut self, _state: &mut AppState, _input: &ViewInput<'_>) -> ToolFlow {
            ToolFlow::Ignored
        }
        fn key_input(&mut self, _state: &mut AppState, ctx: &egui::Context) -> bool {
            self.calls += 1;
            ctx.input_mut(|i| {
                while i.consume_key(Modifiers::NONE, Key::Tab) {
                    self.tabs += 1;
                }
                while i.consume_key(Modifiers::NONE, Key::ArrowRight) {
                    self.arrows += 1;
                }
            });
            false
        }
    }

    const CTRL: Modifiers = Modifiers { alt: false, ctrl: true, shift: false, mac_cmd: false, command: true };

    fn key(key: Key, modifiers: Modifiers) -> Event {
        Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers }
    }

    fn release(key: Key) -> Event {
        Event::Key { key, physical_key: Some(key), pressed: false, repeat: false, modifiers: Modifiers::NONE }
    }

    struct Harness {
        ctx: egui::Context,
        router: KeyRouter,
        state: AppState,
        shell: hp_ui_shell::Shell,
        tool: KeyTool,
        text: String,
        focus_field: bool,
        focus_button: bool,
        open_combo: bool,
        list_hovered: bool,
        button_clicks: usize,
        routed: Vec<Routed>,
        dialog: bool,
        dialog_fields: Vec<egui::Id>,
        values: [f64; 3],
    }

    impl Harness {
        fn new() -> Self {
            let mut state = hp_app::testing::test_state();
            hp_ui_shell::register_translations();
            hp_app::i18n::register(crate::app::TR);
            state.doc.select_all();
            Self {
                ctx: egui::Context::default(),
                router: KeyRouter::default(),
                state,
                shell: hp_ui_shell::Shell::new(),
                tool: KeyTool::default(),
                text: "hello".into(),
                focus_field: false,
                focus_button: false,
                open_combo: false,
                list_hovered: false,
                button_clicks: 0,
                routed: Vec::new(),
                dialog: false,
                dialog_fields: Vec::new(),
                values: [0.0; 3],
            }
        }

        fn capture(&self) -> AppCapture {
            AppCapture { dialog: self.shell.dialog_open(&self.state), ..AppCapture::default() }
        }

        fn frame(&mut self, events: Vec<Event>) {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
                events,
                ..Default::default()
            };
            let capture = self.capture();
            self.router.before_frame(&self.ctx, &mut raw, false, capture);
            let mut out = self.ctx.run_ui(raw, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        let _ = ui.button("Open");
                    });
                });
                let r = ui.text_edit_singleline(&mut self.text);
                if std::mem::take(&mut self.focus_field) {
                    r.request_focus();
                }
                ui.add(egui::Label::new("Linear").sense(egui::Sense::click()));
                let combo = egui::ComboBox::from_id_salt("combo").selected_text("a").show_ui(ui, |ui| {
                    ui.label("b");
                });
                if std::mem::take(&mut self.open_combo) {
                    egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&combo.response));
                }
                let button = ui.button("Apply");
                if button.clicked() {
                    self.button_clicks += 1;
                }
                if std::mem::take(&mut self.focus_button) {
                    button.request_focus();
                }
                if self.list_hovered {
                    ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowRight));
                }
                let ctx = ui.ctx().clone();
                if self.dialog {
                    let values = &mut self.values;
                    let shown = hp_app::chrome::Window::new("Transform").open(&mut self.dialog).show(&ctx, |ui| {
                        values.iter_mut().map(|v| ui.add(egui::DragValue::new(v)).id).collect::<Vec<_>>()
                    });
                    if let Some(ids) = shown.and_then(|r| r.inner) {
                        self.dialog_fields = ids;
                    }
                }
                self.shell.dialogs(&ctx, &mut self.state);
                let capture = AppCapture { dialog: self.shell.dialog_open(&self.state), ..AppCapture::default() };
                let routed = self.router.after_ui(&ctx, &mut self.state, &mut self.tool, capture);
                self.routed.push(routed);
            });
            out.textures_delta.clear();
        }

        fn last(&self) -> &Routed {
            self.routed.last().expect("one frame ran")
        }

        fn selected(&self) -> usize {
            self.state.doc.selected_objects().len()
        }
    }

    #[test]
    fn ctrl_shortcuts_reach_the_bindings_when_nothing_is_focused() {
        let mut h = Harness::new();
        h.frame(vec![]);
        h.frame(vec![key(Key::A, CTRL)]);
        assert_eq!(h.last().actions, vec![Action::SelectAll]);
        h.frame(vec![release(Key::A), key(Key::Delete, Modifiers::NONE)]);
        assert_eq!(h.last().actions, vec![Action::Delete]);
        assert_eq!(h.tool.calls, 3, "the tool sees the keys once per frame");
    }

    #[test]
    fn text_fields_keep_every_key() {
        let mut h = Harness::new();
        h.focus_field = true;
        h.frame(vec![]);
        h.frame(vec![]);
        assert!(h.ctx.text_edit_focused());
        let calls = h.tool.calls;
        for events in [
            vec![key(Key::A, CTRL)],
            vec![key(Key::Z, CTRL)],
            vec![key(Key::Y, CTRL)],
            vec![key(Key::Delete, Modifiers::NONE)],
            vec![key(Key::H, Modifiers::NONE)],
            vec![Event::Copy],
            vec![Event::Paste("abc".into())],
        ] {
            h.frame(events);
            assert_eq!(h.last(), &Routed::default());
        }
        assert_eq!(h.tool.calls, calls, "tools get no keys while typing");
        assert!(h.selected() > 1, "the selection was not touched");
    }

    #[test]
    fn escape_in_a_text_field_keeps_the_selection() {
        let mut h = Harness::new();
        let before = h.selected();
        h.focus_field = true;
        h.frame(vec![]);
        h.frame(vec![]);
        assert!(h.ctx.text_edit_focused());
        h.frame(vec![key(Key::Escape, Modifiers::NONE)]);
        assert!(!h.ctx.text_edit_focused());
        assert_eq!(h.selected(), before);
        h.frame(vec![release(Key::Escape)]);
        h.frame(vec![key(Key::Escape, Modifiers::NONE)]);
        assert_eq!(h.selected(), 0);
    }

    #[test]
    fn escape_closes_an_open_popup_first() {
        let mut h = Harness::new();
        let before = h.selected();
        h.open_combo = true;
        h.frame(vec![]);
        h.frame(vec![]);
        assert!(egui::Popup::is_any_open(&h.ctx));
        h.frame(vec![key(Key::Escape, Modifiers::NONE)]);
        assert!(!egui::Popup::is_any_open(&h.ctx), "egui closed the popup");
        assert_eq!(h.selected(), before, "the Escape that closed the popup did not clear the selection");
        h.open_combo = true;
        h.frame(vec![]);
        h.frame(vec![key(Key::A, CTRL)]);
        assert!(h.last().actions.is_empty());
    }

    #[test]
    fn components_consume_their_keys_first() {
        let mut h = Harness::new();
        h.list_hovered = true;
        h.frame(vec![key(Key::ArrowRight, Modifiers::NONE)]);
        assert_eq!(h.tool.arrows, 0, "the hovered list took the arrow key");
        h.list_hovered = false;
        h.frame(vec![release(Key::ArrowRight), key(Key::ArrowRight, Modifiers::NONE)]);
        assert_eq!(h.tool.arrows, 1);
    }

    #[test]
    fn tab_goes_to_the_tool_not_to_the_menu_bar() {
        let mut h = Harness::new();
        h.frame(vec![]);
        h.frame(vec![key(Key::Tab, Modifiers::NONE)]);
        h.frame(vec![release(Key::Tab)]);
        assert_eq!(h.tool.tabs, 1);
        assert_eq!(h.ctx.memory(|m| m.focused()), None, "no widget took the keyboard focus");
        h.frame(vec![key(Key::Enter, Modifiers::NONE)]);
        assert!(!egui::Popup::is_any_open(&h.ctx));
    }

    #[test]
    fn tab_moves_through_an_open_dialog_and_never_turns_the_2d_view() {
        let mut h = Harness::new();
        h.frame(vec![]);
        h.dialog = true;
        h.frame(vec![]);
        h.frame(vec![]);
        let focused = |h: &Harness| h.ctx.memory(|m| m.focused());
        for (i, shift) in [(0, false), (1, false), (2, false), (1, true)] {
            let modifiers = if shift { Modifiers::SHIFT } else { Modifiers::NONE };
            h.frame(vec![key(Key::Tab, modifiers)]);
            h.frame(vec![release(Key::Tab)]);
            assert_eq!(focused(&h), Some(h.dialog_fields[i]), "Tab step to field {i}");
            assert!(h.routed.iter().all(|r| !r.actions.contains(&Action::CycleView2d)));
        }
        assert_eq!(h.tool.tabs, 0, "the tool never saw the Tab presses");
        h.dialog = false;
        h.frame(vec![]);
        h.frame(vec![]);
        h.frame(vec![key(Key::Tab, Modifiers::NONE)]);
        h.frame(vec![release(Key::Tab)]);
        assert_eq!(h.tool.tabs, 1, "with the dialog closed Tab works in the views again");
    }

    #[test]
    fn tab_still_moves_between_text_fields() {
        let mut h = Harness::new();
        h.focus_field = true;
        h.frame(vec![]);
        h.frame(vec![]);
        let field = h.ctx.memory(|m| m.focused());
        assert!(field.is_some());
        h.frame(vec![key(Key::Tab, Modifiers::NONE)]);
        h.frame(vec![]);
        assert_ne!(h.ctx.memory(|m| m.focused()), field, "egui moved the focus");
        assert_eq!(h.tool.tabs, 0);
    }

    #[test]
    fn clipboard_events_paste_the_event_text() {
        let mut h = Harness::new();
        h.frame(vec![]);
        h.frame(vec![Event::Paste("versioninfo {}".into())]);
        assert_eq!(h.last().actions, vec![Action::Paste]);
        assert_eq!(h.last().paste_text.as_deref(), Some("versioninfo {}"));
        h.frame(vec![Event::Copy]);
        assert_eq!(h.last().actions, vec![Action::Copy]);
        assert_eq!(h.last().paste_text, None);
    }

    #[test]
    fn extra_blocks_stop_everything() {
        let mut h = Harness::new();
        let before = h.selected();
        h.frame(vec![]);
        let calls = h.tool.calls;
        let mut raw =
            egui::RawInput { events: vec![key(Key::Escape, Modifiers::NONE), key(Key::D, CTRL)], ..Default::default() };
        h.router.before_frame(&h.ctx, &mut raw, false, AppCapture::default());
        let mut routed = Routed::default();
        let (router, state, tool) = (&mut h.router, &mut h.state, &mut h.tool);
        let mut out = h.ctx.run_ui(raw, |ui| {
            let ctx = ui.ctx().clone();
            routed = router.after_ui(&ctx, state, tool, AppCapture { keyboard: true, ..AppCapture::default() });
        });
        out.textures_delta.clear();
        assert_eq!(routed, Routed::default());
        assert_eq!(h.tool.calls, calls);
        assert_eq!(h.selected(), before);
    }

    #[test]
    fn a_focused_button_keeps_typing_and_navigation_keys() {
        let mut h = Harness::new();
        let before = h.selected();
        h.focus_button = true;
        h.frame(vec![]);
        h.frame(vec![]);
        assert!(h.ctx.egui_wants_keyboard_input(), "the button has the keyboard focus");
        assert!(!h.ctx.text_edit_focused());
        let calls = h.tool.calls;
        h.frame(vec![key(Key::A, CTRL)]);
        assert_eq!(h.last().actions, vec![Action::SelectAll]);
        for k in [Key::Delete, Key::Backspace, Key::H, Key::U, Key::Num1] {
            h.frame(vec![release(Key::A), key(k, Modifiers::NONE)]);
            assert_eq!(h.last(), &Routed::default(), "{k:?} reached the bindings");
            h.frame(vec![release(k)]);
        }
        h.frame(vec![key(Key::H, Modifiers::SHIFT)]);
        assert_eq!(h.last(), &Routed::default(), "Shift+H types a capital");
        h.frame(vec![release(Key::H), key(Key::ArrowRight, Modifiers::NONE)]);
        assert_eq!(h.tool.arrows, 0, "the arrow key moves the focus, not the selection");
        assert!(h.tool.calls > calls, "the tool still runs every frame");
        assert_eq!(h.button_clicks, 0);
        assert_eq!(h.selected(), before);
    }

    #[test]
    fn escape_on_a_focused_button_only_drops_the_focus() {
        let mut h = Harness::new();
        let before = h.selected();
        h.focus_button = true;
        h.frame(vec![]);
        h.frame(vec![]);
        assert!(h.ctx.memory(|m| m.focused()).is_some());
        h.frame(vec![key(Key::Escape, Modifiers::NONE)]);
        assert_eq!(h.ctx.memory(|m| m.focused()), None, "egui dropped the focus");
        assert_eq!(h.selected(), before, "the first Escape left the button, not the selection");
        h.frame(vec![release(Key::Escape), key(Key::Delete, Modifiers::NONE)]);
        assert_eq!(h.last().actions, vec![Action::Delete]);
        h.frame(vec![release(Key::Delete), key(Key::Escape, Modifiers::NONE)]);
        assert_eq!(h.selected(), 0);
    }

    #[test]
    fn tab_out_of_a_text_field_never_edits_the_map() {
        let mut h = Harness::new();
        let before = h.selected();
        h.focus_field = true;
        h.frame(vec![]);
        h.frame(vec![]);
        assert!(h.ctx.text_edit_focused());
        h.frame(vec![Event::Text("abc".into())]);
        h.frame(vec![key(Key::Tab, Modifiers::NONE)]);
        h.frame(vec![release(Key::Tab)]);
        assert!(h.ctx.egui_wants_keyboard_input(), "Tab moved the focus to the next widget");
        assert!(!h.ctx.text_edit_focused(), "the next widget is not a text field");
        assert_eq!(h.tool.tabs, 0, "egui used the Tab to move the focus");
        let calls = h.tool.calls;
        for k in [Key::Delete, Key::H, Key::S, Key::U, Key::T] {
            h.frame(vec![key(k, Modifiers::NONE), Event::Text(k.name().to_lowercase())]);
            assert_eq!(h.last(), &Routed::default(), "{k:?} reached the bindings");
            h.frame(vec![release(k)]);
        }
        assert_eq!(h.selected(), before);
        h.frame(vec![key(Key::A, CTRL)]);
        assert_eq!(h.last().actions, vec![Action::SelectAll]);
        assert!(h.tool.calls > calls);
    }

    #[test]
    fn widget_keys_are_typing_and_navigation_keys() {
        let alt = Modifiers::ALT;
        let altgr = Modifiers { alt: true, ctrl: true, shift: false, mac_cmd: false, command: true };
        for (k, m) in [
            (Key::H, Modifiers::NONE),
            (Key::H, Modifiers::SHIFT),
            (Key::Delete, Modifiers::NONE),
            (Key::Backspace, Modifiers::NONE),
            (Key::Num4, Modifiers::NONE),
            (Key::OpenBracket, Modifiers::NONE),
            (Key::OpenBracket, altgr),
            (Key::Tab, Modifiers::SHIFT),
            (Key::ArrowLeft, Modifiers::NONE),
            (Key::Enter, Modifiers::NONE),
            (Key::Space, Modifiers::NONE),
            (Key::Escape, Modifiers::NONE),
        ] {
            assert!(is_widget_key(&key(k, m)), "{k:?} {m:?}");
        }
        for (k, m) in [(Key::A, CTRL), (Key::H, alt), (Key::Num4, alt), (Key::F9, Modifiers::NONE), (Key::Z, CTRL)] {
            assert!(!is_widget_key(&key(k, m)), "{k:?} {m:?}");
        }
        assert!(!is_widget_key(&release(Key::H)));
        assert!(!is_widget_key(&Event::Copy));
    }

    #[test]
    fn shell_dialogs_block_shortcuts_and_tool_keys() {
        for which in 0..5 {
            let mut h = Harness::new();
            h.state.settings.language_chosen = false;
            let before = h.selected();
            match which {
                0 => h.state.ui.show_command_palette = true,
                1 => h.state.ui.show_settings = true,
                2 => h.state.ui.show_shortcuts = true,
                3 => h.state.ui.show_about = true,
                _ => h.state.ui.show_first_run = true,
            }
            h.frame(vec![]);
            h.frame(vec![]);
            assert!(h.shell.dialog_open(&h.state), "dialog {which} is open");
            let (calls, frames) = (h.tool.calls, h.routed.len());
            h.frame(vec![key(Key::A, CTRL)]);
            h.frame(vec![release(Key::A), key(Key::Delete, Modifiers::NONE)]);
            h.frame(vec![release(Key::Delete), key(Key::ArrowRight, Modifiers::NONE)]);
            h.frame(vec![release(Key::ArrowRight), Event::Copy]);
            for routed in &h.routed[frames..] {
                assert_eq!(routed, &Routed::default(), "dialog {which} let a shortcut through");
            }
            assert_eq!(h.tool.calls, calls, "dialog {which}: the tool got keys");
            h.frame(vec![key(Key::Escape, Modifiers::NONE)]);
            h.frame(vec![release(Key::Escape)]);
            assert!(!h.shell.dialog_open(&h.state), "dialog {which} stayed open");
            assert_eq!(h.selected(), before, "dialog {which}: the Escape reached the map");
            let calls = h.tool.calls;
            h.frame(vec![key(Key::A, CTRL)]);
            assert_eq!(h.last().actions, vec![Action::SelectAll], "dialog {which}");
            assert_eq!(h.tool.calls, calls + 1, "dialog {which}");
        }
    }

    #[test]
    fn a_dialog_open_when_the_frame_started_blocks_its_keys() {
        let mut h = Harness::new();
        h.frame(vec![]);
        let (before, calls) = (h.selected(), h.tool.calls);
        let mut raw =
            egui::RawInput { events: vec![key(Key::Escape, Modifiers::NONE), key(Key::A, CTRL)], ..Default::default() };
        h.router.before_frame(&h.ctx, &mut raw, false, AppCapture { dialog: true, ..AppCapture::default() });
        let mut routed = Routed::default();
        let (router, state, tool) = (&mut h.router, &mut h.state, &mut h.tool);
        let mut out = h.ctx.run_ui(raw, |ui| {
            let ctx = ui.ctx().clone();
            routed = router.after_ui(&ctx, state, tool, AppCapture::default());
        });
        out.textures_delta.clear();
        assert_eq!(routed, Routed::default());
        assert_eq!(h.tool.calls, calls);
        assert_eq!(h.selected(), before);
    }
}
