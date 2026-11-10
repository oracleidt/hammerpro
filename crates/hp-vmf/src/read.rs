use std::borrow::Cow;

use glam::DVec3;
use hp_kv::{Block, BlockRef};
use rayon::prelude::*;

use crate::num::{
    num_tokens, parse_bool, parse_f32_prefix, parse_f64, parse_fixed, parse_i32, parse_i32_prefix, parse_u32,
    parse_vec3_strict,
};
use crate::scan::{Child, Top};
use crate::{
    CONNECTION_ESC, Camera, Cameras, Connection, Cordon, CordonBox, CordonStyle, Cordons, DispInfo, EditorInfo, Entity,
    Fragment, Group, MAX_DISP_POWER, MAX_NESTING, Map, Side, Solid, TexAxis, VersionInfo, VisGroup, VmfError,
};

pub(crate) const UNREADABLE_AXIS: TexAxis = TexAxis { axis: DVec3::ZERO, shift: 0.0, scale: 0.25 };

const PAR_CHILDREN: usize = 64;

#[inline]
fn is(name: &str, want: &str) -> bool {
    name.eq_ignore_ascii_case(want)
}

#[inline]
fn set<T>(slot: &mut T, value: Option<T>) -> bool {
    match value {
        Some(v) => {
            *slot = v;
            true
        }
        None => false,
    }
}

pub(crate) fn tops_from_root(root: Block) -> Vec<Top<'static>> {
    root.children.into_iter().map(top_from_block).collect()
}

fn top_from_block(b: Block) -> Top<'static> {
    Top { name: Cow::Owned(b.name), props: b.props, children: b.children.into_iter().map(Child::Parsed).collect() }
}

enum EntityJob<'a> {
    Top(Top<'a>),
    Hidden(Child<'a>),
}

impl EntityJob<'_> {
    fn build(self) -> Result<Entity, VmfError> {
        match self {
            EntityJob::Top(top) => entity_from_top(top, false, false),
            EntityJob::Hidden(child) => entity_from_top(top_from_block(child.into_block()?), false, true),
        }
    }
}

pub(crate) fn build_map(tops: Vec<Top<'_>>) -> Result<Map, VmfError> {
    let mut map = Map {
        view_settings: Block::new("viewsettings"),
        cordons: Cordons { style: CordonStyle::Modern, ..Cordons::default() },
        ..Map::default()
    };
    let mut found = false;
    let mut worlds = Vec::new();
    let mut jobs = Vec::new();

    for top in tops {
        let name = top.name.as_ref();
        if is(name, "world") {
            found = true;
            worlds.push(top);
        } else if is(name, "entity") {
            found = true;
            jobs.push(EntityJob::Top(top));
        } else if is(name, "hidden") {
            let Top { name, props, children } = top;
            let mut rest = Vec::new();
            for child in children {
                if is(child.name(), "entity") {
                    found = true;
                    jobs.push(EntityJob::Hidden(child));
                } else {
                    rest.push(child.into_block()?);
                }
            }
            if !props.is_empty() || !rest.is_empty() {
                map.extra_blocks.push(Block { name: name.into_owned(), props, children: rest });
            }
        } else {
            let mut block = top.into_block()?;
            let name = block.name.as_str();
            if is(name, "versioninfo") {
                found = true;
                map.version_info = version_info_from_block(block);
            } else if is(name, "visgroups") {
                map.visgroups.extend(
                    block.children.into_iter().filter(|c| is(&c.name, "visgroup")).map(|c| visgroup_from_block(c, 2)),
                );
            } else if is(name, "viewsettings") {
                "viewsettings".clone_into(&mut block.name);
                map.view_settings = block;
            } else if is(name, "cameras") {
                map.cameras = cameras_from_block(&block);
            } else if is(name, "cordon") {
                legacy_cordon(&mut map.cordons, &block);
            } else if is(name, "cordons") {
                modern_cordons(&mut map.cordons, &block);
            } else {
                map.extra_blocks.push(block);
            }
        }
    }

    if !found {
        return Err(VmfError::Parse("no VMF data found (expected versioninfo, world or entity blocks)".to_owned()));
    }

    let (world, entities) = rayon::join(
        || build_world(worlds),
        || jobs.into_par_iter().map(EntityJob::build).collect::<Result<Vec<_>, _>>(),
    );
    map.world = world?;
    map.entities = entities?;
    Ok(map)
}

fn build_world(worlds: Vec<Top<'_>>) -> Result<Entity, VmfError> {
    let mut iter = worlds.into_iter();
    let mut world = match iter.next() {
        Some(top) => entity_from_top(top, true, false)?,
        None => Entity::default(),
    };
    for top in iter {
        let more = entity_from_top(top, true, false)?;
        for (k, v) in more.props {
            world.set(&k, v);
        }
        world.solids.extend(more.solids);
        world.groups.extend(more.groups);
        world.connections.extend(more.connections);
        world.extra_blocks.extend(more.extra_blocks);
    }
    if world.classname.is_empty() {
        "worldspawn".clone_into(&mut world.classname);
    }
    Ok(world)
}

pub(crate) fn build_fragment(tops: Vec<Top<'_>>) -> Result<Fragment, VmfError> {
    enum Job<'a> {
        SolidTop(Top<'a>),
        Solid(Child<'a>, bool),
        Entity(EntityJob<'a>),
        World(Top<'a>),
    }
    let mut jobs = Vec::new();
    let mut visgroups = Vec::new();
    for top in tops {
        let name = top.name.as_ref();
        if is(name, "solid") {
            jobs.push(Job::SolidTop(top));
        } else if is(name, "entity") {
            jobs.push(Job::Entity(EntityJob::Top(top)));
        } else if is(name, "world") {
            jobs.push(Job::World(top));
        } else if is(name, "visgroups") {
            let block = top.into_block()?;
            visgroups.extend(
                block.children.into_iter().filter(|c| is(&c.name, "visgroup")).map(|c| visgroup_from_block(c, 2)),
            );
        } else if is(name, "hidden") {
            for child in top.children {
                if is(child.name(), "solid") {
                    jobs.push(Job::Solid(child, true));
                } else if is(child.name(), "entity") {
                    jobs.push(Job::Entity(EntityJob::Hidden(child)));
                }
            }
        }
    }
    type Part = (Vec<Solid>, Option<Entity>, Vec<Group>);
    let parts = jobs
        .into_par_iter()
        .map(|job| -> Result<Part, VmfError> {
            Ok(match job {
                Job::SolidTop(top) => (vec![solid_from_block(top.into_block()?, false)], None, Vec::new()),
                Job::Solid(child, hidden) => (vec![solid_from_child(child, hidden)?], None, Vec::new()),
                Job::Entity(job) => (Vec::new(), Some(job.build()?), Vec::new()),
                Job::World(top) => {
                    let world = entity_from_top(top, true, false)?;
                    (world.solids, None, world.groups)
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut frag = Fragment { visgroups, ..Fragment::default() };
    for (s, e, g) in parts {
        frag.solids.extend(s);
        frag.entities.extend(e);
        frag.groups.extend(g);
    }
    Ok(frag)
}

enum Item<'a> {
    Solid(Solid),
    Hidden(Vec<Solid>, Option<Block>),
    Child(Child<'a>),
}

fn classify(child: Child<'_>) -> Result<Item<'_>, VmfError> {
    let name = child.name();
    if is(name, "solid") {
        Ok(Item::Solid(solid_from_child(child, false)?))
    } else if is(name, "hidden") {
        Ok(match child {
            Child::Span { name, text } => {
                let root = hp_kv::parse_ref(text);
                let mut block = root.children.into_iter().next().unwrap_or_default();
                block.name = name;
                hidden_solids(&block)
            }
            Child::Parsed(b) => hidden_solids(&b.as_ref()),
        })
    } else {
        Ok(Item::Child(child))
    }
}

fn solid_from_child(child: Child<'_>, hidden: bool) -> Result<Solid, VmfError> {
    Ok(match child {
        Child::Span { text, .. } => {
            let root = hp_kv::parse_ref(text);
            match root.children.first() {
                Some(block) => solid_from_ref(block, hidden),
                None => solid_from_ref(&BlockRef::default(), hidden),
            }
        }
        Child::Parsed(b) => solid_from_block(b, hidden),
    })
}

fn hidden_solids<'a>(b: &BlockRef<'_>) -> Item<'a> {
    let mut solids = Vec::new();
    let mut rest = Vec::new();
    for c in &b.children {
        if is(c.name, "solid") {
            solids.push(solid_from_ref(c, true));
        } else {
            rest.push(c.to_block());
        }
    }
    let residual = (!b.props.is_empty() || !rest.is_empty()).then(|| Block {
        name: b.name.to_owned(),
        props: b.props.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
        children: rest,
    });
    Item::Hidden(solids, residual)
}

fn entity_from_top(top: Top<'_>, is_world: bool, hidden: bool) -> Result<Entity, VmfError> {
    let Top { props, children, .. } = top;
    let items: Vec<Item> = if children.len() >= PAR_CHILDREN {
        children.into_par_iter().with_min_len(16).map(classify).collect::<Result<_, _>>()?
    } else {
        children.into_iter().map(classify).collect::<Result<_, _>>()?
    };
    assemble_entity(props, items, is_world, hidden)
}

fn assemble_entity(
    props: Vec<(String, String)>,
    items: Vec<Item<'_>>,
    is_world: bool,
    hidden: bool,
) -> Result<Entity, VmfError> {
    let mut e = Entity { hidden, ..Entity::default() };
    e.props.reserve(props.len());
    for (k, v) in props {
        if is(&k, "id") {
            if let Some(id) = parse_u32(&v) {
                e.id = id;
            }
        } else if is(&k, "classname") {
            e.classname = v;
        } else if is_world && is(&k, "mapversion") {
        } else {
            e.props.push((k, v));
        }
    }
    let solid_count = items
        .iter()
        .map(|i| match i {
            Item::Solid(_) => 1,
            Item::Hidden(s, _) => s.len(),
            Item::Child(_) => 0,
        })
        .sum();
    e.solids.reserve_exact(solid_count);
    let mut have_editor = false;
    for item in items {
        match item {
            Item::Solid(s) => e.solids.push(s),
            Item::Hidden(solids, residual) => {
                e.solids.extend(solids);
                e.extra_blocks.extend(residual);
            }
            Item::Child(child) => {
                let name = child.name();
                if is(name, "connections") {
                    with_ref(&child, |b| {
                        e.connections.extend(b.props.iter().map(|(k, v)| parse_connection((*k).to_owned(), v)));
                    });
                } else if is(name, "group") {
                    e.groups.push(with_ref(&child, group_from_ref));
                } else if is(name, "editor") && !have_editor {
                    e.editor = with_ref(&child, editor_from_ref);
                    have_editor = true;
                } else {
                    e.extra_blocks.push(child.into_block()?);
                }
            }
        }
    }
    Ok(e)
}

fn with_ref<R>(child: &Child<'_>, f: impl FnOnce(&BlockRef<'_>) -> R) -> R {
    match child {
        Child::Span { text, .. } => {
            let root = hp_kv::parse_ref(text);
            match root.children.first() {
                Some(block) => f(block),
                None => f(&BlockRef::default()),
            }
        }
        Child::Parsed(b) => f(&b.as_ref()),
    }
}

pub(crate) fn parse_connection(output: String, value: &str) -> Connection {
    let separator = if value.contains(CONNECTION_ESC) { CONNECTION_ESC } else { ',' };
    let parts: Vec<&str> = value.split(separator).collect();
    let field = |i: usize| parts.get(i).copied().unwrap_or("");
    let n = parts.len();
    let mut c = Connection { output, separator, ..Connection::default() };
    field(0).clone_into(&mut c.target);
    field(1).clone_into(&mut c.input);
    if n > 5 {
        let mut sep_buf = [0u8; 4];
        let sep_str: &str = separator.encode_utf8(&mut sep_buf);
        c.param = parts.get(2..n - 2).map(|p| p.join(sep_str)).unwrap_or_default();
        c.delay = parse_f32_prefix(field(n - 2)).unwrap_or(0.0);
        c.times = parse_i32_prefix(field(n - 1)).unwrap_or(-1);
    } else {
        field(2).clone_into(&mut c.param);
        c.delay = parse_f32_prefix(field(3)).unwrap_or(0.0);
        c.times = parse_i32_prefix(field(4)).unwrap_or(-1);
    }
    c
}

fn group_from_ref(b: &BlockRef<'_>) -> Group {
    let mut g = Group::default();
    if let Some(id) = b.get("id").and_then(parse_u32) {
        g.id = id;
    }
    if let Some(editor) = b.children.iter().find(|c| is(c.name, "editor")) {
        g.editor = editor_from_ref(editor);
    }
    g
}

fn parse_color(s: &str) -> Option<[u8; 3]> {
    let mut out = [0u8; 3];
    let mut it = num_tokens(s);
    for slot in &mut out {
        let v = parse_f64(it.next()?)?;
        *slot = v.clamp(0.0, 255.0) as u8;
    }
    it.next().is_none().then_some(out)
}

#[cfg(test)]
pub(crate) fn editor_from_block(b: Block) -> EditorInfo {
    editor_from_ref(&b.as_ref())
}

fn editor_from_ref(b: &BlockRef<'_>) -> EditorInfo {
    let mut e = EditorInfo::default();
    for &(k, v) in &b.props {
        let ok = match k.len() {
            5 if is(k, "color") => set(&mut e.color, parse_color(v)),
            7 if is(k, "groupid") => set(&mut e.group_id, parse_u32(v).map(Some)),
            8 if is(k, "comments") => {
                e.comments = Some(v.to_owned());
                continue;
            }
            10 if is(k, "visgroupid") => match parse_u32(v) {
                Some(id) => {
                    e.visgroup_ids.push(id);
                    true
                }
                None => false,
            },
            10 if is(k, "logicalpos") => {
                e.logical_pos = Some(v.to_owned());
                continue;
            }
            13 if is(k, "visgroupshown") => set(&mut e.visgroup_shown, parse_bool(v)),
            17 if is(k, "visgroupautoshown") => set(&mut e.visgroup_auto_shown, parse_bool(v)),
            _ => false,
        };
        if !ok {
            e.extra_props.push((k.to_owned(), v.to_owned()));
        }
    }
    e
}

pub(crate) fn solid_from_block(b: Block, hidden: bool) -> Solid {
    solid_from_ref(&b.as_ref(), hidden)
}

fn solid_from_ref(b: &BlockRef<'_>, hidden: bool) -> Solid {
    let mut s = Solid {
        id: 0,
        sides: Vec::with_capacity(b.children.len()),
        editor: EditorInfo::default(),
        hidden,
        extra_props: Vec::new(),
        extra_blocks: Vec::new(),
    };
    for &(k, v) in &b.props {
        if is(k, "id") {
            if let Some(id) = parse_u32(v) {
                s.id = id;
            }
            continue;
        }
        s.extra_props.push((k.to_owned(), v.to_owned()));
    }
    let mut have_editor = false;
    for c in &b.children {
        if is(c.name, "side") {
            s.sides.push(side_from_ref(c));
        } else if is(c.name, "editor") && !have_editor {
            s.editor = editor_from_ref(c);
            have_editor = true;
        } else {
            s.extra_blocks.push(c.to_block());
        }
    }
    s
}

fn parse_plane(s: &str) -> Option<[DVec3; 3]> {
    let [ax, ay, az, bx, by, bz, cx, cy, cz] = parse_fixed::<9>(s)?;
    Some([DVec3::new(ax, ay, az), DVec3::new(bx, by, bz), DVec3::new(cx, cy, cz)])
}

fn parse_axis(s: &str) -> Option<TexAxis> {
    let [x, y, z, shift, scale] = parse_fixed::<5>(s)?;
    Some(TexAxis { axis: DVec3::new(x, y, z), shift, scale })
}

#[inline]
fn set_or<T>(slot: &mut T, value: Option<T>, unread: T) -> bool {
    let ok = value.is_some();
    *slot = value.unwrap_or(unread);
    ok
}

#[cfg(test)]
pub(crate) fn side_from_block(b: Block) -> Side {
    side_from_ref(&b.as_ref())
}

fn side_from_ref(b: &BlockRef<'_>) -> Side {
    let mut side = Side {
        id: 0,
        plane: Side::UNREADABLE_PLANE,
        material: String::new(),
        uaxis: TexAxis { axis: DVec3::X, shift: 0.0, scale: 0.25 },
        vaxis: TexAxis { axis: DVec3::NEG_Y, shift: 0.0, scale: 0.25 },
        rotation: 0.0,
        lightmap_scale: 16,
        smoothing_groups: 0,
        disp: None,
        vertices_plus: None,
        extra_props: Vec::new(),
        extra_blocks: Vec::new(),
    };
    for &(k, v) in &b.props {
        let ok = match k.len() {
            2 if is(k, "id") => {
                set(&mut side.id, parse_u32(v));
                continue;
            }
            5 if is(k, "plane") => set_or(&mut side.plane, parse_plane(v), Side::UNREADABLE_PLANE),
            5 if is(k, "uaxis") => set_or(&mut side.uaxis, parse_axis(v), UNREADABLE_AXIS),
            5 if is(k, "vaxis") => set_or(&mut side.vaxis, parse_axis(v), UNREADABLE_AXIS),
            8 if is(k, "material") => {
                v.clone_into(&mut side.material);
                continue;
            }
            8 if is(k, "rotation") => set(&mut side.rotation, parse_f64(v)),
            13 if is(k, "lightmapscale") => set(&mut side.lightmap_scale, parse_i32(v)),
            16 if is(k, "smoothing_groups") => set(&mut side.smoothing_groups, parse_u32(v)),
            _ => false,
        };
        if !ok {
            side.extra_props.push((k.to_owned(), v.to_owned()));
        }
    }
    for c in &b.children {
        if is(c.name, "dispinfo") && side.disp.is_none() {
            match disp_from_ref(c, side.normal()) {
                Some(d) => side.disp = Some(Box::new(d)),
                None => side.extra_blocks.push(c.to_block()),
            }
        } else if is(c.name, "vertices_plus") && side.vertices_plus.is_none() {
            match vertices_plus_from_ref(c) {
                Some(v) => side.vertices_plus = Some(v),
                None => side.extra_blocks.push(c.to_block()),
            }
        } else {
            side.extra_blocks.push(c.to_block());
        }
    }
    side
}

fn vertices_plus_from_ref(b: &BlockRef<'_>) -> Option<Vec<DVec3>> {
    if !b.children.is_empty() {
        return None;
    }
    let mut verts = Vec::with_capacity(b.props.len());
    for &(k, v) in &b.props {
        match parse_vec3_strict(v) {
            Some(p) if is(k, "v") => verts.push(p),
            _ => return None,
        }
    }
    Some(verts)
}

fn row_index(key: &str) -> Option<usize> {
    let head = key.get(..3)?;
    if !head.eq_ignore_ascii_case("row") {
        return None;
    }
    key.get(3..)?.parse().ok()
}

fn fill_f64_grid(grid: &mut [f64], b: &BlockRef<'_>, rows: usize, cols: usize) {
    for &(k, v) in &b.props {
        let Some(r) = row_index(k).filter(|r| *r < rows) else { continue };
        let row = grid.iter_mut().skip(r * cols).take(cols);
        for (slot, tok) in row.zip(num_tokens(v)) {
            if let Some(x) = parse_f64(tok) {
                *slot = x;
            }
        }
    }
}

fn fill_i32_grid(grid: &mut [i32], b: &BlockRef<'_>, rows: usize, cols: usize) {
    for &(k, v) in &b.props {
        let Some(r) = row_index(k).filter(|r| *r < rows) else { continue };
        let row = grid.iter_mut().skip(r * cols).take(cols);
        for (slot, tok) in row.zip(num_tokens(v)) {
            if let Some(x) = parse_i32(tok) {
                *slot = x;
            }
        }
    }
}

fn fill_vec3_grid(grid: &mut [DVec3], b: &BlockRef<'_>, rows: usize, cols: usize) {
    for &(k, v) in &b.props {
        let Some(r) = row_index(k).filter(|r| *r < rows) else { continue };
        let mut toks = num_tokens(v);
        'row: for slot in grid.iter_mut().skip(r * cols).take(cols) {
            let mut arr = slot.to_array();
            for c in &mut arr {
                let Some(tok) = toks.next() else {
                    *slot = DVec3::from_array(arr);
                    break 'row;
                };
                if let Some(x) = parse_f64(tok) {
                    *c = x;
                }
            }
            *slot = DVec3::from_array(arr);
        }
    }
}

fn disp_from_ref(b: &BlockRef<'_>, face_normal: DVec3) -> Option<DispInfo> {
    let power = match b.get("power").and_then(parse_u32) {
        Some(p) if p <= MAX_DISP_POWER => p,
        _ => return None,
    };
    let n = 1usize << power;
    let w = n + 1;
    let count = w * w;
    let mut d = DispInfo {
        power,
        start_position: DVec3::ZERO,
        flags: 0,
        elevation: 0.0,
        subdiv: false,
        normals: vec![face_normal; count],
        distances: vec![0.0; count],
        offsets: vec![DVec3::ZERO; count],
        offset_normals: vec![face_normal; count],
        alphas: vec![0.0; count],
        triangle_tags: vec![0; 2 * n * n],
        allowed_verts: vec![-1; 10],
        extra_props: Vec::new(),
        extra_blocks: Vec::new(),
    };
    let mut seen_power = false;
    for &(k, v) in &b.props {
        let ok = if is(k, "power") {
            let first = !seen_power && parse_u32(v) == Some(power);
            seen_power |= first;
            first
        } else if is(k, "startposition") {
            set(&mut d.start_position, parse_vec3_strict(v))
        } else if is(k, "flags") {
            set(&mut d.flags, parse_i32(v))
        } else if is(k, "elevation") {
            set(&mut d.elevation, parse_f64(v))
        } else if is(k, "subdiv") {
            set(&mut d.subdiv, parse_bool(v))
        } else {
            false
        };
        if !ok {
            d.extra_props.push((k.to_owned(), v.to_owned()));
        }
    }
    for c in &b.children {
        let name = c.name;
        if is(name, "normals") {
            fill_vec3_grid(&mut d.normals, c, w, w);
        } else if is(name, "distances") {
            fill_f64_grid(&mut d.distances, c, w, w);
        } else if is(name, "offsets") {
            fill_vec3_grid(&mut d.offsets, c, w, w);
        } else if is(name, "offset_normals") {
            fill_vec3_grid(&mut d.offset_normals, c, w, w);
        } else if is(name, "alphas") {
            fill_f64_grid(&mut d.alphas, c, w, w);
        } else if is(name, "triangle_tags") {
            fill_i32_grid(&mut d.triangle_tags, c, n, 2 * n);
        } else if is(name, "allowed_verts") {
            let value = c.get("10").or_else(|| c.props.first().map(|(_, v)| *v)).unwrap_or("");
            for (slot, tok) in d.allowed_verts.iter_mut().zip(num_tokens(value)) {
                if let Some(x) = parse_i32(tok) {
                    *slot = x;
                }
            }
        } else {
            d.extra_blocks.push(c.to_block());
        }
    }
    Some(d)
}

fn version_info_from_block(b: Block) -> VersionInfo {
    let mut vi = VersionInfo::default();
    for (k, v) in b.props {
        let ok = if is(&k, "editorversion") {
            set(&mut vi.editor_version, parse_i32(&v))
        } else if is(&k, "editorbuild") {
            set(&mut vi.editor_build, parse_i32(&v))
        } else if is(&k, "mapversion") {
            set(&mut vi.map_version, parse_i32(&v))
        } else if is(&k, "formatversion") {
            set(&mut vi.format_version, parse_i32(&v))
        } else if is(&k, "prefab") {
            set(&mut vi.prefab, parse_bool(&v))
        } else {
            false
        };
        if !ok {
            vi.extra_props.push((k, v));
        }
    }
    vi
}

fn visgroup_from_block(b: Block, depth: usize) -> VisGroup {
    let mut g = VisGroup::default();
    for (k, v) in b.props {
        let ok = if is(&k, "name") {
            g.name = v;
            continue;
        } else if is(&k, "visgroupid") {
            set(&mut g.id, parse_u32(&v))
        } else if is(&k, "color") {
            set(&mut g.color, parse_color(&v))
        } else {
            false
        };
        if !ok {
            g.extra_props.push((k, v));
        }
    }
    if depth < MAX_NESTING {
        g.children = b
            .children
            .into_iter()
            .filter(|c| is(&c.name, "visgroup"))
            .map(|c| visgroup_from_block(c, depth + 1))
            .collect();
    }
    g
}

fn vec3_or(b: &Block, key: &str, default: DVec3) -> DVec3 {
    b.get(key).and_then(parse_vec3_strict).unwrap_or(default)
}

fn cameras_from_block(b: &Block) -> Cameras {
    Cameras {
        active: b.get("activecamera").and_then(parse_i32).unwrap_or(-1),
        cameras: b
            .children_named("camera")
            .map(|c| Camera { position: vec3_or(c, "position", DVec3::ZERO), look: vec3_or(c, "look", DVec3::ZERO) })
            .collect(),
    }
}

fn legacy_cordon(cordons: &mut Cordons, b: &Block) {
    cordons.style = CordonStyle::Legacy;
    cordons.active = b.get("active").and_then(parse_bool).unwrap_or(false);
    cordons.cordons.push(Cordon {
        name: "cordon".to_owned(),
        active: true,
        mins: vec3_or(b, "mins", DVec3::splat(-1024.0)),
        maxs: vec3_or(b, "maxs", DVec3::splat(1024.0)),
    });
}

fn modern_cordons(cordons: &mut Cordons, b: &Block) {
    cordons.style = CordonStyle::Modern;
    cordons.active = b.get("active").and_then(parse_bool).unwrap_or(false);
    for c in b.children_named("cordon") {
        let index = cordons.cordons.len();
        let mut boxes =
            c.children_named("box").map(|bx| (vec3_or(bx, "mins", DVec3::ZERO), vec3_or(bx, "maxs", DVec3::ZERO)));
        let (mins, maxs) = boxes.next().unwrap_or((DVec3::ZERO, DVec3::ZERO));
        cordons.cordons.push(Cordon {
            name: c.get("name").unwrap_or_default().to_owned(),
            active: c.get("active").and_then(parse_bool).unwrap_or(true),
            mins,
            maxs,
        });
        cordons.extra_boxes.extend(boxes.map(|(mins, maxs)| CordonBox { cordon: index, mins, maxs }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(src: &str) -> Block {
        hp_kv::parse(src).unwrap().children.into_iter().next().unwrap()
    }

    #[test]
    fn side_fields() {
        let s = side_from_block(block(
            r#"side {
                "id" "631"
                "plane" "(-480 480 16) (-352 480 16) (-352 352 16)"
                "material" "DEV/DEV_MEASUREGENERIC01"
                "uaxis" "[1 0 0 -8.5] 0.25"
                "vaxis" "[0 -1 0 0] 0.5"
                "rotation" "15"
                "lightmapscale" "32"
                "smoothing_groups" "3"
                "custom" "kept"
            }"#,
        ));
        assert_eq!(s.id, 631);
        assert_eq!(s.plane[1], DVec3::new(-352.0, 480.0, 16.0));
        assert_eq!(s.material, "DEV/DEV_MEASUREGENERIC01");
        assert_eq!(s.uaxis, TexAxis { axis: DVec3::X, shift: -8.5, scale: 0.25 });
        assert_eq!(s.vaxis.scale, 0.5);
        assert_eq!(s.rotation, 15.0);
        assert_eq!(s.lightmap_scale, 32);
        assert_eq!(s.smoothing_groups, 3);
        assert_eq!(s.extra_props, vec![("custom".to_owned(), "kept".to_owned())]);
        assert_eq!(s.normal(), DVec3::Z);
        assert_eq!(s.plane_distance(), 16.0);
    }

    #[test]
    fn invalid_values_are_kept_raw() {
        let s = side_from_block(block(r#"side { "id" "1" "plane" "(0 0 0) (1 0" "lightmapscale" "big" }"#));
        assert_eq!(s.plane, Side::UNREADABLE_PLANE);
        assert_eq!(s.lightmap_scale, 16);
        assert_eq!(
            s.extra_props,
            vec![("plane".to_owned(), "(0 0 0) (1 0".to_owned()), ("lightmapscale".to_owned(), "big".to_owned())]
        );
        let s = side_from_block(block(
            r#"side { "id" "x" "plane" "(-1.#IND -1.#IND -1.#IND) (0 0 0) (1 0 0)" "uaxis" "[1.#QNAN 0 0 0] 0.25" }"#,
        ));
        assert_eq!(s.id, 0, "an invalid id is dropped");
        assert_eq!(s.plane, Side::UNREADABLE_PLANE);
        assert_eq!(s.uaxis, UNREADABLE_AXIS);
        assert_eq!(s.vaxis.axis, DVec3::NEG_Y);
        assert_eq!(s.extra_props.len(), 2);
        assert_eq!(side_from_block(block(r#"side { "id" "2" }"#)).plane, Side::UNREADABLE_PLANE);
        let solid = solid_from_block(block(r#"solid { "id" "abc" "x" "y" }"#), false);
        assert_eq!((solid.id, solid.extra_props.len()), (0, 1));
    }

    #[test]
    fn vertices_plus() {
        let s = side_from_block(block(
            r#"side { "id" "1" vertices_plus { "v" "-64 -64 64" "v" "-64 64 64" "v" "64 64 64" } }"#,
        ));
        assert_eq!(
            s.vertices_plus,
            Some(vec![DVec3::new(-64., -64., 64.), DVec3::new(-64., 64., 64.), DVec3::new(64., 64., 64.)])
        );
        let s = side_from_block(block(r#"side { vertices_plus { "v" "1 2" "x" "y" } }"#));
        assert_eq!(s.vertices_plus, None);
        assert_eq!(s.extra_blocks.len(), 1);
    }

    #[test]
    fn displacement_lenient() {
        let s = side_from_block(block(
            r#"side {
                "plane" "(0 0 0) (1 1 0) (0 1 0)"
                dispinfo {
                    "power" "2"
                    "startposition" "[-64 -64 0]"
                    "flags" "4"
                    "elevation" "1.5"
                    "subdiv" "1"
                    normals { "row0" "0 0 1 0 0 1 0 0 1 0 0 1 0 0 1 9 9 9" "row9" "1 1 1" }
                    distances { "row1" "1 2 3" "row4" "5 5 5 5 5" }
                    alphas { "row2" "255 255 255 255 255" }
                    triangle_tags { "row0" "9 9 9 9 9 9 9 9" "row3" "1 1" }
                    allowed_verts { "10" "-1 -1 -1 -1 -1 -1 -1 -1 -1 -1" }
                    multiblend { "row0" "1" }
                }
            }"#,
        ));
        let d = s.disp.as_ref().unwrap();
        assert_eq!(d.power, 2);
        assert_eq!(d.row_len(), 5);
        assert_eq!(d.start_position, DVec3::new(-64., -64., 0.));
        assert_eq!((d.flags, d.elevation, d.subdiv), (4, 1.5, true));
        assert_eq!(d.normals.len(), 25);
        let face = s.normal();
        assert_eq!(face, DVec3::NEG_Z);
        assert_eq!(d.normals[0], DVec3::Z);
        assert_eq!(d.normals[5], face, "missing rows use the face normal");
        assert_eq!(&d.distances[5..10], &[1., 2., 3., 0., 0.]);
        assert_eq!(&d.distances[20..25], &[5.; 5]);
        assert_eq!(d.alphas[10], 255.0);
        assert_eq!(d.triangle_tags.len(), 32);
        assert_eq!(&d.triangle_tags[24..27], &[1, 1, 0]);
        assert_eq!(d.allowed_verts, vec![-1; 10]);
        assert_eq!(d.extra_blocks.len(), 1);
        assert_eq!(d.extra_blocks[0].name, "multiblend");
    }

    #[test]
    fn displacement_bad_power_kept_raw() {
        let s = side_from_block(block(r#"side { dispinfo { "power" "30" } }"#));
        assert!(s.disp.is_none());
        assert_eq!(s.extra_blocks[0].name, "dispinfo");
        let s = side_from_block(block(r#"side { dispinfo { "startposition" "[0 0 0]" } }"#));
        assert!(s.disp.is_none());
    }

    #[test]
    fn connections_comma_and_esc() {
        let c = parse_connection("OnTrigger".into(), "door,Open,,0.5,-1");
        assert_eq!((c.target.as_str(), c.input.as_str(), c.param.as_str()), ("door", "Open", ""));
        assert_eq!((c.delay, c.times, c.separator), (0.5, -1, ','));
        let c = parse_connection("OnPressed".into(), "prop\x1bSkin\x1b2,3\x1b1.9\x1b1");
        assert_eq!((c.param.as_str(), c.delay, c.times, c.separator), ("2,3", 1.9, 1, '\x1b'));
        let c = parse_connection("OnX".into(), "t,In");
        assert_eq!((c.param.as_str(), c.delay, c.times), ("", 0.0, -1));
        let c = parse_connection("OnX".into(), "t,AddOutput,a,b,c,2,3");
        assert_eq!((c.param.as_str(), c.delay, c.times), ("a,b,c", 2.0, 3));
        let c = parse_connection("OnX".into(), "t,In,p,0.25s,x");
        assert_eq!((c.delay, c.times), (0.25, -1));
    }

    #[test]
    fn editor_block() {
        let e = editor_from_block(block(
            r#"editor { "color" "220 30 220" "visgroupid" "5" "visgroupid" "7" "groupid" "31"
               "visgroupshown" "0" "visgroupautoshown" "1" "comments" "hi" "logicalpos" "[0 500]" "x" "y" }"#,
        ));
        assert_eq!(e.color, [220, 30, 220]);
        assert_eq!(e.visgroup_ids, vec![5, 7]);
        assert_eq!(e.group_id, Some(31));
        assert!(!e.visgroup_shown && e.visgroup_auto_shown);
        assert_eq!(e.comments.as_deref(), Some("hi"));
        assert_eq!(e.logical_pos.as_deref(), Some("[0 500]"));
        assert_eq!(e.extra_props.len(), 1);
    }

    #[test]
    fn cordon_styles() {
        let mut c = Cordons::default();
        legacy_cordon(&mut c, &block(r#"cordon { "mins" "(-1 -2 -3)" "maxs" "(1 2 3)" "active" "1" }"#));
        assert_eq!(c.style, CordonStyle::Legacy);
        assert!(c.active);
        assert_eq!(c.cordons[0].maxs, DVec3::new(1., 2., 3.));

        let mut c = Cordons::default();
        modern_cordons(
            &mut c,
            &block(
                r#"cordons { "active" "1" cordon { "name" "a" "active" "0" box { "mins" "(0 0 0)" "maxs" "(1 1 1)" }
                   box { "mins" "(5 5 5)" "maxs" "(6 6 6)" } } cordon { "name" "b" } }"#,
            ),
        );
        assert_eq!(c.style, CordonStyle::Modern);
        assert_eq!(c.cordons.len(), 2);
        assert!(!c.cordons[0].active && c.cordons[1].active);
        assert_eq!(c.extra_boxes, vec![CordonBox { cordon: 0, mins: DVec3::splat(5.), maxs: DVec3::splat(6.) }]);
    }

    #[test]
    fn nested_visgroups() {
        let g = visgroup_from_block(
            block(
                r#"visgroup { "name" "A" "visgroupid" "1" "color" "1 2 3" visgroup { "name" "B" "visgroupid" "2" } }"#,
            ),
            2,
        );
        assert_eq!((g.name.as_str(), g.id, g.color), ("A", 1, [1, 2, 3]));
        assert_eq!(g.children[0].name, "B");
    }
}
