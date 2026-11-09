#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hp_app::AssetsApi;
use hp_app::settings::{CustomDirSetting, Settings};

pub const GMOD_ROOT: &str = r"D:\SteamLibrary\steamapps\common\GarrysMod";

pub fn real_gmod() -> Option<PathBuf> {
    let p = PathBuf::from(GMOD_ROOT);
    p.join("garrysmod").is_dir().then_some(p)
}

pub fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-assets/scratch").join(name);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn write(path: &Path, data: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create folder");
    }
    std::fs::write(path, data).expect("write file");
}

pub fn vtf(size: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut px = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let k = if ((x / 8) + (y / 8)) % 2 == 0 { 1.0 } else { 0.8 };
            px.extend_from_slice(&[
                (f32::from(rgba[0]) * k) as u8,
                (f32::from(rgba[1]) * k) as u8,
                (f32::from(rgba[2]) * k) as u8,
                rgba[3],
            ]);
        }
    }
    let img = hp_vtf::RgbaImage::new(size, size, px).expect("image");
    hp_vtf::encode(&img, &hp_vtf::EncodeOptions::default()).expect("encode")
}

pub const FAKE_FGD: &str = r#"
@PointClass size(-16 -16 0, 16 16 72) color(0 255 0) = info_player_start : "Player start" []
@PointClass iconsprite("editor/light.vmt") size(-8 -8 -8, 8 8 8) color(255 255 0) = light : "Light" []
@PointClass studioprop() = prop_static : "Prop" []
@SolidClass color(220 30 220) = trigger_multiple : "Trigger" []
@SolidClass = func_detail : "Detail" []
@PointClass decal() = infodecal : "Decal" []
"#;

pub fn fake_install(name: &str) -> PathBuf {
    let root = scratch(name);
    let _ = std::fs::remove_dir_all(&root);
    let mats = root.join("garrysmod").join("materials");
    let mat = |n: &str, vmt: &str, color: [u8; 4], size: u32| {
        write(&mats.join(format!("{n}.vmt")), vmt.as_bytes());
        write(&mats.join(format!("{n}.vtf")), &vtf(size, color));
    };
    mat(
        "test/brick",
        r#""LightmappedGeneric" { "$basetexture" "test/brick" "$surfaceprop" "brick" }"#,
        [170, 80, 50, 255],
        64,
    );
    mat("test/glass", r#""UnlitGeneric" { "$basetexture" "test/glass" "$translucent" 1 }"#, [80, 160, 255, 120], 32);
    mat(
        "dev/dev_measuregeneric01b",
        r#""LightmappedGeneric" { "$basetexture" "dev/dev_measuregeneric01b" }"#,
        [140, 140, 140, 255],
        128,
    );
    mat(
        "concrete/concretefloor001a",
        r#""LightmappedGeneric" { "$basetexture" "concrete/concretefloor001a" }"#,
        [120, 115, 100, 255],
        128,
    );
    mat(
        "tools/toolstrigger",
        r#""LightmappedGeneric" { "$basetexture" "tools/toolstrigger" "%compiletrigger" 1 }"#,
        [230, 140, 30, 255],
        32,
    );
    mat(
        "tools/toolsnodraw",
        r#""LightmappedGeneric" { "$basetexture" "tools/toolsnodraw" "%compilenodraw" 1 }"#,
        [230, 220, 40, 255],
        32,
    );
    mat("editor/light", r#""Sprite" { "$basetexture" "editor/light" "$translucent" 1 }"#, [255, 255, 160, 255], 32);
    mat(
        "decals/test",
        r#""LightmappedGeneric" { "$basetexture" "decals/test" "$decal" 1 "$translucent" 1 "$decalscale" 2 }"#,
        [250, 230, 20, 200],
        32,
    );
    write(&root.join("bin").join("garrysmod.fgd"), FAKE_FGD.as_bytes());
    let custom = root.join("custom_art");
    std::fs::create_dir_all(&custom).expect("custom folder");
    root
}

pub fn fake_settings(root: &Path) -> Settings {
    let mut s = Settings::default();
    s.game.gmod_root = Some(root.to_path_buf());
    s.game.mount_addons = false;
    s.game.mount_workshop = false;
    s.game.mount_cfg = false;
    s.game.custom_texture_dirs.push(CustomDirSetting {
        path: root.join("custom_art"),
        prefix: "custom/art".into(),
        enabled: true,
        include_models: true,
    });
    s
}

pub fn real_settings(root: &Path) -> Settings {
    let mut s = Settings::default();
    s.game.gmod_root = Some(root.to_path_buf());
    s
}

pub fn frames_until<T>(
    am: &mut hp_assets::AssetManager,
    timeout: Duration,
    mut f: impl FnMut(&mut hp_assets::AssetManager, &egui::Context) -> Option<T>,
) -> Option<T> {
    let ctx = egui::Context::default();
    let deadline = Instant::now() + timeout;
    loop {
        let mut result = None;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            am.begin_frame(&ctx, None);
            result = f(am, &ctx);
        });
        out.textures_delta.clear();
        if result.is_some() {
            return result;
        }
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
