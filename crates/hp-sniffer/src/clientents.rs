use std::collections::{BTreeMap, HashMap};

use hp_bsp::Entity;

use crate::clientmem::{
    Image, Rtti, StringTable, find_module, find_recv_table, find_string_tables, parse_rtti, rd32, rd64, read_span,
    recv_offset, scan_item_arrays, user_pointer,
};
use crate::procmem::MemSource;

pub const ENTITY_SLOTS: usize = 16384;
pub const ENTRY_STRIDE: usize = 32;
const ENTITY_READ: usize = 12288;
const HANDLE_MASK: u32 = 0x3fff;
const MODEL_SLOT_MASK: i32 = 0xffff;
const LIST_OFFSETS: [usize; 8] = [8, 16, 24, 32, 40, 48, 56, 64];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientEntity {
    pub index: u32,
    pub cpp_class: String,
    pub map_id: i32,
    pub class_index: i32,
    pub model_index: i32,
    pub classname: String,
    pub model: String,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub spawnflags: i32,
    pub render_mode: i32,
    pub render_fx: i32,
    pub color: [u8; 4],
    pub health: i32,
    pub skin: i32,
    pub body: i32,
    pub scale: f32,
    pub parent: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default)]
struct Offsets {
    origin: Option<i32>,
    angles: Option<i32>,
    model_index: Option<i32>,
    spawnflags: Option<i32>,
    render_mode: Option<i32>,
    render_fx: Option<i32>,
    color: Option<i32>,
    health: Option<i32>,
    class_index: Option<i32>,
    map_id: Option<i32>,
    parent: Option<i32>,
    skin: Option<i32>,
    body: Option<i32>,
    scale: Option<i32>,
}

pub struct ClientTracker {
    pub list_address: u64,
    rtti: Rtti,
    offsets: Offsets,
    classes: Vec<StringTable>,
    models: Vec<StringTable>,
    pub notes: Vec<String>,
    seen: BTreeMap<u32, ClientEntity>,
    pub observations: usize,
}

fn read_i32(d: &[u8], at: Option<i32>) -> Option<i32> {
    let at = usize::try_from(at?).ok()?;
    rd32(d, at).map(|v| v as i32)
}

fn read_f32(d: &[u8], at: Option<i32>) -> Option<f32> {
    read_i32(d, at).map(|v| f32::from_bits(v as u32))
}

fn read_vec3(d: &[u8], at: Option<i32>) -> Option<[f32; 3]> {
    let at = usize::try_from(at?).ok()?;
    let mut v = [0f32; 3];
    for (i, x) in v.iter_mut().enumerate() {
        *x = f32::from_bits(rd32(d, at + 4 * i)?);
    }
    v.iter().all(|x| x.is_finite() && x.abs() < 1.0e7).then_some(v)
}

fn lookup(tables: &[StringTable], index: i32) -> Option<String> {
    let i = usize::try_from(index).ok()?;
    tables.iter().find_map(|t| t.get(i)).map(str::to_owned)
}

impl ClientTracker {
    pub fn open<S: MemSource + ?Sized>(src: &S, map_hint: Option<&str>) -> Result<ClientTracker, String> {
        let client = find_module(src, "client.dll").ok_or("client.dll was not found in the game memory")?;
        let engine = find_module(src, "engine.dll").ok_or("engine.dll was not found in the game memory")?;
        let rtti = parse_rtti(&client);
        let engine_rtti = parse_rtti(&engine);
        let notes = Vec::new();
        let list_address = find_entity_list(&client, &rtti).ok_or("the client entity list was not found")?;
        let offsets = resolve_offsets(src, &client);
        if offsets.map_id.is_none() || offsets.origin.is_none() {
            return Err("the networked entity tables (DT_BaseEntity) were not recognised".to_owned());
        }
        let tables = find_string_tables(src, &engine_rtti, &["networkstring", "modelprecache"]);
        let mut tracker = ClientTracker {
            list_address,
            rtti,
            offsets,
            classes: Vec::new(),
            models: Vec::new(),
            notes,
            seen: BTreeMap::new(),
            observations: 0,
        };
        let world = tracker.read_list(src).and_then(|l| l.into_iter().find(|e| e.cpp_class == "C_World"));
        let (class_index, model_index) = world.map(|w| (w.class_index, w.model_index)).unwrap_or((0, 0));
        let bsp_name = map_hint.map(|m| format!("maps/{}.bsp", m.trim().to_ascii_lowercase()));
        let entities = tracker.read_list(src).unwrap_or_default();
        let from_objects =
            |name: &str| -> Vec<StringTable> { tables.iter().filter(|t| t.name == name).cloned().collect() };
        let mut class_candidates: Vec<StringTable> =
            from_objects("networkstring").into_iter().filter(|t| world_class_ok(t, class_index)).collect();
        if best_score(&class_candidates, |t| class_score(t, &entities)) == 0 && class_index > 0 {
            for items in scan_item_arrays(src, "worldspawn", class_index as usize, &|i| i.iter().flatten().count() > 8)
            {
                class_candidates.push(StringTable { name: "networkstring".to_owned(), items });
            }
        }
        let mut model_candidates: Vec<StringTable> = from_objects("modelprecache")
            .into_iter()
            .filter(|t| world_model_ok(t, model_index, bsp_name.as_deref()))
            .collect();
        if best_score(&model_candidates, |t| model_score(t, &entities)) == 0
            && let Some(bsp) = bsp_name.as_deref()
        {
            for items in
                scan_item_arrays(src, bsp, (model_index & MODEL_SLOT_MASK).max(1) as usize, &model_items_look_right)
            {
                model_candidates.push(StringTable { name: "modelprecache".to_owned(), items });
            }
        }
        tracker.classes = pick_best(class_candidates, |t| class_score(t, &entities));
        tracker.models = pick_best(model_candidates, |t| model_score(t, &entities));
        if tracker.classes.is_empty() {
            tracker.notes.push(
                "the string table with the real class names was not found: networked entities have no class name"
                    .to_owned(),
            );
        }
        if tracker.models.is_empty() {
            tracker
                .notes
                .push("the model string table of this map was not found: networked models are not named".to_owned());
        }
        Ok(tracker)
    }

    pub fn observe<S: MemSource + ?Sized>(&mut self, src: &S) -> usize {
        let Some(found) = self.read_list(src) else { return 0 };
        self.observations += 1;
        let n = found.len();
        for e in found {
            self.seen.insert(e.index, e);
        }
        n
    }

    pub fn tables(&self) -> impl Iterator<Item = &StringTable> {
        self.classes.iter().chain(self.models.iter())
    }

    pub fn entities(&self) -> Vec<ClientEntity> {
        self.seen.values().cloned().collect()
    }

    fn read_list<S: MemSource + ?Sized>(&self, src: &S) -> Option<Vec<ClientEntity>> {
        let raw = read_span(src, self.list_address, LIST_OFFSETS[LIST_OFFSETS.len() - 1] + ENTITY_SLOTS * ENTRY_STRIDE);
        let first = LIST_OFFSETS.iter().copied().find(|&o| {
            rd64(&raw, o)
                .filter(|p| user_pointer(*p))
                .is_some_and(|p| self.class_of(src, p).as_deref() == Some("C_World"))
        })?;
        let mut out = Vec::new();
        for slot in 0..ENTITY_SLOTS {
            let Some(ptr) = rd64(&raw, first + slot * ENTRY_STRIDE) else { break };
            if !user_pointer(ptr) {
                continue;
            }
            let Some(e) = self.read_entity(src, slot as u32, ptr) else { continue };
            out.push(e);
        }
        Some(out)
    }

    fn class_of<S: MemSource + ?Sized>(&self, src: &S, ptr: u64) -> Option<String> {
        let vt = rd64(&src.read_vec(ptr, 8), 0)?;
        self.rtti.vtables.get(&vt).cloned()
    }

    fn read_entity<S: MemSource + ?Sized>(&self, src: &S, index: u32, ptr: u64) -> Option<ClientEntity> {
        let d = read_span(src, ptr, ENTITY_READ);
        let vt = rd64(&d, 0)?;
        let cpp_class = self.rtti.vtables.get(&vt)?.clone();
        if !self.rtti.derives(&cpp_class, "C_BaseEntity") {
            return None;
        }
        let o = &self.offsets;
        let animating = self.rtti.derives(&cpp_class, "C_BaseAnimating");
        let map_id = read_i32(&d, o.map_id)?;
        let class_index = read_i32(&d, o.class_index).unwrap_or(0);
        let model_index = read_i32(&d, o.model_index).unwrap_or(0);
        let classname =
            if class_index > 0 { lookup(&self.classes, class_index).unwrap_or_default() } else { String::new() };
        let model_slot = model_index & MODEL_SLOT_MASK;
        let model = if model_slot > 1 { lookup(&self.models, model_slot).unwrap_or_default() } else { String::new() };
        let color = read_i32(&d, o.color).map(|c| c.to_le_bytes()).unwrap_or([255; 4]);
        let parent = read_i32(&d, o.parent).map(|h| h as u32).filter(|h| *h != u32::MAX).map(|h| h & HANDLE_MASK);
        Some(ClientEntity {
            index,
            cpp_class,
            map_id,
            class_index,
            model_index,
            classname,
            model,
            origin: read_vec3(&d, o.origin)?,
            angles: read_vec3(&d, o.angles).unwrap_or([0.0; 3]),
            spawnflags: read_i32(&d, o.spawnflags).unwrap_or(0),
            render_mode: read_i32(&d, o.render_mode).map(|v| v & 0xff).unwrap_or(0),
            render_fx: read_i32(&d, o.render_fx).map(|v| v & 0xff).unwrap_or(0),
            color,
            health: read_i32(&d, o.health).unwrap_or(0),
            skin: if animating { read_i32(&d, o.skin).unwrap_or(0) } else { 0 },
            body: if animating { read_i32(&d, o.body).unwrap_or(0) } else { 0 },
            scale: if animating { read_f32(&d, o.scale).unwrap_or(1.0) } else { 1.0 },
            parent,
        })
    }
}

fn world_class_ok(t: &StringTable, class_index: i32) -> bool {
    if class_index > 0 {
        lookup(std::slice::from_ref(t), class_index).as_deref() == Some("worldspawn")
    } else {
        t.items.iter().take(1024).any(|i| i.as_deref() == Some("worldspawn"))
    }
}

fn world_model_ok(t: &StringTable, model_index: i32, bsp_name: Option<&str>) -> bool {
    let name = lookup(std::slice::from_ref(t), (model_index & MODEL_SLOT_MASK).max(1)).unwrap_or_default();
    let name = norm_model(&name);
    name.ends_with(".bsp") && bsp_name.is_none_or(|b| name == b)
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
}

fn same_letters(a: &str, b: &str) -> bool {
    let (mut x, mut y): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    x.sort_unstable();
    y.sort_unstable();
    x == y
}

pub fn class_name_fits(name: &str, cpp_class: &str) -> bool {
    let n = squash(name);
    let c = squash(cpp_class.strip_prefix("C_").unwrap_or(cpp_class));
    !n.is_empty() && (n == c || same_letters(&n, &c) || (c == "world" && n == "worldspawn"))
}

fn class_score(t: &StringTable, entities: &[ClientEntity]) -> usize {
    entities
        .iter()
        .filter(|e| e.class_index > 0)
        .filter(|e| lookup(std::slice::from_ref(t), e.class_index).is_some_and(|n| class_name_fits(&n, &e.cpp_class)))
        .count()
}

fn model_score(t: &StringTable, entities: &[ClientEntity]) -> usize {
    entities
        .iter()
        .filter(|e| (e.model_index & MODEL_SLOT_MASK) > 1)
        .filter(|e| {
            let slot = e.model_index & MODEL_SLOT_MASK;
            match lookup(std::slice::from_ref(t), slot) {
                Some(n) if n.starts_with('*') => n[1..].parse::<i32>().ok() == Some(slot - 1),
                Some(n) => n.starts_with("models/"),
                None => false,
            }
        })
        .count()
}

fn best_score(tables: &[StringTable], score: impl Fn(&StringTable) -> usize) -> usize {
    tables.iter().map(score).max().unwrap_or(0)
}

fn pick_best(tables: Vec<StringTable>, score: impl Fn(&StringTable) -> usize) -> Vec<StringTable> {
    let mut ranked: Vec<(usize, usize, StringTable)> =
        tables.into_iter().map(|t| (score(&t), t.items.iter().flatten().count(), t)).collect();
    ranked.sort_by_key(|(s, n, _)| std::cmp::Reverse((*s, *n)));
    ranked.into_iter().take(1).map(|(_, _, t)| t).collect()
}

fn model_items_look_right(items: &[Option<String>]) -> bool {
    let second = items.get(2).and_then(|s| s.as_deref()).unwrap_or("");
    second.starts_with('*') || second.starts_with("models/")
}

fn find_entity_list(client: &Image, rtti: &Rtti) -> Option<u64> {
    rtti.vtables_of("CClientEntityList")
        .into_iter()
        .find_map(|vt| client.qword_positions(vt).into_iter().next())
        .map(|at| client.base + at as u64)
}

fn resolve_offsets<S: MemSource + ?Sized>(src: &S, client: &Image) -> Offsets {
    let ent = find_recv_table(src, client, "DT_BaseEntity");
    let anim = find_recv_table(src, client, "DT_BaseAnimating");
    let e = |name: &str| ent.and_then(|t| recv_offset(src, t, name));
    let a = |name: &str| anim.and_then(|t| recv_offset(src, t, name));
    Offsets {
        origin: e("m_vecOrigin"),
        angles: e("m_angRotation"),
        model_index: e("m_nModelIndex"),
        spawnflags: e("m_spawnflags"),
        render_mode: e("m_nRenderMode"),
        render_fx: e("m_nRenderFX"),
        color: e("m_clrRender"),
        health: e("m_iHealth"),
        class_index: e("m_RealClassName"),
        map_id: e("m_iMapCreatedID"),
        parent: e("moveparent"),
        skin: a("m_nSkin"),
        body: a("m_nBody"),
        scale: a("m_flModelScale"),
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientRecovery {
    pub attempted: bool,
    pub error: Option<String>,
    pub observations: usize,
    pub networked: usize,
    pub map_created: usize,
    pub unnamed: usize,
    pub matched: usize,
    pub added: usize,
    pub parents_restored: usize,
    pub lump_entities: usize,
    pub original_at_least: usize,
    pub unrecoverable_at_least: usize,
    pub id_base: Option<i32>,
    pub added_classes: Vec<(String, usize)>,
    pub notes: Vec<String>,
}

pub fn format_number(v: f32) -> String {
    let r = (f64::from(v) * 1.0e4).round() / 1.0e4;
    if r == r.trunc() {
        format!("{}", r as i64)
    } else {
        let s = format!("{r:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

fn vec_text(v: [f32; 3]) -> String {
    format!("{} {} {}", format_number(v[0]), format_number(v[1]), format_number(v[2]))
}

fn norm_model(s: &str) -> String {
    s.trim().replace('\\', "/").to_ascii_lowercase()
}

fn model_matches(c: &ClientEntity, lump_model: &str) -> bool {
    if lump_model.trim().is_empty() || c.model.is_empty() {
        return true;
    }
    norm_model(&c.model) == norm_model(lump_model)
}

fn lump_origin(e: &Entity) -> [f64; 3] {
    e.origin().unwrap_or([0.0; 3])
}

fn near(a: [f32; 3], b: [f64; 3], tolerance: f64) -> bool {
    (0..3).all(|i| (f64::from(a[i]) - b[i]).abs() <= tolerance)
}

fn entity_from_client(c: &ClientEntity) -> Entity {
    let mut e = Entity::default();
    e.pairs.push(("classname".to_owned(), c.classname.clone()));
    e.pairs.push(("origin".to_owned(), vec_text(c.origin)));
    e.pairs.push(("angles".to_owned(), vec_text(c.angles)));
    if !c.model.is_empty() && !c.model.to_ascii_lowercase().ends_with(".bsp") {
        e.pairs.push(("model".to_owned(), c.model.clone()));
    }
    if c.spawnflags != 0 {
        e.pairs.push(("spawnflags".to_owned(), c.spawnflags.to_string()));
    }
    if c.render_mode != 0 {
        e.pairs.push(("rendermode".to_owned(), c.render_mode.to_string()));
    }
    if c.render_fx != 0 {
        e.pairs.push(("renderfx".to_owned(), c.render_fx.to_string()));
    }
    if c.color != [255; 4] {
        e.pairs.push(("rendercolor".to_owned(), format!("{} {} {}", c.color[0], c.color[1], c.color[2])));
        e.pairs.push(("renderamt".to_owned(), c.color[3].to_string()));
    }
    if c.skin != 0 {
        e.pairs.push(("skin".to_owned(), c.skin.to_string()));
    }
    if c.body != 0 {
        e.pairs.push(("body".to_owned(), c.body.to_string()));
    }
    if c.scale.is_finite() && (c.scale - 1.0).abs() > 1.0e-4 && c.scale > 0.0 {
        e.pairs.push(("modelscale".to_owned(), format_number(c.scale)));
    }
    e
}

pub struct Merged {
    pub text: String,
    pub report: ClientRecovery,
}

pub fn merge_client_entities(lump_text: &str, observed: &[ClientEntity], observations: usize) -> Merged {
    let mut lump = hp_bsp::parse_entities(lump_text);
    let mut report = ClientRecovery {
        attempted: true,
        observations,
        networked: observed.len(),
        lump_entities: lump.len(),
        ..ClientRecovery::default()
    };
    let world_id = observed.iter().find(|c| c.cpp_class == "C_World").map(|c| c.map_id).filter(|id| *id >= 0);
    report.id_base = world_id;
    let mut mapped: Vec<&ClientEntity> =
        observed.iter().filter(|c| c.map_id >= 0 && c.cpp_class != "C_World").collect();
    mapped.sort_by_key(|c| (c.map_id, c.index));
    report.map_created = mapped.len();
    let mut taken: Vec<Option<u32>> = vec![None; lump.len()];
    let mut client_to_lump: HashMap<u32, usize> = HashMap::new();
    let mut leftover: Vec<&ClientEntity> = Vec::new();
    let named: Vec<&ClientEntity> = mapped.iter().copied().filter(|c| !c.classname.is_empty()).collect();
    report.unnamed = mapped.len() - named.len();
    for c in &named {
        let idx = world_id.map(|b| c.map_id - b).and_then(|i| usize::try_from(i).ok());
        let hit = idx.filter(|&i| {
            i < lump.len()
                && taken[i].is_none()
                && lump[i].classname().eq_ignore_ascii_case(&c.classname)
                && model_matches(c, lump[i].get("model").unwrap_or(""))
        });
        if let Some(i) = hit {
            taken[i] = Some(c.index);
            client_to_lump.insert(c.index, i);
        } else {
            leftover.push(c);
        }
    }
    let mut second: Vec<&ClientEntity> = Vec::new();
    for c in leftover {
        let found = (0..lump.len()).find(|&i| {
            taken[i].is_none()
                && lump[i].classname().eq_ignore_ascii_case(&c.classname)
                && model_matches(c, lump[i].get("model").unwrap_or(""))
                && near(c.origin, lump_origin(&lump[i]), 1.0)
        });
        match found {
            Some(i) => {
                taken[i] = Some(c.index);
                client_to_lump.insert(c.index, i);
            }
            None => second.push(c),
        }
    }
    let mut added: Vec<&ClientEntity> = Vec::new();
    for c in second {
        let mut best: Option<(usize, f64)> = None;
        for i in 0..lump.len() {
            if taken[i].is_some()
                || !lump[i].classname().eq_ignore_ascii_case(&c.classname)
                || !model_matches(c, lump[i].get("model").unwrap_or(""))
            {
                continue;
            }
            let o = lump_origin(&lump[i]);
            let d = (0..3).map(|k| (f64::from(c.origin[k]) - o[k]).powi(2)).sum::<f64>();
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        match best {
            Some((i, _)) => {
                taken[i] = Some(c.index);
                client_to_lump.insert(c.index, i);
            }
            None => added.push(c),
        }
    }
    report.matched = client_to_lump.len();
    let by_index: HashMap<u32, &ClientEntity> = observed.iter().map(|c| (c.index, c)).collect();
    let mut parents = 0usize;
    let targetname_of = |lump: &[Entity], client_index: u32, map: &HashMap<u32, usize>| -> Option<String> {
        let li = map.get(&client_index)?;
        lump[*li].get("targetname").filter(|t| !t.is_empty()).map(str::to_owned)
    };
    for (&ci, &li) in &client_to_lump {
        let Some(c) = by_index.get(&ci) else { continue };
        let Some(parent) = c.parent else { continue };
        if lump[li].get("parentname").is_some_and(|p| !p.is_empty()) {
            continue;
        }
        if let Some(name) = targetname_of(&lump, parent, &client_to_lump) {
            lump[li].set("parentname", name);
            parents += 1;
        }
    }
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut added_entities: Vec<Entity> = Vec::new();
    for c in &added {
        let mut e = entity_from_client(c);
        if let Some(parent) = c.parent
            && let Some(name) = targetname_of(&lump, parent, &client_to_lump)
        {
            e.pairs.push(("parentname".to_owned(), name));
            parents += 1;
        }
        *counts.entry(c.classname.clone()).or_default() += 1;
        added_entities.push(e);
    }
    report.parents_restored = parents;
    report.added = added_entities.len();
    report.added_classes = counts.into_iter().collect();
    if let Some(base) = world_id {
        let max = mapped.iter().map(|c| c.map_id - base).max().unwrap_or(0).max(0) as usize + 1;
        report.original_at_least = max.max(lump.len() + added_entities.len());
        report.unrecoverable_at_least = report.original_at_least - (lump.len() + added_entities.len());
    }
    lump.extend(added_entities);
    let bytes = hp_bsp::write_entities(&lump);
    let text = String::from_utf8_lossy(&bytes).trim_end_matches('\0').to_owned();
    Merged { text, report }
}

pub struct ClientCapture {
    tracker: Option<ClientTracker>,
    error: Option<String>,
}

impl ClientCapture {
    pub fn start<S: MemSource + ?Sized>(src: &S, map_hint: Option<&str>) -> ClientCapture {
        match ClientTracker::open(src, map_hint) {
            Ok(mut t) => {
                t.observe(src);
                ClientCapture { tracker: Some(t), error: None }
            }
            Err(e) => ClientCapture { tracker: None, error: Some(e) },
        }
    }

    pub fn observe<S: MemSource + ?Sized>(&mut self, src: &S) {
        if let Some(t) = self.tracker.as_mut() {
            t.observe(src);
        }
    }

    pub fn finish<S: MemSource + ?Sized>(mut self, src: &S, lump_text: &str) -> Merged {
        self.observe(src);
        match self.tracker {
            Some(t) => {
                let mut m = merge_client_entities(lump_text, &t.entities(), t.observations);
                m.report.notes.extend(t.notes.iter().cloned());
                m
            }
            None => Merged {
                text: lump_text.to_owned(),
                report: ClientRecovery { attempted: true, error: self.error, ..ClientRecovery::default() },
            },
        }
    }
}

pub fn classes_seen(observed: &[ClientEntity]) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for c in observed.iter().filter(|c| c.map_id >= 0) {
        let key = if c.classname.is_empty() { format!("({})", c.cpp_class) } else { c.classname.clone() };
        *m.entry(key).or_default() += 1;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(index: u32, id: i32, class: &str, cpp: &str, origin: [f32; 3]) -> ClientEntity {
        ClientEntity {
            index,
            cpp_class: cpp.to_owned(),
            map_id: id,
            classname: class.to_owned(),
            origin,
            scale: 1.0,
            color: [255; 4],
            ..ClientEntity::default()
        }
    }

    fn lump() -> String {
        let mut text = String::new();
        let rows = [
            ("worldspawn", "", ""),
            ("func_brush", "*1", "0 0 0"),
            ("prop_physics", "models/a.mdl", "10 20 30"),
            ("light", "", "5 5 5"),
        ];
        for (c, m, o) in rows {
            text.push_str("{\n\"classname\" \"");
            text.push_str(c);
            text.push_str("\"\n");
            if !m.is_empty() {
                text.push_str(&format!("\"model\" \"{m}\"\n"));
            }
            if !o.is_empty() {
                text.push_str(&format!("\"origin\" \"{o}\"\n"));
            }
            if c == "prop_physics" {
                text.push_str("\"targetname\" \"drum\"\n");
            }
            text.push_str("}\n");
        }
        text
    }

    #[test]
    fn numbers_are_written_like_hammer() {
        assert_eq!(format_number(12.0), "12");
        assert_eq!(format_number(-0.5), "-0.5");
        assert_eq!(format_number(1.23456), "1.2346");
        assert_eq!(format_number(-2048.0), "-2048");
    }

    #[test]
    fn networked_entities_that_the_lump_lacks_are_added_and_the_others_matched() {
        let mut brush = client(3, 1236, "func_brush", "C_BaseEntity", [0.0; 3]);
        brush.model = "*1".into();
        let mut prop = client(4, 1237, "prop_physics", "C_PhysicsProp", [10.0, 20.0, 30.0]);
        prop.model = "models/a.mdl".into();
        let mut smg = client(5, 1250, "weapon_smg1", "C_WeaponSMG1", [100.0, 200.0, 300.0]);
        smg.angles = [0.0, 90.0, 0.0];
        smg.spawnflags = 2;
        let world = client(0, 1235, "worldspawn", "C_World", [0.0; 3]);
        let player = client(1, -1, "player", "C_GMOD_Player", [1.0; 3]);
        let m = merge_client_entities(&lump(), &[world, brush, prop, smg, player], 1);
        let r = &m.report;
        assert_eq!((r.matched, r.added, r.map_created, r.id_base), (2, 1, 3, Some(1235)));
        assert_eq!(r.added_classes, vec![("weapon_smg1".to_owned(), 1)]);
        assert_eq!(r.original_at_least, 16);
        assert_eq!(r.unrecoverable_at_least, 16 - 5);
        let ents = hp_bsp::parse_entities(&m.text);
        assert_eq!(ents.len(), 5);
        let smg = ents.iter().find(|e| e.classname() == "weapon_smg1").expect("added");
        assert_eq!(smg.get("origin"), Some("100 200 300"));
        assert_eq!(smg.get("angles"), Some("0 90 0"));
        assert_eq!(smg.get("spawnflags"), Some("2"));
        assert!(ents.iter().all(|e| e.classname() != "player"));
    }

    #[test]
    fn a_moved_prop_is_still_matched_to_its_lump_entity() {
        let mut prop = client(4, 1999, "prop_physics", "C_PhysicsProp", [500.0, 500.0, 5.0]);
        prop.model = "models/a.mdl".into();
        let world = client(0, 1235, "worldspawn", "C_World", [0.0; 3]);
        let m = merge_client_entities(&lump(), &[world, prop], 1);
        assert_eq!((m.report.matched, m.report.added), (1, 0));
        assert_eq!(hp_bsp::parse_entities(&m.text).len(), 4);
    }

    #[test]
    fn the_parent_name_of_a_stripped_key_comes_from_the_networked_parent() {
        let mut brush = client(3, 1236, "func_brush", "C_BaseEntity", [0.0; 3]);
        brush.model = "*1".into();
        let mut prop = client(4, 1237, "prop_physics", "C_PhysicsProp", [10.0, 20.0, 30.0]);
        prop.model = "models/a.mdl".into();
        let mut child = client(9, 1260, "item_battery", "C_BaseAnimating", [1.0, 2.0, 3.0]);
        child.parent = Some(4);
        let mut kept_child = brush.clone();
        kept_child.parent = Some(4);
        let world = client(0, 1235, "worldspawn", "C_World", [0.0; 3]);
        let m = merge_client_entities(&lump(), &[world, kept_child, prop, child], 1);
        assert_eq!(m.report.parents_restored, 2);
        let ents = hp_bsp::parse_entities(&m.text);
        let brush = ents.iter().find(|e| e.classname() == "func_brush").expect("brush");
        assert_eq!(brush.get("parentname"), Some("drum"));
        let battery = ents.iter().find(|e| e.classname() == "item_battery").expect("battery");
        assert_eq!(battery.get("parentname"), Some("drum"));
    }

    #[test]
    fn entities_without_a_class_name_are_counted_and_never_invented() {
        let mut anon = client(7, 1300, "", "C_BaseEntity", [9.0; 3]);
        anon.model = String::new();
        let world = client(0, 1235, "worldspawn", "C_World", [0.0; 3]);
        let m = merge_client_entities(&lump(), &[world, anon], 1);
        assert_eq!((m.report.unnamed, m.report.added), (1, 0));
    }

    #[test]
    fn a_game_without_the_client_module_leaves_the_lump_alone_and_says_why() {
        let mem = crate::procmem::VecMem::default();
        let m = ClientCapture::start(&mem, None).finish(&mem, &lump());
        assert_eq!(m.text, lump());
        assert!(m.report.attempted);
        assert!(m.report.error.as_deref().is_some_and(|e| e.contains("client.dll")));
        assert_eq!((m.report.added, m.report.matched), (0, 0));
    }

    #[test]
    fn a_model_the_tables_could_not_name_never_makes_a_duplicate_of_a_lump_entity() {
        let mut prop = client(4, 1999, "prop_physics", "C_PhysicsProp", [500.0, 500.0, 5.0]);
        prop.model_index = 77;
        let world = client(0, 1235, "worldspawn", "C_World", [0.0; 3]);
        let m = merge_client_entities(&lump(), &[world, prop], 1);
        assert_eq!((m.report.matched, m.report.added), (1, 0));
    }

    #[test]
    fn an_item_with_a_model_on_the_client_and_none_in_the_lump_is_not_added_twice() {
        let mut text = lump();
        text.push_str(
            "{
\"classname\" \"weapon_smg1\"
\"origin\" \"10 10 10\"
}
{
\"classname\" \"weapon_smg1\"
\"origin\" \"500 500 10\"
}
",
        );
        let mut near = client(6, 1300, "weapon_smg1", "C_WeaponSMG1", [498.0, 502.0, -8.0]);
        near.model = "models/weapons/w_smg1.mdl".into();
        let mut other = client(7, 1299, "weapon_smg1", "C_WeaponSMG1", [12.0, 9.0, -9.0]);
        other.model = "models/weapons/w_smg1.mdl".into();
        let world = client(0, 1235, "worldspawn", "C_World", [0.0; 3]);
        let m = merge_client_entities(&text, &[world, near, other], 1);
        assert_eq!((m.report.matched, m.report.added), (2, 0));
    }

    #[test]
    fn a_class_name_fits_its_client_class_when_the_letters_agree() {
        assert!(class_name_fits("weapon_smg1", "C_WeaponSMG1"));
        assert!(class_name_fits("prop_physics", "C_PhysicsProp"));
        assert!(class_name_fits("worldspawn", "C_World"));
        assert!(class_name_fits("npc_barnacle", "C_NPC_Barnacle"));
        assert!(!class_name_fits("func_brush", "C_WeaponSMG1"));
        assert!(!class_name_fits("", "C_World"));
    }

    #[test]
    fn the_table_that_names_the_entities_consistently_wins_over_a_stale_one() {
        let mut live = StringTable { name: "networkstring".into(), items: vec![None; 12] };
        live.items[3] = Some("weapon_smg1".into());
        live.items[5] = Some("prop_physics".into());
        let mut stale = StringTable { name: "networkstring".into(), items: vec![None; 12] };
        stale.items[3] = Some("prop_physics".into());
        stale.items[5] = Some("weapon_smg1".into());
        stale.items[7] = Some("filler".into());
        let mut smg = client(1, 1300, "", "C_WeaponSMG1", [0.0; 3]);
        smg.class_index = 3;
        let mut prop = client(2, 1301, "", "C_PhysicsProp", [0.0; 3]);
        prop.class_index = 5;
        let entities = [smg, prop];
        assert_eq!(class_score(&live, &entities), 2);
        assert_eq!(class_score(&stale, &entities), 0);
        let picked = pick_best(vec![stale, live], |t| class_score(t, &entities));
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].items[3].as_deref(), Some("weapon_smg1"));
    }

    #[test]
    fn brush_model_names_must_follow_their_index() {
        let mut t = StringTable { name: "modelprecache".into(), items: vec![None; 8] };
        t.items[1] = Some("maps/x.bsp".into());
        t.items[2] = Some("*1".into());
        t.items[3] = Some("*2".into());
        t.items[4] = Some("models/props/a.mdl".into());
        let mut a = client(1, 1300, "", "C_BaseEntity", [0.0; 3]);
        a.model_index = 3;
        let mut b = client(2, 1301, "", "C_BaseEntity", [0.0; 3]);
        b.model_index = 65536 + 4;
        let mut c = client(3, 1302, "", "C_BaseEntity", [0.0; 3]);
        c.model_index = 2;
        let mut wrong = t.clone();
        wrong.items.swap(2, 3);
        assert_eq!(model_score(&t, &[a.clone(), b.clone(), c.clone()]), 3);
        assert_eq!(model_score(&wrong, &[a, b, c]), 1);
    }
}
