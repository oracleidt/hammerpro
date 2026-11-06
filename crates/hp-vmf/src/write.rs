use std::ops::{Deref, DerefMut};

use glam::DVec3;
use hp_kv::Block;
use rayon::prelude::*;

use crate::num::{push_f32, push_f64_styled, push_i64};
use crate::{
    Cameras, Connection, CordonStyle, Cordons, DispInfo, EditorInfo, Entity, Group, LineEnding, Map, NumberStyle, Side,
    Solid, TexAxis, TextFormat, VersionInfo, VisGroup,
};

const PAR_MIN_ITEMS: usize = 512;
const PAR_CHUNK: usize = 128;
const PAR_BATCH_CHUNKS: usize = 64;
const WRITE_POWER_CAP: u32 = 10;

const TABS: &str = "\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t";

pub(crate) const AXIS_KEYS: [&str; 2] = ["uaxis", "vaxis"];
pub(crate) const SIDE_SCALAR_KEYS: [&str; 3] = ["rotation", "lightmapscale", "smoothing_groups"];
const SIDE_TYPED: [&str; 7] = ["id", "plane", "uaxis", "vaxis", "rotation", "lightmapscale", "smoothing_groups"];
const EDITOR_TYPED: [&str; 4] = ["color", "groupid", "visgroupshown", "visgroupautoshown"];
const VISGROUP_TYPED: [&str; 2] = ["visgroupid", "color"];
const VERSION_TYPED: [&str; 5] = ["editorversion", "editorbuild", "mapversion", "formatversion", "prefab"];
const DISP_TYPED: [&str; 4] = ["startposition", "flags", "elevation", "subdiv"];

pub(crate) struct Out {
    buf: String,
    nl: &'static str,
    numbers: NumberStyle,
}

impl Deref for Out {
    type Target = String;
    fn deref(&self) -> &String {
        &self.buf
    }
}

impl DerefMut for Out {
    fn deref_mut(&mut self) -> &mut String {
        &mut self.buf
    }
}

impl Out {
    fn new(capacity: usize, format: &TextFormat) -> Self {
        Self { buf: String::with_capacity(capacity), nl: format.line_ending.as_str(), numbers: format.numbers }
    }

    fn sibling(&self, capacity: usize) -> Self {
        Self { buf: String::with_capacity(capacity), nl: self.nl, numbers: self.numbers }
    }

    fn into_string(self) -> String {
        self.buf
    }

    #[inline]
    fn nl(&mut self) {
        self.buf.push_str(self.nl);
    }

    #[inline]
    fn f64(&mut self, v: f64) {
        push_f64_styled(&mut self.buf, v, self.numbers);
    }

    #[inline]
    fn vec3(&mut self, v: DVec3) {
        self.f64(v.x);
        self.buf.push(' ');
        self.f64(v.y);
        self.buf.push(' ');
        self.f64(v.z);
    }

    #[inline]
    fn int(&mut self, v: i64) {
        push_i64(&mut self.buf, v);
    }
}

#[inline]
fn tabs(out: &mut Out, n: usize) {
    let mut left = n;
    while left > 0 {
        let k = left.min(TABS.len());
        out.push_str(TABS.get(..k).unwrap_or_default());
        left -= k;
    }
}

#[inline]
fn open(out: &mut Out, ind: usize, name: &str) {
    tabs(out, ind);
    out.push_str(name);
    out.nl();
    tabs(out, ind);
    out.push('{');
    out.nl();
}

#[inline]
fn close(out: &mut Out, ind: usize) {
    tabs(out, ind);
    out.push('}');
    out.nl();
}

#[inline]
fn push_text(out: &mut String, s: &str) {
    if memchr::memchr(b'"', s.as_bytes()).is_some() {
        out.push_str(&s.replace('"', "'"));
    } else {
        out.push_str(s);
    }
}

#[inline]
fn key(out: &mut Out, ind: usize, k: &str) {
    tabs(out, ind);
    out.push('"');
    out.push_str(k);
    out.push_str("\" \"");
}

#[inline]
fn end(out: &mut Out) {
    out.push('"');
    out.nl();
}

#[inline]
fn kv_text(out: &mut Out, ind: usize, k: &str, v: &str) {
    tabs(out, ind);
    out.push('"');
    push_text(out, k);
    out.push_str("\" \"");
    push_text(out, v);
    end(out);
}

#[inline]
fn kv_int(out: &mut Out, ind: usize, k: &str, v: i64) {
    key(out, ind, k);
    out.int(v);
    end(out);
}

#[inline]
fn kv_f64(out: &mut Out, ind: usize, k: &str, v: f64) {
    key(out, ind, k);
    out.f64(v);
    end(out);
}

#[inline]
fn kv_bool(out: &mut Out, ind: usize, k: &str, v: bool) {
    key(out, ind, k);
    out.push(if v { '1' } else { '0' });
    end(out);
}

#[inline]
fn kv_vec3(out: &mut Out, ind: usize, k: &str, v: DVec3, l: char, r: char) {
    key(out, ind, k);
    out.push(l);
    out.vec3(v);
    out.push(r);
    end(out);
}

#[inline]
fn props(out: &mut Out, ind: usize, props: &[(String, String)]) {
    for (k, v) in props {
        kv_text(out, ind, k, v);
    }
}

#[inline]
fn props_except(out: &mut Out, ind: usize, props: &[(String, String)], typed: &[&str]) {
    for (k, v) in props {
        if !typed.iter().any(|t| k.eq_ignore_ascii_case(t)) {
            kv_text(out, ind, k, v);
        }
    }
}

#[inline]
fn raw<'a>(extra: &'a [(String, String)], key: &str, unread: bool) -> Option<(&'a str, &'a str)> {
    if !unread || extra.is_empty() {
        return None;
    }
    extra.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(k, v)| (k.as_str(), v.as_str()))
}

#[inline]
fn kv_raw(out: &mut Out, ind: usize, extra: &[(String, String)], key: &str, unread: bool) -> bool {
    match raw(extra, key, unread) {
        Some((k, v)) => {
            kv_text(out, ind, k, v);
            true
        }
        None => false,
    }
}

fn push_block_name(out: &mut Out, name: &str) {
    let needs_quotes = name.bytes().any(|c| c.is_ascii_whitespace() || matches!(c, b'{' | b'}' | b'"'))
        || name.starts_with('[')
        || name.contains("//");
    if needs_quotes {
        out.push('"');
        push_text(out, name);
        out.push('"');
    } else {
        out.push_str(name);
    }
}

pub(crate) fn write_raw_block(out: &mut Out, b: &Block, ind: usize) {
    tabs(out, ind);
    push_block_name(out, &b.name);
    out.nl();
    tabs(out, ind);
    out.push('{');
    out.nl();
    write_raw_contents(out, b, ind + 1);
    close(out, ind);
}

fn write_raw_contents(out: &mut Out, b: &Block, ind: usize) {
    props(out, ind, &b.props);
    for c in &b.children {
        write_raw_block(out, c, ind);
    }
}

fn write_list<T: Sync>(
    out: &mut Out,
    items: &[T],
    ind: usize,
    write_one: fn(&mut Out, &T, usize),
    estimate: fn(&T) -> usize,
) {
    if items.len() < PAR_MIN_ITEMS {
        for it in items {
            write_one(out, it, ind);
        }
        return;
    }
    for batch in items.chunks(PAR_CHUNK * PAR_BATCH_CHUNKS) {
        let template = out.sibling(0);
        let parts: Vec<String> = batch
            .par_chunks(PAR_CHUNK)
            .map(|chunk| {
                let mut s = template.sibling(chunk.iter().map(estimate).sum());
                for it in chunk {
                    write_one(&mut s, it, ind);
                }
                s.into_string()
            })
            .collect();
        out.reserve(parts.iter().map(String::len).sum());
        for p in &parts {
            out.push_str(p);
        }
    }
}

fn estimate_props(p: &[(String, String)]) -> usize {
    p.iter().map(|(k, v)| k.len() + v.len() + 12).sum()
}

fn estimate_disp(d: &DispInfo) -> usize {
    let w = 1usize << d.power.min(WRITE_POWER_CAP);
    let w = w + 1;
    1_000 + w * w * 110
}

fn estimate_side(s: &Side) -> usize {
    330 + s.material.len()
        + estimate_props(&s.extra_props)
        + s.vertices_plus.as_ref().map_or(0, |v| 48 + v.len() * 40)
        + s.disp.as_deref().map_or(0, estimate_disp)
        + s.extra_blocks.len() * 128
}

fn estimate_solid(s: &Solid) -> usize {
    200 + s.sides.iter().map(estimate_side).sum::<usize>() + estimate_props(&s.extra_props) + s.extra_blocks.len() * 128
}

fn estimate_entity(e: &Entity) -> usize {
    250 + estimate_props(&e.props)
        + e.connections.len() * 96
        + e.solids.iter().map(estimate_solid).sum::<usize>()
        + e.groups.len() * 160
        + e.extra_blocks.len() * 256
}

fn estimate_map(map: &Map) -> usize {
    let base = 2_048 + estimate_entity(&map.world) + map.extra_blocks.len() * 512;
    let ents: usize = if map.entities.len() >= PAR_MIN_ITEMS {
        map.entities.par_iter().map(estimate_entity).sum()
    } else {
        map.entities.iter().map(estimate_entity).sum()
    };
    let total = base + ents;
    let total = total + total / 16;
    if map.format.line_ending == LineEnding::CrLf { total + total / 24 } else { total }
}

pub(crate) fn write_map(map: &Map) -> String {
    let mut out = Out::new(estimate_map(map), &map.format);
    if map.format.bom {
        out.push('\u{feff}');
    }
    write_version_info(&mut out, &map.version_info);
    write_visgroups(&mut out, &map.visgroups);
    open(&mut out, 0, "viewsettings");
    write_raw_contents(&mut out, &map.view_settings, 1);
    close(&mut out, 0);
    write_entity_body(&mut out, &map.world, 0, Some(map.version_info.map_version));
    write_list(&mut out, &map.entities, 0, write_entity_wrapped, estimate_entity);
    write_cameras(&mut out, &map.cameras);
    write_cordons(&mut out, &map.cordons);
    for b in &map.extra_blocks {
        write_raw_block(&mut out, b, 0);
    }
    out.into_string()
}

pub(crate) fn write_solid_fragment(s: &Solid) -> String {
    let mut out = Out::new(estimate_solid(s) + 32, &TextFormat::default());
    write_solid_wrapped(&mut out, s, 0);
    out.into_string()
}

pub(crate) fn write_entity_fragment(e: &Entity) -> String {
    let mut out = Out::new(estimate_entity(e) + 32, &TextFormat::default());
    write_entity_wrapped(&mut out, e, 0);
    out.into_string()
}

fn write_version_info(out: &mut Out, v: &VersionInfo) {
    let d = VersionInfo::default();
    let x = &v.extra_props;
    open(out, 0, "versioninfo");
    if !kv_raw(out, 1, x, "editorversion", v.editor_version == d.editor_version) {
        kv_int(out, 1, "editorversion", v.editor_version.into());
    }
    if !kv_raw(out, 1, x, "editorbuild", v.editor_build == d.editor_build) {
        kv_int(out, 1, "editorbuild", v.editor_build.into());
    }
    if !kv_raw(out, 1, x, "mapversion", v.map_version == d.map_version) {
        kv_int(out, 1, "mapversion", v.map_version.into());
    }
    if !kv_raw(out, 1, x, "formatversion", v.format_version == d.format_version) {
        kv_int(out, 1, "formatversion", v.format_version.into());
    }
    if !kv_raw(out, 1, x, "prefab", v.prefab == d.prefab) {
        kv_bool(out, 1, "prefab", v.prefab);
    }
    props_except(out, 1, x, &VERSION_TYPED);
    close(out, 0);
}

fn write_visgroups(out: &mut Out, groups: &[VisGroup]) {
    open(out, 0, "visgroups");
    for g in groups {
        write_visgroup(out, g, 1);
    }
    close(out, 0);
}

fn write_visgroup(out: &mut Out, g: &VisGroup, ind: usize) {
    open(out, ind, "visgroup");
    let i = ind + 1;
    let x = &g.extra_props;
    kv_text(out, i, "name", &g.name);
    if !kv_raw(out, i, x, "visgroupid", g.id == 0) {
        kv_int(out, i, "visgroupid", g.id.into());
    }
    if !kv_raw(out, i, x, "color", g.color == [0, 0, 0]) {
        write_color(out, i, "color", g.color);
    }
    props_except(out, i, x, &VISGROUP_TYPED);
    for c in &g.children {
        write_visgroup(out, c, i);
    }
    close(out, ind);
}

fn write_color(out: &mut Out, ind: usize, k: &str, c: [u8; 3]) {
    key(out, ind, k);
    let [r, g, b] = c;
    out.int(r.into());
    out.push(' ');
    out.int(g.into());
    out.push(' ');
    out.int(b.into());
    end(out);
}

fn write_cameras(out: &mut Out, c: &Cameras) {
    open(out, 0, "cameras");
    kv_int(out, 1, "activecamera", c.active.into());
    for cam in &c.cameras {
        open(out, 1, "camera");
        kv_vec3(out, 2, "position", cam.position, '[', ']');
        kv_vec3(out, 2, "look", cam.look, '[', ']');
        close(out, 1);
    }
    close(out, 0);
}

fn write_cordons(out: &mut Out, c: &Cordons) {
    let legacy = c.style == CordonStyle::Legacy && c.cordons.len() <= 1 && c.extra_boxes.is_empty();
    if legacy {
        let (mins, maxs) =
            c.cordons.first().map_or((DVec3::splat(-1024.0), DVec3::splat(1024.0)), |cd| (cd.mins, cd.maxs));
        open(out, 0, "cordon");
        kv_vec3(out, 1, "mins", mins, '(', ')');
        kv_vec3(out, 1, "maxs", maxs, '(', ')');
        kv_bool(out, 1, "active", c.active);
        close(out, 0);
        return;
    }
    open(out, 0, "cordons");
    kv_bool(out, 1, "active", c.active);
    for (index, cd) in c.cordons.iter().enumerate() {
        open(out, 1, "cordon");
        kv_text(out, 2, "name", &cd.name);
        kv_bool(out, 2, "active", cd.active);
        let extra = c.extra_boxes.iter().filter(|b| b.cordon == index).map(|b| (b.mins, b.maxs));
        for (mins, maxs) in std::iter::once((cd.mins, cd.maxs)).chain(extra) {
            open(out, 2, "box");
            kv_vec3(out, 3, "mins", mins, '(', ')');
            kv_vec3(out, 3, "maxs", maxs, '(', ')');
            close(out, 2);
        }
        close(out, 1);
    }
    close(out, 0);
}

fn write_entity_wrapped(out: &mut Out, e: &Entity, ind: usize) {
    if e.hidden {
        open(out, ind, "hidden");
        write_entity_body(out, e, ind + 1, None);
        close(out, ind);
    } else {
        write_entity_body(out, e, ind, None);
    }
}

#[inline]
fn is_reserved_key(k: &str, is_world: bool) -> bool {
    k.eq_ignore_ascii_case("id")
        || k.eq_ignore_ascii_case("classname")
        || (is_world && k.eq_ignore_ascii_case("mapversion"))
}

fn write_entity_body(out: &mut Out, e: &Entity, ind: usize, world_version: Option<i32>) {
    let is_world = world_version.is_some();
    let world_particles = !e.solids.is_empty() && crate::is_world_particle_volume(&e.classname);
    open(out, ind, if is_world { "world" } else { "entity" });
    let i = ind + 1;
    kv_int(out, i, "id", e.id.into());
    if let Some(v) = world_version {
        kv_int(out, i, "mapversion", v.into());
    }
    kv_text(out, i, "classname", &e.classname);
    let mut props = e.props.as_slice();
    let mut deferred_origin = None;
    if !is_world
        && e.solids.is_empty()
        && !e.connections.is_empty()
        && let Some(((k, v), rest)) = props.split_last()
        && k.eq_ignore_ascii_case("origin")
    {
        deferred_origin = Some((k, v));
        props = rest;
    }
    for (k, v) in props {
        if !is_reserved_key(k, is_world) && !(world_particles && k.eq_ignore_ascii_case("origin")) {
            kv_text(out, i, k, v);
        }
    }
    if !e.connections.is_empty() {
        open(out, i, "connections");
        for c in &e.connections {
            tabs(out, i + 1);
            out.push('"');
            push_text(out, &c.output);
            out.push_str("\" \"");
            push_connection_value(out, c);
            end(out);
        }
        close(out, i);
    }
    if let Some((k, v)) = deferred_origin {
        kv_text(out, i, k, v);
    }
    if world_particles {
        for solid in &e.solids {
            let origin_helper = solid
                .sides
                .first()
                .is_some_and(|side| side.material.replace('\\', "/").trim().eq_ignore_ascii_case("tools/toolsorigin"));
            if !origin_helper {
                write_solid_wrapped(out, solid, i);
            }
        }
    } else {
        write_list(out, &e.solids, i, write_solid_wrapped, estimate_solid);
    }
    for g in &e.groups {
        write_group(out, g, i);
    }
    if !is_world || e.editor != EditorInfo::default() {
        write_editor(out, &e.editor, i);
    }
    for b in &e.extra_blocks {
        write_raw_block(out, b, i);
    }
    close(out, ind);
}

pub(crate) fn push_connection_value(out: &mut String, c: &Connection) {
    let sep = c.effective_separator();
    push_text(out, &c.target);
    out.push(sep);
    push_text(out, &c.input);
    out.push(sep);
    push_text(out, &c.param);
    out.push(sep);
    push_f32(out, c.delay);
    out.push(sep);
    push_i64(out, c.times.into());
}

fn write_group(out: &mut Out, g: &Group, ind: usize) {
    open(out, ind, "group");
    kv_int(out, ind + 1, "id", g.id.into());
    write_editor(out, &g.editor, ind + 1);
    close(out, ind);
}

fn write_editor(out: &mut Out, e: &EditorInfo, ind: usize) {
    open(out, ind, "editor");
    let i = ind + 1;
    let x = &e.extra_props;
    let d = EditorInfo::default();
    if !kv_raw(out, i, x, "color", e.color == d.color) {
        write_color(out, i, "color", e.color);
    }
    for id in &e.visgroup_ids {
        kv_int(out, i, "visgroupid", (*id).into());
    }
    if let Some(g) = e.group_id {
        kv_int(out, i, "groupid", g.into());
    } else {
        kv_raw(out, i, x, "groupid", true);
    }
    if !kv_raw(out, i, x, "visgroupshown", e.visgroup_shown == d.visgroup_shown) {
        kv_bool(out, i, "visgroupshown", e.visgroup_shown);
    }
    if !kv_raw(out, i, x, "visgroupautoshown", e.visgroup_auto_shown == d.visgroup_auto_shown) {
        kv_bool(out, i, "visgroupautoshown", e.visgroup_auto_shown);
    }
    if let Some(c) = &e.comments {
        kv_text(out, i, "comments", c);
    }
    if let Some(p) = &e.logical_pos {
        kv_text(out, i, "logicalpos", p);
    }
    props_except(out, i, x, &EDITOR_TYPED);
    close(out, ind);
}

fn write_solid_wrapped(out: &mut Out, s: &Solid, ind: usize) {
    if s.hidden {
        open(out, ind, "hidden");
        write_solid_body(out, s, ind + 1);
        close(out, ind);
    } else {
        write_solid_body(out, s, ind);
    }
}

fn write_solid_body(out: &mut Out, s: &Solid, ind: usize) {
    open(out, ind, "solid");
    let i = ind + 1;
    kv_int(out, i, "id", s.id.into());
    props_except(out, i, &s.extra_props, &["id"]);
    for side in &s.sides {
        write_side(out, side, i);
    }
    write_editor(out, &s.editor, i);
    for b in &s.extra_blocks {
        write_raw_block(out, b, i);
    }
    close(out, ind);
}

fn write_axis(out: &mut Out, ind: usize, k: &str, a: &TexAxis) {
    key(out, ind, k);
    out.push('[');
    out.vec3(a.axis);
    out.push(' ');
    out.f64(a.shift);
    out.push_str("] ");
    out.f64(a.scale);
    end(out);
}

fn write_side(out: &mut Out, s: &Side, ind: usize) {
    open(out, ind, "side");
    let i = ind + 1;
    let x = &s.extra_props;
    kv_int(out, i, "id", s.id.into());
    if !kv_raw(out, i, x, "plane", s.normal() == DVec3::ZERO) {
        key(out, i, "plane");
        let [a, b, c] = s.plane;
        out.push('(');
        out.vec3(a);
        out.push_str(") (");
        out.vec3(b);
        out.push_str(") (");
        out.vec3(c);
        out.push(')');
        end(out);
    }
    if let Some(verts) = &s.vertices_plus {
        open(out, i, "vertices_plus");
        for v in verts {
            key(out, i + 1, "v");
            out.vec3(*v);
            end(out);
        }
        close(out, i);
    }
    kv_text(out, i, "material", &s.material);
    if !kv_raw(out, i, x, "uaxis", s.uaxis.axis == DVec3::ZERO) {
        write_axis(out, i, "uaxis", &s.uaxis);
    }
    if !kv_raw(out, i, x, "vaxis", s.vaxis.axis == DVec3::ZERO) {
        write_axis(out, i, "vaxis", &s.vaxis);
    }
    if !kv_raw(out, i, x, "rotation", s.rotation == 0.0) {
        kv_f64(out, i, "rotation", s.rotation);
    }
    if !kv_raw(out, i, x, "lightmapscale", s.lightmap_scale == 16) {
        kv_int(out, i, "lightmapscale", s.lightmap_scale.into());
    }
    if !kv_raw(out, i, x, "smoothing_groups", s.smoothing_groups == 0) {
        kv_int(out, i, "smoothing_groups", s.smoothing_groups.into());
    }
    props_except(out, i, x, &SIDE_TYPED);
    if let Some(d) = &s.disp {
        write_disp(out, d, i);
    }
    for b in &s.extra_blocks {
        write_raw_block(out, b, i);
    }
    close(out, ind);
}

fn row_key(out: &mut Out, ind: usize, row: usize) {
    tabs(out, ind);
    out.push_str("\"row");
    out.int(row as i64);
    out.push_str("\" \"");
}

fn grid_vec3(out: &mut Out, ind: usize, name: &str, grid: &[DVec3], rows: usize, cols: usize) {
    open(out, ind, name);
    for r in 0..rows {
        row_key(out, ind + 1, r);
        for c in 0..cols {
            if c > 0 {
                out.push(' ');
            }
            out.vec3(grid.get(r * cols + c).copied().unwrap_or_default());
        }
        end(out);
    }
    close(out, ind);
}

fn grid_f64(out: &mut Out, ind: usize, name: &str, grid: &[f64], rows: usize, cols: usize) {
    open(out, ind, name);
    for r in 0..rows {
        row_key(out, ind + 1, r);
        for c in 0..cols {
            if c > 0 {
                out.push(' ');
            }
            out.f64(grid.get(r * cols + c).copied().unwrap_or_default());
        }
        end(out);
    }
    close(out, ind);
}

fn grid_i32(out: &mut Out, ind: usize, name: &str, grid: &[i32], rows: usize, cols: usize) {
    open(out, ind, name);
    for r in 0..rows {
        row_key(out, ind + 1, r);
        for c in 0..cols {
            if c > 0 {
                out.push(' ');
            }
            out.int(grid.get(r * cols + c).copied().unwrap_or_default().into());
        }
        end(out);
    }
    close(out, ind);
}

fn write_disp(out: &mut Out, d: &DispInfo, ind: usize) {
    open(out, ind, "dispinfo");
    let i = ind + 1;
    let x = &d.extra_props;
    kv_int(out, i, "power", d.power.into());
    if !kv_raw(out, i, x, "startposition", d.start_position == DVec3::ZERO) {
        kv_vec3(out, i, "startposition", d.start_position, '[', ']');
    }
    if !kv_raw(out, i, x, "flags", d.flags == 0) {
        kv_int(out, i, "flags", d.flags.into());
    }
    if !kv_raw(out, i, x, "elevation", d.elevation == 0.0) {
        kv_f64(out, i, "elevation", d.elevation);
    }
    if !kv_raw(out, i, x, "subdiv", !d.subdiv) {
        kv_bool(out, i, "subdiv", d.subdiv);
    }
    props_except(out, i, x, &DISP_TYPED);
    let n = 1usize << d.power.min(WRITE_POWER_CAP);
    let w = n + 1;
    grid_vec3(out, i, "normals", &d.normals, w, w);
    grid_f64(out, i, "distances", &d.distances, w, w);
    grid_vec3(out, i, "offsets", &d.offsets, w, w);
    grid_vec3(out, i, "offset_normals", &d.offset_normals, w, w);
    grid_f64(out, i, "alphas", &d.alphas, w, w);
    grid_i32(out, i, "triangle_tags", &d.triangle_tags, n, 2 * n);
    open(out, i, "allowed_verts");
    key(out, i + 1, "10");
    for k in 0..10 {
        if k > 0 {
            out.push(' ');
        }
        out.int(d.allowed_verts.get(k).copied().unwrap_or(-1).into());
    }
    end(out);
    close(out, i);
    for b in &d.extra_blocks {
        write_raw_block(out, b, i);
    }
    close(out, ind);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out() -> Out {
        Out::new(0, &TextFormat::default())
    }

    #[test]
    fn particle_brush_volumes_export_in_world_space_without_changing_the_brushes() {
        for class in ["env_embers", "func_smokevolume", "func_dustcloud", "func_dustmotes", "func_tracktrain"] {
            let entity = Entity {
                id: 2,
                classname: class.into(),
                props: vec![("origin".into(), "880 560 77.5".into()), ("spawnflags".into(), "1".into())],
                solids: vec![Solid {
                    id: 3,
                    sides: vec![Side { id: 4, material: "tools/toolstrigger".into(), ..Default::default() }],
                    ..Default::default()
                }],
                ..Default::default()
            };
            let mut entity = entity;
            let mut helper = entity.solids[0].clone();
            helper.id = 5;
            helper.sides[0].material = "tools/toolsorigin".into();
            entity.solids.push(helper);
            let text = write_entity_fragment(&entity);
            assert_eq!(text.contains("\"origin\""), class == "func_tracktrain", "{class}: {text}");
            assert!(text.contains("\"spawnflags\" \"1\""));
            assert!(text.contains("tools/toolstrigger"));
            assert_eq!(text.contains("tools/toolsorigin"), class == "func_tracktrain");
            assert_eq!(entity.solids.len(), 2);
            assert_eq!(entity.origin(), Some(DVec3::new(880.0, 560.0, 77.5)));
            let point = Entity { solids: Vec::new(), ..entity };
            assert!(write_entity_fragment(&point).contains("\"origin\""), "point origins are still positions");
        }
    }

    #[test]
    fn disp_layout_matches_hammer() {
        let mut d = DispInfo::new_flat(2, DVec3::new(-64.0, -64.0, 0.0), DVec3::Z);
        d.distances[6] = 12.5;
        let mut o = out();
        write_disp(&mut o, &d, 0);
        let out = o.into_string();
        let row_n = "0 0 1 0 0 1 0 0 1 0 0 1 0 0 1";
        let row_z = "0 0 0 0 0 0 0 0 0 0 0 0 0 0 0";
        let mut want = String::from(
            "dispinfo\n{\n\t\"power\" \"2\"\n\t\"startposition\" \"[-64 -64 0]\"\n\t\"flags\" \"0\"\n\t\"elevation\" \"0\"\n\t\"subdiv\" \"0\"\n",
        );
        let grid = |want: &mut String, name: &str, rows: &[&str]| {
            want.push_str(&format!("\t{name}\n\t{{\n"));
            for (i, r) in rows.iter().enumerate() {
                want.push_str(&format!("\t\t\"row{i}\" \"{r}\"\n"));
            }
            want.push_str("\t}\n");
        };
        grid(&mut want, "normals", &[row_n; 5]);
        grid(&mut want, "distances", &["0 0 0 0 0", "0 12.5 0 0 0", "0 0 0 0 0", "0 0 0 0 0", "0 0 0 0 0"]);
        grid(&mut want, "offsets", &[row_z; 5]);
        grid(&mut want, "offset_normals", &[row_n; 5]);
        grid(&mut want, "alphas", &["0 0 0 0 0"; 5]);
        grid(&mut want, "triangle_tags", &["9 9 9 9 9 9 9 9"; 4]);
        want.push_str("\tallowed_verts\n\t{\n\t\t\"10\" \"-1 -1 -1 -1 -1 -1 -1 -1 -1 -1\"\n\t}\n}\n");
        assert_eq!(out, want);
    }

    #[test]
    fn raw_block_names_are_quoted_when_needed() {
        let mut b = Block::new("my block");
        b.push_prop("k\"q", "v");
        b.children.push(Block::new(""));
        let mut o = out();
        write_raw_block(&mut o, &b, 0);
        let out = o.into_string();
        assert_eq!(out, "\"my block\"\n{\n\t\"k'q\" \"v\"\n\t\n\t{\n\t}\n}\n");
        let back = hp_kv::parse(&out).unwrap();
        assert_eq!(back.children[0].name, "my block");
        assert_eq!(back.children[0].children[0].name, "");
    }

    #[test]
    fn crlf_and_exponents() {
        let format =
            TextFormat { line_ending: LineEnding::CrLf, numbers: NumberStyle::Exponent(3), ..TextFormat::default() };
        let mut o = Out::new(0, &format);
        kv_vec3(&mut o, 1, "v", DVec3::new(-1.19209e-7, 0.5, 2e6), '[', ']');
        assert_eq!(o.into_string(), "\t\"v\" \"[-1.19209e-007 0.5 2e+006]\"\r\n");
    }

    #[test]
    fn unreadable_values_are_written_once_in_place() {
        let side = Side {
            plane: Side::UNREADABLE_PLANE,
            uaxis: crate::read::UNREADABLE_AXIS,
            extra_props: vec![
                ("plane".into(), "(-1.#IND -1.#IND -1.#IND) (0 0 0) (1 0 0)".into()),
                ("uaxis".into(), "[1.#QNAN 0 0 0] 0.25".into()),
                ("rotation".into(), "-1.#IND".into()),
                ("custom".into(), "kept".into()),
            ],
            ..Side::default()
        };
        let mut o = out();
        write_side(&mut o, &side, 0);
        let text = o.into_string();
        assert_eq!(text.matches("\"plane\"").count(), 1);
        assert!(text.contains("\t\"plane\" \"(-1.#IND -1.#IND -1.#IND) (0 0 0) (1 0 0)\"\n\t\"material\""));
        assert!(text.contains("\t\"uaxis\" \"[1.#QNAN 0 0 0] 0.25\"\n\t\"vaxis\" \"[0 -1 0 0] 0.25\"\n"));
        assert_eq!(text.matches("\"rotation\"").count(), 1);
        assert!(text.contains("\"rotation\" \"-1.#IND\""));
        assert!(text.contains("\"custom\" \"kept\""));

        let mut fixed = side;
        fixed.plane = [DVec3::new(0.0, 0.0, 8.0), DVec3::new(1.0, 0.0, 8.0), DVec3::new(1.0, -1.0, 8.0)];
        fixed.uaxis = TexAxis::default();
        fixed.rotation = 15.0;
        let mut o = out();
        write_side(&mut o, &fixed, 0);
        let text = o.into_string();
        assert!(!text.contains("#IND") && !text.contains("#QNAN"));
        assert_eq!(text.matches("\"plane\"").count(), 1);
        assert!(text.contains("\"rotation\" \"15\""));
    }
}
