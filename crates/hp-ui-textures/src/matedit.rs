use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};

use egui::RichText;
use hp_app::settings::CustomDirSetting;
use hp_app::{AppState, Severity, t, tf, theme};

use crate::util::{normalize_material, same_path};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParamOp {
    Keep,
    Set(String),
    Remove,
}

pub fn apply_ops(text: &str, ops: &[(String, ParamOp)]) -> Result<String, hp_vmt::VmtError> {
    let mut out = text.to_owned();
    for (key, op) in ops {
        out = match op {
            ParamOp::Keep => continue,
            ParamOp::Set(v) => hp_vmt::set_param_in_text(&out, key, v)?,
            ParamOp::Remove => hp_vmt::remove_param_in_text(&out, key)?,
        };
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteOutcome {
    pub backup: Option<PathBuf>,
    pub created: bool,
}

pub fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".bak");
    PathBuf::from(s)
}

pub fn write_with_backup(path: &Path, text: &str) -> std::io::Result<WriteOutcome> {
    let existed = path.is_file();
    let mut backup = None;
    if existed {
        let bak = backup_path(path);
        if !bak.exists() {
            std::fs::copy(path, &bak)?;
            backup = Some(bak);
        }
    } else if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, text)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(WriteOutcome { backup, created: !existed })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VmtLocation {
    File(PathBuf),
    Virtual(PathBuf),
    Archive(String),
    Missing,
}

#[derive(Clone, Debug)]
pub struct LoadedVmt {
    pub name: String,
    pub text: Option<String>,
    pub material: Option<hp_vmt::Material>,
    pub location: VmtLocation,
}

pub fn load_vmt(fs: &hp_fs::GameFs, name: &str) -> LoadedVmt {
    let vpath = format!("materials/{name}.vmt");
    let text = fs.read_text(&vpath);
    let material = text.as_deref().and_then(|t| hp_vmt::parse(t).ok());
    let location = if text.is_none() {
        VmtLocation::Missing
    } else if let Some(p) = fs.disk_path(&vpath) {
        VmtLocation::File(p)
    } else {
        let source = fs.which_source(&vpath).and_then(|id| fs.source(id).map(|s| (id, s)));
        match source {
            Some((id, info)) => match (&info.kind, fs.custom_dir_layout(id)) {
                (hp_fs::SourceKind::Custom(dir), Some(layout)) => {
                    VmtLocation::Virtual(crate::folders::vmt_target(&dir.path, &layout, name).0)
                }
                _ => VmtLocation::Archive(info.label),
            },
            None => VmtLocation::Archive(String::new()),
        }
    };
    LoadedVmt { name: name.to_owned(), text, material, location }
}

#[derive(Clone, Debug)]
pub struct ApplyItem {
    pub name: String,
    pub new_name: String,
    pub path: PathBuf,
    pub base_text: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ApplyResult {
    pub item: ApplyItem,
    pub outcome: Result<Option<WriteOutcome>, String>,
}

pub fn apply_item(item: &ApplyItem, ops: &[(String, ParamOp)]) -> Result<Option<WriteOutcome>, String> {
    let base = match &item.base_text {
        Some(t) if !item.path.is_file() => t.clone(),
        _ => std::fs::read_to_string(&item.path).map_err(|e| e.to_string())?,
    };
    let edited = apply_ops(&base, ops).map_err(|e| e.to_string())?;
    let new_file = !item.path.is_file();
    if edited == base && !new_file {
        return Ok(None);
    }
    write_with_backup(&item.path, &edited).map(Some).map_err(|e| e.to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    Texture,
    Text,
    Flag,
    SurfaceProp,
    Color,
}

pub const COMMON_PARAMS: &[(&str, ParamKind, &str)] = &[
    ("$basetexture", ParamKind::Texture, "tex.me.p.basetexture"),
    ("$bumpmap", ParamKind::Texture, "tex.me.p.bumpmap"),
    ("$surfaceprop", ParamKind::SurfaceProp, "tex.me.p.surfaceprop"),
    ("$translucent", ParamKind::Flag, "tex.me.p.translucent"),
    ("$alphatest", ParamKind::Flag, "tex.me.p.alphatest"),
    ("$nocull", ParamKind::Flag, "tex.me.p.nocull"),
    ("$selfillum", ParamKind::Flag, "tex.me.p.selfillum"),
    ("$envmap", ParamKind::Text, "tex.me.p.envmap"),
    ("$envmaptint", ParamKind::Color, "tex.me.p.envmaptint"),
    ("%keywords", ParamKind::Text, "tex.me.p.keywords"),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpChoice {
    #[default]
    Keep,
    Set,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamEdit {
    pub key: String,
    pub kind: ParamKind,
    pub op: OpChoice,
    pub value: String,
}

impl ParamEdit {
    pub fn to_op(&self) -> Option<(String, ParamOp)> {
        let key = self.key.trim();
        if key.is_empty() || key.contains('"') {
            return None;
        }
        let op = match (self.op, self.kind) {
            (OpChoice::Keep, _) => return None,
            (OpChoice::Set, ParamKind::Flag) => ParamOp::Set("1".to_owned()),
            (OpChoice::Set, _) => {
                if self.value.contains('"') {
                    return None;
                }
                ParamOp::Set(self.value.trim().to_owned())
            }
            (OpChoice::Remove, _) => ParamOp::Remove,
        };
        Some((key.to_owned(), op))
    }
}

pub fn collect_ops(rows: &[ParamEdit]) -> Vec<(String, ParamOp)> {
    rows.iter().filter_map(ParamEdit::to_op).collect()
}

pub fn plan(loaded: &[&LoadedVmt], copy_into: Option<(&Path, &hp_fs::CustomLayout)>) -> Vec<ApplyItem> {
    let mut out = Vec::new();
    for m in loaded {
        match &m.location {
            VmtLocation::File(p) => {
                out.push(ApplyItem { name: m.name.clone(), new_name: m.name.clone(), path: p.clone(), base_text: None })
            }
            VmtLocation::Virtual(p) => out.push(ApplyItem {
                name: m.name.clone(),
                new_name: m.name.clone(),
                path: p.clone(),
                base_text: m.text.clone(),
            }),
            VmtLocation::Archive(_) => {
                if let (Some((dir, layout)), Some(text)) = (copy_into, &m.text) {
                    let (path, new_name) = crate::folders::vmt_target(dir, layout, &m.name);
                    out.push(ApplyItem { name: m.name.clone(), new_name, path, base_text: Some(text.clone()) });
                }
            }
            VmtLocation::Missing => {}
        }
    }
    out
}

enum TargetState {
    Loading,
    Loaded(Box<LoadedVmt>),
}

struct Target {
    name: String,
    state: TargetState,
}

pub struct MaterialEditor {
    targets: Vec<Target>,
    loader: Option<(u64, Receiver<(u64, LoadedVmt)>)>,
    load_serial: u64,
    rows: Vec<ParamEdit>,
    custom: Vec<ParamEdit>,
    add_name: String,
    copy_archived: bool,
    copy_dest: usize,
    applying: Option<Receiver<Vec<ApplyResult>>>,
    last_report: Option<(Severity, String)>,
    dest_layout: Option<(PathBuf, hp_fs::CustomLayout)>,
    was_open: bool,
    reload_after_apply: Option<PendingReload>,
    escape: crate::util::DialogEscape,
    batch: bool,
    active: usize,
    single: crate::matedit_single::SingleEditor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingReload {
    Now,
    AfterRemount,
}

impl Default for MaterialEditor {
    fn default() -> Self {
        Self {
            targets: Vec::new(),
            loader: None,
            load_serial: 0,
            rows: COMMON_PARAMS
                .iter()
                .map(|(k, kind, _)| ParamEdit {
                    key: (*k).to_owned(),
                    kind: *kind,
                    op: OpChoice::Keep,
                    value: String::new(),
                })
                .collect(),
            custom: Vec::new(),
            add_name: String::new(),
            copy_archived: false,
            copy_dest: 0,
            applying: None,
            last_report: None,
            dest_layout: None,
            was_open: false,
            reload_after_apply: None,
            escape: crate::util::DialogEscape::default(),
            batch: false,
            active: 0,
            single: crate::matedit_single::SingleEditor::default(),
        }
    }
}

impl MaterialEditor {
    pub fn len(&self) -> usize {
        self.targets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    pub fn names(&self) -> Vec<String> {
        self.targets.iter().map(|t| t.name.clone()).collect()
    }

    pub fn rows_mut(&mut self) -> &mut Vec<ParamEdit> {
        &mut self.rows
    }

    pub fn loaded(&self, name: &str) -> Option<&LoadedVmt> {
        self.targets.iter().find(|t| t.name == name).and_then(|t| match &t.state {
            TargetState::Loaded(l) => Some(l.as_ref()),
            TargetState::Loading => None,
        })
    }

    pub fn is_busy(&self) -> bool {
        self.applying.is_some() || self.loader.is_some() || self.reload_after_apply.is_some()
    }

    pub fn apply(&mut self, ctx: &egui::Context, state: &AppState) {
        self.start_apply(ctx, state);
    }

    pub fn set_materials(&mut self, ctx: &egui::Context, state: &AppState, names: Vec<String>) {
        self.targets.clear();
        self.add_materials(ctx, state, names);
    }

    pub fn add_materials(&mut self, ctx: &egui::Context, state: &AppState, names: Vec<String>) {
        for n in names {
            let n = normalize_material(&n);
            if !n.is_empty() && !self.targets.iter().any(|t| t.name == n) {
                self.targets.push(Target { name: n, state: TargetState::Loading });
            }
        }
        self.reload(ctx, state);
    }

    fn reload(&mut self, ctx: &egui::Context, state: &AppState) {
        for t in &mut self.targets {
            t.state = TargetState::Loading;
        }
        self.load_serial += 1;
        let serial = self.load_serial;
        let Some(fs) = state.assets.fs().cloned() else {
            for t in &mut self.targets {
                t.state = TargetState::Loaded(Box::new(LoadedVmt {
                    name: t.name.clone(),
                    text: None,
                    material: None,
                    location: VmtLocation::Missing,
                }));
            }
            self.loader = None;
            return;
        };
        let names = self.names();
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-vmt-load".into()).spawn(move || {
            for n in names {
                if tx.send((serial, load_vmt(&fs, &n))).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
        match spawned {
            Ok(_) => self.loader = Some((serial, rx)),
            Err(e) => {
                log::warn!("cannot start the material loader: {e}");
                self.loader = None;
            }
        }
    }

    fn poll(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if let Some((serial, rx)) = &self.loader {
            loop {
                match rx.try_recv() {
                    Ok((s, loaded)) if s == *serial => {
                        if let Some(t) = self.targets.iter_mut().find(|t| t.name == loaded.name) {
                            t.state = TargetState::Loaded(Box::new(loaded));
                        }
                    }
                    Ok(_) => {}
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.loader = None;
                        break;
                    }
                }
            }
        }
        if let Some(rx) = &self.applying {
            match rx.try_recv() {
                Ok(results) => {
                    self.applying = None;
                    self.finish_apply(state, results);
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.applying = None,
            }
        }
        let reload = match self.reload_after_apply {
            Some(PendingReload::Now) => true,
            Some(PendingReload::AfterRemount) => state.assets.status() != hp_app::state::AssetStatus::Mounting,
            None => false,
        };
        if reload {
            self.reload_after_apply = None;
            self.reload(ctx, state);
        } else if self.reload_after_apply.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn finish_apply(&mut self, state: &mut AppState, results: Vec<ApplyResult>) {
        let mut changed = 0usize;
        let mut created = false;
        let mut errors = 0usize;
        for r in &results {
            match &r.outcome {
                Ok(Some(w)) => {
                    changed += 1;
                    created |= w.created;
                    state.assets.invalidate_material(&r.item.name);
                    if r.item.new_name != r.item.name {
                        state.assets.invalidate_material(&r.item.new_name);
                    }
                    if let Some(b) = &w.backup {
                        state.log(Severity::Info, tf!("tex.me.backup_made", b.display()));
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    errors += 1;
                    state.log(Severity::Error, tf!("tex.me.failed", r.item.name, e));
                }
            }
        }
        if created {
            state.assets.remount(&state.settings);
        }
        if changed > 0 {
            self.reload_after_apply = Some(if created { PendingReload::AfterRemount } else { PendingReload::Now });
        }
        let (sev, text) = if errors > 0 {
            (Severity::Warning, tf!("tex.me.done_errors", changed, errors))
        } else {
            (Severity::Success, tf!("tex.me.done", changed))
        };
        state.notify(sev, text.clone());
        self.last_report = Some((sev, text));
    }

    fn start_apply(&mut self, ctx: &egui::Context, state: &AppState) {
        let dest = if self.copy_archived { self.dest_layout(state) } else { None };
        let loaded: Vec<&LoadedVmt> = self
            .targets
            .iter()
            .filter_map(|t| match &t.state {
                TargetState::Loaded(l) => Some(l.as_ref()),
                TargetState::Loading => None,
            })
            .collect();
        let items = plan(&loaded, dest.as_ref().map(|(p, l)| (p.as_path(), l)));
        let mut ops = collect_ops(&self.rows);
        ops.extend(collect_ops(&self.custom));
        if items.is_empty() || ops.is_empty() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-vmt-write".into()).spawn(move || {
            let results: Vec<ApplyResult> =
                items.into_iter().map(|item| ApplyResult { outcome: apply_item(&item, &ops), item }).collect();
            let _ = tx.send(results);
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => {
                self.applying = Some(rx);
                self.last_report = None;
            }
            Err(e) => log::warn!("cannot start the material writer: {e}"),
        }
    }

    fn dest_layout(&mut self, state: &AppState) -> Option<(PathBuf, hp_fs::CustomLayout)> {
        let dir: &CustomDirSetting = state.settings.game.custom_texture_dirs.get(self.copy_dest)?;
        if let Some((p, l)) = &self.dest_layout
            && same_path(p, &dir.path)
        {
            return Some((p.clone(), l.clone()));
        }
        let layout = crate::folders::layout_of(state, dir);
        self.dest_layout = Some((dir.path.clone(), layout.clone()));
        Some((dir.path.clone(), layout))
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if let Some(link) = crate::shared::take_texture_link(ctx) {
            self.single.set(&link.key, Some(&link.texture));
        }
        if let Some(req) = crate::shared::take_editor(ctx) {
            self.set_materials(ctx, state, req.0);
            state.ui.show_material_editor = true;
        }
        self.poll(ctx, state);
        if !state.ui.show_material_editor {
            self.was_open = false;
            return;
        }
        if !self.was_open {
            self.was_open = true;
            if self.targets.is_empty() {
                let browser = crate::shared::selection(ctx);
                let names = if browser.is_empty() { vec![state.current_material.clone()] } else { browser };
                self.set_materials(ctx, state, names);
            }
        }
        let mut open = true;
        self.escape.begin(ctx);
        let response = hp_app::chrome::Window::new(t!("tex.me.title"))
            .id(egui::Id::unique("hp_textures_material_editor"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([1000.0, 680.0])
            .show(ctx, |ui| self.ui(ui, state));
        let escape = self.escape.update(ctx, response.as_ref().map(|r| &r.response));
        if !open || escape {
            state.ui.show_material_editor = false;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("tex.me.load"));
            if ui.button(t!("tex.me.load_current")).on_hover_text(&state.current_material).clicked() {
                let names = vec![state.current_material.clone()];
                self.set_materials(&ctx, state, names);
            }
            let browser = crate::shared::selection(&ctx);
            if ui
                .add_enabled(!browser.is_empty(), egui::Button::new(tf!("tex.me.load_browser", browser.len())))
                .on_hover_text(t!("tex.me.load_browser_tip"))
                .clicked()
            {
                self.set_materials(&ctx, state, browser);
            }
            if ui.button(t!("tex.me.load_map")).on_hover_text(t!("tex.me.load_map_tip")).clicked() {
                let names = selection_materials(&state.doc);
                self.set_materials(&ctx, state, names);
            }
            ui.separator();
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.add_name).hint_text(t!("tex.me.add_hint")).desired_width(200.0),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button(t!("common.add")).clicked() || enter) && !self.add_name.trim().is_empty() {
                let name = std::mem::take(&mut self.add_name);
                self.add_materials(&ctx, state, vec![name]);
            }
            if ui.add_enabled(!self.targets.is_empty(), egui::Button::new(t!("tex.me.reload"))).clicked() {
                self.reload(&ctx, state);
            }
            if ui.add_enabled(!self.targets.is_empty(), egui::Button::new(t!("common.clear"))).clicked() {
                self.targets.clear();
            }
        });
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.batch, false, RichText::new(t!("tex.me1.mode_single")).strong())
                .on_hover_text(t!("tex.me1.mode_single_tip"));
            ui.selectable_value(&mut self.batch, true, RichText::new(t!("tex.me1.mode_batch")).strong())
                .on_hover_text(t!("tex.me1.mode_batch_tip"));
        });
        ui.separator();
        if !self.batch {
            self.single_ui(ui, state);
            return;
        }

        egui::Panel::bottom("hp_me_footer").resizable(false).frame(egui::Frame::NONE).show(ui, |ui| {
            ui.separator();
            self.footer_ui(ui, state);
            ui.add_space(2.0);
        });
        let table_h = (ui.available_height() * 0.4).max(110.0);
        ui.allocate_ui(egui::vec2(ui.available_width(), table_h), |ui| self.table_ui(ui));
        ui.separator();
        egui::ScrollArea::vertical().id_salt("hp_me_changes").auto_shrink([false, false]).show(ui, |ui| {
            self.changes_ui(ui);
        });
    }

    fn single_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        if self.targets.is_empty() {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| ui.weak(t!("tex.me.empty")));
            return;
        }
        self.active = self.active.min(self.targets.len() - 1);
        let names = self.names();
        let loaded = match &self.targets[self.active].state {
            TargetState::Loaded(l) => l.as_ref().clone(),
            TargetState::Loading => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t!("common.loading"));
                });
                return;
            }
        };
        let fresh = self.single.name.as_deref() != Some(loaded.name.as_str());
        if fresh || (!self.single.is_dirty() && loaded.text.as_deref().is_some_and(|t| t != self.single.saved)) {
            self.single.load(&loaded.name, loaded.text.as_deref());
        }
        let archived = matches!(loaded.location, VmtLocation::Archive(_));
        let dirs = state.settings.game.custom_texture_dirs.clone();
        let mut copy_dest = self.copy_dest;
        let mut location = |ui: &mut egui::Ui| {
            location_label(ui, &loaded.location);
            if archived {
                if dirs.is_empty() {
                    ui.colored_label(theme::warning(), t!("tex.me.no_custom"));
                } else {
                    copy_dest = copy_dest.min(dirs.len() - 1);
                    let current = dirs.get(copy_dest).map(|d| crate::util::folder_label(&d.path)).unwrap_or_default();
                    ui.label(t!("tex.me1.copy_to"));
                    egui::ComboBox::from_id_salt("hp_me1_copy_dest").selected_text(current).show_ui(ui, |ui| {
                        for (i, d) in dirs.iter().enumerate() {
                            ui.selectable_value(&mut copy_dest, i, crate::util::folder_label(&d.path))
                                .on_hover_text(d.path.display().to_string());
                        }
                    });
                }
            }
        };
        let action = self.single.ui(ui, state, &names, self.active, &mut location);
        self.copy_dest = copy_dest;
        match action {
            Some(crate::matedit_single::SingleAction::Save) => self.save_single(state, &loaded),
            Some(crate::matedit_single::SingleAction::Select(i)) => {
                if self.single.is_dirty() {
                    state.notify(Severity::Warning, t!("tex.me1.unsaved"));
                } else {
                    self.active = i;
                }
            }
            None => {}
        }
    }

    fn save_single(&mut self, state: &mut AppState, loaded: &LoadedVmt) {
        let dest = if matches!(loaded.location, VmtLocation::Archive(_)) { self.dest_layout(state) } else { None };
        let dest =
            dest.or_else(|| matches!(loaded.location, VmtLocation::Missing).then(|| self.dest_layout(state)).flatten());
        let mut items = plan(&[loaded], dest.as_ref().map(|(p, l)| (p.as_path(), l)));
        if items.is_empty()
            && matches!(loaded.location, VmtLocation::Missing)
            && let Some((dir, layout)) = &dest
        {
            let (path, new_name) = crate::folders::vmt_target(dir, layout, &loaded.name);
            items.push(ApplyItem { name: loaded.name.clone(), new_name, path, base_text: None });
        }
        let Some(item) = items.first() else {
            state.notify(Severity::Warning, t!("tex.me1.no_target"));
            return;
        };
        match write_with_backup(&item.path, &self.single.text) {
            Ok(w) => {
                state.assets.invalidate_material(&item.name);
                if item.new_name != item.name {
                    state.assets.invalidate_material(&item.new_name);
                }
                if let Some(b) = &w.backup {
                    state.log(Severity::Info, tf!("tex.me.backup_made", b.display()));
                }
                if w.created {
                    state.assets.remount(&state.settings);
                }
                self.single.saved = self.single.text.clone();
                self.reload_after_apply =
                    Some(if w.created { PendingReload::AfterRemount } else { PendingReload::Now });
                state.notify(Severity::Success, tf!("tex.me1.saved", item.path.display()));
            }
            Err(e) => state.notify(Severity::Error, tf!("tex.me.failed", item.name, e)),
        }
    }

    fn table_ui(&mut self, ui: &mut egui::Ui) {
        if self.targets.is_empty() {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| ui.weak(t!("tex.me.empty")));
            return;
        }
        let name_w = 240.0;
        let loc_w = 96.0;
        let col_w = 120.0;
        let row_h = 20.0;
        let mut remove: Option<usize> = None;
        egui::ScrollArea::horizontal().id_salt("hp_me_table_h").show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.add_space(22.0);
                    fixed(ui, name_w, |ui| ui.strong(t!("tex.me.col_material")));
                    fixed(ui, loc_w, |ui| ui.strong(t!("tex.me.col_location")));
                    fixed(ui, col_w, |ui| ui.strong(t!("tex.me.shader")));
                    for (key, _, _) in COMMON_PARAMS {
                        fixed(ui, col_w, |ui| ui.strong(*key));
                    }
                });
                egui::ScrollArea::vertical().id_salt("hp_me_table_v").auto_shrink([false, false]).show_rows(
                    ui,
                    row_h,
                    self.targets.len(),
                    |ui, range| {
                        for i in range {
                            let Some(target) = self.targets.get(i) else { continue };
                            ui.horizontal(|ui| {
                                ui.set_height(row_h);
                                if crate::icons::icon_button(ui, crate::icons::Icon::Close, t!("tex.me.remove_row"))
                                    .clicked()
                                {
                                    remove = Some(i);
                                }
                                fixed(ui, name_w, |ui| ui.label(&target.name).on_hover_text(&target.name));
                                match &target.state {
                                    TargetState::Loading => {
                                        fixed(ui, loc_w, |ui| ui.weak(t!("common.loading")));
                                    }
                                    TargetState::Loaded(l) => {
                                        fixed(ui, loc_w, |ui| location_label(ui, &l.location));
                                        let shader = l.material.as_ref().map_or("", |m| m.shader.as_str());
                                        fixed(ui, col_w, |ui| ui.label(shader));
                                        for (key, _, _) in COMMON_PARAMS {
                                            let v = l.material.as_ref().and_then(|m| m.get(key)).unwrap_or("");
                                            fixed(ui, col_w, |ui| {
                                                if v.is_empty() { ui.weak("-") } else { ui.label(v).on_hover_text(v) }
                                            });
                                        }
                                    }
                                }
                            });
                        }
                    },
                );
            });
        });
        if let Some(i) = remove
            && i < self.targets.len()
        {
            self.targets.remove(i);
        }
    }

    fn changes_ui(&mut self, ui: &mut egui::Ui) {
        hp_app::widgets::section(ui, t!("tex.me.changes"));
        egui::Grid::new("hp_me_params").num_columns(3).spacing([8.0, 4.0]).striped(true).show(ui, |ui| {
            for (row, (_, _, label_key)) in self.rows.iter_mut().zip(COMMON_PARAMS) {
                ui.label(t!(label_key)).on_hover_text(&row.key);
                op_selector(ui, row);
                value_editor(ui, row);
                ui.end_row();
            }
        });
        ui.add_space(4.0);
        hp_app::widgets::section(ui, t!("tex.me.custom"));
        let mut remove = None;
        egui::Grid::new("hp_me_custom").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
            for (i, row) in self.custom.iter_mut().enumerate() {
                ui.add(egui::TextEdit::singleline(&mut row.key).hint_text("$parameter").desired_width(140.0));
                op_selector(ui, row);
                value_editor(ui, row);
                if crate::icons::icon_button(ui, crate::icons::Icon::Close, t!("common.remove")).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = remove {
            self.custom.remove(i);
        }
        if ui.button(format!("+ {}", t!("tex.me.add_param"))).clicked() {
            self.custom.push(ParamEdit {
                key: String::new(),
                kind: ParamKind::Text,
                op: OpChoice::Set,
                value: String::new(),
            });
        }
    }

    fn footer_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let (mut files, mut virt, mut archived, mut missing, mut loading) = (0, 0, 0, 0, 0);
        for t in &self.targets {
            match &t.state {
                TargetState::Loading => loading += 1,
                TargetState::Loaded(l) => match l.location {
                    VmtLocation::File(_) => files += 1,
                    VmtLocation::Virtual(_) => virt += 1,
                    VmtLocation::Archive(_) => archived += 1,
                    VmtLocation::Missing => missing += 1,
                },
            }
        }
        ui.label(tf!("tex.me.summary", files, virt, archived, missing));
        if archived > 0 {
            let dirs = &state.settings.game.custom_texture_dirs;
            ui.horizontal(|ui| {
                ui.add_enabled(
                    !dirs.is_empty(),
                    egui::Checkbox::new(&mut self.copy_archived, t!("tex.me.copy_archived")),
                );
                if dirs.is_empty() {
                    ui.weak(t!("tex.me.no_custom"));
                } else {
                    self.copy_dest = self.copy_dest.min(dirs.len() - 1);
                    let current =
                        dirs.get(self.copy_dest).map(|d| crate::util::folder_label(&d.path)).unwrap_or_default();
                    egui::ComboBox::from_id_salt("hp_me_copy_dest").selected_text(current).show_ui(ui, |ui| {
                        for (i, d) in dirs.iter().enumerate() {
                            ui.selectable_value(&mut self.copy_dest, i, crate::util::folder_label(&d.path))
                                .on_hover_text(d.path.display().to_string());
                        }
                    });
                }
            });
            if self.copy_archived
                && let Some((dir, layout)) = self.dest_layout(state)
            {
                let mut shown = 0;
                for t in &self.targets {
                    if let TargetState::Loaded(l) = &t.state
                        && matches!(l.location, VmtLocation::Archive(_))
                    {
                        let (_, new_name) = crate::folders::vmt_target(&dir, &layout, &l.name);
                        if new_name != l.name && shown < 5 {
                            ui.colored_label(theme::warning(), tf!("tex.me.renamed", l.name, new_name));
                            shown += 1;
                        }
                    }
                }
            }
        }
        ui.weak(t!("tex.me.backup_note"));

        let mut ops = collect_ops(&self.rows);
        ops.extend(collect_ops(&self.custom));
        let writable = files
            + virt
            + if self.copy_archived && !state.settings.game.custom_texture_dirs.is_empty() { archived } else { 0 };
        ui.horizontal(|ui| {
            let busy = self.applying.is_some();
            let enabled = !busy && loading == 0 && writable > 0 && !ops.is_empty();
            let button = egui::Button::new(RichText::new(tf!("tex.me.apply", writable)).strong());
            let r = ui.add_enabled(enabled, button);
            let r = if ops.is_empty() { r.on_disabled_hover_text(t!("tex.me.nothing")) } else { r };
            if r.clicked() {
                let ctx = ui.ctx().clone();
                self.start_apply(&ctx, state);
            }
            if busy {
                ui.spinner();
                ui.label(t!("tex.me.applying"));
            } else if let Some((sev, text)) = &self.last_report {
                ui.colored_label(sev.color(), text);
            }
        });
    }
}

pub fn selection_materials(doc: &hp_editor::Document) -> Vec<String> {
    let mut faces = doc.selected_faces();
    faces.extend(doc.all_faces_of(&doc.selected_objects()));
    let mut out: Vec<String> = Vec::new();
    for f in faces {
        if let Some(side) = doc.face_side(f) {
            let n = normalize_material(&side.material);
            if !out.contains(&n) {
                out.push(n);
            }
        }
    }
    out
}

fn fixed<R>(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.allocate_ui_with_layout(
        egui::vec2(width, ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(width);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            add(ui)
        },
    )
    .inner
}

fn location_label(ui: &mut egui::Ui, loc: &VmtLocation) -> egui::Response {
    match loc {
        VmtLocation::File(p) => ui
            .colored_label(theme::success(), t!("tex.me.loc_file"))
            .on_hover_text(tf!("tex.me.loc_file_tip", p.display())),
        VmtLocation::Virtual(p) => ui
            .colored_label(theme::info(), t!("tex.me.loc_virtual"))
            .on_hover_text(tf!("tex.me.loc_virtual_tip", p.display())),
        VmtLocation::Archive(label) => ui
            .colored_label(theme::warning(), t!("tex.me.loc_archive"))
            .on_hover_text(tf!("tex.me.loc_archive_tip", label)),
        VmtLocation::Missing => {
            ui.colored_label(theme::error(), t!("tex.me.loc_missing")).on_hover_text(t!("tex.me.loc_missing_tip"))
        }
    }
}

fn op_selector(ui: &mut egui::Ui, row: &mut ParamEdit) {
    ui.horizontal(|ui| {
        let (set, remove) = if row.kind == ParamKind::Flag {
            (t!("tex.me.op_on"), t!("tex.me.op_off"))
        } else {
            (t!("tex.me.op_set"), t!("tex.me.op_remove"))
        };
        ui.selectable_value(&mut row.op, OpChoice::Keep, t!("tex.me.op_keep"));
        ui.selectable_value(&mut row.op, OpChoice::Set, set);
        ui.selectable_value(&mut row.op, OpChoice::Remove, remove);
    });
}

fn value_editor(ui: &mut egui::Ui, row: &mut ParamEdit) {
    let enabled = row.op == OpChoice::Set && row.kind != ParamKind::Flag;
    ui.add_enabled_ui(enabled, |ui| match row.kind {
        ParamKind::Flag => {
            ui.label("");
        }
        ParamKind::SurfaceProp => {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut row.value).desired_width(140.0));
                egui::ComboBox::from_id_salt(("hp_me_sp", row.key.as_str())).selected_text("").width(24.0).show_ui(
                    ui,
                    |ui| {
                        for sp in crate::util::SURFACE_PROPS {
                            ui.selectable_value(&mut row.value, (*sp).to_owned(), *sp);
                        }
                    },
                );
            });
        }
        ParamKind::Color => {
            ui.add(egui::TextEdit::singleline(&mut row.value).hint_text("[1 1 1]").desired_width(180.0));
        }
        ParamKind::Texture | ParamKind::Text => {
            ui.add(egui::TextEdit::singleline(&mut row.value).desired_width(260.0));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const VMT: &str = "// my brick\n\"LightmappedGeneric\"\n{\n\t\"$basetexture\" \"custom/brick\"\n\t\"$surfaceprop\" \"concrete\" // was stone\n\t\"$translucent\" 1\n}\n";

    fn scratch(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("hp-ui-textures")
            .join("scratch")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ops_keep_formatting() {
        let ops = vec![
            ("$surfaceprop".to_owned(), ParamOp::Set("brick".into())),
            ("$translucent".to_owned(), ParamOp::Remove),
            ("$nocull".to_owned(), ParamOp::Set("1".into())),
            ("$envmap".to_owned(), ParamOp::Keep),
        ];
        let out = apply_ops(VMT, &ops).unwrap();
        assert!(out.starts_with("// my brick"), "{out}");
        let m = hp_vmt::parse(&out).unwrap();
        assert_eq!(m.get("$surfaceprop"), Some("brick"));
        assert_eq!(m.get("$translucent"), None);
        assert_eq!(m.get("$nocull"), Some("1"));
        assert_eq!(m.get("$basetexture"), Some("custom/brick"));
        assert!(apply_ops("not a material", &ops).is_err());
    }

    #[test]
    fn rows_to_ops() {
        let flag = ParamEdit { key: "$nocull".into(), kind: ParamKind::Flag, op: OpChoice::Set, value: String::new() };
        assert_eq!(flag.to_op(), Some(("$nocull".to_owned(), ParamOp::Set("1".into()))));
        let off = ParamEdit { op: OpChoice::Remove, ..flag.clone() };
        assert_eq!(off.to_op(), Some(("$nocull".to_owned(), ParamOp::Remove)));
        let keep = ParamEdit { op: OpChoice::Keep, ..flag };
        assert_eq!(keep.to_op(), None);
        let text = ParamEdit {
            key: " $envmap ".into(),
            kind: ParamKind::Text,
            op: OpChoice::Set,
            value: " env_cubemap ".into(),
        };
        assert_eq!(text.to_op(), Some(("$envmap".to_owned(), ParamOp::Set("env_cubemap".into()))));
        let bad = ParamEdit { key: "".into(), ..text.clone() };
        assert_eq!(bad.to_op(), None);
        let quote = ParamEdit { value: "a\"b".into(), ..text };
        assert_eq!(quote.to_op(), None);
    }

    #[test]
    fn edit_files_with_backup() {
        let dir = scratch("matedit");
        let path = dir.join("brick.vmt");
        std::fs::write(&path, VMT).unwrap();
        let item = ApplyItem {
            name: "custom/brick".into(),
            new_name: "custom/brick".into(),
            path: path.clone(),
            base_text: None,
        };
        let ops = vec![("$surfaceprop".to_owned(), ParamOp::Set("brick".into()))];
        let w = apply_item(&item, &ops).unwrap().unwrap();
        let bak = backup_path(&path);
        assert_eq!(w.backup.as_deref(), Some(bak.as_path()));
        assert!(!w.created);
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), VMT);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"$surfaceprop\" \"brick\""), "{text}");
        assert!(text.starts_with("// my brick"));

        let ops2 = vec![("$nocull".to_owned(), ParamOp::Set("1".into()))];
        let w2 = apply_item(&item, &ops2).unwrap().unwrap();
        assert!(w2.backup.is_none());
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), VMT);

        assert!(apply_item(&item, &ops2).unwrap().is_none());

        let new_path = dir.join("sub").join("new.vmt");
        let item = ApplyItem {
            name: "custom/new".into(),
            new_name: "custom/new".into(),
            path: new_path.clone(),
            base_text: Some(VMT.to_owned()),
        };
        let w3 = apply_item(&item, &ops).unwrap().unwrap();
        assert!(w3.created);
        assert!(w3.backup.is_none());
        assert!(std::fs::read_to_string(&new_path).unwrap().contains("brick"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plans() {
        let file = LoadedVmt {
            name: "a".into(),
            text: Some(VMT.into()),
            material: None,
            location: VmtLocation::File(PathBuf::from("C:/x/a.vmt")),
        };
        let arch = LoadedVmt {
            name: "concrete/floor".into(),
            text: Some(VMT.into()),
            material: None,
            location: VmtLocation::Archive("hl2.vpk".into()),
        };
        let missing = LoadedVmt { name: "m".into(), text: None, material: None, location: VmtLocation::Missing };
        let p = plan(&[&file, &arch, &missing], None);
        assert_eq!(p.len(), 1);
        let layout = hp_fs::CustomLayout { mode: hp_fs::CustomMode::Prefixed, prefix: "custom/x".into() };
        let p = plan(&[&file, &arch, &missing], Some((Path::new("C:/X"), &layout)));
        assert_eq!(p.len(), 2);
        assert_eq!(p[1].new_name, "custom/x/concrete/floor");
        assert!(p[1].base_text.is_some());
    }

    #[test]
    fn selection_materials_of_test_map() {
        let mut state = hp_app::testing::test_state();
        state.doc.select(&[hp_editor::ObjRef::Solid(2), hp_editor::ObjRef::Solid(3)], hp_editor::SelectOp::Replace);
        let m = selection_materials(&state.doc);
        assert_eq!(m, ["dev/dev_measuregeneric01b", "concrete/concretefloor001a"]);
    }
}
