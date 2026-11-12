use std::collections::{HashMap, HashSet, VecDeque};

use glam::DVec3;
use hp_app::{AppState, t, tf};
use hp_editor::limits::world_light::LightCmd;
use hp_vmf::{Entity, Id};

use crate::player::{self, Doors};

const MAX_PER_FRAME: usize = 256;
const LOG_LINES: usize = 6;
const MAX_QUEUE: usize = 4096;

#[derive(Clone, Debug)]
struct Pending {
    at: f64,
    target: String,
    input: String,
    param: String,
    caller: Option<Id>,
}

type TriggerList = ((u64, usize), Vec<(Id, String)>);

#[derive(Debug, Default)]
pub struct IoSim {
    time: f64,
    queue: Vec<Pending>,
    left: HashMap<(Id, usize), i32>,
    disabled: HashSet<Id>,
    hidden: HashSet<Id>,
    locked: HashSet<Id>,
    inside: HashMap<Id, f64>,
    timers: HashMap<Id, f64>,
    counters: HashMap<Id, f64>,
    pub portals: HashMap<String, bool>,
    pub teleport: Option<(DVec3, Option<f64>)>,
    log: VecDeque<String>,
    touch_doors: Vec<Id>,
    start_open: Vec<Id>,
    touching: HashSet<Id>,
    light_inputs: Vec<(String, LightCmd)>,
    train_inputs: Vec<(Id, String, String)>,
    moved: HashMap<Id, glam::DMat4>,
    projectors: HashMap<Id, hp_app::state::ProjectorOverride>,
    triggers: Option<TriggerList>,
}

fn flag(e: &Entity, bit: i64) -> bool {
    e.get("spawnflags").and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0) & bit != 0
}

fn key_true(e: &Entity, key: &str) -> bool {
    e.get(key).is_some_and(|v| v.trim() == "1")
}

fn float(e: &Entity, key: &str, default: f64) -> f64 {
    e.get(key).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).unwrap_or(default)
}

fn class(e: &Entity) -> String {
    e.classname.to_ascii_lowercase()
}

fn label(e: &Entity) -> String {
    match e.get("targetname").map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => n.to_owned(),
        None => format!("{}#{}", e.classname, e.id),
    }
}

fn is_light(class: &str) -> bool {
    matches!(class, "light" | "light_spot" | "light_glspot" | "light_environment")
}

fn is_door(class: &str) -> bool {
    matches!(class, "prop_door_rotating" | "func_door_rotating" | "func_rotating_door" | "func_door")
}

fn toggles_visibility(class: &str) -> bool {
    matches!(class, "func_brush" | "func_wall_toggle" | "func_illusionary" | "func_lod")
}

pub(crate) fn moved_bounds(b: hp_geom::Aabb, m: &glam::DMat4) -> hp_geom::Aabb {
    let mut out = hp_geom::Aabb::EMPTY;
    for k in 0..8 {
        let pick = |bit: usize, lo: f64, hi: f64| if k & bit == 0 { lo } else { hi };
        let corner = DVec3::new(pick(1, b.min.x, b.max.x), pick(2, b.min.y, b.max.y), pick(4, b.min.z, b.max.z));
        out.extend(m.transform_point3(corner));
    }
    out
}

fn names(e: &Entity, target: &str) -> bool {
    let target = target.trim();
    if target.is_empty() {
        return false;
    }
    let matches = |value: &str| match target.strip_suffix('*') {
        Some(prefix) => value.to_ascii_lowercase().starts_with(&prefix.to_ascii_lowercase()),
        None => value.eq_ignore_ascii_case(target),
    };
    e.get("targetname").is_some_and(|n| matches(n.trim())) || matches(&e.classname)
}

impl IoSim {
    pub fn new(state: &AppState) -> IoSim {
        let mut sim = IoSim::default();
        let scene = state.doc.scene();
        let mut autos = Vec::new();
        for n in scene.entities().values() {
            let e = &n.entity;
            let c = class(e);
            if key_true(e, "StartDisabled") {
                sim.disabled.insert(e.id);
                if toggles_visibility(&c) {
                    sim.hidden.insert(e.id);
                }
            }
            if (is_door(&c) || c == "func_button" || c == "func_rot_button") && flag(e, 2048) {
                sim.locked.insert(e.id);
            }
            if is_door(&c) && c != "prop_door_rotating" && flag(e, 1024) {
                sim.touch_doors.push(e.id);
            }
            let spawnpos = e.get("spawnpos").map(str::trim).unwrap_or("0");
            let starts_open = if c == "prop_door_rotating" {
                spawnpos == "1" || spawnpos == "2"
            } else {
                is_door(&c) && (flag(e, 1) || spawnpos == "1")
            };
            if starts_open {
                sim.start_open.push(e.id);
            }
            if c == "logic_timer" && !key_true(e, "StartDisabled") {
                sim.timers.insert(e.id, float(e, "RefireTime", 1.0).max(0.01));
            }
            if c == "math_counter" {
                sim.counters.insert(e.id, float(e, "startvalue", 0.0));
            }
            if c == "logic_auto" {
                autos.push(e.id);
            }
            if c == "infodecal" && e.get("targetname").is_some_and(|n| !n.trim().is_empty()) {
                sim.hidden.insert(e.id);
            }
        }
        for id in autos {
            sim.fire(state, id, "OnMapSpawn");
            sim.fire(state, id, "OnNewGame");
        }
        sim
    }

    pub fn hidden(&self) -> &HashSet<Id> {
        &self.hidden
    }

    pub fn log(&self) -> impl Iterator<Item = &String> {
        self.log.iter()
    }

    pub fn note_no_control(&mut self, state: &AppState, id: Id) {
        if let Some(n) = state.doc.scene().entity(id) {
            let line = tf!("viewport.io.train_no_control", label(&n.entity));
            self.note(line);
        }
    }

    fn note(&mut self, line: String) {
        self.log.push_back(format!("{:>5.1}s  {line}", self.time));
        while self.log.len() > LOG_LINES {
            self.log.pop_front();
        }
    }

    pub fn fire(&mut self, state: &AppState, id: Id, output: &str) {
        let Some(node) = state.doc.scene().entity(id) else { return };
        let e = &node.entity;
        let mut fired = false;
        for (k, c) in e.connections.iter().enumerate() {
            if !c.output.eq_ignore_ascii_case(output) {
                continue;
            }
            if c.times > 0 {
                let left = self.left.entry((id, k)).or_insert(c.times);
                if *left <= 0 {
                    continue;
                }
                *left -= 1;
            }
            if self.queue.len() >= MAX_QUEUE {
                break;
            }
            let delay = if c.delay.is_finite() { f64::from(c.delay.max(0.0)) } else { 0.0 };
            self.queue.push(Pending {
                at: self.time + delay,
                target: c.target.clone(),
                input: c.input.clone(),
                param: c.param.clone(),
                caller: Some(id),
            });
            fired = true;
        }
        if fired {
            let line = format!("{}.{output}", label(e));
            self.note(line);
        }
    }

    pub fn use_entity(&mut self, state: &AppState, doors: &mut Doors, id: Id, feet: DVec3) {
        let Some(node) = state.doc.scene().entity(id) else { return };
        let c = class(&node.entity);
        let locked = self.locked.contains(&id);
        if is_door(&c) {
            let usable = if c == "prop_door_rotating" { !flag(&node.entity, 32768) } else { flag(&node.entity, 256) };
            if locked {
                self.fire(state, id, "OnLockedUse");
            } else if usable {
                match doors.set_door(state, id, None, feet) {
                    Some(true) => self.fire(state, id, "OnOpen"),
                    Some(false) => self.fire(state, id, "OnClose"),
                    None => {}
                }
            }
        } else if c == "func_button" || c == "func_rot_button" {
            if locked || self.disabled.contains(&id) {
                self.fire(state, id, "OnUseLocked");
            } else {
                self.fire(state, id, "OnPressed");
                self.fire(state, id, "OnIn");
            }
        }
        self.fire(state, id, "OnPlayerUse");
    }

    pub fn step(&mut self, state: &AppState, doors: &mut Doors, feet: DVec3, height: f64, dt: f64) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        self.time += dt;
        for id in std::mem::take(&mut self.start_open) {
            doors.start_open(state, id);
        }
        self.touch_doors(state, doors, feet, height);
        self.touch(state, feet, height);
        for (id, open) in doors.take_events() {
            self.fire(state, id, if open { "OnFullyOpen" } else { "OnFullyClosed" });
        }
        let due: Vec<Id> = self.timers.iter().filter(|(_, at)| **at <= self.time).map(|(id, _)| *id).collect();
        for id in due {
            let refire = state.doc.scene().entity(id).map_or(1.0, |n| float(&n.entity, "RefireTime", 1.0)).max(0.01);
            self.timers.insert(id, self.time + refire);
            self.fire(state, id, "OnTimer");
        }
        for _ in 0..MAX_PER_FRAME {
            let Some(i) = self
                .queue
                .iter()
                .enumerate()
                .filter(|(_, p)| p.at <= self.time)
                .min_by(|a, b| a.1.at.total_cmp(&b.1.at))
                .map(|(i, _)| i)
            else {
                break;
            };
            let p = self.queue.remove(i);
            self.input(state, doors, &p, feet);
        }
    }

    fn touch_doors(&mut self, state: &AppState, doors: &mut Doors, feet: DVec3, height: f64) {
        for id in self.touch_doors.clone() {
            if self.hidden.contains(&id) {
                continue;
            }
            let touching = doors.touches(state, id, feet, height);
            let new = touching && self.touching.insert(id);
            if !touching {
                self.touching.remove(&id);
            }
            if !new || !doors.is_closed(id) {
                continue;
            }
            if self.locked.contains(&id) {
                self.fire(state, id, "OnLockedUse");
            } else if doors.set_door(state, id, Some(true), feet) == Some(true) {
                self.fire(state, id, "OnOpen");
            }
        }
    }

    fn touch(&mut self, state: &AppState, feet: DVec3, height: f64) {
        let scene = state.doc.scene();
        let key = (state.doc.scene_revision(), scene.entities().len());
        if self.triggers.as_ref().is_none_or(|(k, _)| *k != key) {
            let list = scene
                .entities()
                .values()
                .map(|n| (n.entity.id, class(&n.entity)))
                .filter(|(_, c)| c.starts_with("trigger_"))
                .collect();
            self.triggers = Some((key, list));
        }
        let hull = hp_geom::Aabb::new(
            feet - DVec3::new(player::HALF_WIDTH, player::HALF_WIDTH, 0.0),
            feet + DVec3::new(player::HALF_WIDTH, player::HALF_WIDTH, height),
        );
        let mut now: Vec<(Id, String)> = Vec::new();
        for (id, c) in self.triggers.as_ref().map_or(&[][..], |(_, list)| list.as_slice()) {
            if self.disabled.contains(id) || self.hidden.contains(id) {
                continue;
            }
            let moved = self.moved.get(id);
            let touches = scene.entity_solid_ids(*id).into_iter().filter_map(|s| scene.solid(s)).any(|s| {
                let b = moved.map_or(s.bounds, |m| moved_bounds(s.bounds, m));
                s.owner == hp_editor::Owner::Entity(*id) && b.min.cmple(hull.max).all() && b.max.cmpge(hull.min).all()
            });
            if touches {
                now.push((*id, c.clone()));
            }
        }
        let left: Vec<Id> = self.inside.keys().filter(|id| !now.iter().any(|(n, _)| n == *id)).copied().collect();
        for id in left {
            self.inside.remove(&id);
            self.fire(state, id, "OnEndTouch");
            self.fire(state, id, "OnEndTouchAll");
        }
        for (id, c) in now {
            match self.inside.get(&id).copied() {
                None => {
                    self.fire(state, id, "OnStartTouch");
                    self.fire(state, id, "OnStartTouchAll");
                    let wait = scene.entity(id).map_or(1.0, |n| float(&n.entity, "wait", 1.0));
                    if c == "trigger_once" || c == "trigger_multiple" {
                        self.fire(state, id, "OnTrigger");
                    }
                    if c == "trigger_once" {
                        self.disabled.insert(id);
                    }
                    let again =
                        if c == "trigger_multiple" && wait >= 0.0 { self.time + wait.max(0.05) } else { f64::MAX };
                    self.inside.insert(id, again);
                }
                Some(at) if at <= self.time => {
                    self.fire(state, id, "OnTrigger");
                    let wait = scene.entity(id).map_or(1.0, |n| float(&n.entity, "wait", 1.0)).max(0.05);
                    self.inside.insert(id, self.time + wait);
                }
                Some(_) => {}
            }
        }
    }

    fn input(&mut self, state: &AppState, doors: &mut Doors, p: &Pending, feet: DVec3) {
        let scene = state.doc.scene();
        let target = p.target.trim();
        let targets: Vec<Id> = match target.to_ascii_lowercase().as_str() {
            "!self" | "!caller" => p.caller.into_iter().collect(),
            "!player" | "!activator" | "!pvsplayer" => Vec::new(),
            _ => scene.entities().values().filter(|n| names(&n.entity, target)).map(|n| n.entity.id).collect(),
        };
        let input = p.input.trim();
        if targets.is_empty() {
            self.note(tf!("viewport.io.no_target", target, input));
            return;
        }
        let mut known = false;
        for id in targets {
            let Some(node) = scene.entity(id) else { continue };
            let e = &node.entity;
            known |= self.apply(state, doors, id, e, input, &p.param, feet);
        }
        let line = if p.param.trim().is_empty() {
            format!("-> {target}.{input}")
        } else {
            format!("-> {target}.{input}({})", p.param.trim())
        };
        if known {
            self.note(line);
        } else {
            self.note(format!("{line} {}", t!("viewport.io.not_simulated")));
        }
    }

    pub fn take_light_inputs(&mut self) -> Vec<(String, LightCmd)> {
        std::mem::take(&mut self.light_inputs)
    }

    pub fn projector_overrides(&self) -> Vec<hp_app::state::ProjectorOverride> {
        let mut out: Vec<hp_app::state::ProjectorOverride> = self.projectors.values().cloned().collect();
        out.sort_by_key(|o| o.id);
        out
    }

    pub fn take_train_inputs(&mut self) -> Vec<(Id, String, String)> {
        std::mem::take(&mut self.train_inputs)
    }

    pub fn set_moved(&mut self, moved: &[(Id, glam::DMat4)]) {
        self.moved.clear();
        self.moved.extend(moved.iter().copied());
    }

    #[allow(clippy::too_many_arguments)]
    fn apply(
        &mut self,
        state: &AppState,
        doors: &mut Doors,
        id: Id,
        e: &Entity,
        input: &str,
        param: &str,
        feet: DVec3,
    ) -> bool {
        let c = class(e);
        let input = input.to_ascii_lowercase();
        match input.as_str() {
            x if crate::trains::accepts(&c, x) => {
                self.train_inputs.push((id, input.clone(), param.to_owned()));
            }
            "activate" if c == "infodecal" => {
                self.hidden.remove(&id);
            }
            "turnon" | "turnoff" | "fov" | "enableshadows" | "spotlighttexture" | "lightworld"
                if c == "env_projectedtexture" =>
            {
                let o = self
                    .projectors
                    .entry(id)
                    .or_insert_with(|| hp_app::state::ProjectorOverride { id, ..Default::default() });
                let truth = |p: &str| p.trim().parse::<f64>().map_or(!p.trim().is_empty(), |v| v != 0.0);
                match input.as_str() {
                    "turnon" => o.on = Some(true),
                    "turnoff" => o.on = Some(false),
                    "fov" => o.fov = param.trim().parse::<f64>().ok().filter(|v| v.is_finite()).or(o.fov),
                    "enableshadows" => o.shadows = Some(truth(param)),
                    "lightworld" => o.light_world = Some(truth(param)),
                    _ => o.texture = Some(param.trim().to_owned()).filter(|t| !t.is_empty()).or(o.texture.take()),
                }
            }
            "lighton" | "lightoff" if c == "point_spotlight" => {
                let o = self
                    .projectors
                    .entry(id)
                    .or_insert_with(|| hp_app::state::ProjectorOverride { id, ..Default::default() });
                o.on = Some(input == "lighton");
            }
            "kill" | "killhierarchy" | "break" => {
                self.hidden.insert(id);
                self.disabled.insert(id);
                self.timers.remove(&id);
            }
            "fireuser1" | "fireuser2" | "fireuser3" | "fireuser4" => {
                let n = &input[8..];
                self.fire(state, id, &format!("OnUser{n}"));
            }
            "open" | "close" | "toggle" if is_door(&c) => {
                if !self.locked.contains(&id) || input == "close" {
                    let open = match input.as_str() {
                        "open" => Some(true),
                        "close" => Some(false),
                        _ => None,
                    };
                    match doors.set_door(state, id, open, feet) {
                        Some(true) => self.fire(state, id, "OnOpen"),
                        Some(false) => self.fire(state, id, "OnClose"),
                        None => {}
                    }
                }
            }
            "turnon" | "turnoff" | "toggle" | "setpattern" | "fadetopattern" if is_light(&c) => {
                let Some(name) = e.get("targetname").map(str::trim).filter(|n| !n.is_empty()) else { return false };
                let cmd = match input.as_str() {
                    "turnon" => LightCmd::On,
                    "turnoff" => LightCmd::Off,
                    "toggle" => LightCmd::Toggle,
                    _ => LightCmd::Pattern(param.trim().to_owned()),
                };
                self.light_inputs.push((name.to_owned(), cmd));
            }
            "lock" => {
                self.locked.insert(id);
            }
            "unlock" => {
                self.locked.remove(&id);
            }
            "open" | "close" | "toggle" if c == "func_areaportal" || c == "func_areaportalwindow" => {
                let name = e.get("targetname").unwrap_or_default().trim().to_owned();
                let start_open = e.get("StartOpen").is_none_or(|v| v.trim() != "0");
                let now = self.portals.get(&name).copied().unwrap_or(start_open);
                let open = match input.as_str() {
                    "open" => true,
                    "close" => false,
                    _ => !now,
                };
                self.portals.insert(name, open);
            }
            "trigger" if c == "logic_relay" => {
                if !self.disabled.contains(&id) {
                    self.fire(state, id, "OnTrigger");
                    if flag(e, 1) {
                        self.disabled.insert(id);
                    }
                }
            }
            "press" if c == "func_button" || c == "func_rot_button" => {
                self.fire(state, id, "OnPressed");
            }
            "enable" => {
                self.disabled.remove(&id);
                if toggles_visibility(&c) {
                    self.hidden.remove(&id);
                }
                if c == "logic_timer" {
                    self.timers.insert(id, self.time + float(e, "RefireTime", 1.0).max(0.01));
                }
            }
            "disable" => {
                self.disabled.insert(id);
                if toggles_visibility(&c) {
                    self.hidden.insert(id);
                }
                self.timers.remove(&id);
                self.inside.remove(&id);
            }
            "toggle" => {
                if !self.disabled.remove(&id) {
                    self.disabled.insert(id);
                }
                if toggles_visibility(&c) && !self.hidden.remove(&id) {
                    self.hidden.insert(id);
                }
            }
            "showsprite" | "turnon" | "show" => {
                self.hidden.remove(&id);
            }
            "hidesprite" | "turnoff" | "hide" => {
                if c.starts_with("env_sprite") || c.starts_with("func_") || c.starts_with("prop_") {
                    self.hidden.insert(id);
                }
            }
            "firetimer" if c == "logic_timer" => {
                self.fire(state, id, "OnTimer");
            }
            "refiretime" if c == "logic_timer" => {
                if let Ok(v) = param.trim().parse::<f64>()
                    && self.timers.contains_key(&id)
                {
                    self.timers.insert(id, self.time + v.max(0.01));
                }
            }
            "add" | "subtract" | "setvalue" if c == "math_counter" => {
                let v: f64 = param.trim().parse().unwrap_or(0.0);
                let old = self.counters.get(&id).copied().unwrap_or(0.0);
                let new = match input.as_str() {
                    "add" => old + v,
                    "subtract" => old - v,
                    _ => v,
                };
                let (min, max) = (float(e, "min", 0.0), float(e, "max", 0.0));
                let new = if max > min { new.clamp(min, max) } else { new };
                self.counters.insert(id, new);
                self.fire(state, id, "OutValue");
                if max > min && new >= max && old < max {
                    self.fire(state, id, "OnHitMax");
                }
                if max > min && new <= min && old > min {
                    self.fire(state, id, "OnHitMin");
                }
            }
            "teleport" if c == "point_teleport" => {
                if let Some(o) = e.origin().filter(|o| o.is_finite()) {
                    let yaw = e.angles().map(|a| a.y);
                    self.teleport = Some(match self.moved.get(&id) {
                        Some(m) => {
                            let (sy, cy) = yaw.unwrap_or(0.0).to_radians().sin_cos();
                            let f = m.transform_vector3(DVec3::new(cy, sy, 0.0));
                            (m.transform_point3(o), Some(f.y.atan2(f.x).to_degrees()))
                        }
                        None => (o, yaw),
                    });
                }
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_targetnames_classes_and_wildcards() {
        let mut e = Entity { classname: "func_door".into(), ..Default::default() };
        e.set("targetname", "door_left");
        assert!(names(&e, "door_left"));
        assert!(names(&e, "DOOR_LEFT"));
        assert!(names(&e, "door_*"));
        assert!(names(&e, "func_door"));
        assert!(!names(&e, "door_right"));
        assert!(!names(&e, ""));
    }

    #[test]
    fn light_inputs_reach_the_named_lights() {
        let mut state = hp_app::testing::test_state();
        let mut lamp = Entity { id: 2, classname: "light_spot".into(), ..Default::default() };
        lamp.set("targetname", "lamp");
        let mut sprite = Entity { id: 4, classname: "env_sprite".into(), ..Default::default() };
        sprite.set("targetname", "glow");
        let mut relay = Entity { id: 3, classname: "logic_relay".into(), ..Default::default() };
        relay.set("targetname", "relay");
        let c = |target: &str, input: &str, param: &str| hp_vmf::Connection {
            output: "OnTrigger".into(),
            target: target.into(),
            input: input.into(),
            param: param.into(),
            delay: 0.0,
            times: -1,
            separator: ',',
        };
        relay.connections = vec![
            c("lamp", "TurnOff", ""),
            c("lamp", "SetPattern", "aaazzz"),
            c("LAMP", "Toggle", ""),
            c("lamp", "TurnOn", ""),
            c("glow", "TurnOff", ""),
        ];
        let map = hp_vmf::Map { entities: vec![lamp, sprite, relay], ..Default::default() };
        state.doc = hp_editor::Document::from_map(map);
        let mut sim = IoSim::default();
        let mut doors = Doors::default();
        sim.fire(&state, 3, "OnTrigger");
        sim.step(&state, &mut doors, DVec3::ZERO, 72.0, 0.0);
        let want = [LightCmd::Off, LightCmd::Pattern("aaazzz".into()), LightCmd::Toggle, LightCmd::On];
        let got = sim.take_light_inputs();
        assert_eq!(got, want.map(|c| ("lamp".to_owned(), c)), "the light inputs, not the sprite's");
        assert!(sim.take_light_inputs().is_empty(), "taken once");
        assert!(sim.hidden.contains(&4), "the sprite is hidden as before");
    }

    #[test]
    fn named_decals_wait_for_their_activate_input() {
        let mut state = hp_app::testing::test_state();
        let mut named = Entity { id: 2, classname: "infodecal".into(), ..Default::default() };
        named.set("targetname", "tag");
        let plain = Entity { id: 4, classname: "infodecal".into(), ..Default::default() };
        let mut relay = Entity { id: 3, classname: "logic_relay".into(), ..Default::default() };
        relay.connections = vec![hp_vmf::Connection {
            output: "OnTrigger".into(),
            target: "tag".into(),
            input: "Activate".into(),
            param: String::new(),
            delay: 0.0,
            times: -1,
            separator: ',',
        }];
        let map = hp_vmf::Map { entities: vec![named, plain, relay], ..Default::default() };
        state.doc = hp_editor::Document::from_map(map);
        let mut sim = IoSim::new(&state);
        assert!(sim.hidden.contains(&2) && !sim.hidden.contains(&4), "a named decal waits, like in game");
        let mut doors = Doors::default();
        sim.fire(&state, 3, "OnTrigger");
        sim.step(&state, &mut doors, DVec3::ZERO, 72.0, 0.0);
        assert!(!sim.hidden.contains(&2), "Activate applies it");
    }

    #[test]
    fn projected_texture_inputs_drive_the_preview() {
        let mut state = hp_app::testing::test_state();
        let mut torch = Entity { id: 2, classname: "env_projectedtexture".into(), ..Default::default() };
        torch.set("targetname", "torch");
        let mut relay = Entity { id: 3, classname: "logic_relay".into(), ..Default::default() };
        relay.set("targetname", "relay");
        let c = |input: &str, param: &str| hp_vmf::Connection {
            output: "OnTrigger".into(),
            target: "torch".into(),
            input: input.into(),
            param: param.into(),
            delay: 0.0,
            times: -1,
            separator: ',',
        };
        relay.connections = vec![
            c("TurnOn", ""),
            c("FOV", "70"),
            c("EnableShadows", "0"),
            c("SpotlightTexture", "effects/flashlight002"),
            c("TurnOff", ""),
        ];
        let map = hp_vmf::Map { entities: vec![torch, relay], ..Default::default() };
        state.doc = hp_editor::Document::from_map(map);
        let mut sim = IoSim::default();
        let mut doors = Doors::default();
        sim.fire(&state, 3, "OnTrigger");
        sim.step(&state, &mut doors, DVec3::ZERO, 72.0, 0.0);
        let got = sim.projector_overrides();
        assert_eq!(got.len(), 1);
        let o = &got[0];
        assert_eq!((o.id, o.on, o.fov, o.shadows), (2, Some(false), Some(70.0), Some(false)));
        assert_eq!(o.texture.as_deref(), Some("effects/flashlight002"));
        assert!(!sim.hidden.contains(&2), "TurnOff does not hide the entity, it switches the light");
    }

    #[test]
    fn spotlight_inputs_switch_the_beam_and_its_light() {
        let mut state = hp_app::testing::test_state();
        let mut spot = Entity { id: 2, classname: "point_spotlight".into(), ..Default::default() };
        spot.set("targetname", "beam");
        let mut relay = Entity { id: 3, classname: "logic_relay".into(), ..Default::default() };
        let c = |input: &str| hp_vmf::Connection {
            output: "OnTrigger".into(),
            target: "beam".into(),
            input: input.into(),
            param: String::new(),
            delay: 0.0,
            times: -1,
            separator: ',',
        };
        relay.connections = vec![c("LightOff")];
        let map = hp_vmf::Map { entities: vec![spot, relay.clone()], ..Default::default() };
        state.doc = hp_editor::Document::from_map(map);
        let mut sim = IoSim::default();
        let mut doors = Doors::default();
        sim.fire(&state, 3, "OnTrigger");
        sim.step(&state, &mut doors, DVec3::ZERO, 72.0, 0.0);
        assert_eq!(sim.projector_overrides().iter().map(|o| (o.id, o.on)).collect::<Vec<_>>(), [(2, Some(false))]);
        relay.connections = vec![c("LightOn")];
        state.doc.edit_entity(3, |e| e.connections = relay.connections.clone()).expect("edit");
        sim.fire(&state, 3, "OnTrigger");
        sim.step(&state, &mut doors, DVec3::ZERO, 72.0, 0.0);
        assert_eq!(sim.projector_overrides()[0].on, Some(true));
        assert!(!sim.hidden.contains(&2));
    }
}
