use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use hp_app::{AppState, Severity};
use hp_ui_shell::{SettingsTab, Shell};

struct Preview {
    state: AppState,
    shell: Shell,
    screenshot: Option<PathBuf>,
    frames: u64,
    shot_requested: bool,
    shot_done: Arc<AtomicBool>,
    wizard_step: Option<usize>,
}

impl Preview {
    fn new(scene: &str, fr: bool, screenshot: Option<PathBuf>, ctx: &egui::Context) -> Self {
        let mut state = hp_app::testing::test_state();
        hp_ui_shell::register_translations();
        if fr {
            state.settings.language = hp_app::i18n::Lang::Fr;
            hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        }
        hp_app::theme::apply(ctx, state.settings.ui.theme, state.settings.ui.ui_scale);
        state.settings.recent_files =
            vec![PathBuf::from("C:/maps/ttt_minecraft_b5.vmf"), PathBuf::from("C:/maps/gm_flatgrass_edit.vmf")];
        state.console.info("Hammer Pro 1.0.0 started");
        state.console.info("Mounted 14 VPK archives, 37 addons (1.2 s)");
        state.console.success("Loaded 18432 materials, 9211 models");
        state.console.warn("Missing material: custom/old/brick_broken");
        state.console.error("FGD error: base.fgd line 210: unknown property type");
        state.hover_world = Some(glam::DVec3::new(128.0, -64.0, 32.0));
        state.doc.select_all();
        state.notify(Severity::Success, "Saved test.vmf");
        state.notify(Severity::Warning, "3 problems found in the map");
        let mut shell = Shell::new();
        let mut wizard_step = None;
        match scene {
            "palette" => state.ui.show_command_palette = true,
            "shortcuts" => state.ui.show_shortcuts = true,
            "about" => state.ui.show_about = true,
            s if s.starts_with("wizard-") => {
                state.ui.show_first_run = true;
                wizard_step = s.trim_start_matches("wizard-").parse().ok();
            }
            s if s.starts_with("settings-") => {
                let tab = match s.trim_start_matches("settings-") {
                    "game" => SettingsTab::Game,
                    "viewport" => SettingsTab::Viewport,
                    "editing" => SettingsTab::Editing,
                    "compile" => SettingsTab::Compile,
                    "keyboard" => SettingsTab::Keyboard,
                    _ => SettingsTab::General,
                };
                state.settings.game.gmod_root = Some(PathBuf::from(r"D:\SteamLibrary\steamapps\common\GarrysMod"));
                state.settings.game.custom_texture_dirs.push(hp_app::settings::CustomDirSetting {
                    path: PathBuf::from(r"C:\Users\me\Pictures\Textures Été"),
                    prefix: "custom/textures_ete".into(),
                    enabled: true,
                    include_models: true,
                });
                shell.open_settings(&mut state, tab);
            }
            _ => {}
        }
        Self {
            state,
            shell,
            screenshot,
            frames: 0,
            shot_requested: false,
            shot_done: Arc::new(AtomicBool::new(false)),
            wizard_step,
        }
    }
}

impl eframe::App for Preview {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frames += 1;
        self.state.frame.time = ctx.input(|i| i.time);
        self.state.frame.fps = 144.0;
        if self.frames == 2
            && let Some(step) = self.wizard_step
        {
            self.shell.wizard().step = step;
        }
        let state = &mut self.state;
        let shell = &mut self.shell;
        egui::Panel::top("top").show(ui, |ui| {
            shell.menu_bar(ui, state);
            ui.add_space(2.0);
            shell.toolbar(ui, state);
            ui.add_space(2.0);
        });
        egui::Panel::bottom("status").exact_size(24.0).show(ui, |ui| shell.status_bar(ui, state, None));
        shell.tool_strip(ui, state);
        egui::Panel::bottom("console").resizable(true).default_size(170.0).show(ui, |ui| shell.console(ui, state));
        egui::CentralPanel::default().show(ui, |ui| {
            let rect = ui.available_rect_before_wrap();
            ui.painter().rect_filled(rect, 0.0, hp_app::theme::rgb(state.settings.view.background_3d));
        });
        shell.dialogs(&ctx, state);
        shell.toasts(&ctx, state);
        for action in state.requested_actions.drain(..) {
            state.console.info(format!("requested action: {}", action.id()));
        }
        if let Some(tool) = state.requested_tool.take() {
            state.tool = tool;
        }
        if let Some(path) = state.requested_open.take() {
            state.console.info(format!("open: {}", path.display()));
        }
        if let Some(path) = self.screenshot.clone() {
            if self.frames == 30 && !self.shot_requested {
                self.shot_requested = true;
                let done = self.shot_done.clone();
                ctx.request_screenshot(move |image| {
                    let [w, h] = image.size;
                    let bytes = image.as_raw().to_vec();
                    match image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                        Some(img) => {
                            if let Err(e) = img.save(&path) {
                                eprintln!("could not save screenshot: {e}");
                            }
                        }
                        None => eprintln!("bad screenshot buffer"),
                    }
                    done.store(true, Ordering::SeqCst);
                });
            }
            if self.shot_done.load(Ordering::SeqCst) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }
    }
}

fn main() -> eframe::Result<()> {
    let mut scene = "main".to_owned();
    let mut fr = false;
    let mut screenshot = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--fr" => fr = true,
            "--screenshot" => screenshot = args.next().map(PathBuf::from),
            other => scene = other.to_owned(),
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("Hammer Pro shell preview"),
        ..Default::default()
    };
    eframe::run_native(
        "hp-shell-preview",
        options,
        Box::new(move |cc| Ok(Box::new(Preview::new(&scene, fr, screenshot, &cc.egui_ctx)))),
    )
}
