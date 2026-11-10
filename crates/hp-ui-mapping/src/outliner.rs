use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use egui::{Align2, FontId, Key, Modifiers, RichText, Sense, Stroke, Ui, vec2};
use hp_app::{AppState, Severity, t, tf, theme};
use hp_editor::{Document, Id, ObjRef, Owner, Scene, SelectOp};

use crate::util::{DocKey, contains_ci};

const ROW_HEIGHT: f32 = 20.0;
const INDENT: f32 = 14.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum NodeKey {
    World,
    Group(Id),
    Entities,
    Class(u32),
    Entity(Id),
    Solid(Id),
}

impl NodeKey {
    fn is_object(self) -> bool {
        matches!(self, NodeKey::Entity(_) | NodeKey::Solid(_))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Flags {
    pub count: u32,
    pub visible: u32,
    pub hidden_flag: u32,
    pub selected: u32,
}

impl Flags {
    fn add(&mut self, o: Flags) {
        self.count += o.count;
        self.visible += o.visible;
        self.hidden_flag += o.hidden_flag;
        self.selected += o.selected;
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub key: NodeKey,
    pub children: Vec<Node>,
    pub flags: Flags,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Row {
    pub key: NodeKey,
    pub depth: u16,
    pub expandable: bool,
    pub expanded: bool,
    pub flags: Flags,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ClassNames {
    ids: HashMap<String, u32>,
    names: Vec<String>,
}

impl ClassNames {
    pub(crate) fn intern(&mut self, class: &str) -> u32 {
        let key = class.trim().to_ascii_lowercase();
        if let Some(id) = self.ids.get(&key) {
            return *id;
        }
        let id = self.names.len() as u32;
        self.names.push(class.trim().to_owned());
        self.ids.insert(key, id);
        id
    }

    pub(crate) fn name(&self, id: u32) -> &str {
        self.names.get(id as usize).map_or("", String::as_str)
    }

    fn get(&self, class: &str) -> Option<u32> {
        self.ids.get(&class.trim().to_ascii_lowercase()).copied()
    }
}

fn leaf_flags(doc: &Document, obj: ObjRef) -> Flags {
    let scene = doc.scene();
    let hidden = match obj {
        ObjRef::Solid(id) => scene.solid(id).is_some_and(|n| n.solid.hidden),
        ObjRef::Entity(id) => scene.entity(id).is_some_and(|n| n.entity.hidden),
    };
    Flags {
        count: 1,
        visible: u32::from(doc.is_visible(obj)),
        hidden_flag: u32::from(hidden),
        selected: u32::from(doc.is_selected(obj)),
    }
}

fn solid_node(doc: &Document, id: Id) -> Node {
    Node { key: NodeKey::Solid(id), children: Vec::new(), flags: leaf_flags(doc, ObjRef::Solid(id)) }
}

fn entity_node(doc: &Document, id: Id) -> Node {
    let scene = doc.scene();
    let mut flags = leaf_flags(doc, ObjRef::Entity(id));
    let children: Vec<Node> = scene.entity_solid_ids(id).into_iter().map(|s| solid_node(doc, s)).collect();
    if !children.is_empty() {
        let mut f = Flags::default();
        for c in &children {
            f.add(c.flags);
        }
        flags.count = f.count.max(1);
        flags.visible = if flags.visible > 0 { f.visible } else { 0 };
        flags.hidden_flag = if flags.hidden_flag > 0 { flags.count } else { f.hidden_flag };
        flags.selected = if flags.selected > 0 { flags.count } else { f.selected };
    }
    Node { key: NodeKey::Entity(id), children, flags }
}

pub(crate) fn build_tree(doc: &Document, classes: &mut ClassNames) -> Vec<Node> {
    let scene = doc.scene();
    let groups = scene.groups();

    let mut solids_by_group: HashMap<Id, Vec<Id>> = HashMap::new();
    let mut ungrouped: Vec<Id> = Vec::new();
    for n in scene.solids().values() {
        if n.owner != Owner::World {
            continue;
        }
        match n.solid.editor.group_id.filter(|g| groups.contains_key(g)) {
            Some(g) => solids_by_group.entry(g).or_default().push(n.solid.id),
            None => ungrouped.push(n.solid.id),
        }
    }
    let mut entities_by_group: HashMap<Id, Vec<Id>> = HashMap::new();
    for n in scene.entities().values() {
        if let Some(g) = n.entity.editor.group_id.filter(|g| groups.contains_key(g)) {
            entities_by_group.entry(g).or_default().push(n.entity.id);
        }
    }
    let mut child_groups: HashMap<Id, Vec<Id>> = HashMap::new();
    let mut root_groups: Vec<Id> = Vec::new();
    for gid in groups.keys() {
        match scene.group_parent(*gid) {
            Some(p) => child_groups.entry(p).or_default().push(*gid),
            None => root_groups.push(*gid),
        }
    }

    struct GroupIndex {
        child_groups: HashMap<Id, Vec<Id>>,
        solids: HashMap<Id, Vec<Id>>,
        entities: HashMap<Id, Vec<Id>>,
    }

    fn group_node(doc: &Document, gid: Id, idx: &GroupIndex, visited: &mut HashSet<Id>) -> Option<Node> {
        if !visited.insert(gid) {
            return None;
        }
        let mut children = Vec::new();
        for c in idx.child_groups.get(&gid).into_iter().flatten() {
            if let Some(n) = group_node(doc, *c, idx, visited) {
                children.push(n);
            }
        }
        for s in idx.solids.get(&gid).into_iter().flatten() {
            children.push(solid_node(doc, *s));
        }
        for e in idx.entities.get(&gid).into_iter().flatten() {
            children.push(entity_node(doc, *e));
        }
        let mut flags = Flags::default();
        for c in &children {
            flags.add(c.flags);
        }
        Some(Node { key: NodeKey::Group(gid), children, flags })
    }

    let idx = GroupIndex { child_groups, solids: solids_by_group, entities: entities_by_group };
    let mut visited = HashSet::new();
    let mut world_children: Vec<Node> = Vec::new();
    for g in &root_groups {
        if let Some(n) = group_node(doc, *g, &idx, &mut visited) {
            world_children.push(n);
        }
    }
    for gid in groups.keys() {
        if !visited.contains(gid)
            && let Some(n) = group_node(doc, *gid, &idx, &mut visited)
        {
            world_children.push(n);
        }
    }
    world_children.extend(ungrouped.iter().map(|s| solid_node(doc, *s)));
    let mut world_flags = Flags::default();
    for c in &world_children {
        world_flags.add(c.flags);
    }

    let mut by_class: HashMap<u32, Vec<Id>> = HashMap::new();
    for n in scene.entities().values() {
        let cid = classes.intern(&n.entity.classname);
        by_class.entry(cid).or_default().push(n.entity.id);
    }
    let mut class_ids: Vec<u32> = by_class.keys().copied().collect();
    class_ids.sort_by_cached_key(|c| classes.name(*c).to_ascii_lowercase());
    let mut class_nodes = Vec::with_capacity(class_ids.len());
    let mut entities_flags = Flags::default();
    for cid in class_ids {
        let children: Vec<Node> = by_class.get(&cid).into_iter().flatten().map(|e| entity_node(doc, *e)).collect();
        let mut flags = Flags::default();
        for c in &children {
            flags.add(c.flags);
        }
        entities_flags.add(flags);
        class_nodes.push(Node { key: NodeKey::Class(cid), children, flags });
    }

    vec![
        Node { key: NodeKey::World, children: world_children, flags: world_flags },
        Node { key: NodeKey::Entities, children: class_nodes, flags: entities_flags },
    ]
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Filter {
    pub text: String,
}

impl Filter {
    pub(crate) fn new(text: &str) -> Self {
        Self { text: text.trim().to_ascii_lowercase() }
    }

    fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn matches(&self, key: NodeKey, scene: &Scene, classes: &ClassNames) -> bool {
        let f = self.text.strip_prefix('#').unwrap_or(&self.text);
        let numeric = !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit());
        let id_match = |id: Id| numeric && itoa_buf(id).contains(f);
        match key {
            NodeKey::World | NodeKey::Entities => false,
            NodeKey::Group(id) => id_match(id),
            NodeKey::Class(c) => contains_ci(classes.name(c), &self.text),
            NodeKey::Entity(id) => {
                id_match(id)
                    || scene.entity(id).is_some_and(|n| {
                        contains_ci(&n.entity.classname, &self.text)
                            || n.entity.get("targetname").is_some_and(|t| contains_ci(t, &self.text))
                    })
            }
            NodeKey::Solid(id) => {
                id_match(id)
                    || scene.solid(id).is_some_and(|n| {
                        let mut prev: Option<&str> = None;
                        n.solid.sides.iter().any(|s| {
                            let m = s.material.as_str();
                            if prev == Some(m) {
                                return false;
                            }
                            prev = Some(m);
                            contains_ci(m, &self.text)
                        })
                    })
            }
        }
    }
}

fn itoa_buf(mut v: Id) -> String {
    let mut digits = [0u8; 10];
    let mut i = digits.len();
    loop {
        i -= 1;
        digits[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    String::from_utf8_lossy(&digits[i..]).into_owned()
}

pub(crate) fn flatten(
    tree: &[Node],
    expanded: &HashSet<NodeKey>,
    filter: &Filter,
    scene: &Scene,
    classes: &ClassNames,
) -> Vec<Row> {
    fn row(n: &Node, depth: u16, expanded: bool) -> Row {
        Row { key: n.key, depth, expandable: !n.children.is_empty(), expanded, flags: n.flags }
    }

    fn normal(n: &Node, depth: u16, expanded: &HashSet<NodeKey>, out: &mut Vec<Row>) {
        let open = !n.children.is_empty() && expanded.contains(&n.key);
        out.push(row(n, depth, open));
        if open {
            for c in &n.children {
                normal(c, depth + 1, expanded, out);
            }
        }
    }

    fn filtered(
        n: &Node,
        depth: u16,
        expanded: &HashSet<NodeKey>,
        filter: &Filter,
        scene: &Scene,
        classes: &ClassNames,
        out: &mut Vec<Row>,
    ) -> bool {
        let self_match = filter.matches(n.key, scene, classes);
        if self_match && expanded.contains(&n.key) && !n.children.is_empty() {
            normal(n, depth, expanded, out);
            return true;
        }
        let idx = out.len();
        out.push(row(n, depth, false));
        let mut any = false;
        for c in &n.children {
            any |= filtered(c, depth + 1, expanded, filter, scene, classes, out);
        }
        if any {
            if let Some(r) = out.get_mut(idx) {
                r.expanded = true;
            }
            true
        } else if self_match {
            out.truncate(idx + 1);
            true
        } else {
            out.truncate(idx);
            false
        }
    }

    let mut out = Vec::new();
    for n in tree {
        if filter.is_empty() {
            normal(n, 0, expanded, &mut out);
        } else {
            filtered(n, 0, expanded, filter, scene, classes, &mut out);
        }
    }
    out
}

pub(crate) fn selectable(doc: &Document, objs: Vec<ObjRef>) -> Vec<ObjRef> {
    objs.into_iter().filter(|o| doc.is_visible(*o)).collect()
}

pub(crate) fn objects_of(key: NodeKey, doc: &Document, classes: &ClassNames) -> Vec<ObjRef> {
    let scene = doc.scene();
    match key {
        NodeKey::World => {
            scene.solids().values().filter(|n| n.owner == Owner::World).map(|n| ObjRef::Solid(n.solid.id)).collect()
        }
        NodeKey::Group(g) => scene.group_members(g),
        NodeKey::Entities => scene.entities().keys().map(|id| ObjRef::Entity(*id)).collect(),
        NodeKey::Class(c) => {
            let name = classes.name(c);
            scene
                .entities()
                .values()
                .filter(|n| n.entity.classname.trim().eq_ignore_ascii_case(name))
                .map(|n| ObjRef::Entity(n.entity.id))
                .collect()
        }
        NodeKey::Entity(id) => vec![ObjRef::Entity(id)],
        NodeKey::Solid(id) => vec![ObjRef::Solid(id)],
    }
}

pub(crate) fn ancestors_of(obj: ObjRef, scene: &Scene, classes: &ClassNames) -> Vec<NodeKey> {
    let mut out = Vec::new();
    match obj {
        ObjRef::Entity(id) => {
            out.push(NodeKey::Entities);
            if let Some(c) = scene.entity(id).and_then(|n| classes.get(&n.entity.classname)) {
                out.push(NodeKey::Class(c));
            }
        }
        ObjRef::Solid(id) => match scene.solid(id).map(|n| n.owner) {
            Some(Owner::Entity(e)) => {
                out.push(NodeKey::Entities);
                if let Some(c) = scene.entity(e).and_then(|n| classes.get(&n.entity.classname)) {
                    out.push(NodeKey::Class(c));
                }
                out.push(NodeKey::Entity(e));
            }
            Some(Owner::World) => {
                out.push(NodeKey::World);
                let mut chain = Vec::new();
                let mut g = scene.object_group(obj);
                let mut guard = 0;
                while let Some(gid) = g {
                    chain.push(NodeKey::Group(gid));
                    g = scene.group_parent(gid);
                    guard += 1;
                    if guard > scene.groups().len() {
                        break;
                    }
                }
                chain.reverse();
                out.extend(chain);
            }
            None => {}
        },
    }
    out
}

fn selection_signature(doc: &Document) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for o in &doc.selection().objects {
        o.hash(&mut h);
    }
    doc.selection().objects.len().hash(&mut h);
    h.finish()
}

pub struct Outliner {
    classes: ClassNames,
    tree: Vec<Node>,
    rows: Vec<Row>,
    tree_key: Option<DocKey>,
    rows_dirty: bool,
    expanded: HashSet<NodeKey>,
    search: String,
    filter: Filter,
    anchor: Option<NodeKey>,
    selection_sig: u64,
    own_change: bool,
    scroll_to: Option<usize>,
    counts: (usize, usize),
    list_rect: egui::Rect,
    pending: bool,
    build_ms: f64,
    built_at: f64,
}

const SLOW_BUILD_MS: f64 = 2.0;
const REBUILD_INTERVAL_S: f64 = 0.25;

impl Default for Outliner {
    fn default() -> Self {
        Self::new()
    }
}

impl Outliner {
    pub fn new() -> Self {
        let mut expanded = HashSet::new();
        expanded.insert(NodeKey::World);
        expanded.insert(NodeKey::Entities);
        Self {
            classes: ClassNames::default(),
            tree: Vec::new(),
            rows: Vec::new(),
            tree_key: None,
            rows_dirty: true,
            expanded,
            search: String::new(),
            filter: Filter::default(),
            anchor: None,
            selection_sig: 0,
            own_change: false,
            scroll_to: None,
            counts: (0, 0),
            list_rect: egui::Rect::NOTHING,
            pending: false,
            build_ms: 0.0,
            built_at: 0.0,
        }
    }

    fn refresh(&mut self, state: &AppState) {
        let key = DocKey::of(&state.doc);
        self.pending = false;
        if self.tree_key.is_some()
            && self.tree_key != Some(key)
            && self.build_ms > SLOW_BUILD_MS
            && (0.0..REBUILD_INTERVAL_S).contains(&(state.frame.time - self.built_at))
        {
            self.pending = true;
        } else if self.tree_key != Some(key) {
            let start = std::time::Instant::now();
            self.tree = build_tree(&state.doc, &mut self.classes);
            self.build_ms = start.elapsed().as_secs_f64() * 1000.0;
            self.built_at = state.frame.time;
            self.tree_key = Some(key);
            let scene = state.doc.scene();
            self.counts = (scene.solids().values().filter(|n| n.owner == Owner::World).count(), scene.entities().len());
            self.rows_dirty = true;
            let sig = selection_signature(&state.doc);
            if sig != self.selection_sig {
                self.selection_sig = sig;
                if !self.own_change {
                    self.reveal_selection(state);
                }
            }
            self.own_change = false;
        }
        if self.rows_dirty {
            self.rows = flatten(&self.tree, &self.expanded, &self.filter, state.doc.scene(), &self.classes);
            self.rows_dirty = false;
        }
    }

    fn reveal_selection(&mut self, state: &AppState) {
        let Some(first) = state.doc.selection().objects.first().copied() else { return };
        for k in ancestors_of(first, state.doc.scene(), &self.classes) {
            self.expanded.insert(k);
        }
        self.rows = flatten(&self.tree, &self.expanded, &self.filter, state.doc.scene(), &self.classes);
        self.rows_dirty = false;
        let target = match first {
            ObjRef::Solid(id) => NodeKey::Solid(id),
            ObjRef::Entity(id) => NodeKey::Entity(id),
        };
        self.scroll_to = self.rows.iter().position(|r| r.key == target);
        self.anchor = Some(target);
    }

    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.refresh(state);
        if self.pending {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(REBUILD_INTERVAL_S));
        }

        ui.horizontal(|ui| {
            let width = (ui.available_width() - 64.0).max(80.0);
            ui.allocate_ui(vec2(width, ui.spacing().interact_size.y), |ui| {
                if hp_app::widgets::search_box(ui, &mut self.search, t!("outliner.search_hint")) {
                    self.filter = Filter::new(&self.search);
                    self.rows_dirty = true;
                }
            });
            if ui.small_button("⏷").on_hover_text(t!("outliner.expand_all")).clicked() {
                self.expand_all();
            }
            if ui.small_button("⏶").on_hover_text(t!("outliner.collapse_all")).clicked() {
                self.expanded.clear();
                self.expanded.insert(NodeKey::World);
                self.expanded.insert(NodeKey::Entities);
                self.rows_dirty = true;
            }
        });
        if self.rows_dirty {
            self.rows = flatten(&self.tree, &self.expanded, &self.filter, state.doc.scene(), &self.classes);
            self.rows_dirty = false;
        }
        let (solids, entities) = self.counts;
        ui.label(
            RichText::new(tf!("outliner.summary", solids, entities, state.doc.selection().objects.len()))
                .weak()
                .small(),
        )
        .on_hover_text(t!("outliner.keys_tip"));
        ui.separator();

        if !self.filter.is_empty() && self.rows.is_empty() {
            ui.label(RichText::new(t!("outliner.no_match")).weak().italics());
            return;
        }

        let list_id = ui.make_persistent_id("hp_outliner_list");
        let mut scroll = egui::ScrollArea::vertical().id_salt("hp_outliner_scroll").auto_shrink([false, false]);
        if let Some(index) = self.scroll_to.take() {
            let row_full = ROW_HEIGHT + ui.spacing().item_spacing.y;
            let view_h = ui.available_height();
            let offset = (index as f32 * row_full - view_h * 0.5 + row_full).max(0.0);
            scroll = scroll.vertical_scroll_offset(offset);
        }

        let mut clicked: Option<(usize, Modifiers, bool)> = None;
        let mut toggle_expand: Option<NodeKey> = None;
        let mut toggle_eye: Option<usize> = None;
        let rows = &self.rows;
        let classes = &self.classes;
        let anchor = self.anchor;
        let scene = state.doc.scene();
        let out = scroll.show_rows(ui, ROW_HEIGHT, rows.len(), |ui, range| {
            for i in range {
                let Some(row) = rows.get(i) else { continue };
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
                let painter = ui.painter_at(rect);
                let visuals = ui.visuals();
                let all_selected = row.flags.count > 0 && row.flags.selected == row.flags.count;
                if all_selected {
                    painter.rect_filled(rect, 2.0, visuals.selection.bg_fill);
                } else if row.flags.selected > 0 {
                    painter.rect_filled(rect, 2.0, visuals.selection.bg_fill.gamma_multiply(0.4));
                } else if resp.hovered() {
                    painter.rect_filled(rect, 2.0, visuals.widgets.hovered.weak_bg_fill.gamma_multiply(0.5));
                }
                if anchor == Some(row.key) && all_selected {
                    painter.rect_stroke(
                        rect.shrink(0.5),
                        2.0,
                        Stroke::new(1.0, theme::accent()),
                        egui::StrokeKind::Inside,
                    );
                }
                let cy = rect.center().y;

                let eye_rect =
                    egui::Rect::from_center_size(egui::pos2(rect.right() - 12.0, cy), vec2(20.0, ROW_HEIGHT));
                let eye = ui.interact(eye_rect, ui.scope_id().with(("eye", i)), Sense::click());
                let visible = row.flags.visible > 0;
                let eye_color = if eye.hovered() {
                    visuals.strong_text_color()
                } else if visible {
                    visuals.text_color()
                } else {
                    visuals.weak_text_color().gamma_multiply(0.6)
                };
                painter.text(eye_rect.center(), Align2::CENTER_CENTER, "👁", FontId::proportional(13.0), eye_color);
                if row.flags.hidden_flag > 0 {
                    let r = eye_rect.shrink2(vec2(4.0, 5.0));
                    painter.line_segment([r.left_bottom(), r.right_top()], Stroke::new(1.5, eye_color));
                }
                let eye = eye.on_hover_text(if row.flags.hidden_flag == row.flags.count && row.flags.count > 0 {
                    t!("outliner.unhide")
                } else {
                    t!("outliner.hide")
                });
                if eye.clicked() {
                    toggle_eye = Some(i);
                }

                let mut x = rect.left() + 2.0 + f32::from(row.depth) * INDENT;
                if row.expandable {
                    let arrow_rect = egui::Rect::from_center_size(egui::pos2(x + 7.0, cy), vec2(14.0, ROW_HEIGHT));
                    let arrow = ui.interact(arrow_rect, ui.scope_id().with(("arrow", i)), Sense::click());
                    let color = if arrow.hovered() { visuals.strong_text_color() } else { visuals.text_color() };
                    painter.text(
                        arrow_rect.center(),
                        Align2::CENTER_CENTER,
                        if row.expanded { "⏷" } else { "⏵" },
                        FontId::proportional(11.0),
                        color,
                    );
                    if arrow.clicked() {
                        toggle_expand = Some(row.key);
                    }
                }
                x += 15.0;

                let text_color = if visible { visuals.text_color() } else { visuals.weak_text_color() };
                let (icon, icon_color) = row_icon(row.key, scene);
                painter.text(
                    egui::pos2(x + 7.0, cy),
                    Align2::CENTER_CENTER,
                    icon,
                    FontId::proportional(12.0),
                    icon_color,
                );
                x += 18.0;

                let (label, detail) = row_label(row.key, scene, classes);
                let count_text = (!row.key.is_object() || row.expandable).then(|| row.flags.count.to_string());
                let right_limit = eye_rect.left() - 4.0;
                let mut text_right = right_limit;
                if let Some(ct) = count_text {
                    let g = painter.layout_no_wrap(ct, FontId::proportional(11.0), visuals.weak_text_color());
                    let pos = egui::pos2(right_limit - g.size().x, cy - g.size().y * 0.5);
                    text_right = pos.x - 6.0;
                    painter.galley(pos, g, visuals.weak_text_color());
                }
                let clip = egui::Rect::from_min_max(egui::pos2(x, rect.top()), egui::pos2(text_right, rect.bottom()));
                let clipped = painter.with_clip_rect(clip.intersect(rect));
                let g = clipped.layout_no_wrap(label, FontId::proportional(13.0), text_color);
                let label_w = g.size().x;
                clipped.galley(egui::pos2(x, cy - g.size().y * 0.5), g, text_color);
                if let Some(d) = detail {
                    let g = clipped.layout_no_wrap(d, FontId::proportional(11.5), visuals.weak_text_color());
                    clipped.galley(egui::pos2(x + label_w + 6.0, cy - g.size().y * 0.5), g, visuals.weak_text_color());
                }

                if resp.clicked() {
                    let mods = ui.input(|inp| inp.modifiers);
                    clicked = Some((i, mods, false));
                }
                if resp.double_clicked() {
                    clicked = Some((i, Modifiers::NONE, true));
                }
            }
        });

        let list_rect = out.inner_rect;
        self.list_rect = list_rect;
        let list_resp = ui.interact(list_rect, list_id, Sense::focusable_noninteractive());
        if list_resp.has_focus() {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    list_id,
                    egui::EventFilter { vertical_arrows: true, horizontal_arrows: true, ..Default::default() },
                );
            });
        }
        if list_resp.has_focus() || ui.rect_contains_pointer(list_rect) {
            self.keyboard(ui, state);
        }

        if let Some(key) = toggle_expand {
            self.toggle_expanded(key);
        }
        if let Some(i) = toggle_eye
            && let Some(row) = self.rows.get(i).copied()
        {
            let objs = objects_of(row.key, &state.doc, &self.classes);
            let unhide = row.flags.count > 0 && row.flags.hidden_flag == row.flags.count;
            state.doc.set_hidden(&objs, !unhide);
        }
        if let Some((i, mods, double)) = clicked {
            self.click_row(state, i, mods);
            if double {
                crate::util::request_center(state);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn set_search(&mut self, text: &str) {
        self.search = text.to_owned();
        self.filter = Filter::new(text);
        self.rows_dirty = true;
    }

    #[cfg(test)]
    pub(crate) fn visible_keys(&self) -> Vec<NodeKey> {
        self.rows.iter().map(|r| r.key).collect()
    }

    fn expand_all(&mut self) {
        fn walk(n: &Node, out: &mut HashSet<NodeKey>) {
            if !n.children.is_empty() && !matches!(n.key, NodeKey::Entity(_)) {
                out.insert(n.key);
                for c in &n.children {
                    walk(c, out);
                }
            }
        }
        for n in &self.tree {
            walk(n, &mut self.expanded);
        }
        self.rows_dirty = true;
    }

    fn toggle_expanded(&mut self, key: NodeKey) {
        if !self.expanded.remove(&key) {
            self.expanded.insert(key);
        }
        self.rows_dirty = true;
    }

    pub(crate) fn click_row(&mut self, state: &mut AppState, i: usize, mods: Modifiers) {
        let Some(row) = self.rows.get(i).copied() else { return };
        let before = state.doc.revision();
        let all_hidden = row.flags.count > 0 && row.flags.visible == 0;
        if mods.shift
            && let Some(a) = self.anchor.and_then(|k| self.rows.iter().position(|r| r.key == k))
        {
            let (lo, hi) = if a <= i { (a, i) } else { (i, a) };
            let mut range: Vec<ObjRef> = Vec::new();
            for r in self.rows.get(lo..=hi).unwrap_or(&[]) {
                range.extend(objects_of(r.key, &state.doc, &self.classes));
            }
            let range = selectable(&state.doc, range);
            let op = if mods.command || mods.ctrl { SelectOp::Add } else { SelectOp::Replace };
            state.doc.select(&range, op);
        } else if mods.command || mods.ctrl {
            let objs = objects_of(row.key, &state.doc, &self.classes);
            let all_selected = row.flags.visible > 0 && row.flags.selected >= row.flags.visible;
            if all_selected {
                state.doc.select(&objs, SelectOp::Remove);
            } else {
                state.doc.select(&selectable(&state.doc, objs), SelectOp::Add);
            }
            self.anchor = Some(row.key);
        } else {
            let objs = selectable(&state.doc, objects_of(row.key, &state.doc, &self.classes));
            if !all_hidden {
                state.doc.set_selection(&objs);
            }
            self.anchor = Some(row.key);
        }
        if all_hidden && !mods.shift {
            state.notify(Severity::Info, t!("outliner.hidden_not_selectable"));
        }
        self.own_change |= state.doc.revision() != before;
    }

    fn keyboard(&mut self, ui: &Ui, state: &mut AppState) {
        let (up, down, left, right, enter) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowLeft),
                i.consume_key(Modifiers::NONE, Key::ArrowRight),
                i.consume_key(Modifiers::NONE, Key::Enter),
            )
        });
        if self.rows.is_empty() {
            return;
        }
        let current = self.anchor.and_then(|k| self.rows.iter().position(|r| r.key == k));
        if up || down {
            let next = match current {
                Some(c) if up => c.saturating_sub(1),
                Some(c) => (c + 1).min(self.rows.len() - 1),
                None => 0,
            };
            self.click_row(state, next, Modifiers::NONE);
            self.scroll_to = Some(next);
        }
        if let Some(c) = current
            && let Some(row) = self.rows.get(c).copied()
        {
            if right && row.expandable && !row.expanded {
                self.toggle_expanded(row.key);
            }
            if left {
                if row.expandable && row.expanded {
                    self.toggle_expanded(row.key);
                } else if let Some(parent) = self.rows[..c].iter().rposition(|r| r.depth < row.depth) {
                    self.click_row(state, parent, Modifiers::NONE);
                    self.scroll_to = Some(parent);
                }
            }
            if enter {
                crate::util::request_center(state);
            }
        }
    }
}

fn row_icon(key: NodeKey, scene: &Scene) -> (&'static str, egui::Color32) {
    match key {
        NodeKey::World => ("🌐", theme::text_weak()),
        NodeKey::Group(_) => ("🔗", theme::success()),
        NodeKey::Entities => ("⚙", theme::accent()),
        NodeKey::Class(_) => ("📂", theme::warning()),
        NodeKey::Entity(id) => {
            if scene.is_brush_entity(id) {
                ("▣", theme::accent())
            } else {
                ("⚫", theme::success())
            }
        }
        NodeKey::Solid(_) => ("▪", theme::info()),
    }
}

fn row_label(key: NodeKey, scene: &Scene, classes: &ClassNames) -> (String, Option<String>) {
    match key {
        NodeKey::World => (t!("outliner.world").to_owned(), None),
        NodeKey::Entities => (t!("outliner.entities").to_owned(), None),
        NodeKey::Group(id) => (tf!("outliner.group", id), None),
        NodeKey::Class(c) => (classes.name(c).to_owned(), None),
        NodeKey::Entity(id) => match scene.entity(id) {
            Some(n) => match n.entity.get("targetname").filter(|t| !t.is_empty()) {
                Some(name) => (name.to_owned(), Some(format!("{} #{id}", n.entity.classname))),
                None => (n.entity.classname.clone(), Some(format!("#{id}"))),
            },
            None => (format!("#{id}"), None),
        },
        NodeKey::Solid(id) => {
            let material = scene.solid(id).and_then(|n| n.solid.sides.first()).map(|s| s.material.to_ascii_lowercase());
            (tf!("outliner.solid", id), material)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree_of(state: &AppState) -> (Vec<Node>, ClassNames) {
        let mut classes = ClassNames::default();
        let tree = build_tree(&state.doc, &mut classes);
        (tree, classes)
    }

    #[test]
    fn tree_structure() {
        let state = hp_app::testing::test_state();
        let (tree, classes) = tree_of(&state);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].key, NodeKey::World);
        assert_eq!(tree[0].children.len(), 2, "two ungrouped world solids");
        assert_eq!(tree[0].flags.count, 2);
        let class_names: Vec<&str> = tree[1]
            .children
            .iter()
            .map(|n| match n.key {
                NodeKey::Class(c) => classes.name(c),
                _ => "",
            })
            .collect();
        assert_eq!(class_names, vec!["info_player_start", "light", "trigger_multiple"]);
        let trigger = &tree[1].children[2].children[0];
        assert_eq!(trigger.key, NodeKey::Entity(6));
        assert_eq!(trigger.children.len(), 1);
        assert_eq!(trigger.children[0].key, NodeKey::Solid(7));
        assert_eq!(tree[1].flags.count, 3, "two point entities and one brush solid");
    }

    #[test]
    fn groups_in_tree() {
        let mut state = hp_app::testing::test_state();
        let g = state.doc.group(&[ObjRef::Solid(2), ObjRef::Solid(3)]).unwrap_or(0);
        let (tree, _) = tree_of(&state);
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].key, NodeKey::Group(g));
        assert_eq!(tree[0].children[0].children.len(), 2);
        let ancestors = ancestors_of(ObjRef::Solid(2), state.doc.scene(), &ClassNames::default());
        assert_eq!(ancestors, vec![NodeKey::World, NodeKey::Group(g)]);
    }

    #[test]
    fn flatten_and_expand() {
        let state = hp_app::testing::test_state();
        let (tree, classes) = tree_of(&state);
        let mut expanded = HashSet::new();
        let rows = flatten(&tree, &expanded, &Filter::default(), state.doc.scene(), &classes);
        assert_eq!(rows.len(), 2);
        expanded.insert(NodeKey::World);
        expanded.insert(NodeKey::Entities);
        let rows = flatten(&tree, &expanded, &Filter::default(), state.doc.scene(), &classes);
        assert_eq!(rows.len(), 7);
        let trigger_class = rows[6].key;
        expanded.insert(trigger_class);
        expanded.insert(NodeKey::Entity(6));
        let rows = flatten(&tree, &expanded, &Filter::default(), state.doc.scene(), &classes);
        assert_eq!(rows.len(), 9);
        assert_eq!(rows[8].key, NodeKey::Solid(7));
        assert_eq!(rows[8].depth, 3);
    }

    #[test]
    fn filter_rows() {
        let state = hp_app::testing::test_state();
        let (tree, classes) = tree_of(&state);
        let expanded = HashSet::new();
        let scene = state.doc.scene();
        let rows = flatten(&tree, &expanded, &Filter::new("main_light"), scene, &classes);
        let keys: Vec<NodeKey> = rows.iter().map(|r| r.key).collect();
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0], NodeKey::Entities);
        assert_eq!(keys[2], NodeKey::Entity(5));
        assert!(rows[0].expanded && rows[1].expanded);

        let rows = flatten(&tree, &expanded, &Filter::new("#7"), scene, &classes);
        assert!(rows.iter().any(|r| r.key == NodeKey::Solid(7)));
        let rows = flatten(&tree, &expanded, &Filter::new("concretefloor"), scene, &classes);
        assert_eq!(rows.iter().filter(|r| r.key.is_object()).count(), 1);
        assert!(rows.iter().any(|r| r.key == NodeKey::Solid(3)));
        let rows = flatten(&tree, &expanded, &Filter::new("trigger_mul"), scene, &classes);
        assert!(rows.iter().any(|r| r.key == NodeKey::Entity(6)));
        assert!(flatten(&tree, &expanded, &Filter::new("zzzz"), scene, &classes).is_empty());
    }

    #[test]
    fn selection_flags_and_clicks() {
        let mut state = hp_app::testing::test_state();
        let mut o = Outliner::new();
        o.refresh(&state);
        let world_row = o.rows.iter().position(|r| r.key == NodeKey::Solid(2)).unwrap_or(0);
        o.click_row(&mut state, world_row, Modifiers::NONE);
        assert_eq!(state.doc.selected_objects(), vec![ObjRef::Solid(2)]);
        o.refresh(&state);
        assert_eq!(o.scroll_to, None, "own clicks do not scroll");
        let row = o.rows.iter().find(|r| r.key == NodeKey::Solid(2)).copied();
        assert_eq!(row.map(|r| r.flags.selected), Some(1));
        let s3 = o.rows.iter().position(|r| r.key == NodeKey::Solid(3)).unwrap_or(0);
        o.click_row(&mut state, s3, Modifiers::SHIFT);
        assert_eq!(state.doc.selection().objects.len(), 2);
        o.refresh(&state);
        let s3 = o.rows.iter().position(|r| r.key == NodeKey::Solid(3)).unwrap_or(0);
        o.click_row(&mut state, s3, Modifiers::CTRL);
        assert_eq!(state.doc.selected_objects(), vec![ObjRef::Solid(2)]);
        o.refresh(&state);
        state.doc.set_selection(&[ObjRef::Solid(7)]);
        o.refresh(&state);
        let idx = o.rows.iter().position(|r| r.key == NodeKey::Solid(7));
        assert!(idx.is_some());
        assert_eq!(o.scroll_to, idx);
    }

    #[test]
    fn hidden_objects_are_not_selected() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut o = Outliner::new();
        crate::autovis::set_builtin_visible(&mut state.doc, crate::autovis::AutoKind::Triggers, false);
        state.doc.set_hidden(&[ObjRef::Solid(2)], true);
        o.refresh(&state);
        let entities = o.rows.iter().position(|r| r.key == NodeKey::Entities).expect("row");
        o.click_row(&mut state, entities, Modifiers::NONE);
        let mut sel = state.doc.selected_objects();
        sel.sort_by_key(|o| o.id());
        assert_eq!(sel, vec![ObjRef::Entity(4), ObjRef::Entity(5)]);
        o.refresh(&state);
        let world = o.rows.iter().position(|r| r.key == NodeKey::World).expect("row");
        o.click_row(&mut state, world, Modifiers::NONE);
        assert_eq!(state.doc.selected_objects(), vec![ObjRef::Solid(3)]);
        o.refresh(&state);
        let floor = o.rows.iter().position(|r| r.key == NodeKey::Solid(2)).expect("row");
        let console = state.console.len();
        o.click_row(&mut state, floor, Modifiers::NONE);
        assert_eq!(state.doc.selected_objects(), vec![ObjRef::Solid(3)]);
        assert!(state.console.len() > console, "the user is told why");
        o.refresh(&state);
        let world = o.rows.iter().position(|r| r.key == NodeKey::World).expect("row");
        o.click_row(&mut state, world, Modifiers::CTRL);
        o.click_row(&mut state, floor, Modifiers::SHIFT);
        assert!(state.doc.selected_objects().iter().all(|obj| state.doc.is_visible(*obj)));
    }

    #[test]
    fn clicks_do_not_take_the_keyboard_focus() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut o = Outliner::new();
        let ctx = egui::Context::default();
        let frame = |events: Vec<egui::Event>, state: &mut AppState, o: &mut Outliner| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 600.0))),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| o.ui(ui, state));
            out.textures_delta.clear();
        };
        frame(Vec::new(), &mut state, &mut o);
        assert_eq!(o.rows.get(1).map(|r| r.key), Some(NodeKey::Solid(2)));
        let row_full = ROW_HEIGHT + ctx.global_style().spacing.item_spacing.y;
        let pos = o.list_rect.min + vec2(150.0, row_full * 1.5);
        let press = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(pos)], &mut state, &mut o);
        frame(vec![press(true)], &mut state, &mut o);
        frame(vec![press(false)], &mut state, &mut o);
        frame(Vec::new(), &mut state, &mut o);
        assert!(!state.doc.selected_objects().is_empty(), "the click selected a row");
        assert!(!ctx.egui_wants_keyboard_input(), "no widget took the keyboard focus");
        let before = state.doc.selected_objects();
        let down = egui::Event::Key {
            key: egui::Key::ArrowDown,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![down], &mut state, &mut o);
        assert_ne!(state.doc.selected_objects(), before);
    }

    #[test]
    fn class_objects_and_ids() {
        let state = hp_app::testing::test_state();
        let (_, classes) = tree_of(&state);
        let light = classes.get("LIGHT").map(NodeKey::Class);
        let objs = light.map(|k| objects_of(k, &state.doc, &classes)).unwrap_or_default();
        assert_eq!(objs, vec![ObjRef::Entity(5)]);
        assert_eq!(objects_of(NodeKey::World, &state.doc, &classes).len(), 2);
        assert_eq!(itoa_buf(0), "0");
        assert_eq!(itoa_buf(4_294_967_295), "4294967295");
    }
}
