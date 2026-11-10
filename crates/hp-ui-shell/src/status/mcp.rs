use egui::{RichText, Ui};
use hp_app::extend::{StatusContext, StatusSegment, StatusSide, status_order};
use hp_app::i18n::TrTable;
use hp_app::{Action, AppState, t, tf};

pub(super) fn segments() -> Vec<Box<dyn StatusSegment>> {
    vec![Box::new(McpSegment)]
}

pub(crate) fn clients_text(n: usize) -> String {
    if n == 1 { t!("shell.mcp.client_one").to_owned() } else { tf!("shell.mcp.client_many", n) }
}

pub(crate) fn tooltip(state: &AppState) -> String {
    let mcp = &state.runtime.mcp;
    if !mcp.running {
        return match &mcp.error {
            Some(e) => tf!("shell.mcp.tip_failed", e),
            None => t!("shell.mcp.tip_off").to_owned(),
        };
    }
    let port = mcp.port.map(|p| p.to_string()).unwrap_or_default();
    let mut tip = tf!("shell.mcp.tip_on", port, clients_text(mcp.clients));
    if let Some(tool) = &mcp.last_tool {
        tip.push('\n');
        tip.push_str(&tf!("shell.mcp.tip_last", tool, mcp.calls));
    }
    tip
}

struct McpSegment;

impl StatusSegment for McpSegment {
    fn id(&self) -> &'static str {
        "mcp_server"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Right
    }

    fn order(&self) -> i32 {
        status_order::CONTENT + 30
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, _cx: &mut StatusContext) {
        if !state.runtime.mcp.running {
            return;
        }
        ui.separator();
        let text = format!("MCP · {}", clients_text(state.runtime.mcp.clients));
        let color = ui.visuals().strong_text_color();
        let r = ui.add(egui::Label::new(RichText::new(text).small().color(color)).sense(egui::Sense::click()));
        if crate::common::lazy_hover_text(r, || tooltip(state)).clicked() {
            state.request(Action::McpPanel);
        }
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.mcp.pill_off", "MCP off", "MCP inactif"),
    ("shell.mcp.pill_on", "MCP on", "MCP actif"),
    ("shell.mcp.client_one", "1 client", "1 client"),
    ("shell.mcp.client_many", "{0} clients", "{0} clients"),
    (
        "shell.mcp.tip_off",
        "MCP server of this window: off. Click to switch it on so AI apps (Claude, Codex, Gemini) can edit this map.",
        "Serveur MCP de cette fenêtre : inactif. Cliquez pour l'activer et permettre aux applications d'IA (Claude, Codex, Gemini) de modifier cette map.",
    ),
    (
        "shell.mcp.tip_on",
        "MCP server of this window: on (local port {0}, {1}). Click to switch it off.",
        "Serveur MCP de cette fenêtre : actif (port local {0}, {1}). Cliquez pour le désactiver.",
    ),
    ("shell.mcp.tip_last", "Last tool: {0} ({1} calls)", "Dernier outil : {0} ({1} appels)"),
    ("shell.mcp.tip_failed", "The MCP server could not start: {0}", "Le serveur MCP n'a pas pu démarrer : {0}"),
    (
        "shell.mcp.more",
        "Install in AI apps, setup and tools",
        "Installation dans les applications d'IA, configuration et outils",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_segment_draws_on_and_off() {
        let mut state = hp_app::testing::test_state();
        crate::register_translations();
        for running in [false, true] {
            state.runtime.mcp.running = running;
            state.runtime.mcp.port = running.then_some(51234);
            state.runtime.mcp.clients = usize::from(running);
            state.runtime.mcp.last_tool = running.then(|| "create_brush".to_owned());
            hp_app::testing::run_ui(&mut state, |ui, state| {
                let mut cx = StatusContext::default();
                McpSegment.ui(ui, state, &mut cx);
            });
            let tip = tooltip(&state);
            assert_eq!(tip.contains("51234"), running, "{tip}");
        }
        assert_eq!(clients_text(1), "1 client");
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        let font = egui::FontId::proportional(14.0);
        for g in ["·", "⏷", "🔌", "⬇"] {
            assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, g)), "missing glyph {g}");
        }
        assert_eq!(clients_text(3), "3 clients");
        assert!(hp_app::i18n::lint_table(TR).is_empty());
    }

    #[test]
    fn the_pill_toggles_the_server() {
        let mut state = hp_app::testing::test_state();
        crate::register_translations();
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 60.0));
        let frame = |state: &mut AppState, events: Vec<egui::Event>| {
            let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
            let mut rect = egui::Rect::NOTHING;
            let mut out = ctx.run_ui(input, |ui| {
                ui.horizontal(|ui| rect = crate::toolbar::mcp_pill(ui, state).rect);
            });
            out.textures_delta.clear();
            rect
        };
        let rect = frame(&mut state, vec![]);
        assert!(rect.width() > 10.0);
        let p = rect.center();
        let button = |pressed| egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut state, vec![egui::Event::PointerMoved(p), button(true)]);
        frame(&mut state, vec![button(false)]);
        assert_eq!(state.requested_actions, vec![Action::ToggleMcpServer]);
        state.runtime.mcp.running = true;
        state.runtime.mcp.port = Some(51234);
        let on = frame(&mut state, vec![]);
        assert!(on.width() > rect.width());
    }
}
