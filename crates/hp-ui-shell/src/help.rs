use egui::{RichText, Ui};
use hp_app::settings::NavScheme;
use hp_app::{Action, AppState, t, tf};

use crate::common::{CATEGORIES, EscapeGuard, accent_text, category_label, heading, search_field, window_layer};
use crate::fuzzy::{Query, fold};
use crate::keys::{alias_text, shortcut_search_text, shortcut_text};

pub const REPOSITORY_URL: &str = "https://github.com/oracleidt/hammerpro";
pub const VDC_URL: &str = "https://developer.valvesoftware.com/wiki/Valve_Hammer_Editor";

pub fn scheme_label_key(scheme: NavScheme) -> &'static str {
    match scheme {
        NavScheme::HammerPro => "shell.view.scheme_hammerpro",
        NavScheme::Hammer => "shell.view.scheme_hammer",
        NavScheme::HammerPlusPlus => "shell.view.scheme_hammerpp",
    }
}

pub fn scheme_controls(scheme: NavScheme) -> Vec<(&'static str, &'static str)> {
    let mut out =
        vec![("shell.help.ctl_rmb", "shell.help.ctl_rmb_desc"), ("shell.help.ctl_wasd", "shell.help.ctl_wasd_desc")];
    if scheme.middle_looks() {
        out.push(("shell.view.ctl_mmb", "shell.view.ctl_mmb_look"));
    } else {
        out.push(("shell.help.ctl_mmb", "shell.help.ctl_mmb_desc"));
    }
    if scheme.hammer_gestures() {
        out.push(("shell.view.ctl_space_left", "shell.view.ctl_space_left_desc"));
        out.push(("shell.view.ctl_space_right", "shell.view.ctl_space_right_desc"));
        out.push(("shell.view.ctl_both", "shell.view.ctl_both_desc"));
        out.push(("shell.view.ctl_arrows_3d", "shell.view.ctl_arrows_3d_desc"));
    }
    out.push(("shell.view.ctl_arrows_2d", "shell.view.ctl_arrows_2d_desc"));
    out.push(("shell.view.ctl_digits", "shell.view.ctl_digits_desc"));
    out.push(("shell.help.ctl_fly_click", "shell.help.ctl_fly_click_desc"));
    out
}

fn viewport_controls(scheme: NavScheme) -> Vec<(&'static str, &'static str)> {
    let mut out = scheme_controls(scheme);
    out.insert(2, ("shell.help.ctl_shift", "shell.help.ctl_shift_desc"));
    out.extend([
        ("shell.help.ctl_wheel", "shell.help.ctl_wheel_desc"),
        ("shell.help.ctl_grid_keys", "shell.help.ctl_grid_keys_desc"),
        ("shell.help.ctl_grids", "shell.help.ctl_grids_desc"),
        ("shell.help.ctl_click", "shell.help.ctl_click_desc"),
        ("shell.help.ctl_ctrl_click", "shell.help.ctl_ctrl_click_desc"),
        ("shell.help.ctl_stretch", "shell.help.ctl_stretch_desc"),
    ]);
    out
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShortcutsOutput {
    pub customize: bool,
}

#[derive(Debug, Default)]
pub struct ShortcutsWindow {
    pub filter: String,
    escape: EscapeGuard,
    was_open: bool,
    focus_search: bool,
}

fn shortcuts_window_id() -> egui::Id {
    egui::Id::unique("hp_shell_shortcuts")
}

pub fn shortcuts_search_id() -> egui::Id {
    egui::Id::unique("hp_shell_shortcuts_search")
}

fn about_window_id() -> egui::Id {
    egui::Id::unique("hp_shell_about")
}

impl ShortcutsWindow {
    pub fn matching(&self, state: &AppState, cat: hp_app::actions::Category) -> Vec<Action> {
        let q = Query::new(&self.filter);
        Action::ALL
            .iter()
            .copied()
            .filter(|a| a.category() == cat)
            .filter(|&a| {
                q.is_empty()
                    || q.best_of([
                        fold(a.label()).as_str(),
                        fold(&shortcut_search_text(&state.keys, a)).as_str(),
                        fold(a.id()).as_str(),
                    ])
                    .is_some()
            })
            .collect()
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> ShortcutsOutput {
        let mut out = ShortcutsOutput::default();
        let open_now = state.ui.show_shortcuts;
        if open_now && !self.was_open {
            self.escape.reset();
            self.focus_search = true;
        }
        self.was_open = open_now;
        if !open_now {
            return out;
        }
        let layer = window_layer(shortcuts_window_id());
        if self.escape.should_close(ctx, layer) {
            state.ui.show_shortcuts = false;
            self.was_open = false;
            return out;
        }
        let mut open = true;
        hp_app::chrome::Window::new(format!("⌨ {}", t!("action.keyboard_shortcuts")))
            .id(shortcuts_window_id())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([560.0, 600.0])
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                let row_height = ui.spacing().interact_size.y + 2.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), row_height),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if ui.button(format!("✏ {}", t!("shell.help.customize"))).clicked() {
                            out.customize = true;
                        }
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                let focus = std::mem::take(&mut self.focus_search);
                                search_field(
                                    ui,
                                    shortcuts_search_id(),
                                    &mut self.filter,
                                    t!("shell.keys.search"),
                                    focus,
                                );
                            },
                        );
                    },
                );
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.contents(ui, state));
            });
        if !open {
            state.ui.show_shortcuts = false;
            self.was_open = false;
        } else {
            self.escape.finish(ctx, layer);
        }
        out
    }

    fn contents(&mut self, ui: &mut Ui, state: &AppState) {
        let mut any = false;
        for cat in CATEGORIES {
            let actions = self.matching(state, cat);
            if actions.is_empty() {
                continue;
            }
            any = true;
            heading(ui, category_label(cat));
            egui::Grid::new(("hp_shell_shortcut_ref", cat.label_key()))
                .num_columns(2)
                .striped(true)
                .spacing([24.0, 3.0])
                .min_col_width(240.0)
                .show(ui, |ui| {
                    for a in actions {
                        ui.label(a.label());
                        let text = shortcut_text(&state.keys, a);
                        let aliases = alias_text(&state.keys, a);
                        if text.is_empty() && aliases.is_empty() {
                            ui.label(RichText::new("-").color(ui.visuals().weak_text_color()));
                        } else {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                if !text.is_empty() {
                                    ui.label(RichText::new(text).monospace().color(accent_text(ui.visuals())));
                                }
                                if !aliases.is_empty() {
                                    ui.label(
                                        RichText::new(tf!("shell.keys.also", aliases))
                                            .monospace()
                                            .color(ui.visuals().weak_text_color()),
                                    );
                                }
                            });
                        }
                        ui.end_row();
                    }
                });
        }
        let q = Query::new(&self.filter);
        let scheme = state.settings.workflow.nav_scheme;
        let controls: Vec<(&str, &str)> = viewport_controls(scheme)
            .into_iter()
            .map(|(k, d)| (t!(k), t!(d)))
            .filter(|(k, d)| q.is_empty() || q.best_of([fold(k).as_str(), fold(d).as_str()]).is_some())
            .collect();
        if !controls.is_empty() {
            any = true;
            heading(ui, &tf!("shell.help.viewport_controls_of", t!(scheme_label_key(scheme))));
            egui::Grid::new("hp_shell_shortcut_ref_mouse")
                .num_columns(2)
                .striped(true)
                .spacing([24.0, 3.0])
                .min_col_width(240.0)
                .show(ui, |ui| {
                    for (k, d) in controls {
                        ui.label(d);
                        ui.label(RichText::new(k).monospace().color(accent_text(ui.visuals())));
                        ui.end_row();
                    }
                });
        }
        let gestures: Vec<(&str, &str)> = crate::settings_dialog::TRACKPAD_GESTURES
            .iter()
            .map(|(k, d)| (t!(k), t!(d)))
            .filter(|(k, d)| q.is_empty() || q.best_of([fold(k).as_str(), fold(d).as_str()]).is_some())
            .collect();
        if !gestures.is_empty() {
            any = true;
            heading(ui, t!("shell.input.gestures"));
            egui::Grid::new("hp_shell_shortcut_ref_trackpad")
                .num_columns(2)
                .striped(true)
                .spacing([24.0, 3.0])
                .min_col_width(240.0)
                .show(ui, |ui| {
                    for (k, d) in gestures {
                        ui.label(d);
                        ui.label(RichText::new(k).monospace().color(accent_text(ui.visuals())));
                        ui.end_row();
                    }
                });
        }
        if !any {
            ui.add_space(8.0);
            ui.label(RichText::new(t!("shell.palette.no_results")).color(ui.visuals().weak_text_color()));
        }
    }
}

pub fn about_window(ctx: &egui::Context, state: &mut AppState) {
    if !state.ui.show_about {
        return;
    }
    let layer = window_layer(about_window_id());
    if EscapeGuard::default().should_close(ctx, layer) {
        state.ui.show_about = false;
        return;
    }
    let mut open = true;
    let mut close = false;
    hp_app::chrome::Window::new(t!("action.about"))
        .id(about_window_id())
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .default_width(420.0)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(6.0);
                hp_app::widgets::logo(ui, 64.0);
                ui.add_space(4.0);
                ui.label(RichText::new(t!("app.name")).size(26.0).strong());
                ui.label(
                    RichText::new(format!("{} {}", t!("shell.about.version"), env!("CARGO_PKG_VERSION")))
                        .monospace()
                        .color(ui.visuals().weak_text_color()),
                );
                if let Some(build) = &state.runtime.workflow.build_info {
                    ui.label(
                        RichText::new(tf!("shell.about.build", build))
                            .monospace()
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                }
                ui.add_space(4.0);
                ui.label(RichText::new(t!("app.tagline")).italics());
                ui.add_space(8.0);
                ui.label(t!("shell.about.description"));
                ui.add_space(8.0);
                ui.label(RichText::new(t!("shell.about.license")).small());
                ui.label(RichText::new(t!("shell.about.credits")).small().color(ui.visuals().weak_text_color()));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let width = 300.0;
                    ui.add_space(((ui.available_width() - width) * 0.5).max(0.0));
                    ui.hyperlink_to(format!("🐙 {}", t!("shell.about.github")), REPOSITORY_URL);
                    ui.add_space(12.0);
                    ui.hyperlink_to(format!("📖 {}", t!("shell.about.vdc")), VDC_URL);
                });
                ui.add_space(10.0);
                if ui.button(t!("common.close")).clicked() {
                    close = true;
                }
                ui.add_space(4.0);
            });
        });
    if !open || close {
        state.ui.show_about = false;
    } else {
        EscapeGuard::default().finish(ctx, layer);
    }
}

pub(crate) const TR: hp_app::i18n::TrTable = &[
    ("shell.help.customize", "Customize...", "Personnaliser..."),
    ("shell.help.viewport_controls", "Viewport controls", "Contrôles des vues"),
    ("shell.help.viewport_controls_of", "Viewport controls ({0})", "Contrôles des vues ({0})"),
    ("shell.about.build", "Build {0}", "Compilation {0}"),
    ("shell.help.ctl_rmb", "Right mouse drag", "Clic droit + glisser"),
    ("shell.help.ctl_rmb_desc", "Look around (3D view)", "Regarder autour (vue 3D)"),
    ("shell.help.ctl_wasd", "W A S D (+ Q E)", "Z Q S D (+ A E), W A S D en QWERTY"),
    ("shell.help.ctl_wasd_desc", "Fly while holding the right mouse button", "Se déplacer en maintenant le clic droit"),
    ("shell.help.ctl_shift", "Shift", "Maj"),
    ("shell.help.ctl_shift_desc", "Move the camera faster", "Déplacer la caméra plus vite"),
    ("shell.help.ctl_mmb", "Middle mouse or Space + drag", "Clic molette ou Espace + glisser"),
    ("shell.help.ctl_mmb_desc", "Pan the view", "Faire défiler la vue"),
    ("shell.help.ctl_wheel", "Mouse wheel", "Molette"),
    ("shell.help.ctl_wheel_desc", "Zoom (2D) or move forward (3D)", "Zoomer (2D) ou avancer (3D)"),
    ("shell.help.ctl_grid_keys", "[ and ], Ctrl + wheel (2D)", "[ et ], Ctrl + molette (2D)"),
    (
        "shell.help.ctl_grid_keys_desc",
        "Smaller or larger grid: the global grid, or in the per view mode the grid of the view under the mouse",
        "Grille plus fine ou plus large : la grille globale, ou en mode une grille par vue celle de la vue sous la souris",
    ),
    ("shell.help.ctl_grids", "Grid button (toolbar), Map > Grids", "Bouton Grille (barre d'outils), Map > Grilles"),
    (
        "shell.help.ctl_grids_desc",
        "Choose the grid: global, one per view, a custom grid (step per axis, origin, orientation, limits) or the grid of the selection; grids from the selection or a face, attached to objects",
        "Choisir la grille : globale, une par vue, personnalisée (pas par axe, origine, orientation, limites) ou celle de la sélection ; grilles depuis la sélection ou une face, attachées aux objets",
    ),
    ("shell.help.ctl_click", "Left click", "Clic gauche"),
    ("shell.help.ctl_click_desc", "Use the active tool", "Utiliser l'outil actif"),
    ("shell.help.ctl_ctrl_click", "Ctrl + click", "Ctrl + clic"),
    ("shell.help.ctl_ctrl_click_desc", "Add to or remove from the selection", "Ajouter ou retirer de la sélection"),
    (
        "shell.help.ctl_stretch",
        "Drag a face arrow (3D, Selection tool)",
        "Glisser une flèche de face (3D, outil Sélection)",
    ),
    (
        "shell.help.ctl_stretch_desc",
        "Stretch the selected brushes along that axis, the opposite face stays (Ctrl: from the center, Alt: no snap, Shift: clone)",
        "Étirer les brushes sélectionnés sur cet axe, la face opposée reste en place (Ctrl : depuis le centre, Alt : sans magnétisme, Maj : cloner)",
    ),
    ("shell.help.ctl_fly_click", "Left click while flying (Z)", "Clic gauche en vol (Z)"),
    (
        "shell.help.ctl_fly_click_desc",
        "Select at the crosshair (Ctrl + click: add or remove), also with the Camera tool",
        "Sélectionner sous le viseur (Ctrl + clic : ajouter ou retirer), aussi avec l'outil Caméra",
    ),
    ("shell.about.version", "Version", "Version"),
    (
        "shell.about.description",
        "A fast, modern level editor for Source engine maps, made for Garry's Mod mappers.",
        "Un éditeur de maps rapide et moderne pour le moteur Source, pensé pour les mappeurs de Garry's Mod.",
    ),
    (
        "shell.about.license",
        "Free and open source software under the MIT license.",
        "Logiciel libre et open source sous licence MIT.",
    ),
    (
        "shell.about.credits",
        "Built with Rust, egui and wgpu. Thanks to the Source modding community.",
        "Développé avec Rust, egui et wgpu. Merci à la communauté du modding Source.",
    ),
    ("shell.about.github", "Source code", "Code source"),
    ("shell.about.vdc", "Valve Developer Community", "Valve Developer Community"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_follow_the_scheme() {
        crate::register_translations();
        let pro = viewport_controls(NavScheme::HammerPro);
        let hammer = viewport_controls(NavScheme::Hammer);
        let pp = viewport_controls(NavScheme::HammerPlusPlus);
        assert!(pro.contains(&("shell.help.ctl_mmb", "shell.help.ctl_mmb_desc")));
        assert!(!pro.iter().any(|(k, _)| *k == "shell.view.ctl_space_left"));
        assert!(hammer.iter().any(|(k, _)| *k == "shell.view.ctl_space_left"));
        assert!(pp.contains(&("shell.view.ctl_mmb", "shell.view.ctl_mmb_look")));
        assert!(pro.contains(&("shell.help.ctl_fly_click", "shell.help.ctl_fly_click_desc")));
        for lang in [hp_app::i18n::Lang::En, hp_app::i18n::Lang::Fr] {
            let keys = hp_app::i18n::tr_in("shell.help.ctl_wasd", lang);
            assert!(!keys.contains("Ctrl"), "{keys}");
        }
        for scheme in NavScheme::ALL {
            for (k, d) in viewport_controls(scheme) {
                assert!(hp_app::i18n::has_key(k) && hp_app::i18n::has_key(d), "{k} {d}");
            }
        }
        let mut state = hp_app::testing::test_state();
        state.settings.workflow.nav_scheme = NavScheme::Hammer;
        state.ui.show_shortcuts = true;
        state.ui.show_about = true;
        state.runtime.workflow.build_info = Some("2026-10-21, abc1234".into());
        let mut w = ShortcutsWindow::default();
        hp_app::testing::run_ctx(&mut state, |ctx, state| {
            w.show(ctx, state);
            about_window(ctx, state);
        });
        assert!(state.ui.show_shortcuts && state.ui.show_about);
    }

    #[test]
    fn shortcut_filter() {
        let state = hp_app::testing::test_state();
        let mut w = ShortcutsWindow::default();
        let all: usize = CATEGORIES.iter().map(|c| w.matching(&state, *c).len()).sum();
        assert_eq!(all, Action::ALL.len());
        w.filter = "ctrl+s".into();
        let file = w.matching(&state, hp_app::actions::Category::File);
        assert!(file.contains(&Action::SaveMap), "{file:?}");
        w.filter = "zzzzqqq".into();
        assert!(CATEGORIES.iter().all(|c| w.matching(&state, *c).is_empty()));
    }
}
