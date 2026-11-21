use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use hp_fs::SourceOrigin;
use hp_fs::deps::{DepKind, DepNode, DependencyGraph, MissingAsset};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    Materials,
    Models,
    Sounds,
    Particles,
    Other,
}

impl Group {
    pub const ALL: [Group; 5] = [Group::Materials, Group::Models, Group::Sounds, Group::Particles, Group::Other];

    pub fn of(kind: DepKind) -> Group {
        match kind {
            DepKind::Material | DepKind::Texture => Group::Materials,
            DepKind::Model | DepKind::ModelPart => Group::Models,
            DepKind::Sound | DepKind::SoundScript | DepKind::Soundscape => Group::Sounds,
            DepKind::Particle | DepKind::ParticleManifest | DepKind::ParticleSystem => Group::Particles,
            DepKind::File => Group::Other,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Group::Materials => "materials",
            Group::Models => "models",
            Group::Sounds => "sounds",
            Group::Particles => "particles",
            Group::Other => "other",
        }
    }

    pub fn from_id(id: &str) -> Option<Group> {
        Group::ALL.into_iter().find(|g| g.id() == id)
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    Node(usize),
    Missing(usize),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Show {
    #[default]
    Custom,
    All,
    Missing,
}

impl Show {
    pub fn id(self) -> &'static str {
        match self {
            Show::Custom => "custom",
            Show::All => "all",
            Show::Missing => "missing",
        }
    }

    pub fn from_id(id: &str) -> Option<Show> {
        [Show::Custom, Show::All, Show::Missing].into_iter().find(|s| s.id() == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    pub show: Show,
    pub groups: [bool; 5],
    pub text: String,
}

impl Default for Filter {
    fn default() -> Self {
        Filter { show: Show::Custom, groups: [true; 5], text: String::new() }
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub item: Item,
    pub group: Group,
    pub closure: Vec<Item>,
    pub custom: bool,
    pub missing: bool,
    pub custom_bytes: u64,
    pub all_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub entries: [usize; 5],
    pub files: usize,
    pub textures: usize,
    pub model_files: usize,
    pub bytes: u64,
    pub missing: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LeftOut {
    pub nodes: HashSet<String>,
    pub missing: HashSet<String>,
}

pub fn is_base(node: &DepNode) -> bool {
    match node.origin {
        Some(SourceOrigin::BaseGame) => true,
        Some(SourceOrigin::GameLoose | SourceOrigin::Download) => node.in_base,
        _ => false,
    }
}

#[derive(Clone, Debug)]
pub struct AssetTree {
    pub graph: Arc<DependencyGraph>,
    pub entries: Vec<Entry>,
    children: HashMap<String, Vec<Item>>,
}

impl AssetTree {
    pub fn build(graph: Arc<DependencyGraph>) -> AssetTree {
        let mut index: HashMap<&str, Item> = HashMap::new();
        for (i, n) in graph.nodes.iter().enumerate() {
            index.insert(n.vpath.as_str(), Item::Node(i));
        }
        for (i, m) in graph.missing.iter().enumerate() {
            index.entry(m.name.as_str()).or_insert(Item::Missing(i));
        }
        let mut children: HashMap<String, Vec<Item>> = HashMap::new();
        let mut has_parent: HashSet<Item> = HashSet::new();
        let mut top: Vec<Item> = Vec::new();
        for (parent, child) in &graph.links {
            let Some(&item) = index.get(child.as_str()) else { continue };
            if parent.is_empty() {
                top.push(item);
            } else {
                has_parent.insert(item);
                children.entry(parent.clone()).or_default().push(item);
            }
        }
        let top_set: HashSet<Item> = top.iter().copied().collect();
        for (i, _) in graph.nodes.iter().enumerate() {
            let item = Item::Node(i);
            if !top_set.contains(&item) && !has_parent.contains(&item) {
                top.push(item);
            }
        }
        for (i, _) in graph.missing.iter().enumerate() {
            let item = Item::Missing(i);
            if !top_set.contains(&item) && !has_parent.contains(&item) {
                top.push(item);
            }
        }
        let mut tree = AssetTree { graph, entries: Vec::new(), children };
        let mut seen = HashSet::new();
        let mut entries: Vec<Entry> =
            top.into_iter().filter(|i| seen.insert(*i)).map(|item| tree.entry(item)).collect();
        entries.sort_by(|a, b| (a.group, tree.name(a.item)).cmp(&(b.group, tree.name(b.item))));
        tree.entries = entries;
        tree
    }

    fn entry(&self, item: Item) -> Entry {
        let mut closure = vec![item];
        let mut seen: HashSet<Item> = HashSet::from([item]);
        let mut i = 0;
        while i < closure.len() {
            for &c in self.children(closure[i]) {
                if seen.insert(c) {
                    closure.push(c);
                }
            }
            i += 1;
        }
        let (mut custom, mut missing, mut custom_bytes, mut all_bytes) = (false, false, 0, 0);
        for &c in &closure {
            match self.node(c) {
                Some(n) => {
                    all_bytes += n.size;
                    if !is_base(n) {
                        custom = true;
                        custom_bytes += n.size;
                    }
                }
                None => missing = true,
            }
        }
        Entry { item, group: Group::of(self.kind(item)), closure, custom, missing, custom_bytes, all_bytes }
    }

    pub fn name(&self, item: Item) -> &str {
        match item {
            Item::Node(i) => self.graph.nodes.get(i).map_or("", |n| n.vpath.as_str()),
            Item::Missing(i) => self.graph.missing.get(i).map_or("", |m| m.name.as_str()),
        }
    }

    pub fn kind(&self, item: Item) -> DepKind {
        match item {
            Item::Node(i) => self.graph.nodes.get(i).map_or(DepKind::File, |n| n.kind),
            Item::Missing(i) => self.graph.missing.get(i).map_or(DepKind::File, |m| m.kind),
        }
    }

    pub fn node(&self, item: Item) -> Option<&DepNode> {
        match item {
            Item::Node(i) => self.graph.nodes.get(i),
            Item::Missing(_) => None,
        }
    }

    pub fn missing(&self, item: Item) -> Option<&MissingAsset> {
        match item {
            Item::Missing(i) => self.graph.missing.get(i),
            Item::Node(_) => None,
        }
    }

    pub fn children(&self, item: Item) -> &[Item] {
        self.children.get(self.name(item)).map_or(&[], Vec::as_slice)
    }

    pub fn visible(&self, filter: &Filter) -> Vec<usize> {
        let needle = filter.text.trim().replace('\\', "/").to_ascii_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| filter.groups[e.group.index()])
            .filter(|(_, e)| match filter.show {
                Show::Custom => e.custom || e.missing,
                Show::All => true,
                Show::Missing => e.missing,
            })
            .filter(|(_, e)| {
                needle.is_empty() || e.closure.iter().any(|&i| self.name(i).to_ascii_lowercase().contains(&needle))
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn files_of(&self, entries: &[usize], base_game: bool) -> (Vec<usize>, Vec<usize>) {
        let mut nodes = HashSet::new();
        let mut missing = HashSet::new();
        for e in entries.iter().filter_map(|&i| self.entries.get(i)) {
            for &item in &e.closure {
                match item {
                    Item::Node(i) => {
                        if base_game || !is_base(&self.graph.nodes[i]) {
                            nodes.insert(i);
                        }
                    }
                    Item::Missing(i) => {
                        missing.insert(i);
                    }
                }
            }
        }
        let mut nodes: Vec<usize> = nodes.into_iter().collect();
        nodes.sort_unstable();
        let mut missing: Vec<usize> = missing.into_iter().collect();
        missing.sort_unstable();
        (nodes, missing)
    }

    pub fn left_out(&self, unchecked: &HashSet<String>) -> LeftOut {
        let mut out = LeftOut::default();
        if unchecked.is_empty() {
            return out;
        }
        let mut kept: HashSet<Item> = HashSet::new();
        for e in self.entries.iter().filter(|e| !unchecked.contains(self.name(e.item))) {
            kept.extend(e.closure.iter().copied().filter(|&i| !unchecked.contains(self.name(i))));
        }
        for (i, n) in self.graph.nodes.iter().enumerate() {
            if !kept.contains(&Item::Node(i)) {
                out.nodes.insert(n.vpath.clone());
            }
        }
        for (i, m) in self.graph.missing.iter().enumerate() {
            if !kept.contains(&Item::Missing(i)) {
                out.missing.insert(m.name.clone());
            }
        }
        out
    }

    pub fn totals(&self, entries: &[usize], base_game: bool) -> Totals {
        let mut t = Totals::default();
        for e in entries.iter().filter_map(|&i| self.entries.get(i)) {
            t.entries[e.group.index()] += 1;
        }
        let (nodes, missing) = self.files_of(entries, base_game);
        t.files = nodes.len();
        t.missing = missing.len();
        for n in nodes.iter().map(|&i| &self.graph.nodes[i]) {
            t.bytes += n.size;
            match n.kind {
                DepKind::Texture => t.textures += 1,
                DepKind::Model | DepKind::ModelPart => t.model_files += 1,
                _ => {}
            }
        }
        t
    }
}
