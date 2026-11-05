use std::cell::Cell;
use std::path::Path;
use std::sync::{Arc, Mutex};

use hp_pcf::editor::Document;

#[derive(Clone, Debug)]
pub struct PreviewOptions {
    pub paused: bool,
    pub speed: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub grid: bool,
    pub background: [f32; 3],
    pub points: Vec<[f32; 3]>,
    pub restart: u64,
    pub particles: usize,
    pub cpu_ms: f32,
}

impl Default for PreviewOptions {
    fn default() -> Self {
        Self {
            paused: false,
            speed: 1.0,
            yaw: 215.0,
            pitch: 20.0,
            zoom: 1.0,
            grid: true,
            background: [0.09; 3],
            points: vec![[0.0; 3]],
            restart: 0,
            particles: 0,
            cpu_ms: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EditorSession {
    pub open: bool,
    pub doc: Document,
    pub source: Option<String>,
    pub selected: Option<usize>,
    pub selected_element: Option<usize>,
    pub revision: u64,
    pub saved_revision: u64,
    pub preview: PreviewOptions,
    dirty_cache: Cell<Option<(u64, bool)>>,
}

impl Default for EditorSession {
    fn default() -> Self {
        Self {
            open: false,
            doc: Document::new(),
            source: None,
            selected: None,
            selected_element: None,
            revision: 0,
            saved_revision: 0,
            preview: PreviewOptions::default(),
            dirty_cache: Cell::new(None),
        }
    }
}

pub type EditorHandle = Arc<Mutex<EditorSession>>;

pub fn new_editor_handle() -> EditorHandle {
    Arc::new(Mutex::new(EditorSession::default()))
}

impl EditorSession {
    pub fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.dirty_cache.set(None);
        let systems = self.doc.systems();
        if self.selected.is_none_or(|id| !systems.contains(&id)) {
            self.selected = systems.first().copied();
            self.selected_element = self.selected;
        }
        if self.selected_element.is_some_and(|id| id >= self.doc.dmx().elements.len()) {
            self.selected_element = self.selected;
        }
    }

    pub fn dirty(&self) -> bool {
        let revision = self.doc.revision();
        if let Some((cached_revision, dirty)) = self.dirty_cache.get()
            && revision == cached_revision
        {
            return dirty;
        }
        let dirty = self.doc.is_modified();
        self.dirty_cache.set(Some((revision, dirty)));
        dirty
    }

    pub fn select(&mut self, id: usize) -> bool {
        if !self.doc.systems().contains(&id) {
            return false;
        }
        self.selected = Some(id);
        self.selected_element = Some(id);
        true
    }

    pub fn close_discarding_changes(&mut self) {
        if self.doc.discard_changes() {
            self.selected = self.doc.systems().first().copied();
            self.selected_element = self.selected;
            self.changed();
            self.saved_revision = self.revision;
        }
        self.open = false;
    }

    pub fn new_document(&mut self) {
        self.doc = Document::new();
        self.source = None;
        self.selected = None;
        self.selected_element = None;
        self.changed();
        self.saved_revision = self.revision;
    }

    pub fn load_bytes(&mut self, source: &str, bytes: &[u8]) -> Result<(), String> {
        let doc = Document::parse(bytes).map_err(|e| e.to_string())?;
        self.doc = doc;
        self.source = Some(source.to_owned());
        self.selected = self.doc.systems().first().copied();
        self.selected_element = self.selected;
        self.changed();
        self.saved_revision = self.revision;
        self.open = true;
        Ok(())
    }

    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        if !path.is_absolute() {
            return Err("the particle output path must be absolute".to_owned());
        }
        let bytes = self.doc.to_bytes().map_err(|e| e.to_string())?;
        let parent = path.parent().ok_or("the particle output path has no folder")?;
        if !parent.is_dir() {
            return Err(format!("{} is not a folder", parent.display()));
        }
        let temporary = path.with_extension(format!("pcf.{}.tmp", std::process::id()));
        let result = std::fs::write(&temporary, bytes)
            .and_then(|()| std::fs::rename(&temporary, path))
            .map_err(|e| format!("cannot save {}: {e}", path.display()));
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result?;
        self.source = Some(path.to_string_lossy().into_owned());
        self.doc.mark_saved();
        self.dirty_cache.set(None);
        self.saved_revision = self.revision;
        Ok(())
    }
}
