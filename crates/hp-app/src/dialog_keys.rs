use egui::{Context, Event, Id, Key, LayerId, RawInput};

#[derive(Clone, Default)]
struct DialogKeys {
    pass: u64,
    active: Option<LayerId>,
    shown: Vec<(LayerId, Id)>,
    last: Vec<(LayerId, Id)>,
    tab_editors: Vec<Id>,
    last_tab_editors: Vec<Id>,
    tabs: Vec<bool>,
    started: bool,
}

fn key() -> Id {
    Id::unique("hp_dialog_keys")
}

fn with<R>(ctx: &Context, f: impl FnOnce(&mut DialogKeys, u64) -> R) -> R {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        let k = d.get_temp_mut_or_default::<DialogKeys>(key());
        f(k, pass)
    })
}

fn roll(k: &mut DialogKeys, pass: u64) {
    if k.pass != pass {
        k.last = std::mem::take(&mut k.shown);
        k.last_tab_editors = std::mem::take(&mut k.tab_editors);
        k.started = true;
        k.pass = pass;
    }
}

fn is_focus_tab(event: &Event) -> Option<Option<bool>> {
    match event {
        Event::Key { key: Key::Tab, pressed, modifiers, .. }
            if !modifiers.alt && !modifiers.ctrl && !modifiers.command && !modifiers.mac_cmd =>
        {
            Some(pressed.then_some(modifiers.shift))
        }
        _ => None,
    }
}

fn is_open(k: &DialogKeys, layer: LayerId) -> bool {
    k.shown.iter().chain(k.last.iter()).any(|(l, _)| *l == layer)
}

pub fn keyboard_dialog(ctx: &Context) -> Option<LayerId> {
    ctx.data(|d| d.get_temp::<DialogKeys>(key())).and_then(|k| k.active.filter(|a| is_open(&k, *a)))
}

pub fn has_keyboard(ctx: &Context) -> bool {
    keyboard_dialog(ctx).is_some()
}

pub fn keep_tab_for(ctx: &Context, id: Id) {
    with(ctx, |k, pass| {
        roll(k, pass);
        if !k.tab_editors.contains(&id) {
            k.tab_editors.push(id);
        }
    });
}

pub fn take_tabs(ctx: &Context, raw: &mut RawInput) -> bool {
    if !has_keyboard(ctx) {
        return false;
    }
    let focused = ctx.memory(|m| m.focused());
    let editor_wants_tab =
        with(ctx, |k, _| focused.is_some_and(|f| k.tab_editors.contains(&f) || k.last_tab_editors.contains(&f)));
    if editor_wants_tab {
        return true;
    }
    let mut tabs = Vec::new();
    raw.events.retain(|e| match is_focus_tab(e) {
        Some(Some(shift)) => {
            tabs.push(shift);
            false
        }
        Some(None) => false,
        None => true,
    });
    if !tabs.is_empty() {
        with(ctx, |k, _| k.tabs.extend(tabs));
    }
    true
}

fn root_layer(ctx: &Context, layer: LayerId) -> LayerId {
    let mut l = layer;
    for _ in 0..16 {
        match ctx.memory(|m| m.areas().parent_layer(l)) {
            Some(p) if p != l => l = p,
            _ => break,
        }
    }
    l
}

pub fn finish_frame(ctx: &Context) {
    let pressed = ctx.input(|i| if i.pointer.any_pressed() { i.pointer.interact_pos() } else { None });
    let hit = pressed.map(|p| ctx.layer_id_at(p).map(|l| root_layer(ctx, l)));
    let palettes = crate::widgets::palette_windows(ctx);
    with(ctx, |k, pass| {
        roll(k, pass);
        if let Some(hit) = hit {
            k.active = hit.filter(|l| k.shown.iter().any(|(s, _)| s == l) && !palettes.contains(&l.id));
        }
        if k.active.is_some_and(|a| !k.shown.iter().any(|(s, _)| *s == a)) {
            k.active = None;
        }
        k.tabs.clear();
    });
}

pub(crate) fn window_shown(ctx: &Context, layer: LayerId, content: Id) {
    let palette = crate::widgets::palette_windows(ctx).contains(&layer.id);
    let tabs = with(ctx, |k, pass| {
        roll(k, pass);
        let newly = k.started && !k.last.iter().any(|(l, _)| *l == layer);
        k.shown.retain(|(l, _)| *l != layer);
        k.shown.push((layer, content));
        if newly && !palette {
            k.active = Some(layer);
        }
        if k.active == Some(layer) { std::mem::take(&mut k.tabs) } else { Vec::new() }
    });
    for shift in tabs {
        cycle_focus(ctx, layer, content, shift);
    }
}

pub fn focusable_widgets(ctx: &Context, layer: LayerId, content: Id) -> Vec<Id> {
    ctx.viewport(|v| {
        let widgets = &v.this_pass.widgets;
        let inside = |mut parent: Id| {
            for _ in 0..64 {
                if parent == content {
                    return true;
                }
                match widgets.get(parent) {
                    Some(w) if w.parent_id != parent => parent = w.parent_id,
                    _ => return false,
                }
            }
            false
        };
        widgets
            .get_layer(layer)
            .filter(|w| w.enabled && w.visible && w.sense.is_focusable() && w.sense.interactive())
            .filter(|w| w.id != content && inside(w.parent_id))
            .map(|w| w.id)
            .collect()
    })
}

fn cycle_focus(ctx: &Context, layer: LayerId, content: Id, shift: bool) {
    let list = focusable_widgets(ctx, layer, content);
    if list.is_empty() {
        return;
    }
    let focused = ctx.memory(|m| m.focused());
    let n = list.len();
    let next = match focused.and_then(|f| list.iter().position(|id| *id == f)) {
        Some(i) if shift => (i + n - 1) % n,
        Some(i) => (i + 1) % n,
        None if shift => n - 1,
        None => 0,
    };
    focus_select_all(ctx, list[next]);
}

pub fn focus_select_all(ctx: &Context, id: Id) {
    ctx.memory_mut(|m| m.request_focus(id));
    let mut st = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
        egui::text::CCursor::new(0),
        egui::text::CCursor::new(usize::MAX / 2),
    )));
    st.store(ctx, id);
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(shift: bool) -> Event {
        let modifiers = if shift { egui::Modifiers::SHIFT } else { egui::Modifiers::NONE };
        Event::Key { key: Key::Tab, physical_key: Some(Key::Tab), pressed: true, repeat: false, modifiers }
    }

    #[derive(Default)]
    struct Form {
        values: [f64; 3],
        open: bool,
        ids: Vec<Id>,
        list: Vec<Id>,
    }

    fn frame(ctx: &Context, form: &mut Form, mut events: Vec<Event>) -> bool {
        let mut raw = RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
            ..Default::default()
        };
        raw.events.append(&mut events);
        let took = take_tabs(ctx, &mut raw);
        let left = raw.events.iter().filter(|e| is_focus_tab(e).is_some()).count();
        let _ = ctx.run_ui(raw, |ui| {
            let _ = ui.button("outside");
            let mut open = form.open;
            if let Some(r) = crate::chrome::Window::new("Form").open(&mut open).show(ui.ctx(), |ui| {
                let mut ids = Vec::new();
                for v in &mut form.values {
                    ids.push(ui.add(egui::DragValue::new(v)).id);
                }
                ids
            }) && let Some(ids) = r.inner
            {
                form.ids = ids;
            }
            form.open = open;
            let shown = ui.ctx().data(|d| d.get_temp::<DialogKeys>(key())).map(|k| k.shown).unwrap_or_default();
            form.list = shown.first().map(|(l, c)| focusable_widgets(ui.ctx(), *l, *c)).unwrap_or_default();
            finish_frame(ui.ctx());
        });
        assert!(!took || left == 0, "the dialog keeps its Tab presses away from the shortcuts");
        took
    }

    #[test]
    fn tab_cycles_inside_the_dialog_that_has_the_keyboard() {
        let ctx = Context::default();
        let mut form = Form { values: [1.0, 2.0, 3.0], ..Form::default() };
        frame(&ctx, &mut form, Vec::new());
        assert!(!frame(&ctx, &mut form, vec![tab(false)]), "no dialog: Tab is left to the app");
        form.open = true;
        frame(&ctx, &mut form, Vec::new());
        frame(&ctx, &mut form, Vec::new());
        assert!(has_keyboard(&ctx), "a dialog that just opened takes the keyboard");
        assert!(form.list.len() == 4 && form.list[..3] == form.ids[..], "only the dialog widgets: {:?}", form.list);
        let focused = || ctx.memory(|m| m.focused());
        assert!(frame(&ctx, &mut form, vec![tab(false)]));
        frame(&ctx, &mut form, Vec::new());
        assert_eq!(focused(), Some(form.ids[0]));
        frame(&ctx, &mut form, vec![tab(false)]);
        frame(&ctx, &mut form, Vec::new());
        assert_eq!(focused(), Some(form.ids[1]));
        frame(&ctx, &mut form, vec![tab(true)]);
        frame(&ctx, &mut form, vec![tab(true)]);
        frame(&ctx, &mut form, Vec::new());
        assert_eq!(focused(), form.list.last().copied(), "Shift+Tab wraps around to the last widget");
        frame(&ctx, &mut form, vec![tab(false)]);
        frame(&ctx, &mut form, Vec::new());
        assert_eq!(focused(), Some(form.ids[0]), "Tab wraps around to the first field");
        let state = egui::text_edit::TextEditState::load(&ctx, form.ids[0]).expect("editing");
        let range = state.cursor.char_range().expect("range");
        assert_ne!(range.primary.index, range.secondary.index, "the field text is selected");
        keep_tab_for(&ctx, form.ids[0]);
        let mut raw = RawInput { events: vec![tab(false)], ..Default::default() };
        assert!(take_tabs(&ctx, &mut raw));
        assert_eq!(raw.events.len(), 1, "a code editor keeps its Tab key");
        form.open = false;
        frame(&ctx, &mut form, Vec::new());
        frame(&ctx, &mut form, Vec::new());
        assert!(!has_keyboard(&ctx), "a closed dialog gives the keyboard back");
    }

    #[test]
    fn a_click_outside_gives_the_keyboard_back() {
        let ctx = Context::default();
        let mut form = Form { open: true, ..Form::default() };
        frame(&ctx, &mut form, Vec::new());
        frame(&ctx, &mut form, Vec::new());
        frame(&ctx, &mut form, Vec::new());
        assert!(!has_keyboard(&ctx), "windows open at startup do not take the keyboard");
        let press = |pos: egui::Pos2, pressed: bool| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let far = egui::pos2(790.0, 590.0);
        frame(&ctx, &mut form, vec![Event::PointerMoved(far)]);
        frame(&ctx, &mut form, vec![press(far, true)]);
        frame(&ctx, &mut form, vec![press(far, false)]);
        assert!(!has_keyboard(&ctx), "clicking a viewport gives the shortcuts back");
        assert!(!frame(&ctx, &mut form, vec![tab(false)]));
        let inside = ctx.memory(|m| m.area_rect(form_layer(&ctx).id)).expect("rect").center();
        frame(&ctx, &mut form, vec![Event::PointerMoved(inside)]);
        frame(&ctx, &mut form, vec![press(inside, true)]);
        frame(&ctx, &mut form, vec![press(inside, false)]);
        assert!(has_keyboard(&ctx), "clicking the dialog takes the keyboard again");
    }

    fn form_layer(ctx: &Context) -> LayerId {
        ctx.data(|d| d.get_temp::<DialogKeys>(key())).and_then(|k| k.shown.first().map(|(l, _)| *l)).expect("shown")
    }
}
