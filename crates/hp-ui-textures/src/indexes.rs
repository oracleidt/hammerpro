use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use hp_app::AppState;
use hp_fs::{AssetEntry, MaterialKeywords};

use crate::search::MaterialFlagsIndex;

pub(crate) type MountKey = (u64, u64);

pub(crate) type ListKey = (usize, usize, u64);

struct Published<K, T>(K, Arc<T>);

impl<K: Clone, T> Clone for Published<K, T> {
    fn clone(&self) -> Self {
        Published(self.0.clone(), Arc::clone(&self.1))
    }
}

fn slot(name: &str) -> egui::Id {
    egui::Id::unique(("hp_ui_textures_index", name))
}

const KEYWORDS: &str = "keywords";
const FLAGS: &str = "flags";

#[derive(Default)]
pub(crate) struct KeywordIndex {
    key: Option<MountKey>,
    index: Option<Arc<MaterialKeywords>>,
    pending: Option<(MountKey, Receiver<Arc<MaterialKeywords>>)>,
}

impl KeywordIndex {
    pub(crate) fn index(&self) -> Option<&Arc<MaterialKeywords>> {
        self.index.as_ref()
    }

    pub(crate) fn pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn update(&mut self, state: &AppState, ctx: &egui::Context, needed: bool) -> bool {
        let mut changed = false;
        if let Some((key, rx)) = &self.pending {
            let key = *key;
            loop {
                match rx.try_recv() {
                    Ok(index) => {
                        let complete = index.is_complete();
                        self.key = Some(key);
                        self.index = Some(Arc::clone(&index));
                        changed = true;
                        if complete {
                            ctx.data_mut(|d| d.insert_temp(slot(KEYWORDS), Published(key, index)));
                            self.pending = None;
                            break;
                        }
                    }
                    Err(TryRecvError::Empty) => {
                        ctx.request_repaint_after(Duration::from_millis(100));
                        break;
                    }
                    Err(TryRecvError::Disconnected) => {
                        self.pending = None;
                        break;
                    }
                }
            }
        }
        if !needed {
            return changed;
        }
        let Some(fs) = state.assets.fs() else { return changed };
        let key = (state.assets.generation(), fs.generation());
        let pending_for_key = self.pending.as_ref().is_some_and(|(k, _)| *k == key);
        let done_for_key = self.key == Some(key) && self.index.as_ref().is_some_and(|i| i.is_complete());
        if pending_for_key || done_for_key {
            return changed;
        }
        let published = ctx.data(|d| d.get_temp::<Published<MountKey, MaterialKeywords>>(slot(KEYWORDS)));
        if let Some(Published(k, index)) = published
            && k == key
            && index.is_complete()
        {
            self.key = Some(key);
            self.index = Some(index);
            self.pending = None;
            return true;
        }
        let fs = fs.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-keyword-index".into()).spawn(move || {
            let quick = hp_editor::background::run(|| fs.material_keywords_quick());
            let complete = quick.is_complete();
            if tx.send(quick).is_err() {
                return;
            }
            repaint.request_repaint();
            if !complete {
                let start = std::time::Instant::now();
                let full = hp_editor::background::run(|| fs.material_keywords());
                log::info!(
                    "material keywords: {} materials read in {:.1} s, {} with keywords",
                    full.scanned(),
                    start.elapsed().as_secs_f64(),
                    full.len()
                );
                let _ = tx.send(full);
                repaint.request_repaint();
            }
        });
        match spawned {
            Ok(_) => self.pending = Some((key, rx)),
            Err(e) => log::warn!("cannot start the keyword index: {e}"),
        }
        changed
    }
}

#[derive(Default)]
pub(crate) struct FlagsIndex {
    key: Option<ListKey>,
    index: Option<Arc<MaterialFlagsIndex>>,
    pending: Option<(ListKey, Receiver<MaterialFlagsIndex>)>,
}

impl FlagsIndex {
    pub(crate) fn index(&self) -> Option<&Arc<MaterialFlagsIndex>> {
        self.index.as_ref()
    }

    pub(crate) fn pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn update(
        &mut self,
        state: &AppState,
        ctx: &egui::Context,
        key: Option<ListKey>,
        list: &Arc<Vec<AssetEntry>>,
        needed: bool,
    ) -> bool {
        let mut changed = false;
        if let Some((k, rx)) = &self.pending {
            let k = *k;
            match rx.try_recv() {
                Ok(index) => {
                    let index = Arc::new(index);
                    ctx.data_mut(|d| d.insert_temp(slot(FLAGS), Published(k, Arc::clone(&index))));
                    self.key = Some(k);
                    self.index = Some(index);
                    self.pending = None;
                    changed = true;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
                Err(TryRecvError::Disconnected) => self.pending = None,
            }
        }
        if !needed {
            return changed;
        }
        let Some(fs) = state.assets.fs() else { return changed };
        let Some(key) = key else { return changed };
        if self.key == Some(key) || self.pending.as_ref().is_some_and(|(k, _)| *k == key) {
            return changed;
        }
        let published = ctx.data(|d| d.get_temp::<Published<ListKey, MaterialFlagsIndex>>(slot(FLAGS)));
        if let Some(Published(k, index)) = published
            && k == key
        {
            self.key = Some(key);
            self.index = Some(index);
            self.pending = None;
            return true;
        }
        let (fs, list) = (fs.clone(), Arc::clone(list));
        let (tx, rx) = std::sync::mpsc::channel();
        let repaint = ctx.clone();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
        let spawned = std::thread::Builder::new().name("hp-material-flags".into()).spawn(move || {
            let _ = tx.send(MaterialFlagsIndex::build(&fs, &list, threads));
            repaint.request_repaint();
        });
        match spawned {
            Ok(_) => self.pending = Some((key, rx)),
            Err(e) => log::warn!("cannot start the material flags index: {e}"),
        }
        changed
    }
}
