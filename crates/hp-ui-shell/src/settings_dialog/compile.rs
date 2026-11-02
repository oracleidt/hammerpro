use egui::{RichText, Ui};
use hp_app::extend::SettingsPage;
use hp_app::i18n::TrTable;
use hp_app::runtime::CompileBrowse;
use hp_app::settings::{
    CompilePresetSetting, CompileSettings, LightingSetting, PackPolicy, background_precompile_default,
};
use hp_app::{AppState, t, tf};

use crate::common::heading;
use crate::game_ui::{GameJobs, optional_path_edit, path_row};

fn preset_label(p: CompilePresetSetting) -> &'static str {
    match p {
        CompilePresetSetting::Draft => t!("shell.compile.preset_draft"),
        CompilePresetSetting::Normal => t!("shell.compile.preset_normal"),
        CompilePresetSetting::Final => t!("shell.compile.preset_final"),
        CompilePresetSetting::Custom => t!("shell.compile.preset_custom"),
    }
}

fn lighting_label(l: LightingSetting) -> &'static str {
    match l {
        LightingSetting::Ldr => "LDR",
        LightingSetting::Hdr => "HDR",
        LightingSetting::Both => t!("shell.compile.lighting_both"),
    }
}

pub(super) fn page(ui: &mut Ui, state: &mut AppState, _jobs: &mut GameJobs) {
    let before = state.settings.compile.clone();
    defaults(ui, &mut state.settings.compile);
    if speed(ui, &mut state.settings.compile, &mut state.settings.visibility.cordon_material) {
        state.settings_dirty = true;
    }
    after(ui, &mut state.settings.compile);
    compiler(ui, state);
    if state.settings.compile != before {
        state.settings_dirty = true;
    }
}

fn defaults(ui: &mut Ui, c: &mut CompileSettings) {
    heading(ui, t!("shell.compile.defaults"));
    egui::Grid::new("hp_shell_compile_defaults").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        ui.label(t!("shell.compile.preset"));
        ui.horizontal(|ui| {
            for p in [
                CompilePresetSetting::Draft,
                CompilePresetSetting::Normal,
                CompilePresetSetting::Final,
                CompilePresetSetting::Custom,
            ] {
                if ui.selectable_label(c.preset == p, preset_label(p)).clicked() {
                    c.preset = p;
                    if p != CompilePresetSetting::Custom {
                        c.run_bsp = true;
                        c.run_vis = true;
                        c.run_rad = true;
                    }
                }
            }
        });
        ui.end_row();
        ui.label(t!("shell.compile.lighting"));
        ui.horizontal(|ui| {
            for l in [LightingSetting::Ldr, LightingSetting::Hdr, LightingSetting::Both] {
                ui.selectable_value(&mut c.lighting, l, lighting_label(l));
            }
        });
        ui.end_row();
        ui.label(t!("shell.compile.steps"));
        ui.horizontal(|ui| {
            ui.checkbox(&mut c.run_bsp, t!("shell.compile.step_bsp"));
            ui.checkbox(&mut c.run_vis, t!("shell.compile.step_vis"));
            ui.checkbox(&mut c.run_rad, t!("shell.compile.step_rad"));
        });
        ui.end_row();
        ui.label(t!("shell.compile.static_props"));
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut c.static_prop_lighting, t!("shell.compile.prop_lighting"))
                .on_hover_text(t!("shell.compile.prop_lighting_tip"));
            ui.checkbox(&mut c.static_prop_shadows, t!("shell.compile.prop_shadows"))
                .on_hover_text(t!("shell.compile.prop_shadows_tip"));
        });
        ui.end_row();
        ui.label(t!("shell.compile.threads"));
        ui.horizontal(|ui| {
            let auto_n = hp_app_threads(c.low_priority);
            let mut auto = c.threads.is_none();
            if ui.checkbox(&mut auto, tf!("shell.compile.threads_auto_n", auto_n)).changed() {
                c.threads = if auto { None } else { Some(auto_n) };
            }
            if let Some(n) = &mut c.threads {
                ui.add(egui::DragValue::new(n).range(1..=256));
            }
            ui.checkbox(&mut c.low_priority, t!("shell.compile.low_priority"))
                .on_hover_text(t!("shell.compile.low_priority_tip"));
        });
        ui.end_row();
    });
}

fn hp_app_threads(low_priority: bool) -> u32 {
    let n = std::thread::available_parallelism().map_or(1, |n| u32::try_from(n.get()).unwrap_or(u32::MAX));
    if low_priority { n.saturating_sub(1).max(1) } else { n }
}

fn speed(ui: &mut Ui, c: &mut CompileSettings, seal: &mut String) -> bool {
    let mut seal_changed = false;
    heading(ui, t!("shell.compile.speed"));
    egui::Grid::new("hp_shell_compile_speed").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
        ui.label(t!("shell.compile.smart"));
        ui.checkbox(&mut c.smart_recompile, "").on_hover_text(t!("shell.compile.smart_tip"));
        ui.end_row();
        ui.label(t!("shell.compile.background"));
        ui.add_enabled(c.low_priority, egui::Checkbox::new(&mut c.background_precompile, ""))
            .on_hover_text(t!("shell.compile.background_tip"));
        ui.end_row();
        if !background_precompile_default() {
            ui.label("");
            ui.label(RichText::new(t!("shell.compile.background_few_cores")).small().weak());
            ui.end_row();
        }
        ui.label(t!("shell.compile.stop_on_leak"));
        ui.checkbox(&mut c.stop_on_leak, "").on_hover_text(t!("shell.compile.stop_on_leak_tip"));
        ui.end_row();
        ui.label(t!("shell.compile.compile_hidden"));
        ui.checkbox(&mut c.compile_hidden, "").on_hover_text(t!("shell.compile.compile_hidden_tip"));
        ui.end_row();
        ui.label(t!("shell.compile.cordon_only"));
        ui.checkbox(&mut c.compile_cordon_only, "");
        ui.end_row();
        ui.label(t!("shell.compile.cordon_material"));
        seal_changed = ui
            .add(
                egui::TextEdit::singleline(seal)
                    .desired_width(ui.available_width().clamp(120.0, 240.0))
                    .hint_text(hp_app::settings::VisibilitySettings::default().cordon_material),
            )
            .changed();
        ui.end_row();
        ui.label(t!("shell.compile.pack"));
        ui.horizontal(|ui| {
            ui.checkbox(&mut c.pack_custom_content, "");
            ui.add_enabled_ui(c.pack_custom_content, |ui| {
                ui.selectable_value(&mut c.pack_policy, PackPolicy::ReleaseOnly, t!("shell.compile.pack_release_only"));
                ui.selectable_value(&mut c.pack_policy, PackPolicy::Always, t!("shell.compile.pack_always"));
            });
        });
        ui.end_row();
        ui.label(t!("shell.compile.dev_addon"));
        ui.checkbox(&mut c.dev_addon, "").on_hover_text(t!("shell.compile.dev_addon_tip"));
        ui.end_row();
        ui.label(t!("shell.compile.export_materials"));
        ui.checkbox(&mut c.export_custom_materials, "");
        ui.end_row();
        ui.label(t!("shell.compile.progressive"));
        ui.checkbox(&mut c.progressive, "").on_hover_text(t!("shell.compile.progressive_tip"));
        ui.end_row();
        ui.label(t!("shell.compile.history"));
        ui.checkbox(&mut c.history, "");
        ui.end_row();
    });
    seal_changed
}

fn after(ui: &mut Ui, c: &mut CompileSettings) {
    heading(ui, t!("shell.compile.after"));
    egui::Grid::new("hp_shell_compile_after").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
        for (value, key) in [
            (&mut c.save_before_compile, "shell.compile.save_before"),
            (&mut c.copy_to_maps, "shell.compile.copy_to_maps"),
            (&mut c.launch_game, "shell.compile.launch_game"),
            (&mut c.launch_via_steam, "shell.compile.launch_steam"),
            (&mut c.load_pointfile_on_leak, "shell.compile.load_pointfile"),
        ] {
            ui.label(t!(key));
            ui.checkbox(value, "");
            ui.end_row();
        }
        ui.label(t!("shell.compile.game_args"));
        ui.add(
            egui::TextEdit::singleline(&mut c.game_args)
                .desired_width(ui.available_width().clamp(120.0, 300.0))
                .font(egui::TextStyle::Monospace),
        );
        ui.end_row();
    });
    heading(ui, t!("shell.compile.extra_args"));
    egui::Grid::new("hp_shell_compile_args").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
        for (value, name, tip) in [
            (&mut c.extra_bsp_args, "shell.compile.step_bsp", "shell.compile.extra_bsp_tip"),
            (&mut c.extra_rad_args, "shell.compile.step_rad", "shell.compile.extra_rad_tip"),
        ] {
            ui.label(t!(name));
            ui.add(
                egui::TextEdit::singleline(value)
                    .desired_width(ui.available_width().clamp(120.0, 300.0))
                    .font(egui::TextStyle::Monospace),
            )
            .on_hover_text(t!(tip));
            ui.end_row();
        }
    });
    if hp_app::extend::compilers_ui().is_none()
        && let Some(menus) = hp_app::extend::compile_options_ui()
    {
        heading(ui, t!("shell.compile.options"));
        menus(ui, c);
    }
}

fn compiler(ui: &mut Ui, state: &mut AppState) {
    let visible = ui.clip_rect().right() - ui.cursor().left();
    ui.scope(|ui| {
        if visible.is_finite() && visible > 200.0 && visible < ui.available_width() {
            ui.set_max_width(visible);
        }
        compiler_section(ui, state);
    });
}

fn compiler_section(ui: &mut Ui, state: &mut AppState) {
    heading(ui, t!("shell.compile.compiler"));
    ui.label(tf!("shell.compile.compiler_builtin", env!("CARGO_PKG_VERSION")));
    ui.add(egui::Label::new(RichText::new(t!("shell.compile.compiler_tip")).small().weak()).wrap());
    ui.add_space(4.0);
    if let Some(editor) = hp_app::extend::compilers_ui() {
        editor(ui, state);
        ui.add_space(4.0);
    }
    let c = &mut state.settings.compile;
    let (out, _) = path_row(
        ui,
        t!("shell.compile.game_exe"),
        Some(64.0),
        &mut c.game_exe_path,
        t!("shell.compile.game_exe_auto"),
        true,
    );
    if out.browse {
        state.runtime.compile.browse_request = Some(CompileBrowse::GameExe);
    }
    let c = &mut state.settings.compile;
    ui.horizontal(|ui| {
        ui.label(t!("shell.compile.maps_dir"));
        let width = (ui.available_width() - 90.0).clamp(120.0, 300.0);
        optional_path_edit(ui, &mut c.maps_dir_override, t!("shell.compile.maps_dir_hint"), width);
        if ui.button(t!("common.browse")).clicked() {
            state.runtime.compile.browse_request = Some(CompileBrowse::MapsDir);
        }
    });
}

pub(super) fn reset(state: &mut AppState) {
    state.settings.compile = CompileSettings::default();
}

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    Vec::new()
}

pub(crate) const TR: TrTable = &[
    (
        "shell.compile.defaults",
        "Defaults (F9 and the compile window)",
        "Valeurs par défaut (F9 et fenêtre de compilation)",
    ),
    ("shell.compile.preset", "Preset", "Préréglage"),
    ("shell.compile.preset_draft", "Draft", "Brouillon"),
    ("shell.compile.preset_normal", "Normal", "Normal"),
    ("shell.compile.preset_final", "Final", "Final"),
    ("shell.compile.preset_custom", "Custom", "Personnalisé"),
    ("shell.compile.lighting", "Dynamic range", "Plage dynamique"),
    ("shell.compile.lighting_both", "LDR + HDR", "LDR + HDR"),
    ("shell.compile.steps", "Steps", "Étapes"),
    ("shell.compile.step_bsp", "Geometry", "Géométrie"),
    ("shell.compile.step_vis", "Visibility", "Visibilité"),
    ("shell.compile.step_rad", "Lighting", "Éclairage"),
    ("shell.compile.threads", "Threads", "Threads"),
    ("shell.compile.static_props", "Static props", "Props statiques"),
    ("shell.compile.prop_lighting", "Light static props (per vertex)", "Éclairer les props statiques (par sommet)"),
    (
        "shell.compile.prop_lighting_tip",
        "When the quality asks for it (Final), the lighting bakes the light of every static prop vertex (-StaticPropLighting): slow on big maps. Off: never, the game lights the props itself.",
        "Quand la qualité le demande (Final), l'éclairage précalcule la lumière de chaque sommet des props statiques (-StaticPropLighting) : lent sur les grandes maps. Décoché : jamais, le jeu éclaire lui-même les props.",
    ),
    ("shell.compile.prop_shadows", "Static prop shadows", "Ombres des props statiques"),
    (
        "shell.compile.prop_shadows_tip",
        "Static props cast shadows in the lighting (lightmaps, bounced light, detail props, ambient light, prop lighting). Off: no prop shadow at all and their models are not even loaded (-nostaticpropshadows), much faster on maps with many props.",
        "Les props statiques projettent des ombres dans l'éclairage (lightmaps, lumière rebondie, props de détail, lumière ambiante, éclairage des props). Décoché : aucune ombre de prop et leurs modèles ne sont même pas chargés (-nostaticpropshadows), bien plus rapide sur les maps avec beaucoup de props.",
    ),
    ("shell.compile.options", "Compiler options", "Options du compilateur"),
    ("shell.compile.threads_auto_n", "Automatic ({0})", "Automatique ({0})"),
    ("shell.compile.low_priority", "Keep a core for the editor", "Garder un cœur pour l'éditeur"),
    (
        "shell.compile.low_priority_tip",
        "The compiler uses every processor core but one, so the editor stays smooth.",
        "Le compilateur utilise tous les cœurs du processeur sauf un, l'éditeur reste donc fluide.",
    ),
    ("shell.compile.speed", "Speed", "Vitesse"),
    ("shell.compile.smart", "Smart recompile", "Recompilation intelligente"),
    (
        "shell.compile.smart_tip",
        "Draft and Normal only run what the changes need: entities only, lighting only, props only, or everything.",
        "Brouillon et Normal n'exécutent que ce que demandent les changements : entités seules, éclairage seul, props seuls ou tout.",
    ),
    ("shell.compile.background", "Compile while idle", "Compiler pendant les pauses"),
    (
        "shell.compile.background_tip",
        "The geometry is built in the background 3 seconds after the last change: F9 reuses it and leaks show up early. Paused on battery.",
        "La géométrie est construite en arrière-plan 3 secondes après la dernière modification : F9 la réutilise et les fuites apparaissent tôt. En pause sur batterie.",
    ),
    (
        "shell.compile.background_few_cores",
        "Off by default on this machine (fewer than 8 processor threads).",
        "Désactivé par défaut sur cette machine (moins de 8 threads processeur).",
    ),
    ("shell.compile.stop_on_leak", "Stop when the map leaks", "S'arrêter quand la map fuit"),
    (
        "shell.compile.stop_on_leak_tip",
        "The compile stops right after the geometry on a leak instead of computing the lighting for nothing.",
        "La compilation s'arrête juste après la géométrie en cas de fuite au lieu de calculer l'éclairage pour rien.",
    ),
    ("shell.compile.compile_hidden", "Compile hidden objects", "Compiler les objets masqués"),
    (
        "shell.compile.compile_hidden_tip",
        "Like Hammer, objects hidden in the editor are not compiled unless this is checked.",
        "Comme dans Hammer, les objets masqués dans l'éditeur ne sont pas compilés, sauf si cette case est cochée.",
    ),
    ("shell.compile.cordon_only", "Compile only the active cordon", "Compiler seulement le cordon actif"),
    ("shell.compile.cordon_material", "Cordon seal material", "Matériau de scellement du cordon"),
    ("shell.compile.pack", "Pack custom content into the BSP", "Intégrer le contenu personnalisé au BSP"),
    ("shell.compile.pack_release_only", "Release builds only", "Versions finales uniquement"),
    ("shell.compile.pack_always", "Always", "Toujours"),
    ("shell.compile.dev_addon", "Development addon", "Addon de développement"),
    (
        "shell.compile.dev_addon_tip",
        "Builds that do not pack copy the custom materials into garrysmod/addons/hammerpro_dev for the game.",
        "Les compilations qui n'empaquettent pas copient les matériaux personnalisés dans garrysmod/addons/hammerpro_dev pour le jeu.",
    ),
    (
        "shell.compile.export_materials",
        "Export custom materials to the game",
        "Exporter les matériaux personnalisés vers le jeu",
    ),
    (
        "shell.compile.progressive",
        "Play now, finish the lighting later",
        "Jouer tout de suite, finir l'éclairage ensuite",
    ),
    (
        "shell.compile.progressive_tip",
        "A Draft compile starts the game, then the chosen quality is compiled in the background and replaces the map.",
        "Une compilation Brouillon lance le jeu, puis la qualité choisie est compilée en arrière-plan et remplace la map.",
    ),
    ("shell.compile.history", "Keep a timing history", "Garder un historique des durées"),
    ("shell.compile.after", "Options", "Options"),
    (
        "shell.compile.save_before",
        "Save the map before compiling (the compile never needs it)",
        "Enregistrer la map avant de compiler (la compilation n'en a jamais besoin)",
    ),
    ("shell.compile.copy_to_maps", "Copy the BSP to garrysmod/maps", "Copier le BSP dans garrysmod/maps"),
    ("shell.compile.launch_game", "Launch the game after compiling", "Lancer le jeu après la compilation"),
    ("shell.compile.launch_steam", "Launch through Steam", "Lancer via Steam"),
    (
        "shell.compile.load_pointfile",
        "Load the leak pointfile automatically",
        "Charger automatiquement le pointfile de fuite",
    ),
    ("shell.compile.game_args", "Game arguments", "Arguments du jeu"),
    ("shell.compile.extra_args", "Extra compiler arguments", "Arguments supplémentaires du compilateur"),
    (
        "shell.compile.extra_bsp_tip",
        "Added to the geometry step (hammerpro --bspc), for example -fastvis.",
        "Ajoutés à l'étape de géométrie (hammerpro --bspc), par exemple -fastvis.",
    ),
    (
        "shell.compile.extra_rad_tip",
        "Added to the lighting step (hammerpro --rad), for example -bounce 4.",
        "Ajoutés à l'étape d'éclairage (hammerpro --rad), par exemple -bounce 4.",
    ),
    ("shell.compile.compiler", "Compiler", "Compilateur"),
    (
        "shell.compile.compiler_builtin",
        "Hammer Pro {0} has its own compiler, used by default.",
        "Hammer Pro {0} a son propre compilateur, utilisé par défaut.",
    ),
    (
        "shell.compile.compiler_tip",
        "The geometry, portals, leak test and visibility (hammerpro --bspc), the lighting (hammerpro --rad) and the packing all run in Hammer Pro: no other program is needed. For each game you can also choose the Valve tools found next to it (vbsp, vvis, vrad) or a compiler of your own, with its programs and arguments.",
        "La géométrie, les portails, la recherche de fuites et la visibilité (hammerpro --bspc), l'éclairage (hammerpro --rad) et l'empaquetage tournent dans Hammer Pro : aucun autre programme n'est nécessaire. Pour chaque jeu, vous pouvez aussi choisir les outils de Valve trouvés à côté de lui (vbsp, vvis, vrad) ou un compilateur à vous, avec ses programmes et ses arguments.",
    ),
    ("shell.compile.game_exe", "Game", "Jeu"),
    ("shell.compile.game_exe_auto", "Detected automatically", "Détecté automatiquement"),
    ("shell.compile.maps_dir", "Maps folder", "Dossier des maps"),
    ("shell.compile.maps_dir_hint", "garrysmod/maps of the game", "garrysmod/maps du jeu"),
];
