use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::{DMat4, DVec3};
use hp_app::AppState;
use hp_app::runtime::TrainReport;
use hp_editor::limits::world::BspWorld;
use hp_vmf::{Entity, Id};

use crate::player;

const SF_NO_PITCH: i64 = 1;
const SF_NO_CONTROL: i64 = 2;
const SF_PASSABLE: i64 = 8;
const SF_FIXED_ORIENTATION: i64 = 16;
const SF_ALLOW_ROLL: i64 = 1024;
const SF_GOLDSRC: i64 = 32768;

const PATH_DISABLED: i64 = 1;
const PATH_FIRE_ONCE: i64 = 2;
const PATH_ALT_REVERSE: i64 = 4;
const PATH_DISABLE_TRAIN: i64 = 8;
const PATH_TELEPORT: i64 = 16;

const CORNER_WAIT: i64 = 1;
const CORNER_TELEPORT: i64 = 2;

const START_DELAY: f64 = 0.1;
const TICK: f64 = 0.015;
const LOOK_TIME: f64 = 0.1;
const DEFAULT_LOOK: f64 = 100.0;
const MAX_HOPS: usize = 256;
const PARENT_DEPTH: usize = 16;
const STAND_REACH: f64 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrainKind {
    Track,
    Plain,
}

pub fn train_kind(class: &str) -> Option<TrainKind> {
    let c = class.trim();
    if c.eq_ignore_ascii_case("func_tracktrain") || c.eq_ignore_ascii_case("func_tanktrain") {
        Some(TrainKind::Track)
    } else if c.eq_ignore_ascii_case("func_train") {
        Some(TrainKind::Plain)
    } else {
        None
    }
}

fn node_class(class: &str) -> Option<bool> {
    let c = class.trim();
    if c.eq_ignore_ascii_case("path_track") {
        Some(true)
    } else if c.eq_ignore_ascii_case("path_corner") {
        Some(false)
    } else {
        None
    }
}

pub fn accepts(class: &str, input: &str) -> bool {
    let input = input.trim().to_ascii_lowercase();
    if let Some(kind) = train_kind(class) {
        let list: &[&str] = match kind {
            TrainKind::Track => &[
                "startforward",
                "startbackward",
                "stop",
                "toggle",
                "resume",
                "reverse",
                "setspeed",
                "setspeeddir",
                "setspeeddiraccel",
                "setspeedreal",
                "setmaxspeed",
                "teleporttopathtrack",
                "teleporttopathnode",
                "lockorientation",
                "unlockorientation",
            ],
            TrainKind::Plain => &["toggle", "start", "resume", "stop", "setspeed"],
        };
        return list.contains(&input.as_str());
    }
    match node_class(class) {
        Some(true) => matches!(
            input.as_str(),
            "enablepath"
                | "disablepath"
                | "togglepath"
                | "enablealternatepath"
                | "disablealternatepath"
                | "togglealternatepath"
        ),
        Some(false) => input == "setnextpathcorner",
        None => false,
    }
}

fn spawnflags(e: &Entity) -> i64 {
    e.get("spawnflags").and_then(|s| s.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).map_or(0, |v| v as i64)
}

fn num(e: &Entity, key: &str, default: f64) -> f64 {
    e.get(key).and_then(|s| s.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).unwrap_or(default)
}

fn lower_name(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

fn angle_diff(a: f64, b: f64) -> f64 {
    (a - b + 180.0).rem_euclid(360.0) - 180.0
}

fn approach_angle(target: f64, value: f64, speed: f64) -> f64 {
    let speed = speed.abs();
    let d = angle_diff(target, value);
    if d > speed {
        value + speed
    } else if d < -speed {
        value - speed
    } else {
        value + d
    }
}

fn spline(p: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    p * p * (3.0 - 2.0 * p)
}

fn vector_angles(dir: DVec3) -> Option<DVec3> {
    if !dir.is_finite() || dir.length_squared() < 1e-12 {
        return None;
    }
    let xy = (dir.x * dir.x + dir.y * dir.y).sqrt();
    let yaw = if xy < 1e-9 { 0.0 } else { dir.y.atan2(dir.x).to_degrees() };
    Some(DVec3::new((-dir.z).atan2(xy).to_degrees(), yaw, 0.0))
}

fn rotation(a: DVec3) -> DMat4 {
    DMat4::from_mat3(hp_geom::angle_matrix(a))
}

fn inverse_rotation(a: DVec3) -> DMat4 {
    DMat4::from_mat3(hp_geom::angle_matrix(a).transpose())
}

fn trunc(v: DVec3) -> DVec3 {
    let f = v.as_vec3();
    DVec3::new(f64::from(f.x.trunc()), f64::from(f.y.trunc()), f64::from(f.z.trunc()))
}

#[derive(Clone, Debug)]
struct Node {
    id: Id,
    name: String,
    origin: DVec3,
    angles: DVec3,
    target_name: String,
    alt_name: String,
    target: Option<usize>,
    alt: Option<usize>,
    prev: Option<usize>,
    flags: i64,
    disabled: bool,
    alt_on: bool,
    speed: f64,
    wait: f64,
    orient: i64,
    track: bool,
}

#[derive(Clone, Copy, Debug)]
enum Facing {
    Angles(DVec3),
    Dir(DVec3),
}

#[derive(Debug, Default)]
struct Graph {
    nodes: Vec<Node>,
    names: HashMap<String, usize>,
}

impl Graph {
    fn find(&self, name: &str) -> Option<usize> {
        self.names.get(&lower_name(name)).copied()
    }

    fn next(&self, n: usize) -> Option<usize> {
        let nd = &self.nodes[n];
        if nd.alt_on && nd.flags & PATH_ALT_REVERSE == 0 && nd.alt.is_some() { nd.alt } else { nd.target }
    }

    fn prev(&self, n: usize) -> Option<usize> {
        let nd = &self.nodes[n];
        if nd.alt_on && nd.flags & PATH_ALT_REVERSE != 0 && nd.alt.is_some() { nd.alt } else { nd.prev }
    }

    fn next_in_dir(&self, n: usize, fwd: bool) -> Option<usize> {
        if fwd { self.next(n) } else { self.prev(n) }
    }

    fn valid_next(&self, n: usize, fwd: bool) -> Option<usize> {
        self.next_in_dir(n, fwd).filter(|&m| !self.nodes[m].disabled)
    }

    fn link(&mut self) {
        let count = self.nodes.len();
        for i in 0..count {
            let target = self.find(&self.nodes[i].target_name.clone()).filter(|&t| t != i);
            let alt =
                if self.nodes[i].track { self.find(&self.nodes[i].alt_name.clone()).filter(|&t| t != i) } else { None };
            self.nodes[i].target = target;
            self.nodes[i].alt = alt;
        }
        for pass in [false, true] {
            for i in 0..count {
                let link = if pass { self.nodes[i].alt } else { self.nodes[i].target };
                let Some(t) = link else { continue };
                let from_alt = self.nodes[t].alt_name.eq_ignore_ascii_case(&self.nodes[i].name);
                if self.nodes[t].prev.is_none() && !from_alt {
                    self.nodes[t].prev = Some(i);
                }
            }
        }
    }

    fn look_ahead(&self, pos: DVec3, from: usize, to: Option<usize>, fwd: bool, dist: f64) -> DVec3 {
        let mut pos = pos;
        let mut last = from;
        let mut ahead = to.or_else(|| self.next_in_dir(from, fwd));
        let mut left = dist.max(0.0);
        for _ in 0..MAX_HOPS {
            let Some(t) = ahead else {
                let end = self.nodes[last].origin;
                let dir = self
                    .next_in_dir(last, !fwd)
                    .map_or(DVec3::ZERO, |b| (end - self.nodes[b].origin).normalize_or_zero());
                return end + dir * left;
            };
            let d = self.nodes[t].origin - pos;
            let len = d.length();
            if len > left {
                return pos + d * (left / len);
            }
            left -= len;
            pos = self.nodes[t].origin;
            last = t;
            ahead = self.next_in_dir(t, fwd);
        }
        pos
    }

    fn look_move(&self, pos: DVec3, from: usize, fwd: bool, dist: f64) -> (DVec3, usize, bool) {
        let mut p = pos;
        let mut cur = from;
        let mut left = dist.max(0.0);
        for _ in 0..MAX_HOPS {
            let Some(n) = self.valid_next(cur, fwd) else { return (p, cur, true) };
            let d = self.nodes[n].origin - p;
            let len = d.length();
            if len > left {
                return (p + d * (left / len), cur, false);
            }
            left -= len;
            p = self.nodes[n].origin;
            cur = n;
        }
        (p, cur, false)
    }

    fn orientation(&self, n: usize, fwd: bool) -> Option<Facing> {
        let nd = &self.nodes[n];
        match nd.orient {
            2 => Some(Facing::Angles(nd.angles)),
            0 => None,
            _ => {
                let dir = match self.next_in_dir(n, fwd) {
                    Some(m) => self.nodes[m].origin - nd.origin,
                    None => nd.origin - self.nodes[self.next_in_dir(n, !fwd)?].origin,
                };
                Some(Facing::Dir(if fwd { dir } else { -dir }))
            }
        }
    }
}

#[derive(Default)]
struct Events {
    fired: Vec<(Id, &'static str)>,
    once: HashSet<usize>,
}

impl Events {
    fn pass(&mut self, g: &Graph, n: usize) {
        let nd = &g.nodes[n];
        if nd.track && nd.flags & PATH_FIRE_ONCE != 0 && !self.once.insert(n) {
            return;
        }
        self.fired.push((nd.id, "OnPass"));
    }
}

#[derive(Clone, Debug)]
struct Part {
    owner: Id,
    own: bool,
    tri: [DVec3; 3],
}

#[derive(Clone, Debug)]
struct Train {
    id: Id,
    name: String,
    class: String,
    kind: TrainKind,
    flags: i64,
    target: String,
    max_speed: f64,
    speed: f64,
    dir: f64,
    last_speed: f64,
    height: f64,
    length: f64,
    bank: f64,
    orient_type: i64,
    velocity_type: i64,
    spawn_origin: DVec3,
    spawn_angles: DVec3,
    compiled: Option<(DVec3, DVec3)>,
    origin: DVec3,
    angles: DVec3,
    seg_angles: DVec3,
    at: Option<usize>,
    to: Option<usize>,
    activated: bool,
    locked: bool,
    no_control: bool,
    wait: Option<f64>,
    retrigger: bool,
    parts: Vec<Part>,
    children: Vec<Id>,
    moved: Vec<(Id, [DVec3; 3])>,
    moved_key: Option<DMat4>,
    bounds: hp_geom::Aabb,
    before: DMat4,
}

impl Train {
    fn delta(&self) -> DMat4 {
        DMat4::from_translation(self.origin)
            * rotation(self.angles)
            * inverse_rotation(self.spawn_angles)
            * DMat4::from_translation(-self.spawn_origin)
    }

    fn editor(&self) -> DMat4 {
        DMat4::from_translation(self.origin) * rotation(self.angles) * DMat4::from_translation(-self.spawn_origin)
    }

    fn own(&self) -> DMat4 {
        match self.compiled {
            Some((o, a)) => {
                DMat4::from_translation(self.origin)
                    * rotation(self.angles)
                    * inverse_rotation(a)
                    * DMat4::from_translation(-o)
            }
            None => self.editor(),
        }
    }

    fn turns(&self) -> bool {
        self.kind == TrainKind::Track && self.flags & SF_FIXED_ORIENTATION == 0 && self.orient_type != 0 && !self.locked
    }

    fn look_distance(&self) -> f64 {
        if self.length > 0.0 { self.length } else { DEFAULT_LOOK }
    }

    fn facing(&self, f: Facing) -> Option<DVec3> {
        match f {
            Facing::Angles(a) => Some(a),
            Facing::Dir(d) => {
                let a = vector_angles(d)?;
                Some(if self.flags & SF_GOLDSRC != 0 { DVec3::new(-a.x, a.y + 180.0, 0.0) } else { a })
            }
        }
    }

    fn is_moving(&self) -> bool {
        match self.kind {
            TrainKind::Track => self.speed != 0.0,
            TrainKind::Plain => !self.retrigger && (self.to.is_some() || self.wait.is_some()),
        }
    }

    fn path_pos(&self) -> DVec3 {
        self.origin - DVec3::Z * self.height
    }

    fn set_speed(&mut self, s: f64, ev: &mut Events) {
        let old = self.speed;
        self.speed = self.dir * s;
        if old == 0.0 && self.speed != 0.0 {
            ev.fired.push((self.id, "OnStart"));
        }
        if self.speed == 0.0 && old != 0.0 {
            self.last_speed = old.abs();
        }
    }

    fn place(&mut self, g: &Graph, n: usize) {
        let pos = g.nodes[n].origin;
        self.at = Some(n);
        self.to = None;
        self.origin = pos + DVec3::Z * self.height;
        if self.kind == TrainKind::Track && self.turns() {
            let look = g.look_ahead(pos, n, None, true, self.look_distance());
            if let Some(a) = self.facing(Facing::Dir(look - pos)) {
                let pitch = if self.flags & SF_NO_PITCH != 0 { 0.0 } else { a.x };
                self.angles = DVec3::new(pitch, a.y, 0.0);
            }
        }
        self.seg_angles = self.angles;
    }

    fn activate(&mut self, g: &Graph, ev: &mut Events) {
        let Some(n) = g.find(&self.target) else { return };
        self.activated = true;
        match self.kind {
            TrainKind::Track => {
                self.place(g, n);
                self.arrive(g, ev, n);
            }
            TrainKind::Plain => {
                self.at = Some(n);
                self.origin = g.nodes[n].origin;
                if self.name.is_empty() {
                    self.wait = Some(START_DELAY);
                } else {
                    self.retrigger = true;
                }
            }
        }
    }

    fn arrive(&mut self, g: &Graph, ev: &mut Events, n: usize) {
        ev.pass(g, n);
        let nd = &g.nodes[n];
        if nd.track && nd.flags & PATH_DISABLE_TRAIN != 0 {
            self.no_control = true;
        }
        if self.no_control && nd.speed != 0.0 {
            self.set_speed(nd.speed, ev);
        }
    }

    fn step(&mut self, g: &Graph, ev: &mut Events, dt: f64) {
        self.before = self.delta();
        if !self.activated || dt <= 0.0 {
            return;
        }
        match self.kind {
            TrainKind::Track => self.step_track(g, ev, dt),
            TrainKind::Plain => self.step_plain(g, ev, dt),
        }
    }

    fn blend_speed(&mut self, g: &Graph) {
        let (Some(a), Some(b)) = (self.at, self.to) else { return };
        let prev = if g.nodes[a].speed != 0.0 { g.nodes[a].speed } else { self.speed.abs() };
        let next = if g.nodes[b].speed != 0.0 { g.nodes[b].speed } else { prev };
        let mag = if prev != next {
            let mut p = self.segment_fraction(g, a, b);
            if self.velocity_type == 2 {
                p = spline(p);
            }
            prev * (1.0 - p) + next * p
        } else {
            prev
        };
        self.speed = self.dir * mag;
    }

    fn segment_fraction(&self, g: &Graph, a: usize, b: usize) -> f64 {
        let seg = g.nodes[b].origin - g.nodes[a].origin;
        let len = seg.length();
        if len <= 0.0 { 0.0 } else { ((self.path_pos() - g.nodes[a].origin).length() / len).clamp(0.0, 1.0) }
    }

    fn step_track(&mut self, g: &Graph, ev: &mut Events, dt: f64) {
        let mut left = dt;
        while left > 1e-12 && self.speed != 0.0 && self.at.is_some() {
            let h = left.min(TICK);
            left -= h;
            self.tick_track(g, ev, h);
        }
    }

    fn tick_track(&mut self, g: &Graph, ev: &mut Events, dt: f64) {
        let Some(at) = self.at else { return };
        let fwd = self.speed > 0.0;
        self.to = g.next_in_dir(at, fwd);
        if self.velocity_type == 1 || self.velocity_type == 2 {
            self.blend_speed(g);
        }
        let pos = self.path_pos();
        let (target, node, dead) = g.look_move(pos, at, fwd, self.speed.abs() * LOOK_TIME);
        self.orient(g, dt, fwd);
        let d = target - pos;
        let len = d.length();
        let step = self.speed.abs() * dt;
        let reached = len <= step;
        let next = if reached { target } else { pos + d * (step / len) };
        self.origin = next + DVec3::Z * self.height;
        if dead {
            if reached {
                if node != at {
                    self.pass_to(g, ev, at, node, fwd);
                }
                self.last_speed = self.speed.abs();
                self.speed = 0.0;
            }
        } else if node != at {
            self.pass_to(g, ev, at, node, fwd);
        }
    }

    fn pass_to(&mut self, g: &Graph, ev: &mut Events, from: usize, node: usize, fwd: bool) {
        let mut cur = from;
        for _ in 0..MAX_HOPS {
            let Some(n) = g.valid_next(cur, fwd) else { return };
            cur = n;
            self.at = Some(n);
            self.to = g.next_in_dir(n, fwd);
            self.seg_angles = self.angles;
            self.arrive(g, ev, n);
            if let Some(tp) =
                g.valid_next(n, fwd).filter(|&t| g.nodes[t].track && g.nodes[t].flags & PATH_TELEPORT != 0)
            {
                self.origin = g.nodes[tp].origin + DVec3::Z * self.height;
                self.at = Some(tp);
                self.to = g.next_in_dir(tp, fwd);
                ev.fired.push((g.nodes[tp].id, "OnTeleport"));
                self.arrive(g, ev, tp);
                return;
            }
            if n == node {
                return;
            }
        }
    }

    fn orient(&mut self, g: &Graph, dt: f64, fwd: bool) {
        if !self.turns() {
            return;
        }
        let Some(at) = self.at else { return };
        let cur = self.angles;
        let blend = self.orient_type == 2 || self.orient_type == 3;
        let target = if blend {
            self.blend_angles(g, at, fwd)
        } else {
            let pos = self.path_pos();
            let look = g.look_ahead(pos, at, self.to, fwd, self.look_distance());
            let face = if fwd { look - pos } else { pos - look };
            if face.x.abs() < 1e-9 && face.y.abs() < 1e-9 {
                return;
            }
            self.facing(Facing::Dir(face))
        };
        let Some(t) = target else { return };
        let vx = if self.flags & SF_NO_PITCH != 0 { 0.0 } else { angle_diff(t.x, cur.x) };
        let vy = angle_diff(t.y, cur.y);
        let mut roll = cur.z;
        if blend && self.flags & SF_ALLOW_ROLL != 0 {
            roll = cur.z + angle_diff(t.z, cur.z);
        } else if self.bank != 0.0 {
            let rate = vy / dt;
            let b = self.bank;
            let speed = if rate < -5.0 {
                angle_diff(approach_angle(-b, cur.z, b * 2.0), cur.z)
            } else if rate > 5.0 {
                angle_diff(approach_angle(b, cur.z, b * 2.0), cur.z)
            } else {
                angle_diff(approach_angle(0.0, cur.z, b * 4.0), cur.z) * 4.0
            };
            roll = cur.z + speed * dt;
        }
        self.angles = DVec3::new(cur.x + vx, cur.y + vy, roll);
    }

    fn blend_angles(&self, g: &Graph, at: usize, fwd: bool) -> Option<DVec3> {
        let prev = g.orientation(at, fwd).and_then(|f| self.facing(f)).unwrap_or(self.seg_angles);
        let to = self.to.or_else(|| g.next_in_dir(at, fwd));
        let mut next = to.and_then(|n| g.orientation(n, fwd)).and_then(|f| self.facing(f)).unwrap_or(prev);
        if self.flags & SF_NO_PITCH != 0 {
            next.x = prev.x;
        }
        let Some(to) = to.filter(|_| prev != next) else { return Some(next) };
        let mut p = self.segment_fraction(g, at, to);
        if self.orient_type == 3 {
            p = spline(p);
        }
        let q = hp_geom::angles_to_quat(prev).slerp(hp_geom::angles_to_quat(next), p);
        Some(hp_geom::quat_to_angles(q))
    }

    fn step_plain(&mut self, g: &Graph, ev: &mut Events, dt: f64) {
        let mut left = dt;
        for _ in 0..MAX_HOPS {
            if left <= 0.0 {
                return;
            }
            if let Some(w) = self.wait.as_mut() {
                if *w > left {
                    *w -= left;
                    return;
                }
                left -= *w;
                self.wait = None;
                self.depart(g, ev, 0);
                continue;
            }
            if self.retrigger {
                return;
            }
            let Some(to) = self.to else { return };
            let speed = self.speed.abs();
            if speed <= 0.0 {
                return;
            }
            let target = g.nodes[to].origin;
            let d = target - self.origin;
            let len = d.length();
            let step = speed * left;
            if len > step {
                self.origin += d * (step / len);
                return;
            }
            left -= len / speed;
            self.origin = target;
            self.at = Some(to);
            self.to = None;
            self.arrive_corner(g, ev, 0);
        }
    }

    fn depart(&mut self, g: &Graph, ev: &mut Events, depth: usize) {
        let Some(at) = self.at else { return };
        let Some(n) = g.next(at) else {
            self.to = None;
            return;
        };
        if g.nodes[at].speed != 0.0 {
            self.speed = g.nodes[at].speed.abs();
        }
        if !g.nodes[n].track && g.nodes[n].flags & CORNER_TELEPORT != 0 {
            self.origin = g.nodes[n].origin;
            self.at = Some(n);
            self.to = None;
            self.arrive_corner(g, ev, depth + 1);
        } else {
            self.to = Some(n);
        }
    }

    fn arrive_corner(&mut self, g: &Graph, ev: &mut Events, depth: usize) {
        let Some(at) = self.at else { return };
        ev.pass(g, at);
        let nd = &g.nodes[at];
        if self.retrigger || (!nd.track && nd.flags & CORNER_WAIT != 0) {
            self.retrigger = true;
            return;
        }
        if nd.wait < 0.0 {
            self.retrigger = true;
        } else if nd.wait > 0.0 {
            self.wait = Some(nd.wait);
        } else if depth < MAX_HOPS {
            self.depart(g, ev, depth);
        }
    }

    fn start(&mut self, g: &Graph, ev: &mut Events) {
        match self.kind {
            TrainKind::Track => {
                if self.speed == 0.0 {
                    self.set_speed(self.max_speed, ev);
                }
            }
            TrainKind::Plain => {
                if self.retrigger || !self.is_moving() {
                    self.retrigger = false;
                    self.wait = None;
                    if self.to.is_none() {
                        self.depart(g, ev, 0);
                    }
                }
            }
        }
    }

    fn stop(&mut self, ev: &mut Events) {
        match self.kind {
            TrainKind::Track => self.set_speed(0.0, ev),
            TrainKind::Plain => {
                self.retrigger = true;
                self.wait = None;
            }
        }
    }

    fn toggle(&mut self, g: &Graph, ev: &mut Events) {
        if self.is_moving() {
            self.stop(ev);
        } else {
            self.start(g, ev);
        }
    }

    fn set_dir(&mut self, forward: bool) {
        let dir = if forward { 1.0 } else { -1.0 };
        if dir == self.dir {
            return;
        }
        self.dir = dir;
        self.speed = -self.speed;
        if let Some(to) = self.to {
            self.to = self.at;
            self.at = Some(to);
        }
        self.seg_angles = self.angles;
    }

    fn input(&mut self, g: &Graph, ev: &mut Events, input: &str, param: &str) {
        let value = param.trim().parse::<f64>().ok().filter(|v| v.is_finite());
        if self.kind == TrainKind::Plain {
            match input {
                "toggle" => self.toggle(g, ev),
                "start" | "resume" => self.start(g, ev),
                "stop" => self.stop(ev),
                "setspeed" => {
                    if let Some(v) = value.filter(|v| *v > 0.0) {
                        self.speed = v;
                    }
                }
                _ => {}
            }
            return;
        }
        match input {
            "startforward" => {
                self.set_dir(true);
                self.set_speed(self.max_speed, ev);
            }
            "startbackward" => {
                self.set_dir(false);
                self.set_speed(self.max_speed, ev);
            }
            "stop" => self.set_speed(0.0, ev),
            "toggle" => {
                let s = if self.speed == 0.0 { self.max_speed } else { 0.0 };
                self.set_speed(s, ev);
            }
            "resume" => {
                if self.speed == 0.0 {
                    let s = if self.last_speed > 0.0 { self.last_speed } else { self.max_speed };
                    self.set_speed(s, ev);
                }
            }
            "reverse" => self.set_dir(self.dir < 0.0),
            "setspeed" => self.set_speed(self.max_speed * value.unwrap_or(0.0).clamp(0.0, 1.0), ev),
            "setspeeddir" | "setspeeddiraccel" => {
                let v = value.unwrap_or(0.0);
                self.set_dir(v >= 0.0);
                self.set_speed(self.max_speed * v.abs().clamp(0.0, 1.0), ev);
            }
            "setspeedreal" => self.set_speed(value.unwrap_or(0.0).clamp(0.0, self.max_speed.max(0.0)), ev),
            "setmaxspeed" => {
                if let Some(v) = value {
                    self.max_speed = v.abs();
                    if self.speed.abs() > self.max_speed {
                        self.speed = self.dir * self.max_speed;
                    }
                }
            }
            "teleporttopathtrack" | "teleporttopathnode" => {
                if let Some(n) = g.find(param) {
                    self.activated = true;
                    self.place(g, n);
                    if input == "teleporttopathnode" {
                        self.set_speed(0.0, ev);
                    }
                }
            }
            "lockorientation" => self.locked = true,
            "unlockorientation" => self.locked = false,
            _ => {}
        }
    }

    fn update_moved(&mut self) {
        let own = self.editor();
        if self.moved_key == Some(own) {
            return;
        }
        let child = self.delta();
        self.moved_key = Some(own);
        self.bounds = hp_geom::Aabb::EMPTY;
        self.moved.clear();
        for p in &self.parts {
            let m = if p.own { own } else { child };
            let t = p.tri.map(|v| m.transform_point3(v));
            for v in t {
                self.bounds.extend(v);
            }
            self.moved.push((p.owner, t));
        }
    }

    fn trace(&self, ray: &hp_geom::Ray, max: f64, hidden: Option<&HashSet<Id>>) -> Option<(f64, Id)> {
        if self.moved.is_empty() || hp_geom::ray_aabb(ray, &self.bounds).is_none_or(|t| t > max) {
            return None;
        }
        let mut best: Option<(f64, Id)> = None;
        for (owner, t) in &self.moved {
            if hidden.is_some_and(|h| h.contains(owner)) {
                continue;
            }
            if let Some(d) = hp_geom::ray_triangle(ray, t[0], t[1], t[2], false)
                && d <= max
                && best.is_none_or(|(b, _)| d < b)
            {
                best = Some((d, *owner));
            }
        }
        best
    }

    fn solid(&self, hidden: Option<&HashSet<Id>>) -> bool {
        self.activated && self.flags & SF_PASSABLE == 0 && hidden.is_none_or(|h| !h.contains(&self.id))
    }
}

pub(crate) fn tri_box(t: &[DVec3; 3], c: DVec3, h: DVec3) -> bool {
    let v = [t[0] - c, t[1] - c, t[2] - c];
    for k in 0..3 {
        let lo = v[0][k].min(v[1][k]).min(v[2][k]);
        let hi = v[0][k].max(v[1][k]).max(v[2][k]);
        if lo > h[k] || hi < -h[k] {
            return false;
        }
    }
    let e = [v[1] - v[0], v[2] - v[1], v[0] - v[2]];
    let n = e[0].cross(e[1]);
    if n.dot(v[0]).abs() > h.dot(n.abs()) {
        return false;
    }
    for edge in e {
        for a in [DVec3::X, DVec3::Y, DVec3::Z] {
            let axis = a.cross(edge);
            if axis.length_squared() < 1e-12 {
                continue;
            }
            let p = [v[0].dot(axis), v[1].dot(axis), v[2].dot(axis)];
            let r = h.dot(axis.abs());
            if p[0].min(p[1]).min(p[2]) > r || p[0].max(p[1]).max(p[2]) < -r {
                return false;
            }
        }
    }
    true
}

struct Brush {
    tris: Vec<[DVec3; 3]>,
    bounds: hp_geom::Aabb,
    origin: bool,
    solid: bool,
}

#[derive(Default)]
pub struct Trains {
    graph: Graph,
    trains: Vec<Train>,
    owned: HashMap<Id, usize>,
    children: HashMap<Id, usize>,
    events: Events,
    world: usize,
}

impl std::fmt::Debug for Trains {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Trains").field("trains", &self.trains.len()).field("nodes", &self.graph.nodes.len()).finish()
    }
}

impl Trains {
    pub fn new(state: &AppState) -> Trains {
        let scene = state.doc.scene();
        let mut ents: Vec<&Entity> = scene.entities().values().map(|n| &n.entity).collect();
        ents.sort_by_key(|e| e.id);
        let mut brushes: HashMap<Id, Vec<Brush>> = HashMap::new();
        let mut solid_ids: Vec<(&Id, &hp_editor::SolidNode)> = scene.solids().iter().collect();
        solid_ids.sort_by_key(|(id, _)| **id);
        for (_, n) in solid_ids {
            let hp_editor::Owner::Entity(owner) = n.owner else { continue };
            let first = n.solid.sides.first().map(|s| s.material.trim().replace('\\', "/")).unwrap_or_default();
            let origin = first.eq_ignore_ascii_case("tools/toolsorigin");
            let solid = n.solid.sides.iter().any(|s| !player::non_solid_material(&s.material));
            let mut tris = Vec::new();
            let mut bounds = hp_geom::Aabb::EMPTY;
            for f in &n.geom.faces {
                for i in 1..f.vertices.len().saturating_sub(1) {
                    tris.push([f.vertices[0], f.vertices[i], f.vertices[i + 1]]);
                }
                for v in &f.vertices {
                    bounds.extend(*v);
                }
            }
            if bounds.is_empty() {
                bounds = n.bounds;
            }
            brushes.entry(owner).or_default().push(Brush { tris, bounds, origin, solid });
        }
        let mut t = Trains::default();
        for e in &ents {
            let Some(track) = node_class(&e.classname) else { continue };
            let name = e.get("targetname").unwrap_or_default().trim().to_owned();
            let idx = t.graph.nodes.len();
            if !name.is_empty() {
                t.graph.names.entry(name.to_ascii_lowercase()).or_insert(idx);
            }
            let flags = spawnflags(e);
            t.graph.nodes.push(Node {
                id: e.id,
                name,
                origin: e.origin().filter(|o| o.is_finite()).unwrap_or(DVec3::ZERO),
                angles: e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO),
                target_name: e.get("target").unwrap_or_default().trim().to_owned(),
                alt_name: e.get("altpath").unwrap_or_default().trim().to_owned(),
                target: None,
                alt: None,
                prev: None,
                flags,
                disabled: track && flags & PATH_DISABLED != 0,
                alt_on: false,
                speed: num(e, "speed", 0.0),
                wait: if track { 0.0 } else { num(e, "wait", 0.0) },
                orient: num(e, "orientationtype", 1.0) as i64,
                track,
            });
        }
        t.graph.link();
        let empty: Vec<Brush> = Vec::new();
        let mut train_names: HashMap<String, usize> = HashMap::new();
        for e in &ents {
            let Some(kind) = train_kind(&e.classname) else { continue };
            let bs = brushes.get(&e.id).unwrap_or(&empty);
            let mut solid_bounds = hp_geom::Aabb::EMPTY;
            for b in bs.iter().filter(|b| !b.origin) {
                solid_bounds.extend(b.bounds.min);
                solid_bounds.extend(b.bounds.max);
            }
            let origin_brush = bs.iter().find(|b| b.origin && !b.bounds.is_empty()).map(|b| trunc(b.bounds.center()));
            let spawn_origin = origin_brush
                .or_else(|| e.origin().filter(|o| o.is_finite()))
                .or_else(|| (!solid_bounds.is_empty()).then(|| trunc(solid_bounds.center())))
                .unwrap_or(DVec3::ZERO);
            let spawn_angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
            let flags = spawnflags(e);
            let (max_speed, speed) = match kind {
                TrainKind::Track => {
                    let initial = num(e, "speed", 0.0);
                    let mut max = num(e, "startspeed", 0.0).abs();
                    if max == 0.0 {
                        max = if initial == 0.0 { 100.0 } else { initial.abs() };
                    }
                    (max, initial)
                }
                TrainKind::Plain => {
                    let s = num(e, "speed", 0.0).abs();
                    let s = if s == 0.0 { 100.0 } else { s };
                    (s, s)
                }
            };
            let parts = bs
                .iter()
                .filter(|b| !b.origin && b.solid)
                .flat_map(|b| b.tris.iter().map(|tri| Part { owner: e.id, own: true, tri: *tri }))
                .collect();
            let name = e.get("targetname").unwrap_or_default().trim().to_owned();
            let idx = t.trains.len();
            if !name.is_empty() {
                train_names.entry(name.to_ascii_lowercase()).or_insert(idx);
            }
            t.trains.push(Train {
                id: e.id,
                name,
                class: e.classname.trim().to_ascii_lowercase(),
                kind,
                flags,
                target: e.get("target").unwrap_or_default().trim().to_owned(),
                max_speed,
                speed,
                dir: 1.0,
                last_speed: 0.0,
                height: num(e, "height", 0.0),
                length: num(e, "wheels", 0.0),
                bank: num(e, "bank", 0.0),
                orient_type: num(e, "orientationtype", 1.0) as i64,
                velocity_type: num(e, "velocitytype", 0.0) as i64,
                spawn_origin,
                spawn_angles,
                compiled: None,
                origin: spawn_origin,
                angles: spawn_angles,
                seg_angles: spawn_angles,
                at: None,
                to: None,
                activated: false,
                locked: false,
                no_control: flags & SF_NO_CONTROL != 0,
                wait: None,
                retrigger: false,
                parts,
                children: Vec::new(),
                moved: Vec::new(),
                moved_key: None,
                bounds: hp_geom::Aabb::EMPTY,
                before: DMat4::IDENTITY,
            });
        }
        if !t.trains.is_empty() {
            let mut by_name: HashMap<String, &Entity> = HashMap::new();
            for e in &ents {
                if let Some(n) = e.get("targetname").map(lower_name).filter(|n| !n.is_empty()) {
                    by_name.entry(n).or_insert(e);
                }
            }
            let parent_of = |e: &Entity| {
                e.get("parentname")
                    .map(|p| lower_name(p.split(',').next().unwrap_or_default()))
                    .filter(|p| !p.is_empty())
            };
            for e in &ents {
                if train_kind(&e.classname).is_some() {
                    continue;
                }
                let mut cur = parent_of(e);
                let mut found = None;
                for _ in 0..PARENT_DEPTH {
                    let Some(name) = cur else { break };
                    if let Some(&ti) = train_names.get(&name) {
                        found = Some(ti);
                        break;
                    }
                    cur = by_name.get(&name).and_then(|p| parent_of(p));
                }
                let Some(ti) = found else { continue };
                t.children.insert(e.id, ti);
                t.trains[ti].children.push(e.id);
                if !player::non_solid_class(&e.classname) {
                    for b in brushes.get(&e.id).unwrap_or(&empty).iter().filter(|b| !b.origin && b.solid) {
                        let parts = b.tris.iter().map(|tri| Part { owner: e.id, own: false, tri: *tri });
                        t.trains[ti].parts.extend(parts);
                    }
                    if scene.is_brush_entity(e.id) {
                        t.owned.insert(e.id, ti);
                    }
                }
            }
        }
        let Trains { graph, trains, events, .. } = &mut t;
        for tr in trains.iter_mut() {
            tr.activate(graph, events);
            tr.before = tr.delta();
            tr.update_moved();
        }
        for (i, tr) in t.trains.iter().enumerate() {
            t.owned.insert(tr.id, i);
        }
        t
    }

    pub fn is_empty(&self) -> bool {
        self.trains.is_empty()
    }

    pub fn set_world(&mut self, world: Option<&Arc<BspWorld>>) {
        let addr = world.map_or(0, |w| Arc::as_ptr(w) as usize);
        if addr == self.world {
            return;
        }
        self.world = addr;
        for tr in &mut self.trains {
            tr.compiled = world.and_then(|w| w.entity_models.iter().find(|m| m.id == tr.id)).map(|m| {
                let v = |a: [f32; 3]| DVec3::new(f64::from(a[0]), f64::from(a[1]), f64::from(a[2]));
                (v(m.origin), v(m.angles))
            });
        }
    }

    pub fn step(&mut self, dt: f64, hidden: &HashSet<Id>) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        let Trains { graph, trains, events, .. } = self;
        for tr in trains.iter_mut() {
            if hidden.contains(&tr.id) {
                tr.before = tr.delta();
                continue;
            }
            tr.step(graph, events, dt);
            tr.update_moved();
        }
    }

    pub fn take_fired(&mut self) -> Vec<(Id, &'static str)> {
        std::mem::take(&mut self.events.fired)
    }

    pub fn input(&mut self, id: Id, input: &str, param: &str) -> bool {
        let input = input.trim().to_ascii_lowercase();
        let Trains { graph, trains, events, .. } = self;
        if let Some(tr) = trains.iter_mut().find(|t| t.id == id) {
            tr.input(graph, events, &input, param);
            return true;
        }
        let Some(n) = graph.nodes.iter().position(|n| n.id == id) else { return false };
        let nd = &mut graph.nodes[n];
        match input.as_str() {
            "enablepath" => nd.disabled = false,
            "disablepath" => nd.disabled = true,
            "togglepath" => nd.disabled = !nd.disabled,
            "enablealternatepath" => nd.alt_on = true,
            "disablealternatepath" => nd.alt_on = false,
            "togglealternatepath" => nd.alt_on = !nd.alt_on,
            "setnextpathcorner" => {
                nd.target_name = param.trim().to_owned();
                let next = graph.find(param).filter(|&m| m != n);
                graph.nodes[n].target = next;
            }
            _ => return false,
        }
        true
    }

    pub fn use_entity(&mut self, id: Id) -> Option<bool> {
        let Trains { graph, trains, events, .. } = self;
        let tr = trains.iter_mut().find(|t| t.id == id && t.activated)?;
        if tr.kind == TrainKind::Track && tr.no_control {
            return Some(false);
        }
        tr.toggle(graph, events);
        Some(true)
    }

    pub fn request(&mut self, name: Option<&str>, start: Option<bool>) -> usize {
        let all = name.is_none_or(|n| n.trim().is_empty() || n.trim() == "*");
        let Trains { graph, trains, events, .. } = self;
        let mut count = 0;
        for tr in trains.iter_mut().filter(|t| t.activated) {
            if !all && !name.is_some_and(|n| tr.name.eq_ignore_ascii_case(n.trim())) {
                continue;
            }
            count += 1;
            match start {
                Some(true) => tr.start(graph, events),
                Some(false) => tr.stop(events),
                None => tr.toggle(graph, events),
            }
        }
        count
    }

    pub fn toggle_all(&mut self) -> bool {
        let any = self.trains.iter().any(|t| t.activated && t.is_moving());
        self.request(None, Some(!any));
        !any
    }

    pub fn transforms(&self) -> Vec<(Id, DMat4)> {
        let mut out = Vec::new();
        for tr in self.trains.iter().filter(|t| t.activated) {
            out.push((tr.id, tr.own()));
            let d = tr.delta();
            out.extend(tr.children.iter().map(|c| (*c, d)));
        }
        out
    }

    pub fn child_transform(&self, id: Id) -> Option<DMat4> {
        let tr = &self.trains[*self.children.get(&id)?];
        tr.activated.then(|| tr.delta())
    }

    pub fn owns(&self, id: Id) -> bool {
        self.owned.get(&id).is_some_and(|&i| self.trains[i].activated)
    }

    pub fn carries(&self, id: Id) -> bool {
        self.owns(id) || self.child_transform(id).is_some()
    }

    pub fn trace(&self, ray: &hp_geom::Ray, max: f64, hidden: Option<&HashSet<Id>>) -> Option<(f64, Id)> {
        let mut best: Option<(f64, Id)> = None;
        for tr in self.trains.iter().filter(|t| t.solid(hidden)) {
            if let Some((d, id)) = tr.trace(ray, best.map_or(max, |b| b.0), hidden) {
                best = Some((d, id));
            }
        }
        best
    }

    pub fn use_trace(&self, ray: &hp_geom::Ray, max: f64, hidden: Option<&HashSet<Id>>) -> Option<(f64, Id)> {
        let mut best: Option<(f64, Id)> = None;
        for tr in self.trains.iter().filter(|t| t.activated && hidden.is_none_or(|h| !h.contains(&t.id))) {
            if let Some((d, id)) = tr.trace(ray, best.map_or(max, |b| b.0), hidden) {
                best = Some((d, id));
            }
        }
        best
    }

    pub fn standing_on(
        &self,
        feet: DVec3,
        hidden: Option<&HashSet<Id>>,
        props: Option<&player::PropBodies>,
    ) -> Option<usize> {
        let r = player::HALF_WIDTH - 1.0;
        let mut best: Option<(f64, usize)> = None;
        for (x, y) in [(0.0, 0.0), (r, r), (-r, r), (r, -r), (-r, -r)] {
            let ray = hp_geom::Ray::new(feet + DVec3::new(x, y, STAND_REACH * 0.5), -DVec3::Z);
            for (i, tr) in self.trains.iter().enumerate().filter(|(_, t)| t.solid(hidden)) {
                if let Some((d, _)) = tr.trace(&ray, STAND_REACH, hidden)
                    && best.is_none_or(|(b, _)| d < b)
                {
                    best = Some((d, i));
                }
            }
            if let Some((d, id, _)) = props.and_then(|p| p.trace(&ray, STAND_REACH, None))
                && let Some(&i) = self.children.get(&id)
                && self.trains[i].activated
                && hidden.is_none_or(|h| !h.contains(&self.trains[i].id))
                && best.is_none_or(|(b, _)| d < b)
            {
                best = Some((d, i));
            }
        }
        best.map(|(_, i)| i)
    }

    pub fn frame(&self, i: usize) -> DMat4 {
        self.trains.get(i).map_or(DMat4::IDENTITY, |t| t.delta() * t.before.inverse())
    }

    fn overlapping(&self, feet: DVec3, height: f64, hidden: Option<&HashSet<Id>>) -> Option<usize> {
        let h = DVec3::new(player::HALF_WIDTH - 0.5, player::HALF_WIDTH - 0.5, height * 0.5 - 0.5);
        let c = feet + DVec3::Z * height * 0.5;
        let (lo, hi) = (c - h, c + h);
        self.trains.iter().position(|t| {
            t.solid(hidden)
                && !t.bounds.is_empty()
                && t.bounds.min.cmple(hi).all()
                && t.bounds.max.cmpge(lo).all()
                && t.moved.iter().any(|(o, tri)| hidden.is_none_or(|s| !s.contains(o)) && tri_box(tri, c, h))
        })
    }

    pub fn move_player(
        &self,
        riding: Option<usize>,
        feet: &mut DVec3,
        yaw: &mut f64,
        height: f64,
        hidden: Option<&HashSet<Id>>,
    ) {
        if let Some(i) = riding {
            let m = self.frame(i);
            *feet = m.transform_point3(*feet);
            let f = m.transform_vector3(DVec3::X);
            let turn = f.y.atan2(f.x).to_degrees();
            if turn.is_finite() && turn.abs() > 1e-9 {
                *yaw += turn;
            }
            return;
        }
        for _ in 0..4 {
            let Some(i) = self.overlapping(*feet, height, hidden) else { return };
            let c = *feet + DVec3::Z * height * 0.5;
            let d = self.frame(i).transform_point3(c) - c;
            if d.length_squared() < 1e-12 {
                return;
            }
            *feet += d + d.normalize_or_zero() * 0.1;
        }
    }

    pub fn report(&self) -> Vec<TrainReport> {
        let node_name = |n: Option<usize>| n.map(|n| self.graph.nodes[n].name.clone()).unwrap_or_default();
        self.trains
            .iter()
            .map(|t| TrainReport {
                id: t.id,
                name: t.name.clone(),
                class: t.class.clone(),
                active: t.activated,
                moving: t.activated && t.is_moving(),
                speed: if t.kind == TrainKind::Plain && !t.is_moving() { 0.0 } else { t.speed },
                max_speed: t.max_speed,
                origin: t.origin,
                angles: t.angles,
                node: node_name(t.at),
                next: node_name(t.to),
                user_control: t.kind == TrainKind::Plain || !t.no_control,
            })
            .collect()
    }

    pub fn moving(&self) -> usize {
        self.trains.iter().filter(|t| t.activated && t.is_moving()).count()
    }

    pub fn count(&self) -> usize {
        self.trains.iter().filter(|t| t.activated).count()
    }
}
