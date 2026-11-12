use glam::{DMat4, DVec2, DVec3};
use hp_app::AppState;
use hp_app::runtime::{RealDims, UnitDims};
use hp_editor::{ObjRef, Owner};
use hp_geom::Aabb;
use hp_shape::TriMesh;

use super::place_ops;
use super::place_scan::{self, Class, ScanOpts};
use super::place_world::{self as pw, EnvOptions, Key, MeshChoice, Obj, World};
use super::tools_place::{self as tp, Fit, PlaceSpec, Unit};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum UiPlace {
    OnTop,
    NextTo,
    Against { dir: DVec3, away: bool },
    Fit,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PlaceReport {
    pub(crate) moved: usize,
    pub(crate) units: usize,
    pub(crate) gap: Option<f64>,
    pub(crate) overlaps: usize,
    pub(crate) stayed: Vec<String>,
    pub(crate) other_landing: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SettleReport {
    pub(crate) floating: usize,
    pub(crate) sunk: usize,
    pub(crate) checked: usize,
}

impl SettleReport {
    pub(crate) fn moved(&self) -> usize {
        self.floating + self.sunk
    }
}

fn spec(mode: &str) -> PlaceSpec {
    PlaceSpec {
        mode: mode.to_owned(),
        position: None,
        anchor: None,
        rotate_yaw: 0.0,
        side: "+x".to_owned(),
        align: "center".to_owned(),
        direction: None,
        facing: "keep".to_owned(),
        align_to_surface: false,
        then_drop: None,
        gap: 0.0,
        max: 4096.0,
        choice: MeshChoice::Render,
        pre: DMat4::IDENTITY,
        fit: Fit::Normal,
    }
}

fn key_of(state: &AppState, r: ObjRef) -> Key {
    match r {
        ObjRef::Entity(id) => Key::Entity(id),
        ObjRef::Solid(id) => match state.doc.scene().solid(id).map(|n| n.owner) {
            Some(Owner::Entity(e)) => Key::Entity(e),
            _ => Key::Solid(id),
        },
    }
}

fn obj_of_key(state: &AppState, k: Key) -> Option<Obj> {
    match k {
        Key::Entity(id) => pw::entity_obj(state, id, false),
        Key::Solid(id) => pw::solid_obj(state, id, false),
        Key::Group(id) => pw::group_obj(state, id, false),
    }
}

fn apply(
    state: &mut AppState,
    label: &str,
    f: impl FnOnce(&mut hp_editor::Document) -> hp_editor::EditResult<()>,
) -> Result<(), String> {
    if state.doc.in_transaction() {
        return f(&mut state.doc).map_err(|e| e.to_string());
    }
    state.doc.transaction(label.to_owned(), f).map_err(|e| e.to_string())
}

fn side_toward(from: &Aabb, to: &Aabb) -> &'static str {
    let d = from.center() - to.center();
    if d.x.abs() >= d.y.abs() {
        if d.x >= 0.0 { "+x" } else { "-x" }
    } else if d.y >= 0.0 {
        "+y"
    } else {
        "-y"
    }
}

fn place_compound(
    state: &mut AppState,
    movers: &[ObjRef],
    target: ObjRef,
    on_top: bool,
    label: &str,
) -> Result<PlaceReport, String> {
    let units = tp::units_from(state, Vec::new(), movers.to_vec(), false, false);
    if units.is_empty() {
        return Err("nothing to move".to_owned());
    }
    let tkey = key_of(state, target);
    let refs: Vec<ObjRef> = units.iter().flat_map(|u| u.refs.clone()).collect();
    if units.iter().any(|u| u.refs.iter().any(|r| key_of(state, *r) == tkey)) {
        return Err("the target is one of the moved objects".to_owned());
    }
    let n = units.len();
    let obj = if n == 1 {
        units.into_iter().next().map(|u| u.obj).ok_or("no object")?
    } else {
        pw::compound(state, Key::Group(0), &refs, false).ok_or("no object")?
    };
    let tobj = obj_of_key(state, tkey).ok_or("the target has no geometry")?;
    let mut sp = spec(if on_top { "on" } else { "next_to" });
    if !on_top {
        sp.side = side_toward(&obj.hint, &tobj.hint).to_owned();
    }
    let size = obj.hint.size().max_element().max(16.0);
    let mut region = tp::region_around(&obj.hint, DVec3::NEG_Z, sp.max);
    region = region.union(&tobj.hint.expand(size * 2.0 + 64.0));
    region = region.union(&tp::region_around(&tobj.hint.expand(size + 16.0), DVec3::NEG_Z, sp.max));
    let mut env = tp::env_for(state, &refs, Some(region), false, &[]);
    place_scan::drop_helpers(state, &mut env);
    if !env.iter().any(|o| o.key == tkey) {
        env.push(tobj);
    }
    let fs = state.assets.fs().cloned();
    let r = pw::resolve(&obj, fs.as_ref(), MeshChoice::Render);
    let moving = r.compound();
    let world = World::build(env, fs.as_ref(), MeshChoice::Render, None);
    let tidx = world.objs.iter().position(|o| o.key == tkey).ok_or("the target has no geometry")?;
    let placed = tp::compute_place(&obj, &moving, &world, Some(tidx), &sp, &[])?;
    let fin = moving.transformed(&placed.xf);
    let check = tp::verify(&fin, &world);
    let gap = check["gap_below"].as_f64();
    let overlaps = check["overlaps"].as_array().map_or(0, Vec::len);
    let other_landing = placed.report.get("warning").is_some();
    let xf = placed.xf;
    if xf == DMat4::IDENTITY {
        return Ok(PlaceReport { moved: 0, units: n, gap, overlaps, stayed: Vec::new(), other_landing });
    }
    apply(state, label, |doc| tp::apply_moves(doc, &vec![(refs, xf)]))?;
    Ok(PlaceReport { moved: n, units: n, gap, overlaps, stayed: Vec::new(), other_landing })
}

fn place_each(state: &mut AppState, movers: &[ObjRef], sp: PlaceSpec, label: &str) -> Result<PlaceReport, String> {
    let units = tp::units_from(state, Vec::new(), movers.to_vec(), false, true);
    if units.is_empty() {
        return Err("nothing to move".to_owned());
    }
    let n = units.len();
    let mut prep = tp::drop_prepare(state, units, false, sp);
    prep.verify = true;
    let fs = state.assets.fs().cloned();
    let (moves, report) = tp::drop_run(prep, fs.as_ref());
    let results = report["results"].as_array().cloned().unwrap_or_default();
    let gaps: Vec<f64> = results.iter().filter_map(|v| v["gap_after"].as_f64()).collect();
    let gap = if gaps.is_empty() { None } else { Some(gaps.iter().fold(f64::NEG_INFINITY, |a, b| a.max(*b))) };
    let overlaps = results.iter().filter_map(|v| v["overlaps_after"].as_u64()).sum::<u64>() as usize;
    let stayed: Vec<String> = report["stayed"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v["reason"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    let moved = moves.len();
    if moved > 0 {
        apply(state, label, |doc| tp::apply_moves(doc, &moves))?;
    }
    Ok(PlaceReport { moved, units: n, gap, overlaps, stayed, other_landing: false })
}

pub(crate) fn place(
    state: &mut AppState,
    movers: &[ObjRef],
    target: Option<ObjRef>,
    how: UiPlace,
    label: &str,
) -> Result<PlaceReport, String> {
    match how {
        UiPlace::OnTop | UiPlace::NextTo => {
            let t = target.ok_or("select the objects to move, then the target last")?;
            place_compound(state, movers, t, how == UiPlace::OnTop, label)
        }
        UiPlace::Against { dir, away } => {
            let mut sp = spec("against");
            sp.direction = Some(dir);
            sp.facing = if away { "away" } else { "keep" }.to_owned();
            sp.max = 4096.0;
            place_each(state, movers, sp, label)
        }
        UiPlace::Fit => {
            let mut sp = tp::drop_spec(DVec3::NEG_Z, false, 0.0, 16384.0, MeshChoice::Render);
            sp.fit = Fit::Supports;
            place_each(state, movers, sp, label)
        }
    }
}

pub(crate) fn nearest_wall(state: &AppState, movers: &[ObjRef]) -> Option<DVec3> {
    let units = tp::units_from(state, Vec::new(), movers.to_vec(), false, true);
    let refs: Vec<ObjRef> = units.iter().flat_map(|u| u.refs.clone()).collect();
    let hint = units.iter().fold(Aabb::EMPTY, |a, u| a.union(&u.obj.hint));
    if hint.is_empty() {
        return None;
    }
    let reach = 1024.0;
    let mut env = tp::env_for(state, &refs, Some(hint.expand(reach + 64.0)), false, &[]);
    place_scan::drop_helpers(state, &mut env);
    let fs = state.assets.fs().cloned();
    let world = World::build(env, fs.as_ref(), MeshChoice::Render, None);
    let mut moving = TriMesh::new();
    for u in &units {
        moving.append(&pw::resolve(&u.obj, fs.as_ref(), MeshChoice::Render).compound());
    }
    let mut best: Option<(f64, DVec3)> = None;
    for d in [DVec3::X, DVec3::NEG_X, DVec3::Y, DVec3::NEG_Y] {
        if let Some((t, _)) = place_ops::clearance(&moving, &world, d, reach, &[])
            && best.is_none_or(|b| t < b.0)
        {
            best = Some((t, d));
        }
    }
    best.map(|b| b.1)
}

fn settle_world(state: &AppState, units: Vec<Unit>) -> (World, Vec<usize>) {
    let refs: Vec<ObjRef> = units.iter().flat_map(|u| u.refs.clone()).collect();
    let region = units
        .iter()
        .fold(Aabb::EMPTY, |a, u| a.union(&tp::region_around(&u.obj.hint, DVec3::NEG_Z, place_scan::MAX_DROP)));
    let mut env = tp::env_for(state, &refs, Some(region), false, &[]);
    place_scan::drop_helpers(state, &mut env);
    let keys: Vec<Key> = units.iter().map(|u| u.obj.key).collect();
    env.retain(|o| !keys.contains(&o.key));
    env.extend(units.into_iter().map(|u| u.obj));
    let fs = state.assets.fs().cloned();
    let world = World::build(env, fs.as_ref(), MeshChoice::Render, None);
    let cands: Vec<usize> = (0..world.objs.len()).filter(|i| keys.contains(&world.objs[*i].key)).collect();
    (world, cands)
}

pub(crate) fn settle(state: &mut AppState, objs: Option<&[ObjRef]>, label: &str) -> Result<SettleReport, String> {
    let (mut world, cands) = match objs {
        Some(o) if !o.is_empty() => {
            let units = tp::units_from(state, Vec::new(), o.to_vec(), false, true);
            settle_world(state, units)
        }
        _ => {
            let mut env = pw::environment(
                state,
                &Default::default(),
                &Default::default(),
                &EnvOptions { include_tools: false, include_points: false, region: None },
            );
            place_scan::drop_helpers(state, &mut env);
            let keys: Vec<Key> = env.iter().filter(|o| place_scan::is_candidate(state, o)).map(|o| o.key).collect();
            let fs = state.assets.fs().cloned();
            let world = World::build_chunked(env, fs.as_ref(), MeshChoice::Render);
            let keys: std::collections::HashSet<Key> = keys.into_iter().collect();
            let cands = (0..world.objs.len())
                .filter(|i| keys.contains(&world.objs[*i].key) && world.resolved[*i].notes.is_empty())
                .collect();
            let rooms = place_scan::sky_rooms(&world, &place_scan::sky_cameras(state));
            let cands = place_scan::outside_sky(&world, cands, &rooms);
            (world, cands)
        }
    };
    let whole = objs.is_none_or(<[ObjRef]>::is_empty);
    let opts = ScanOpts { overlaps: false, skip_hanging: whole, ..ScanOpts::default() };
    let findings = place_scan::scan(&mut world, &cands, &opts, None);
    let mut rep = SettleReport { checked: cands.len(), ..SettleReport::default() };
    let mut moves: tp::Moves = Vec::new();
    for f in &findings {
        if f.chain == DVec3::ZERO {
            continue;
        }
        match f.class {
            Class::Floating(_) => rep.floating += 1,
            Class::Sunk(_) => rep.sunk += 1,
            _ => continue,
        }
        moves.push((world.objs[f.idx].refs.clone(), DMat4::from_translation(f.chain)));
    }
    if !moves.is_empty() {
        apply(state, label, |doc| tp::apply_moves(doc, &moves))?;
    }
    Ok(rep)
}

pub(crate) fn auto_settle_created(state: &mut AppState, created: &[ObjRef], amend: bool) -> usize {
    let props: Vec<ObjRef> = created
        .iter()
        .copied()
        .filter(|r| match r {
            ObjRef::Entity(id) => place_scan::prop_entity(state, *id),
            ObjRef::Solid(_) => false,
        })
        .collect();
    if props.is_empty() {
        return 0;
    }
    let units = tp::units_from(state, Vec::new(), props, false, false);
    let prep =
        tp::drop_prepare(state, units, false, tp::drop_spec(DVec3::NEG_Z, false, 0.0, 16384.0, MeshChoice::Render));
    let fs = state.assets.fs().cloned();
    let (moves, _) = tp::drop_run(prep, fs.as_ref());
    if moves.is_empty() {
        return 0;
    }
    let n = moves.len();
    let res = if amend && !state.doc.in_transaction() {
        state.doc.amend_last_step("Settle floating objects", |doc| tp::apply_moves(doc, &moves))
    } else if state.doc.in_transaction() {
        tp::apply_moves(&mut state.doc, &moves)
    } else {
        state.doc.transaction("Settle floating objects", |doc| tp::apply_moves(doc, &moves))
    };
    match res {
        Ok(()) => n,
        Err(e) => {
            log::warn!("auto settle failed: {e}");
            0
        }
    }
}

fn rect_corners(r: &hp_shape::OrientedRect) -> Vec<DVec2> {
    let (s, c) = r.angle.to_radians().sin_cos();
    let ax = DVec2::new(c, s) * r.size.x * 0.5;
    let ay = DVec2::new(-s, c) * r.size.y * 0.5;
    vec![r.center - ax - ay, r.center + ax - ay, r.center + ax + ay, r.center - ax + ay]
}

pub(crate) const MAX_DIM_UNITS: usize = 16;

pub(crate) fn real_dims(state: &AppState, objs: &[ObjRef]) -> RealDims {
    let units = tp::units_from(state, Vec::new(), objs.to_vec(), false, true);
    let mut out = RealDims { objects: units.len(), ..RealDims::default() };
    let fs = state.assets.fs().cloned();
    let mut total = Aabb::EMPTY;
    let refs: Vec<ObjRef> = units.iter().flat_map(|u| u.refs.clone()).collect();
    let region = units
        .iter()
        .take(MAX_DIM_UNITS)
        .fold(Aabb::EMPTY, |a, u| a.union(&tp::region_around(&u.obj.hint, DVec3::NEG_Z, place_scan::MAX_DROP)));
    let mut env = if region.is_empty() { Vec::new() } else { tp::env_for(state, &refs, Some(region), false, &[]) };
    place_scan::drop_helpers(state, &mut env);
    let world = World::build(env, fs.as_ref(), MeshChoice::Render, None);
    for (i, u) in units.iter().enumerate() {
        let r = pw::resolve(&u.obj, fs.as_ref(), MeshChoice::Render);
        let mesh = r.compound();
        let b = mesh.bounds();
        if b.is_empty() {
            continue;
        }
        total = total.union(&b);
        if i >= MAX_DIM_UNITS {
            continue;
        }
        out.notes.extend(r.notes.iter().cloned());
        let hull = hp_shape::footprint(&mesh);
        let rect = hp_shape::min_area_rect(&hull);
        let (size, yaw, corners) = match &rect {
            Some(rc) => (DVec3::new(rc.size.x, rc.size.y, b.size().z), rc.angle, rect_corners(rc)),
            None => (b.size(), 0.0, Vec::new()),
        };
        let contacts = hp_shape::contact_patch(&mesh, 0.5)
            .map(|p| p.outline.iter().map(|q| DVec3::new(q.x, q.y, p.z)).collect::<Vec<_>>())
            .unwrap_or_default();
        let s = place_ops::settle(&mesh, &world, DVec3::NEG_Z, place_scan::MAX_DROP, &[], 0.01);
        let ov = place_ops::overlaps(&mesh, &world, &[], 0.05);
        let worst = ov.iter().max_by(|a, b| a.depth.total_cmp(&b.depth));
        out.units.push(UnitDims {
            label: place_scan::label(&u.obj),
            size,
            yaw,
            min: b.min,
            max: b.max,
            footprint: hull,
            rect: corners,
            contacts,
            gap: s.delta.map(place_ops::round),
            overlap: worst.map_or(0.0, |w| place_ops::round(w.depth)),
            overlap_with: worst.map(|w| place_scan::label(&world.objs[w.owner])),
        });
    }
    out.size = if total.is_empty() { DVec3::ZERO } else { total.size() };
    out
}

pub(crate) struct Sheet {
    pub(crate) image: hp_shape::Image,
    pub(crate) caption: String,
}

pub(crate) struct SheetJob {
    req: tp::SheetReq,
    fs: Option<hp_fs::GameFs>,
}

impl SheetJob {
    pub(crate) fn run(self) -> Result<Sheet, String> {
        tp::draw_sheet(self.req, self.fs.as_ref()).map(|(image, caption)| Sheet { image, caption })
    }
}

fn sheet_job(state: &AppState, objs: Vec<Obj>, label: String, width: u32, height: u32) -> SheetJob {
    let views =
        vec![hp_shape::ViewKind::Top, hp_shape::ViewKind::Front, hp_shape::ViewKind::Side, hp_shape::ViewKind::Iso];
    let req = tp::SheetReq {
        objs,
        mesh: "both".to_owned(),
        views,
        width,
        height,
        markers: true,
        dims: true,
        grid: true,
        label,
        clip: None,
    };
    SheetJob { req, fs: state.assets.fs().cloned() }
}

pub(crate) fn sheet_job_selection(state: &AppState, width: u32, height: u32) -> Result<SheetJob, String> {
    let objs: Vec<Obj> =
        state.doc.selected_objects().into_iter().take(64).filter_map(|r| pw::obj_for(state, r, false)).collect();
    if objs.is_empty() {
        return Err("nothing selected".to_owned());
    }
    let label = if objs.len() == 1 {
        let o = &objs[0];
        format!("{} #{}", o.model_path().map_or(o.class.clone(), str::to_owned), o.key.id()).to_ascii_uppercase()
    } else {
        format!("{} OBJECTS", objs.len())
    };
    Ok(sheet_job(state, objs, label, width, height))
}

pub(crate) fn sheet_job_model(state: &AppState, path: &str, width: u32, height: u32) -> SheetJob {
    let p = path.trim().replace('\\', "/").to_ascii_lowercase();
    let objs = vec![tp::model_obj(&p, DVec3::ZERO, DVec3::ZERO, 1.0, 0, "prop_static")];
    let label = p.to_ascii_uppercase();
    sheet_job(state, objs, label, width, height)
}

pub(crate) fn placement_note(state: &AppState, touched: &[ObjRef]) -> Option<String> {
    let units =
        tp::units_from(state, Vec::new(), touched.iter().copied().take(MAX_NOTE_OBJECTS).collect(), false, false);
    let units: Vec<Unit> = units
        .into_iter()
        .filter(|u| {
            let prop = matches!(u.obj.key, Key::Entity(id) if place_scan::prop_entity(state, id));
            let brush = matches!(u.obj.kind, "solid" | "brush_entity") && !pw::non_solid_class(&u.obj.class);
            prop || brush
        })
        .collect();
    if units.is_empty() {
        return None;
    }
    let keys: Vec<Key> = units.iter().map(|u| u.obj.key).collect();
    let props: Vec<bool> =
        units.iter().map(|u| matches!(u.obj.key, Key::Entity(id) if place_scan::prop_entity(state, id))).collect();
    let (world, cands) = settle_world(state, units);
    let mut lines: Vec<String> = Vec::new();
    let mut floating_ids: Vec<u32> = Vec::new();
    let mut sunk_ids: Vec<u32> = Vec::new();
    for i in cands {
        let k = world.objs[i].key;
        let prop = keys.iter().position(|x| *x == k).is_some_and(|p| props[p]);
        let moving = world.resolved[i].compound();
        let (s, class) = place_scan::classify(&moving, &world, i, place_scan::FLOAT_TOLERANCE, place_scan::MAX_DROP);
        let what = place_scan::label(&world.objs[i]);
        match class {
            Class::Floating(g) => {
                let below = s.contact.as_ref().map_or_else(String::new, |(o, _)| place_scan::label(&world.objs[*o]));
                lines.push(format!("{what} floats {} units above {below}", fmt(g)));
                floating_ids.push(k.id());
            }
            Class::Sunk(d) if prop => {
                let into = s.lifted_by.map_or_else(String::new, |o| place_scan::label(&world.objs[o]));
                lines.push(format!("{what} is sunk {} units into {into}", fmt(d)));
                sunk_ids.push(k.id());
            }
            _ => {}
        }
        let ov = place_ops::overlaps(&moving, &world, &[i], place_scan::OVERLAP_TOLERANCE);
        for o in ov.iter().filter(|o| Some(o.owner) != s.lifted_by && o.push.z <= 0.5 * o.depth.max(1e-9)) {
            let other_prop = matches!(world.objs[o.owner].key, Key::Entity(id) if place_scan::prop_entity(state, id));
            if !prop && !other_prop {
                continue;
            }
            lines.push(format!(
                "{what} overlaps {} by {} units",
                place_scan::label(&world.objs[o.owner]),
                fmt(o.depth)
            ));
        }
    }
    if lines.is_empty() {
        return None;
    }
    lines.truncate(8);
    let mut fixes: Vec<String> = Vec::new();
    if !floating_ids.is_empty() || !sunk_ids.is_empty() {
        let mut ids = floating_ids;
        ids.extend(sunk_ids);
        fixes.push(format!("drop_to_surface with ids {ids:?} (or place_object mode drop)"));
    }
    if lines.iter().any(|l| l.contains(" overlaps ")) {
        fixes.push("place_object (mode next_to, against or on) to move it clear".to_owned());
    }
    Some(format!("placement check: {}. Fix: {}.", lines.join("; "), fixes.join("; ")))
}

const MAX_NOTE_OBJECTS: usize = 32;

fn fmt(x: f64) -> String {
    let r = place_ops::round(x);
    if r == r.trunc() { format!("{}", r as i64) } else { format!("{r}") }
}
