use std::path::PathBuf;
use std::sync::Arc;

use crate::settings::Settings;
use crate::state::{
    AppState, AssetStatus, AssetsApi, BodyGroupInfo, Gfx, ImportOptions, MaterialInfo, ModelInfo, ModelState,
    ParticleCatalog, ParticleSystemInfo, PreviewCamera, SceneOptions, ThumbState,
};

pub struct MockAssets {
    materials: Arc<Vec<hp_fs::AssetEntry>>,
    models: Arc<Vec<hp_fs::AssetEntry>>,
    sounds: Arc<Vec<hp_fs::AssetEntry>>,
    fs: Option<hp_fs::GameFs>,
    fgd: Option<hp_fgd::ResolvedFgd>,
}

impl MockAssets {
    pub fn with_fs(fs: hp_fs::GameFs) -> Self {
        Self { fs: Some(fs), ..Self::default() }
    }

    pub fn with_fgd(text: &str) -> Self {
        Self { fgd: Some(hp_fgd::Fgd::parse(text, None).resolve()), ..Self::default() }
    }
}

impl Default for MockAssets {
    fn default() -> Self {
        let entry = |name: &str| hp_fs::AssetEntry { name: name.to_owned(), source: 0 };
        Self {
            materials: Arc::new(vec![
                entry("concrete/concretefloor001a"),
                entry("dev/dev_measuregeneric01b"),
                entry("tools/toolsnodraw"),
                entry("tools/toolstrigger"),
                entry("custom/mytextures/brick01"),
            ]),
            models: Arc::new(vec![
                entry("models/props_c17/oildrum001.mdl"),
                entry("models/props_junk/wood_crate001a.mdl"),
            ]),
            sounds: Arc::new(vec![entry("sound/ambient/wind/wind1.wav")]),
            fs: None,
            fgd: None,
        }
    }
}

impl AssetsApi for MockAssets {
    fn status(&self) -> AssetStatus {
        AssetStatus::Ready
    }
    fn fs(&self) -> Option<&hp_fs::GameFs> {
        self.fs.as_ref()
    }
    fn fgd(&self) -> Option<&hp_fgd::ResolvedFgd> {
        self.fgd.as_ref()
    }
    fn generation(&self) -> u64 {
        1
    }
    fn remount(&mut self, _settings: &Settings) {}
    fn reload_fgd(&mut self, _settings: &Settings) {}
    fn materials(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.materials.clone()
    }
    fn models(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.models.clone()
    }
    fn sounds(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        self.sounds.clone()
    }
    fn source_label(&self, source: u32) -> String {
        format!("Source {source}")
    }
    fn material_info(&mut self, name: &str) -> Option<Arc<MaterialInfo>> {
        let tool = if name.starts_with("tools/") { Some(hp_vmt::ToolKind::Nodraw) } else { None };
        Some(Arc::new(MaterialInfo {
            name: name.to_owned(),
            width: 512,
            height: 512,
            shader: "LightmappedGeneric".to_owned(),
            tool,
            decal_scale: name.starts_with("decals/").then_some(1.0),
            ..Default::default()
        }))
    }
    fn thumbnail(&mut self, _ctx: &egui::Context, _name: &str) -> ThumbState {
        ThumbState::Loading
    }
    fn model_info(&mut self, path: &str) -> ModelState {
        ModelState::Ready(Arc::new(ModelInfo {
            path: path.to_owned(),
            bounds_min: glam::DVec3::splat(-16.0),
            bounds_max: glam::DVec3::splat(16.0),
            skins: 1,
            body_parts: 1,
            triangles: 120,
            materials: vec!["models/props_c17/oil_drum001a".to_owned()],
            is_static_prop: true,
            surface_prop: "metal".to_owned(),
            hull_min: None,
            hull_max: None,
            body_groups: vec![BodyGroupInfo {
                name: "lid".to_owned(),
                base: 1,
                choices: vec!["closed".to_owned(), "blank".to_owned()],
            }],
            sequences: vec!["idle".to_owned(), "open".to_owned()],
            sequence_frames: vec![1, 30],
            stats: Default::default(),
        }))
    }
    fn model_preview(
        &mut self,
        _gfx: &mut Gfx,
        _path: &str,
        _camera: &PreviewCamera,
        _size_px: [u32; 2],
    ) -> Option<egui::TextureId> {
        None
    }
    fn entity_bounds(&self, entity: &hp_vmf::Entity) -> Option<hp_geom::Aabb> {
        let o = entity.origin()?;
        Some(hp_geom::Aabb::new(o - glam::DVec3::splat(8.0), o + glam::DVec3::splat(8.0)))
    }
    fn invalidate_material(&mut self, _name: &str) {}
    fn import_images(&mut self, files: &[PathBuf], _opts: &ImportOptions) -> Vec<Result<String, String>> {
        files.iter().map(|f| Ok(f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())).collect()
    }
    fn begin_frame(&mut self, _ctx: &egui::Context, _gfx: Option<&mut Gfx>) {}
    fn sync_scene(&mut self, doc: &mut hp_editor::Document, _gfx: &mut Gfx, _opts: &SceneOptions) {
        let _ = doc.take_changes();
    }
    fn render_material_id(&mut self, _name: &str) -> hp_render::MaterialId {
        0
    }
    fn status_line(&self) -> Option<String> {
        None
    }
    fn particles(&mut self) -> Option<Arc<ParticleCatalog>> {
        let system = |name: &str, file: &str, material: &str| ParticleSystemInfo {
            name: name.to_owned(),
            file: file.to_owned(),
            material: material.to_owned(),
            source: 0,
        };
        Some(Arc::new(ParticleCatalog {
            systems: vec![
                system("env_fire_large", "particles/fire_01.pcf", "particle/fire"),
                system("env_fire_small", "particles/fire_01.pcf", "particle/fire"),
                system("smoke_gib_01", "particles/smoke.pcf", "particle/smoke1/smoke1"),
            ],
            files: vec!["particles/fire_01.pcf".to_owned(), "particles/smoke.pcf".to_owned()],
        }))
    }
}

pub const TEST_VMF: &str = r#"versioninfo
{
	"editorversion" "400"
	"editorbuild" "9540"
	"mapversion" "1"
	"formatversion" "100"
	"prefab" "0"
}
world
{
	"id" "1"
	"mapversion" "1"
	"classname" "worldspawn"
	"skyname" "sky_day01_01"
	solid
	{
		"id" "2"
		side { "id" "1" "plane" "(-256 256 0) (256 256 0) (256 -256 0)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "2" "plane" "(-256 -256 -16) (256 -256 -16) (256 256 -16)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "3" "plane" "(-256 256 0) (-256 -256 0) (-256 -256 -16)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "4" "plane" "(256 256 -16) (256 -256 -16) (256 -256 0)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "5" "plane" "(256 256 0) (-256 256 0) (-256 256 -16)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "6" "plane" "(256 -256 -16) (-256 -256 -16) (-256 -256 0)" "material" "DEV/DEV_MEASUREGENERIC01B" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		editor { "color" "0 180 255" "visgroupshown" "1" "visgroupautoshown" "1" }
	}
	solid
	{
		"id" "3"
		side { "id" "7" "plane" "(-256 272 128) (256 272 128) (256 256 128)" "material" "CONCRETE/CONCRETEFLOOR001A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "8" "plane" "(-256 256 0) (256 256 0) (256 272 0)" "material" "CONCRETE/CONCRETEFLOOR001A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "9" "plane" "(-256 272 128) (-256 256 128) (-256 256 0)" "material" "CONCRETE/CONCRETEFLOOR001A" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "10" "plane" "(256 272 0) (256 256 0) (256 256 128)" "material" "CONCRETE/CONCRETEFLOOR001A" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "11" "plane" "(256 272 128) (-256 272 128) (-256 272 0)" "material" "CONCRETE/CONCRETEFLOOR001A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "12" "plane" "(256 256 0) (-256 256 0) (-256 256 128)" "material" "CONCRETE/CONCRETEFLOOR001A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		editor { "color" "0 180 255" "visgroupshown" "1" "visgroupautoshown" "1" }
	}
}
entity
{
	"id" "4"
	"classname" "info_player_start"
	"angles" "0 90 0"
	"origin" "0 -128 0"
	editor { "color" "0 255 0" "visgroupshown" "1" "visgroupautoshown" "1" }
}
entity
{
	"id" "5"
	"classname" "light"
	"_light" "255 240 200 300"
	"targetname" "main_light"
	"origin" "0 0 96"
	editor { "color" "220 30 220" "visgroupshown" "1" "visgroupautoshown" "1" }
}
entity
{
	"id" "6"
	"classname" "trigger_multiple"
	"spawnflags" "1"
	"wait" "1"
	connections
	{
		"OnTrigger" "main_light,TurnOff,,0,-1"
	}
	solid
	{
		"id" "7"
		side { "id" "13" "plane" "(64 64 64) (128 64 64) (128 0 64)" "material" "TOOLS/TOOLSTRIGGER" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "14" "plane" "(64 0 0) (128 0 0) (128 64 0)" "material" "TOOLS/TOOLSTRIGGER" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "15" "plane" "(64 64 64) (64 0 64) (64 0 0)" "material" "TOOLS/TOOLSTRIGGER" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "16" "plane" "(128 64 0) (128 0 0) (128 0 64)" "material" "TOOLS/TOOLSTRIGGER" "uaxis" "[0 1 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "17" "plane" "(128 64 64) (64 64 64) (64 64 0)" "material" "TOOLS/TOOLSTRIGGER" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "18" "plane" "(128 0 0) (64 0 0) (64 0 64)" "material" "TOOLS/TOOLSTRIGGER" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 0 -1 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		editor { "color" "220 30 220" "visgroupshown" "1" "visgroupautoshown" "1" }
	}
	editor { "color" "220 30 220" "visgroupshown" "1" "visgroupautoshown" "1" }
}
cordon
{
	"mins" "(-1024 -1024 -1024)"
	"maxs" "(1024 1024 1024)"
	"active" "0"
}
"#;

pub fn lang_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    crate::i18n::set_lang(crate::i18n::Lang::En);
    guard
}

pub fn test_state() -> AppState {
    test_state_with(MockAssets::default())
}

pub fn test_state_with_fgd(fgd: &str) -> AppState {
    test_state_with(MockAssets::with_fgd(fgd))
}

fn test_state_with(assets: MockAssets) -> AppState {
    crate::register_translations();
    let mut state = AppState::new(Settings::default(), Box::new(assets));
    match hp_vmf::parse(TEST_VMF) {
        Ok(map) => state.doc = hp_editor::Document::from_map(map),
        Err(e) => panic!("TEST_VMF must parse: {e}"),
    }
    state
}

pub fn run_ui(state: &mut AppState, mut f: impl FnMut(&mut egui::Ui, &mut AppState)) {
    let ctx = egui::Context::default();
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| f(ui, state));
        });
        out.textures_delta.clear();
    }
}

pub fn run_ctx(state: &mut AppState, mut f: impl FnMut(&egui::Context, &mut AppState)) {
    let ctx = egui::Context::default();
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            f(&ctx, state);
        });
        out.textures_delta.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_loads_map() {
        let state = test_state();
        let stats = state.doc.stats();
        assert!(stats.entities >= 3, "{stats:?}");
    }

    #[test]
    fn the_language_lock_starts_in_english() {
        {
            let _lang = lang_lock();
            crate::i18n::set_lang(crate::i18n::Lang::Fr);
        }
        let _lang = lang_lock();
        assert_eq!(crate::i18n::lang(), crate::i18n::Lang::En);
    }

    #[test]
    fn run_ui_works() {
        let mut state = test_state();
        let mut frames = 0;
        run_ui(&mut state, |ui, _| {
            ui.label("hello");
            frames += 1;
        });
        assert_eq!(frames, 3);
    }
}
