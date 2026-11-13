use egui::RichText;
use hp_app::{AppState, Severity, t, tf};
use hp_editor::ObjRef;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Start,
    Stop,
    Teleport,
    Back,
    OpenFire,
    Resync,
    MarkCompiled,
    RemoveAddon,
}

fn row(ui: &mut egui::Ui, label: &str, value: impl Into<egui::WidgetText>) {
    ui.label(RichText::new(label).weak());
    ui.label(value);
    ui.end_row();
}

fn ms(d: Option<std::time::Duration>) -> String {
    d.map(|d| format!("{:.0}", d.as_secs_f64() * 1000.0)).unwrap_or_else(|| "-".to_owned())
}

pub fn panel(state: &mut AppState, ctx: &egui::Context) -> Vec<Request> {
    let mut out = Vec::new();
    if !state.runtime.livelink.panel_open {
        return out;
    }
    let mut open = true;
    let root = super::game_root(state);
    let installed = root.as_deref().is_some_and(hp_livelink::addon::is_installed);
    hp_app::chrome::Window::new(t!("livelink.panel_title"))
        .id(egui::Id::unique("hp_livelink_panel"))
        .open(&mut open)
        .default_width(420.0)
        .default_pos([90.0, 110.0])
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            let session = state.runtime.livelink.session.as_ref();
            let link_state = session.map(hp_livelink::Session::state);
            egui::Grid::new("hp_livelink_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                let color = match link_state {
                    Some(hp_livelink::LinkState::Connected) => hp_app::theme::success(),
                    Some(hp_livelink::LinkState::WaitingForGame) => hp_app::theme::warning(),
                    None => ui.visuals().weak_text_color(),
                    Some(_) => hp_app::theme::error(),
                };
                row(ui, t!("livelink.state_label"), RichText::new(super::state_text(link_state)).color(color).strong());
                if let Some(s) = session {
                    let st = &s.status;
                    let same = st.game_map.is_empty() || st.game_map.eq_ignore_ascii_case(&s.map);
                    let map = if same { s.map.clone() } else { format!("{} / {}", s.map, st.game_map) };
                    row(ui, t!("livelink.map_label"), map);
                    let addon = match st.addon.as_str() {
                        "mounted" => t!("livelink.addon_mounted"),
                        "injected" => t!("livelink.addon_injected"),
                        _ if installed => t!("livelink.addon_installed"),
                        _ => t!("livelink.addon_missing"),
                    };
                    row(ui, t!("livelink.addon_label"), addon);
                    row(
                        ui,
                        t!("livelink.latency_label"),
                        tf!("livelink.latency_value", ms(st.latency), ms(st.average_latency())),
                    );
                    row(ui, t!("livelink.pending_label"), st.pending.to_string());
                    row(ui, t!("livelink.changed_label"), st.changed.to_string());
                    if let Some(m) = s.matches.as_ref() {
                        row(
                            ui,
                            t!("livelink.matched_label"),
                            tf!("livelink.matched_value", m.by_id.len(), m.expected()),
                        );
                    }
                    row(
                        ui,
                        t!("livelink.in_game_label"),
                        tf!("livelink.previews_value", st.created, st.ghosts, st.lights),
                    );
                } else {
                    let addon = if installed { t!("livelink.addon_installed") } else { t!("livelink.addon_missing") };
                    row(ui, t!("livelink.addon_label"), addon);
                }
            });
            ui.add_space(6.0);
            match session {
                None => {
                    ui.label(RichText::new(t!("livelink.off_hint")).weak());
                }
                Some(s) => {
                    if !s.is_connected() {
                        ui.label(RichText::new(t!("livelink.waiting_hint")).weak());
                    }
                    ui.label(RichText::new(t!("livelink.needs_compile_label")).strong());
                    if s.status.needs_compile.is_empty() {
                        ui.label(RichText::new(t!("livelink.nothing_to_compile")).weak());
                    }
                    for r in &s.status.needs_compile {
                        ui.label(
                            RichText::new(format!("- {}", super::reason_text(*r))).color(hp_app::theme::warning()),
                        );
                    }
                    if !s.status.skipped.is_empty() {
                        ui.label(RichText::new(t!("livelink.skipped_label")).strong());
                        for (id, why) in s.status.skipped.iter().take(8) {
                            ui.label(RichText::new(format!("#{id}: {why}")).small());
                        }
                    }
                    ui.label(RichText::new(t!("livelink.errors_label")).strong());
                    if s.status.errors.is_empty() {
                        ui.label(RichText::new(t!("livelink.no_errors")).weak());
                    }
                    for e in s.status.errors.iter().rev().take(6) {
                        ui.label(RichText::new(e).small().color(hp_app::theme::error()));
                    }
                }
            }
            ui.separator();
            let on = session.is_some();
            let connected = session.is_some_and(hp_livelink::Session::is_connected);
            ui.horizontal_wrapped(|ui| {
                if !on && ui.button(t!("livelink.start")).clicked() {
                    out.push(Request::Start);
                }
                if on && ui.button(t!("livelink.stop")).clicked() {
                    out.push(Request::Stop);
                }
                if ui.add_enabled(connected, egui::Button::new(t!("livelink.teleport"))).clicked() {
                    out.push(Request::Teleport);
                }
                if ui.add_enabled(connected, egui::Button::new(t!("livelink.back"))).clicked() {
                    out.push(Request::Back);
                }
                if ui.add_enabled(connected, egui::Button::new(t!("livelink.fire"))).clicked() {
                    out.push(Request::OpenFire);
                }
                if ui.add_enabled(on, egui::Button::new(t!("livelink.resync"))).clicked() {
                    out.push(Request::Resync);
                }
                if ui.add_enabled(on, egui::Button::new(t!("livelink.mark_compiled"))).clicked() {
                    out.push(Request::MarkCompiled);
                }
                if ui.add_enabled(installed || on, egui::Button::new(t!("livelink.remove_addon"))).clicked() {
                    out.push(Request::RemoveAddon);
                }
            });
            ui.separator();
            let s = &mut state.settings.livelink;
            let before = s.clone();
            ui.checkbox(&mut s.camera_to_game, t!("action.live_link_camera_to_game"));
            ui.checkbox(&mut s.camera_from_game, t!("action.live_link_camera_from_game"));
            ui.checkbox(&mut s.highlight_selection, t!("livelink.set.highlight"));
            ui.checkbox(&mut s.light_previews, t!("livelink.set.light_previews"));
            ui.checkbox(&mut s.load_into_running_game, t!("livelink.set.load_running"));
            ui.checkbox(&mut s.remove_addon_on_stop, t!("livelink.set.remove_on_stop"));
            if *s != before {
                if s.camera_to_game && !before.camera_to_game {
                    s.camera_from_game = false;
                }
                if s.camera_from_game && !before.camera_from_game {
                    s.camera_to_game = false;
                }
                state.settings_dirty = true;
            }
        });
    if !open {
        state.runtime.livelink.panel_open = false;
    }
    if state.runtime.livelink.session.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }
    out
}

pub fn selected_target(state: &AppState) -> Option<String> {
    let scene = state.doc.scene();
    state.doc.selection().objects.iter().find_map(|o| match *o {
        ObjRef::Entity(id) => {
            let e = &scene.entity(id)?.entity;
            Some(
                e.get("targetname")
                    .filter(|n| !n.trim().is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("#{id}")),
            )
        }
        ObjRef::Solid(_) => None,
    })
}

fn target_class(state: &AppState, target: &str) -> Option<String> {
    let scene = state.doc.scene();
    if let Some(id) = target.strip_prefix('#').and_then(|n| n.parse::<u32>().ok()) {
        return scene.entity(id).map(|n| n.entity.classname.clone());
    }
    scene
        .entities()
        .values()
        .find(|n| n.entity.get("targetname").is_some_and(|v| v.eq_ignore_ascii_case(target)))
        .map(|n| n.entity.classname.clone())
}

pub fn fire_dialog(state: &mut AppState, ctx: &egui::Context) {
    if !state.runtime.livelink.fire_open {
        return;
    }
    let mut open = true;
    let mut fire = false;
    let class = target_class(state, state.runtime.livelink.fire.target.trim());
    let inputs: Vec<String> = class
        .as_deref()
        .and_then(|c| state.assets.fgd().and_then(|f| f.class(c)))
        .map(|c| c.inputs.iter().map(|i| i.name.clone()).collect())
        .unwrap_or_default();
    let selection = selected_target(state);
    let connected = state.runtime.livelink.is_connected();
    hp_app::chrome::Window::new(t!("livelink.fire_title"))
        .id(egui::Id::unique("hp_livelink_fire"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            let form = &mut state.runtime.livelink.fire;
            egui::Grid::new("hp_livelink_fire_grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(t!("livelink.fire_target"));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut form.target).desired_width(180.0));
                    if let Some(sel) = &selection
                        && ui.small_button(t!("livelink.fire_use_selection")).clicked()
                    {
                        form.target = sel.clone();
                    }
                });
                ui.end_row();
                ui.label(t!("livelink.fire_input"));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut form.input).desired_width(140.0));
                    if !inputs.is_empty() {
                        egui::ComboBox::from_id_salt("hp_livelink_fire_inputs").selected_text("").width(24.0).show_ui(
                            ui,
                            |ui| {
                                for name in &inputs {
                                    if ui.selectable_label(form.input == *name, name).clicked() {
                                        form.input = name.clone();
                                    }
                                }
                            },
                        );
                    }
                });
                ui.end_row();
                ui.label(t!("livelink.fire_param"));
                ui.add(egui::TextEdit::singleline(&mut form.param).desired_width(180.0));
                ui.end_row();
                ui.label(t!("livelink.fire_delay"));
                ui.add(egui::DragValue::new(&mut form.delay).range(0.0..=3600.0).speed(0.1));
                ui.end_row();
            });
            ui.label(RichText::new(t!("livelink.fire_hint")).small().weak());
            ui.add_space(4.0);
            if ui.add_enabled(connected, egui::Button::new(t!("livelink.fire_button"))).clicked() {
                fire = true;
            }
        });
    if fire {
        let f = state.runtime.livelink.fire.clone();
        match super::fire(state, &f.target, &f.input, &f.param, f64::from(f.delay)) {
            Ok(text) => state.notify(Severity::Success, text),
            Err(e) => state.notify(Severity::Warning, e),
        }
    }
    if !open {
        state.runtime.livelink.fire_open = false;
    }
}
