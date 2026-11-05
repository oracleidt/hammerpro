use glam::{DQuat, DVec3, EulerRot};
use hp_geom::Aabb;
use hp_kv::Block;
use hp_vmf::{EditorInfo, Group};

use crate::copy_names::{apply_renames, increment_names};
use crate::document::Document;
use crate::ids::{IdMap, id_map};
use crate::ops::Fragment;
use crate::problems::TargetKeyFn;
use crate::selection::{ObjRef, SelectOp};
use crate::{EditError, EditResult, Id};

const HEADER: &str = "hammerpro_clipboard";

#[derive(Clone, Debug, PartialEq)]
pub struct PasteSpecial {
    pub copies: u32,
    pub offset: DVec3,
    pub rotation: DVec3,
    pub pivot: Option<DVec3>,
    pub start_at_original: bool,
    pub group: bool,
    pub unique_names: bool,
    pub name_prefix: String,
}

impl Default for PasteSpecial {
    fn default() -> Self {
        Self {
            copies: 1,
            offset: DVec3::ZERO,
            rotation: DVec3::ZERO,
            pivot: None,
            start_at_original: false,
            group: false,
            unique_names: false,
            name_prefix: String::new(),
        }
    }
}

pub const PREVIEW_MAX_LINES: usize = 40_000;

const MARKER_HALF: f64 = 8.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PastePreview {
    pub lines: Vec<[DVec3; 2]>,
    pub copies: u32,
    pub shown_copies: u32,
    pub objects: usize,
}

fn marker_lines(c: DVec3, out: &mut Vec<[DVec3; 2]>) {
    let h = DVec3::splat(MARKER_HALF);
    let corner = |i: usize| {
        c + DVec3::new(
            if i & 1 == 0 { -h.x } else { h.x },
            if i & 2 == 0 { -h.y } else { h.y },
            if i & 4 == 0 { -h.z } else { h.z },
        )
    };
    for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
        out.push([corner(a), corner(b)]);
    }
}

fn editor_block(e: &EditorInfo) -> Block {
    let mut b = Block::new("editor");
    let [r, g, bl] = e.color;
    b.push_prop("color", format!("{r} {g} {bl}"));
    for v in &e.visgroup_ids {
        b.push_prop("visgroupid", v.to_string());
    }
    if let Some(g) = e.group_id {
        b.push_prop("groupid", g.to_string());
    }
    b.push_prop("visgroupshown", if e.visgroup_shown { "1" } else { "0" });
    b.push_prop("visgroupautoshown", if e.visgroup_auto_shown { "1" } else { "0" });
    b
}

fn group_from_block(b: &Block) -> Option<Group> {
    let id = b.get_parsed::<u32>("id")?;
    let mut g = Group { id, ..Group::default() };
    if let Some(e) = b.child("editor") {
        if let Some(c) = e.get("color") {
            let parts: Vec<u8> = c.split_ascii_whitespace().filter_map(|p| p.parse().ok()).collect();
            if let [r, gr, bl] = parts[..] {
                g.editor.color = [r, gr, bl];
            }
        }
        g.editor.visgroup_ids = e.get_all("visgroupid").filter_map(|v| v.trim().parse().ok()).collect();
        g.editor.group_id = e.get_parsed("groupid");
        g.editor.visgroup_shown = e.get("visgroupshown").is_none_or(|v| v.trim() != "0");
        g.editor.visgroup_auto_shown = e.get("visgroupautoshown").is_none_or(|v| v.trim() != "0");
    }
    Some(g)
}

fn header_text(text: &str) -> Option<&str> {
    let start = text.find(|c: char| !c.is_whitespace())?;
    let rest = text.get(start..)?;
    if !rest.get(..HEADER.len()).is_some_and(|h| h.eq_ignore_ascii_case(HEADER)) {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut depth = 0usize;
    let mut in_quote = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => in_quote = !in_quote,
            b'{' if !in_quote => depth += 1,
            b'}' if !in_quote => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return rest.get(..=i);
                }
            }
            _ => {}
        }
    }
    None
}

fn header_groups(text: &str) -> Vec<Group> {
    let Some(h) = header_text(text) else { return Vec::new() };
    let Ok(root) = hp_kv::parse(h) else { return Vec::new() };
    root.child(HEADER).map(|b| b.children_named("group").filter_map(group_from_block).collect()).unwrap_or_default()
}

fn fragment_bounds(frag: &Fragment) -> Option<Aabb> {
    let mut b = Aabb::EMPTY;
    let solids = frag.solids.iter().chain(frag.entities.iter().flat_map(|e| e.solids.iter()));
    for s in solids {
        let g = hp_geom::build_solid(&s.sides);
        if !g.aabb.is_empty() {
            b = b.union(&g.aabb);
        }
    }
    for e in &frag.entities {
        if e.solids.is_empty()
            && let Some(o) = e.origin()
        {
            b.extend(o);
        }
    }
    (!b.is_empty() && b.min.is_finite() && b.max.is_finite()).then_some(b)
}

impl Document {
    pub(crate) fn auto_rename_copy(&self, frag: &mut Fragment, is_target_key: Option<TargetKeyFn<'_>>) {
        let mut used: std::collections::HashSet<String> = self
            .scene
            .entities
            .values()
            .filter_map(|n| n.entity.get("targetname").map(|t| t.trim().to_ascii_lowercase()))
            .collect();
        increment_names(frag, &mut used, is_target_key);
    }

    pub fn copy(&self, objs: &[ObjRef]) -> String {
        let frag = self.extract_fragment(objs);
        let mut out = String::new();
        if !frag.groups.is_empty() {
            let mut header = Block::new(HEADER);
            header.push_prop("version", "1");
            for g in &frag.groups {
                let mut gb = Block::new("group");
                gb.push_prop("id", g.id.to_string());
                gb.push_child(editor_block(&g.editor));
                header.push_child(gb);
            }
            hp_kv::write_block(&header, 0, &mut out);
        }
        for s in &frag.solids {
            out.push_str(&hp_vmf::write_solid(s));
        }
        for e in &frag.entities {
            out.push_str(&hp_vmf::write_entity(e));
        }
        out
    }

    pub fn copy_selection(&self) -> String {
        let objs = self.selected_objects();
        self.copy(&objs)
    }

    pub fn cut_selection(&mut self) -> EditResult<String> {
        let objs = self.selected_objects();
        let text = self.copy(&objs);
        self.tx("Cut", |doc| doc.delete(&objs))?;
        Ok(text)
    }

    fn parse_clipboard(text: &str) -> EditResult<Fragment> {
        let parsed = hp_vmf::parse_fragment_full(text).map_err(|_| EditError::EmptyClipboard)?;
        let mut groups = header_groups(text);
        groups.extend(parsed.groups);
        let frag = Fragment { solids: parsed.solids, entities: parsed.entities, groups, visgroups: parsed.visgroups };
        if frag.is_empty() {
            return Err(EditError::EmptyClipboard);
        }
        Ok(frag)
    }

    pub fn paste(&mut self, text: &str, offset: DVec3) -> EditResult<Vec<ObjRef>> {
        self.paste_with(text, offset, None)
    }

    pub fn paste_with(
        &mut self,
        text: &str,
        offset: DVec3,
        is_target_key: Option<TargetKeyFn<'_>>,
    ) -> EditResult<Vec<ObjRef>> {
        if !offset.is_finite() {
            return Err(EditError::Invalid("offset is not finite".to_owned()));
        }
        let mut frag = Self::parse_clipboard(text)?;
        self.auto_rename_copy(&mut frag, is_target_key);
        let layout = SideLayout::of(&frag);
        self.tx("Paste", |doc| {
            let objs = doc.instantiate(frag, false)?;
            if let Some(layout) = &layout {
                doc.remap_side_lists(layout, &objs);
            }
            if offset != DVec3::ZERO {
                doc.translate(&objs, offset)?;
            }
            doc.attach_pasted_overlays(&objs);
            doc.select(&objs, SelectOp::Replace);
            Ok(objs)
        })
    }

    pub fn clipboard_bounds(text: &str) -> EditResult<Aabb> {
        let frag = Self::parse_clipboard(text)?;
        fragment_bounds(&frag).ok_or(EditError::EmptyClipboard)
    }

    pub fn paste_special_preview(text: &str, opts: &PasteSpecial) -> EditResult<PastePreview> {
        if opts.copies == 0 || opts.copies > 10_000 {
            return Err(EditError::Invalid("copies must be from 1 to 10000".to_owned()));
        }
        if !opts.offset.is_finite() || !opts.rotation.is_finite() || !opts.pivot.is_none_or(|p| p.is_finite()) {
            return Err(EditError::Invalid("offset, rotation and pivot must be finite".to_owned()));
        }
        let frag = Self::parse_clipboard(text)?;
        let center = opts.pivot.unwrap_or_else(|| fragment_bounds(&frag).map_or(DVec3::ZERO, |b| b.center()));
        let mut base: Vec<[DVec3; 2]> = Vec::new();
        let solids = frag.solids.iter().chain(frag.entities.iter().flat_map(|e| e.solids.iter()));
        for s in solids {
            base.extend(hp_geom::build_solid(&s.sides).edges());
        }
        for e in &frag.entities {
            if e.solids.is_empty()
                && let Some(o) = e.origin().filter(|o| o.is_finite())
            {
                marker_lines(o, &mut base);
            }
        }
        let objects = frag.solids.len() + frag.entities.len();
        let mut preview = PastePreview { lines: Vec::new(), copies: opts.copies, shown_copies: 0, objects };
        if base.is_empty() {
            return Ok(preview);
        }
        for k in 0..opts.copies {
            if preview.lines.len() + base.len() > PREVIEW_MAX_LINES && k > 0 {
                break;
            }
            let step = if opts.start_at_original { f64::from(k) } else { f64::from(k) + 1.0 };
            let angles = opts.rotation * step;
            let q = (angles != DVec3::ZERO).then(|| {
                DQuat::from_euler(EulerRot::ZYX, angles.z.to_radians(), angles.y.to_radians(), angles.x.to_radians())
            });
            let off = opts.offset * step;
            let map = |p: DVec3| match q {
                Some(q) => center + q * (p - center) + off,
                None => p + off,
            };
            preview.lines.extend(base.iter().map(|[a, b]| [map(*a), map(*b)]));
            preview.shown_copies = k + 1;
        }
        Ok(preview)
    }

    pub fn paste_special(&mut self, text: &str, opts: &PasteSpecial) -> EditResult<Vec<ObjRef>> {
        self.paste_special_with(text, opts, None)
    }

    pub fn paste_special_with(
        &mut self,
        text: &str,
        opts: &PasteSpecial,
        is_target_key: Option<TargetKeyFn<'_>>,
    ) -> EditResult<Vec<ObjRef>> {
        if opts.copies == 0 {
            return Err(EditError::Invalid("at least one copy is needed".to_owned()));
        }
        if opts.copies > 10_000 {
            return Err(EditError::Invalid("too many copies".to_owned()));
        }
        if !opts.offset.is_finite() || !opts.rotation.is_finite() || !opts.pivot.is_none_or(|p| p.is_finite()) {
            return Err(EditError::Invalid("offset, rotation and pivot must be finite".to_owned()));
        }
        let frag = Self::parse_clipboard(text)?;
        let center = opts.pivot.unwrap_or_else(|| fragment_bounds(&frag).map_or(DVec3::ZERO, |b| b.center()));
        let layout = SideLayout::of(&frag);
        let mut used: std::collections::HashSet<String> = self
            .scene
            .entities
            .values()
            .filter_map(|n| n.entity.get("targetname").map(str::to_ascii_lowercase))
            .collect();
        self.tx("Paste special", |doc| {
            let mut all = Vec::new();
            for k in 0..opts.copies {
                let step = if opts.start_at_original { f64::from(k) } else { f64::from(k) + 1.0 };
                let mut copy = frag.clone();
                rename_entities(&mut copy, opts, &mut used, is_target_key);
                let objs = doc.instantiate(copy, false)?;
                if let Some(layout) = &layout {
                    doc.remap_side_lists(layout, &objs);
                }
                let angles = opts.rotation * step;
                if angles != DVec3::ZERO {
                    let q = DQuat::from_euler(
                        EulerRot::ZYX,
                        angles.z.to_radians(),
                        angles.y.to_radians(),
                        angles.x.to_radians(),
                    );
                    doc.rotate(&objs, center, q)?;
                }
                let off = opts.offset * step;
                if off != DVec3::ZERO {
                    doc.translate(&objs, off)?;
                }
                doc.attach_pasted_overlays(&objs);
                if opts.group && !objs.is_empty() {
                    doc.group(&objs)?;
                }
                all.extend(objs);
            }
            doc.select(&all, SelectOp::Replace);
            Ok(all)
        })
    }
}

fn rename_entities(
    frag: &mut Fragment,
    opts: &PasteSpecial,
    used: &mut std::collections::HashSet<String>,
    is_target_key: Option<TargetKeyFn<'_>>,
) {
    if opts.name_prefix.is_empty() && !opts.unique_names {
        increment_names(frag, used, is_target_key);
        return;
    }
    let mut renames: Vec<(String, String)> = Vec::new();
    for e in &mut frag.entities {
        let Some(name) = e.get("targetname").map(str::to_owned) else { continue };
        if name.is_empty() {
            continue;
        }
        if let Some((_, new)) = renames.iter().find(|(old, _)| old.eq_ignore_ascii_case(&name)) {
            e.set("targetname", new.clone());
            continue;
        }
        let mut new = format!("{}{}", opts.name_prefix, name);
        if opts.unique_names && used.contains(&new.to_ascii_lowercase()) {
            let base = new.clone();
            let mut k = 2u32;
            loop {
                let candidate = format!("{base}_{k}");
                if !used.contains(&candidate.to_ascii_lowercase()) {
                    new = candidate;
                    break;
                }
                k = k.saturating_add(1);
                if k == u32::MAX {
                    break;
                }
            }
        }
        used.insert(new.to_ascii_lowercase());
        e.set("targetname", new.clone());
        renames.push((name, new));
    }
    if renames.is_empty() {
        return;
    }
    apply_renames(frag, &renames, is_target_key);
}

const SIDE_LIST_KEYS: [&str; 2] = ["sides", "sides2"];

pub(crate) fn parse_side_list(text: &str) -> Vec<Id> {
    text.split_ascii_whitespace().filter_map(|t| t.parse().ok()).collect()
}

pub(crate) fn format_side_list(ids: &[Id]) -> String {
    let mut s = String::with_capacity(ids.len() * 6);
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&id.to_string());
    }
    s
}

#[derive(Debug, Default)]
pub(crate) struct SideLayout {
    world: Vec<Vec<Id>>,
    entities: Vec<Vec<Vec<Id>>>,
}

impl SideLayout {
    pub(crate) fn of(frag: &Fragment) -> Option<Self> {
        let needed = frag.entities.iter().any(|e| SIDE_LIST_KEYS.iter().any(|k| e.get(k).is_some()));
        if !needed {
            return None;
        }
        let ids = |s: &hp_vmf::Solid| s.sides.iter().map(|side| side.id).collect::<Vec<Id>>();
        Some(Self {
            world: frag.solids.iter().map(ids).collect(),
            entities: frag.entities.iter().map(|e| e.solids.iter().map(ids).collect()).collect(),
        })
    }
}

impl Document {
    pub(crate) fn remap_side_lists(&mut self, layout: &SideLayout, new: &[ObjRef]) {
        let mut map: IdMap<Id> = id_map(64);
        let mut add = |old: &[Id], solid: Id, doc: &Document| {
            let Some(n) = doc.scene.solids.get(&solid) else { return };
            if n.solid.sides.len() != old.len() {
                return;
            }
            for (o, s) in old.iter().zip(&n.solid.sides) {
                map.insert(*o, s.id);
            }
        };
        for (i, old) in layout.world.iter().enumerate() {
            if let Some(ObjRef::Solid(id)) = new.get(i) {
                add(old, *id, self);
            }
        }
        let base = layout.world.len();
        for (j, solids) in layout.entities.iter().enumerate() {
            let Some(ObjRef::Entity(e)) = new.get(base + j) else { continue };
            let mut ids = self.scene.entity_solid_ids(*e);
            ids.sort_unstable();
            if ids.len() != solids.len() {
                continue;
            }
            for (old, id) in solids.iter().zip(ids) {
                add(old, id, self);
            }
        }
        if map.is_empty() {
            return;
        }
        let mut updates: Vec<(Id, Vec<(&'static str, String)>)> = Vec::new();
        for o in new.get(base..).unwrap_or(&[]) {
            let ObjRef::Entity(id) = *o else { continue };
            let Some(n) = self.scene.entities.get(&id) else { continue };
            let mut keys = Vec::new();
            for key in SIDE_LIST_KEYS {
                let Some(text) = n.entity.get(key) else { continue };
                let old = parse_side_list(text);
                let mapped: Vec<Id> = old.iter().map(|s| map.get(s).copied().unwrap_or(*s)).collect();
                if mapped != old {
                    keys.push((key, format_side_list(&mapped)));
                }
            }
            if !keys.is_empty() {
                updates.push((id, keys));
            }
        }
        for (id, keys) in updates {
            self.update_entity(id, |e| {
                for (k, v) in keys {
                    e.set(k, v);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_parsing() {
        let text = "hammerpro_clipboard\n{\n\t\"version\" \"1\"\n\tgroup\n\t{\n\t\t\"id\" \"7\"\n\t\teditor\n\t\t{\n\t\t\t\"color\" \"1 2 3\"\n\t\t\t\"groupid\" \"5\"\n\t\t}\n\t}\n}\nsolid\n{\n}\n";
        let groups = header_groups(text);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, 7);
        assert_eq!(groups[0].editor.color, [1, 2, 3]);
        assert_eq!(groups[0].editor.group_id, Some(5));
        assert!(header_groups("solid { }").is_empty());
        assert!(header_text("hammerpro_clipboard { \"a\" \"}\" ").is_none());
        assert_eq!(parse_side_list(" 3 x 12  7"), vec![3, 12, 7]);
        assert_eq!(format_side_list(&[3, 12]), "3 12");
    }
}
