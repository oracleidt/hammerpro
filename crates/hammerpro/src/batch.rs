use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize};

use glam::DVec3;
use hp_app::settings::Settings;
use hp_app::{AppState, Severity, t, tf};
use hp_meshio::UpAxis;
use hp_meshio::smd::Collision;
use hp_ui_entities::model_import::batch::{
    BatchOptions, Formats, GridOptions, clamp_threads, collect_files, names_for, run, scan,
};
use hp_ui_entities::model_import::lod_options;

pub const IMPORT_ARG: &str = "--import-models";

#[derive(Clone, Debug)]
pub struct Spec {
    pub inputs: Vec<PathBuf>,
    pub recursive: bool,
    pub formats: Formats,
    pub opts: BatchOptions,
    pub dry_run: bool,
    pub out: Option<PathBuf>,
    pub quiet: bool,
}

impl Spec {
    pub fn from_settings(settings: &Settings) -> Self {
        let mut opts = BatchOptions::default();
        opts.convert.lod = lod_options(settings.lab.model_lod);
        opts.convert.keep_params = settings.lab.model_keep_params;
        Self {
            inputs: Vec::new(),
            recursive: false,
            formats: Formats::default(),
            opts,
            dry_run: false,
            out: None,
            quiet: false,
        }
    }
}

pub fn parse_collision(word: &str) -> Result<Collision, String> {
    match word.to_ascii_lowercase().as_str() {
        "pieces" => Ok(Collision::Pieces),
        "box" => Ok(Collision::Box),
        "none" => Ok(Collision::None),
        other => Err(format!("unknown collision {other}: pieces, box or none")),
    }
}

pub fn parse_up(word: &str) -> Result<Option<UpAxis>, String> {
    match word.to_ascii_lowercase().as_str() {
        "auto" => Ok(None),
        "y" => Ok(Some(UpAxis::Y)),
        "z" => Ok(Some(UpAxis::Z)),
        other => Err(format!("unknown up axis {other}: auto, y or z")),
    }
}

fn number(word: Option<&String>, flag: &str, min: f64, max: f64) -> Result<f64, String> {
    let v: f64 =
        word.ok_or_else(|| format!("{flag} needs a value"))?.parse().map_err(|_| format!("{flag} needs a number"))?;
    if !v.is_finite() || v < min || v > max {
        return Err(format!("{flag} must be between {min} and {max}"));
    }
    Ok(v)
}

pub fn parse_cli(args: &[String], settings: &Settings) -> Result<Spec, String> {
    let mut spec = Spec::from_settings(settings);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| it.next().cloned().ok_or_else(|| format!("{flag} needs a value"));
        match arg.as_str() {
            "--out" => spec.out = Some(PathBuf::from(value("--out")?)),
            "--recursive" | "-r" => spec.recursive = true,
            "--formats" => spec.formats = Formats::parse(&value("--formats")?)?,
            "--template" => spec.opts.template = value("--template")?,
            "--prefix" => spec.opts.prefix = value("--prefix")?,
            "--scale" => spec.opts.convert.import.scale = number(Some(&value("--scale")?), "--scale", 0.001, 1000.0)?,
            "--no-units" => spec.opts.convert.import.convert_units = false,
            "--up" => spec.opts.convert.import.up = parse_up(&value("--up")?)?,
            "--no-center" => spec.opts.convert.import.center = false,
            "--kind" => match value("--kind")?.to_ascii_lowercase().as_str() {
                "static" => spec.opts.convert.static_prop = true,
                "physics" => spec.opts.convert.static_prop = false,
                other => return Err(format!("unknown kind {other}: static or physics")),
            },
            "--surface" => spec.opts.convert.surface_prop = value("--surface")?,
            "--mass" => spec.opts.convert.mass = number(Some(&value("--mass")?), "--mass", 0.1, 50000.0)?,
            "--collision" => spec.opts.convert.collision = parse_collision(&value("--collision")?)?,
            "--no-lod" => spec.opts.convert.lod.enabled = false,
            "--lod-levels" => {
                spec.opts.convert.lod.levels =
                    number(Some(&value("--lod-levels")?), "--lod-levels", 3.0, 6.0)? as usize;
            }
            "--lod-keep" => {
                let pct = number(Some(&value("--lod-keep")?), "--lod-keep", 20.0, 90.0)?;
                spec.opts.convert.lod.reduction = pct / 100.0;
            }
            "--no-material-params" => spec.opts.convert.keep_params = false,
            "--overwrite" => spec.opts.overwrite = true,
            "--threads" => {
                spec.opts.threads = clamp_threads(number(Some(&value("--threads")?), "--threads", 1.0, 64.0)? as usize)
            }
            "--dry-run" => spec.dry_run = true,
            "--quiet" | "-q" => spec.quiet = true,
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            other => spec.inputs.push(PathBuf::from(other)),
        }
    }
    if spec.inputs.is_empty() {
        return Err("no file or folder given".to_owned());
    }
    if spec.out.is_none() && !spec.dry_run {
        return Err("--out DIR is required (the folder that gets models/ and materials/)".to_owned());
    }
    Ok(spec)
}

pub fn usage() -> &'static str {
    "usage: hammerpro --import-models FILE_OR_DIR... --out DIR [--recursive] [--formats obj,fbx,gltf]\n\
     \x20      [--template T] [--prefix P] [--scale S] [--no-units] [--up auto|y|z] [--no-center]\n\
     \x20      [--kind static|physics] [--surface NAME] [--mass KG] [--collision pieces|box|none]\n\
     \x20      [--no-lod] [--lod-levels N] [--lod-keep PCT] [--no-material-params]\n\
     \x20      [--overwrite] [--threads N] [--dry-run] [--quiet]"
}

#[cfg(windows)]
fn attach_console() {
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
fn attach_console() {}

pub fn run_cli(args: &[String]) -> i32 {
    attach_console();
    hp_app::register_translations();
    hp_ui_entities::register_translations();
    let settings = Settings::load_or_default(&Settings::default_path());
    hp_app::i18n::set_lang(settings.language);
    let spec = match parse_cli(args, &settings) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            eprintln!("{}", usage());
            return 2;
        }
    };
    let files = collect_files(&spec.inputs, spec.recursive, spec.formats);
    if files.is_empty() {
        eprintln!("{}", t!("entities.mb_no_files"));
        return 1;
    }
    let names = names_for(&files, &spec.opts.template, &spec.opts.prefix);
    if spec.dry_run {
        let entries = scan(&files, &spec.opts, spec.out.as_deref(), &AtomicBool::new(false), &AtomicUsize::new(0));
        for e in &entries {
            let state = match &e.error {
                Some(err) => format!("error: {err}"),
                None => format!(
                    "{} triangles, {:.1} x {:.1} x {:.1}{}{}",
                    e.triangles,
                    e.size.x,
                    e.size.y,
                    e.size.z,
                    if e.exists { ", exists" } else { "" },
                    if e.warnings.is_empty() { String::new() } else { format!(", {}", e.warnings.join("; ")) }
                ),
            };
            println!("{} -> {}: {state}", e.path.display(), e.name);
        }
        return i32::from(entries.iter().any(|e| e.error.is_some()));
    }
    let Some(out) = spec.out.clone() else { return 2 };
    let quiet = spec.quiet;
    let progress = move |r: &hp_ui_entities::model_import::batch::FileResult| {
        if !quiet {
            println!("{}", r.line());
        }
    };
    let (_, summary) = run(&files, &names, &spec.opts, &out, &AtomicBool::new(false), &progress);
    println!("{}", summary.text());
    i32::from(summary.errors > 0)
}

pub fn camera_base(state: &mut AppState, reach: f64) -> DVec3 {
    let cam = state.cameras.cam3d;
    let dir = cam.forward();
    let opts = hp_editor::PickOptions { include_entities: false, max_distance: 8192.0, ..Default::default() };
    let hit = state.doc.pick_ray(&hp_geom::Ray::new(cam.pos, dir), &opts, &|e| state.entity_bounds(e));
    match hit {
        Some(h) => h.point,
        None => cam.pos + dir * reach.clamp(64.0, 1024.0),
    }
}

pub fn place_grid(
    state: &mut AppState,
    items: &[(String, (DVec3, DVec3))],
    class: &str,
    grid: GridOptions,
) -> Result<usize, String> {
    if items.is_empty() {
        return Ok(0);
    }
    let bounds: Vec<(DVec3, DVec3)> = items.iter().map(|(_, b)| *b).collect();
    let biggest = bounds.iter().map(|(lo, hi)| (*hi - *lo).length()).fold(0.0, f64::max);
    let base = camera_base(state, biggest * 1.5);
    let offsets = hp_ui_entities::model_import::batch::grid_offsets(&bounds, grid);
    let mut made: Vec<(DVec3, Vec<(String, String)>)> = Vec::with_capacity(items.len());
    for ((model, _), offset) in items.iter().zip(&offsets) {
        let kv = hp_ui_viewports::placement::entity_keyvalues(state, class, &[("model", model)]);
        made.push((base + *offset, kv));
    }
    let ids = state
        .doc
        .transaction("Import models", |doc| {
            let mut ids = Vec::with_capacity(made.len());
            for (origin, kv) in &made {
                ids.push(doc.create_point_entity(class, *origin, kv)?);
            }
            let refs: Vec<hp_editor::ObjRef> = ids.iter().map(|id| hp_editor::ObjRef::Entity(*id)).collect();
            doc.select(&refs, hp_editor::SelectOp::Replace);
            Ok::<_, hp_editor::EditError>(ids)
        })
        .map_err(|e| e.to_string())?;
    state.notify(Severity::Success, tf!("entities.mb_placed", ids.len()));
    Ok(ids.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_command_line_builds_a_batch() {
        let s = Settings::default();
        let spec = parse_cli(
            &args(&[
                "a.obj",
                "dir",
                "--out",
                "out",
                "-r",
                "--formats",
                "obj,glb",
                "--template",
                "{folder}_{name}",
                "--prefix",
                "p_",
                "--scale",
                "2.5",
                "--no-units",
                "--up",
                "z",
                "--kind",
                "physics",
                "--mass",
                "40",
                "--collision",
                "box",
                "--no-lod",
                "--overwrite",
                "--threads",
                "3",
                "--lod-levels",
                "5",
                "--lod-keep",
                "40",
            ]),
            &s,
        )
        .expect("parse");
        assert_eq!(spec.inputs.len(), 2);
        assert!(spec.recursive && spec.opts.overwrite && !spec.formats.fbx && spec.formats.gltf);
        assert_eq!(spec.opts.template, "{folder}_{name}");
        assert_eq!(spec.opts.prefix, "p_");
        assert_eq!(spec.opts.convert.import.scale, 2.5);
        assert!(!spec.opts.convert.import.convert_units);
        assert_eq!(spec.opts.convert.import.up, Some(UpAxis::Z));
        assert!(!spec.opts.convert.static_prop);
        assert_eq!(spec.opts.convert.collision, Collision::Box);
        assert!(!spec.opts.convert.lod.enabled);
        assert_eq!(spec.opts.convert.lod.levels, 5);
        assert!((spec.opts.convert.lod.reduction - 0.4).abs() < 1e-9);
        assert_eq!(spec.opts.threads, 3);
        assert_eq!(spec.out, Some(PathBuf::from("out")));
    }

    #[test]
    fn bad_command_lines_are_refused() {
        let s = Settings::default();
        for bad in [
            vec!["--out", "x"],
            vec!["a.obj"],
            vec!["a.obj", "--out"],
            vec!["a.obj", "--out", "o", "--scale", "abc"],
            vec!["a.obj", "--out", "o", "--scale", "0"],
            vec!["a.obj", "--out", "o", "--collision", "sphere"],
            vec!["a.obj", "--out", "o", "--formats", "stl"],
            vec!["a.obj", "--out", "o", "--wat"],
            vec!["a.obj", "--out", "o", "--up", "x"],
        ] {
            assert!(parse_cli(&args(&bad), &s).is_err(), "{bad:?}");
        }
        assert!(parse_cli(&args(&["a.obj", "--dry-run"]), &s).is_ok(), "a dry run needs no output folder");
    }

    #[test]
    fn a_headless_run_converts_a_folder() {
        let dir = std::env::temp_dir().join(format!("hp_batch_cli_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("in/sub")).unwrap();
        for (f, size) in [("in/a.obj", 40), ("in/sub/b.obj", 60)] {
            std::fs::write(
                dir.join(f),
                format!("v 0 0 0\nv {size} 0 0\nv 0 {size} 0\nv 0 0 {size}\nf 1 2 3\nf 1 2 4\nf 1 3 4\nf 2 3 4\n"),
            )
            .unwrap();
        }
        let out = dir.join("out");
        let a = args(&[
            &dir.join("in").display().to_string(),
            "--out",
            &out.display().to_string(),
            "--recursive",
            "--no-units",
            "--quiet",
            "--prefix",
            "x_",
        ]);
        assert_eq!(run_cli(&a), 0);
        assert!(out.join("models/hammerpro/x_a.mdl").is_file());
        assert!(out.join("models/hammerpro/x_b.mdl").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_grid_of_props_is_one_undo_step() {
        let mut state = hp_app::testing::test_state();
        let before = state.doc.scene().entities().len();
        let items = vec![
            ("models/hammerpro/a.mdl".to_owned(), (DVec3::new(-8.0, -8.0, 0.0), DVec3::new(8.0, 8.0, 16.0))),
            ("models/hammerpro/b.mdl".to_owned(), (DVec3::new(-8.0, -8.0, 0.0), DVec3::new(8.0, 8.0, 16.0))),
            ("models/hammerpro/c.mdl".to_owned(), (DVec3::new(-8.0, -8.0, 0.0), DVec3::new(8.0, 8.0, 16.0))),
        ];
        assert_eq!(place_grid(&mut state, &items, "prop_static", GridOptions { columns: 2, gap: 16.0 }), Ok(3));
        assert_eq!(state.doc.scene().entities().len(), before + 3);
        assert!(state.doc.undo());
        assert_eq!(state.doc.scene().entities().len(), before);
        assert_eq!(place_grid(&mut state, &[], "prop_static", GridOptions::default()), Ok(0));
    }
}
