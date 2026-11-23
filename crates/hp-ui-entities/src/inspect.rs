use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use egui::{RichText, Ui};
use hp_app::{t, tf, theme};

const MAX_MDL_BYTES: usize = 64 << 20;
const CACHE: usize = 32;
const MAX_LISTED: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MatStatus {
    Ok,
    MissingVmt,
    MissingTexture(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Inspection {
    pub skins: Vec<Vec<(String, MatStatus)>>,
    pub includes: Vec<(String, bool)>,
    pub files: Vec<(String, bool)>,
    pub bones: u32,
    pub attachments: u32,
    pub attachment_names: Vec<String>,
    pub lods: Option<u32>,
    pub sequences: u32,
    pub body_parts: u32,
    pub flex_controllers: u32,
    pub surface_prop: String,
    pub error: Option<String>,
}

impl Inspection {
    pub fn problems(&self) -> usize {
        self.skins.iter().flatten().filter(|(_, s)| *s != MatStatus::Ok).count()
            + self.includes.iter().filter(|(_, ok)| !ok).count()
            + self.files.iter().filter(|(_, ok)| !ok).count()
    }
}

const H_NUM_ATTACHMENTS: usize = 240;
const ATTACHMENT_SIZE: usize = 92;
const MAX_NAME: usize = 128;
const MAX_LODS: u32 = 8;

fn i32_at(bytes: &[u8], offset: usize) -> Option<i32> {
    let b = bytes.get(offset..offset.checked_add(4)?)?;
    Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn attachment_names(mdl: &[u8]) -> Vec<String> {
    let count = i32_at(mdl, H_NUM_ATTACHMENTS).and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
    let Some(index) = i32_at(mdl, H_NUM_ATTACHMENTS + 4).and_then(|n| usize::try_from(n).ok()) else {
        return Vec::new();
    };
    (0..count.min(MAX_LISTED))
        .map_while(|i| {
            let entry = index.checked_add(i.checked_mul(ATTACHMENT_SIZE)?)?;
            mdl.get(entry..entry.checked_add(ATTACHMENT_SIZE)?)?;
            let start = i64::try_from(entry).ok()? + i64::from(i32_at(mdl, entry)?);
            let tail = mdl.get(usize::try_from(start).ok()?..)?;
            let end = tail.iter().take(MAX_NAME).position(|&b| b == 0)?;
            Some(String::from_utf8_lossy(&tail[..end]).into_owned())
        })
        .collect()
}

fn vvd_lods(vvd: &[u8]) -> Option<u32> {
    if vvd.get(..4)? != b"IDSV" {
        return None;
    }
    let n = u32::try_from(i32_at(vvd, 12)?).ok()?;
    (1..=MAX_LODS).contains(&n).then_some(n)
}

fn compose(dir: &str, texture: &str) -> String {
    let texture = texture.trim_start_matches(['/', '\\']);
    let dir = dir.trim_start_matches(['/', '\\']);
    let joined = if dir.is_empty() || dir.ends_with(['/', '\\']) {
        format!("{dir}{texture}")
    } else {
        format!("{dir}/{texture}")
    };
    hp_mdl::normalize_material_name(&joined)
}

pub(crate) fn inspect(fs: &hp_fs::GameFs, path: &str) -> Inspection {
    let mut out = Inspection::default();
    let parsed = fs.read_with(path, |bytes| {
        if bytes.len() > MAX_MDL_BYTES {
            return Err("file too big".to_owned());
        }
        let info = hp_mdl::read_header(bytes).map_err(|e| e.to_string())?;
        let deps = hp_mdl::read_dependencies(bytes).map_err(|e| e.to_string())?;
        Ok((info, deps, attachment_names(bytes)))
    });
    let (info, deps, names) = match parsed {
        None => {
            out.error = Some(t!("entities.inspect_not_found").to_owned());
            return out;
        }
        Some(Err(e)) => {
            out.error = Some(e);
            return out;
        }
        Some(Ok(v)) => v,
    };
    out.bones = info.num_bones;
    out.attachments = info.num_attachments;
    out.attachment_names = names;
    out.sequences = info.num_sequences;
    out.body_parts = info.num_body_parts;
    out.flex_controllers = info.num_flex_controllers;
    out.surface_prop = info.surface_prop.clone();
    let (vvd, vtx) = hp_mdl::sibling_paths(path);
    out.files.push((vvd.clone(), fs.exists(&vvd)));
    out.lods = fs.read_with(&vvd, vvd_lods).flatten();
    let vtx_found = vtx.iter().find(|p| fs.exists(p)).cloned();
    out.files
        .push((vtx_found.clone().unwrap_or_else(|| vtx.first().cloned().unwrap_or_default()), vtx_found.is_some()));
    let phy = path.strip_suffix(".mdl").map(|s| format!("{s}.phy"));
    if let Some(phy) = phy {
        let exists = fs.exists(&phy);
        if exists {
            out.files.push((phy, true));
        }
    }
    for inc in deps.include_models.iter().take(MAX_LISTED) {
        out.includes.push((inc.clone(), fs.exists(inc)));
    }
    let mut status_of: HashMap<String, (String, MatStatus)> = HashMap::new();
    let families: Vec<Vec<u16>> = if deps.skin_families.is_empty() {
        vec![(0..deps.textures.len().min(MAX_LISTED) as u16).collect()]
    } else {
        deps.skin_families.iter().take(MAX_LISTED).cloned().collect()
    };
    for family in families {
        let mut row: Vec<(String, MatStatus)> = Vec::new();
        for &t in family.iter().take(MAX_LISTED) {
            let Some(texture) = deps.textures.get(usize::from(t)) else { continue };
            let key = texture.to_ascii_lowercase();
            let entry = status_of.entry(key).or_insert_with(|| {
                let candidates: Vec<String> = if deps.material_dirs.is_empty() {
                    vec![compose("", texture)]
                } else {
                    deps.material_dirs.iter().map(|d| compose(d, texture)).collect()
                };
                match candidates.iter().find(|c| fs.exists(&format!("materials/{c}.vmt"))) {
                    None => (candidates.first().cloned().unwrap_or_default(), MatStatus::MissingVmt),
                    Some(m) => {
                        let tex = fs.load_material(m).and_then(|mat| mat.base_texture());
                        let status = match tex {
                            Some(t) if !fs.exists(&format!("materials/{t}.vtf")) && fs.custom_image(&t).is_none() => {
                                MatStatus::MissingTexture(t)
                            }
                            _ => MatStatus::Ok,
                        };
                        (m.clone(), status)
                    }
                }
            });
            if !row.iter().any(|(m, _)| *m == entry.0) {
                row.push(entry.clone());
            }
        }
        out.skins.push(row);
    }
    out
}

#[derive(Default)]
pub(crate) struct Inspector {
    generation: u64,
    done: HashMap<String, Arc<Inspection>>,
    order: Vec<String>,
    pending: Option<(String, Receiver<Inspection>)>,
}

impl Inspector {
    pub fn get(&mut self, ctx: &egui::Context, assets: &dyn hp_app::AssetsApi, path: &str) -> Option<Arc<Inspection>> {
        if self.generation != assets.generation() {
            *self = Inspector { generation: assets.generation(), ..Inspector::default() };
        }
        if let Some((p, rx)) = &self.pending
            && let Ok(result) = rx.try_recv()
        {
            let p = p.clone();
            self.pending = None;
            self.order.retain(|o| *o != p);
            self.order.push(p.clone());
            self.done.insert(p, Arc::new(result));
            while self.order.len() > CACHE {
                let old = self.order.remove(0);
                self.done.remove(&old);
            }
        }
        if let Some(i) = self.done.get(path) {
            return Some(Arc::clone(i));
        }
        let fs = assets.fs()?;
        if self.pending.as_ref().is_none_or(|(p, _)| p != path) {
            let (tx, rx) = channel();
            let (fs, p, repaint) = (fs.clone(), path.to_owned(), ctx.clone());
            let spawned = std::thread::Builder::new().name("hp-model-inspect".to_owned()).spawn(move || {
                let _ = tx.send(inspect(&fs, &p));
                repaint.request_repaint();
            });
            if spawned.is_ok() {
                self.pending = Some((path.to_owned(), rx));
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        None
    }
}

pub(crate) fn inspector_ui(ui: &mut Ui, inspection: Option<&Inspection>) {
    let Some(i) = inspection else {
        ui.horizontal(|ui| {
            hp_app::widgets::spinner(ui);
            ui.label(t!("common.loading"));
        });
        return;
    };
    if let Some(e) = &i.error {
        ui.colored_label(theme::error(), tf!("entities.model_failed", e));
        return;
    }
    let problems = i.problems();
    if problems == 0 {
        ui.colored_label(theme::success(), t!("entities.inspect_ok"));
    } else {
        ui.colored_label(theme::warning(), tf!("entities.inspect_problems", problems));
    }
    egui::Grid::new("hp_ui_entities.inspect_counts").num_columns(2).spacing([10.0, 2.0]).striped(true).show(ui, |ui| {
        for (label, n) in [
            (t!("entities.inspect_bones"), i.bones),
            (t!("entities.inspect_attachments"), i.attachments),
            (t!("entities.inspect_sequences"), i.sequences),
            (t!("entities.body_parts"), i.body_parts),
            (t!("entities.inspect_flexes"), i.flex_controllers),
        ] {
            ui.label(label);
            ui.label(n.to_string());
            ui.end_row();
        }
        if let Some(lods) = i.lods {
            ui.label(t!("entities.inspect_lods"));
            ui.label(lods.to_string());
            ui.end_row();
        }
    });
    if !i.attachment_names.is_empty() {
        let title = tf!("entities.inspect_attachment_list", i.attachment_names.len());
        egui::CollapsingHeader::new(title).id_salt("hp_ui_entities.inspect_attachments").show(ui, |ui| {
            for name in &i.attachment_names {
                ui.add(egui::Label::new(RichText::new(name).small()).truncate());
            }
        });
    }
    for (file, ok) in &i.files {
        let color = if *ok { ui.visuals().text_color() } else { theme::error() };
        ui.add(egui::Label::new(RichText::new(file).small().color(color)).truncate());
    }
    if !i.includes.is_empty() {
        ui.label(RichText::new(tf!("entities.inspect_includes", i.includes.len())).strong());
        for (m, ok) in &i.includes {
            let color = if *ok { ui.visuals().text_color() } else { theme::error() };
            ui.add(egui::Label::new(RichText::new(m).small().color(color)).truncate());
        }
    }
    for (s, mats) in i.skins.iter().enumerate() {
        let bad = mats.iter().filter(|(_, st)| *st != MatStatus::Ok).count();
        let title = if bad > 0 { tf!("entities.inspect_skin_bad", s, bad) } else { tf!("entities.skin_n", s) };
        egui::CollapsingHeader::new(title).id_salt(("hp_ui_entities.inspect_skin", s)).default_open(bad > 0).show(
            ui,
            |ui| {
                for (m, st) in mats {
                    let (color, tip) = match st {
                        MatStatus::Ok => (ui.visuals().text_color(), String::new()),
                        MatStatus::MissingVmt => (theme::error(), t!("entities.inspect_missing_vmt").to_owned()),
                        MatStatus::MissingTexture(tex) => (theme::warning(), tf!("entities.inspect_missing_tex", tex)),
                    };
                    let resp = ui.add(egui::Label::new(RichText::new(m).small().color(color)).truncate());
                    if !tip.is_empty() {
                        resp.on_hover_text(tip);
                    }
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_compose_like_the_engine() {
        assert_eq!(compose("models\\props_c17\\", "OilDrum001"), "models/props_c17/oildrum001");
        assert_eq!(compose("models/props", "/a"), "models/props/a");
        assert_eq!(compose("", "a"), "a");
    }

    fn with_attachments() -> Vec<u8> {
        let mut mdl = vec![0u8; 460];
        let put = |mdl: &mut Vec<u8>, at: usize, v: i32| mdl[at..at + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut mdl, H_NUM_ATTACHMENTS, 2);
        put(&mut mdl, H_NUM_ATTACHMENTS + 4, 256);
        put(&mut mdl, 256, 440 - 256);
        put(&mut mdl, 256 + ATTACHMENT_SIZE, 447 - (256 + ATTACHMENT_SIZE) as i32);
        mdl[440..447].copy_from_slice(b"muzzle\0");
        mdl[447..452].copy_from_slice(b"eyes\0");
        mdl
    }

    #[test]
    fn attachment_names_and_lods_never_panic() {
        let mdl = with_attachments();
        assert_eq!(attachment_names(&mdl), ["muzzle", "eyes"]);
        for len in 0..mdl.len() {
            assert!(attachment_names(&mdl[..len]).len() <= 2);
        }
        let mut bad = mdl.clone();
        bad[H_NUM_ATTACHMENTS..H_NUM_ATTACHMENTS + 4].copy_from_slice(&i32::MAX.to_le_bytes());
        assert_eq!(attachment_names(&bad), ["muzzle", "eyes"], "stops at the end of the file");
        bad[H_NUM_ATTACHMENTS + 4..H_NUM_ATTACHMENTS + 8].copy_from_slice(&(-5i32).to_le_bytes());
        assert!(attachment_names(&bad).is_empty());
        let mut neg = mdl.clone();
        neg[256..260].copy_from_slice(&(-10_000i32).to_le_bytes());
        assert!(attachment_names(&neg).is_empty());
        let vvd = |n: i32| [b"IDSV".as_slice(), &4i32.to_le_bytes(), &0i32.to_le_bytes(), &n.to_le_bytes()].concat();
        assert_eq!(vvd_lods(&vvd(3)), Some(3));
        assert_eq!(vvd_lods(&vvd(0)), None);
        assert_eq!(vvd_lods(&vvd(-1)), None);
        assert_eq!(vvd_lods(&vvd(99)), None);
        assert_eq!(vvd_lods(b"IDSV"), None);
        assert_eq!(vvd_lods(b"IDST0000000000000000"), None);
    }

    #[test]
    fn broken_and_missing_models_report_an_error() {
        crate::register_translations();
        let dir = std::env::temp_dir().join(format!("hp-ui-entities-inspect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("models"));
        let _ = std::fs::write(dir.join("models").join("bad.mdl"), b"IDST garbage");
        let fs = hp_fs::GameFs::empty();
        assert!(fs.add_game_dir(&dir).is_ok());
        assert!(inspect(&fs, "models/bad.mdl").error.is_some());
        assert!(inspect(&fs, "models/none.mdl").error.is_some());
        let mut state = hp_app::testing::test_state();
        hp_app::testing::run_ui(&mut state, |ui, _| {
            inspector_ui(ui, None);
            inspector_ui(ui, Some(&inspect(&fs, "models/bad.mdl")));
            let mut ok = Inspection::default();
            ok.skins.push(vec![("a".into(), MatStatus::Ok), ("b".into(), MatStatus::MissingVmt)]);
            ok.includes.push(("models/x.mdl".into(), false));
            assert_eq!(ok.problems(), 2);
            inspector_ui(ui, Some(&ok));
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn real_gmod_model() {
        let root = std::path::Path::new("D:/SteamLibrary/steamapps/common/GarrysMod");
        if !root.join("garrysmod").is_dir() {
            return;
        }
        let opts = hp_fs::MountOptions {
            mount_addons: false,
            mount_workshop: false,
            mount_cfg: false,
            ..hp_fs::MountOptions::default()
        };
        let Ok(fs) = hp_fs::GameFs::for_gmod(root, &opts) else { return };
        let i = inspect(&fs, "models/props_c17/oildrum001.mdl");
        assert!(i.error.is_none(), "{:?}", i.error);
        assert!(!i.skins.is_empty() && !i.skins[0].is_empty());
        assert!(i.files.iter().all(|(_, ok)| *ok), "{:?}", i.files);
        eprintln!("{i:#?}");
    }
}
