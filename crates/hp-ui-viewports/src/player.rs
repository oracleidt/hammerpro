use glam::{DVec2, DVec3};
use hp_app::runtime::{DEFAULT_CAMERA_DISTANCE, LightStyleReport, PlayerState};
use hp_app::state::PlayerModelPose;
use hp_app::{AppState, t, tf};
use hp_editor::ObjRef;
use hp_editor::limits::portals::{AreaFlow, ScreenRect, ViewCone};
use hp_editor::limits::world_light::{LightCmd, LightSwitch, MAX_STYLES, StyleTable};

pub const WALK_SPEED: f64 = 200.0;
pub const RUN_SPEED: f64 = 400.0;
pub const CROUCH_SPEED: f64 = 66.0;
pub const NOCLIP_SPEED: f64 = 600.0;
pub const GRAVITY: f64 = 600.0;
pub const JUMP_SPEED: f64 = 200.0;
pub const HALF_WIDTH: f64 = 16.0;
pub const HEIGHT: f64 = 72.0;
pub const CROUCH_HEIGHT: f64 = 36.0;
pub const EYE: f64 = 64.0;
pub const CROUCH_EYE: f64 = 28.0;
pub const TIME_TO_DUCK: f64 = 0.4;
pub const TIME_TO_UNDUCK: f64 = 0.2;
pub const STEP: f64 = 18.0;
pub const CAMERA_DISTANCE_MIN: f64 = 50.0;
pub const CAMERA_DISTANCE_MAX: f64 = 800.0;

pub trait Collider {
    fn trace(&self, from: DVec3, dir: DVec3, max: f64) -> Option<f64>;

    fn trace_prop(&self, from: DVec3, dir: DVec3, max: f64) -> Option<(f64, Option<hp_vmf::Id>)> {
        self.trace(from, dir, max).map(|t| (t, None))
    }
}

pub type Push = (hp_vmf::Id, DVec3);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Move {
    pub forward: f64,
    pub right: f64,
    pub jump: bool,
    pub crouch: bool,
    pub run: bool,
}

pub fn eye(p: &PlayerState) -> DVec3 {
    let t = p.duck.clamp(0.0, 1.0);
    let s = 3.0 * t * t - 2.0 * t * t * t;
    p.feet + DVec3::Z * (EYE + (CROUCH_EYE - EYE) * s)
}

fn sweep(c: &dyn Collider, feet: DVec3, height: f64, dir: DVec3, max: f64) -> Option<(f64, Option<hp_vmf::Id>)> {
    let across = DVec3::new(-dir.y, dir.x, 0.0);
    let mut best: Option<(f64, Option<hp_vmf::Id>)> = None;
    for z in [STEP + 1.0, height * 0.5, height - 2.0] {
        for s in [-HALF_WIDTH + 1.0, 0.0, HALF_WIDTH - 1.0] {
            let from = feet + DVec3::Z * z + across * s;
            if let Some((d, prop)) = c.trace_prop(from, dir, max + HALF_WIDTH) {
                let d = (d - HALF_WIDTH).max(0.0);
                if best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, prop));
                }
            }
        }
    }
    best
}

fn ground(c: &dyn Collider, feet: DVec3, reach: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    let r = HALF_WIDTH - 1.0;
    for (x, y) in [(0.0, 0.0), (r, r), (-r, r), (r, -r), (-r, -r)] {
        let from = feet + DVec3::new(x, y, STEP);
        if let Some(d) = c.trace(from, -DVec3::Z, STEP + reach) {
            let z = from.z - d;
            best = Some(best.map_or(z, |b: f64| b.max(z)));
        }
    }
    best
}

pub fn step(p: &mut PlayerState, m: Move, yaw: f64, pitch: f64, dt: f64, c: &dyn Collider) -> Vec<Push> {
    let mut pushes = Vec::new();
    if !(dt.is_finite() && dt > 0.0) {
        return pushes;
    }
    let dt = dt.min(0.1);
    let (sy, cy) = yaw.to_radians().sin_cos();
    let fwd = DVec3::new(cy, sy, 0.0);
    let right = DVec3::new(sy, -cy, 0.0);
    if p.noclip {
        let (sp, cp) = pitch.to_radians().sin_cos();
        let look = DVec3::new(cy * cp, sy * cp, sp);
        let up = f64::from(u8::from(m.jump)) - f64::from(u8::from(m.crouch));
        let dir = look * m.forward + right * m.right + DVec3::Z * up;
        let speed = NOCLIP_SPEED * if m.run { 3.0 } else { 1.0 };
        p.velocity = dir.normalize_or_zero() * speed;
        p.feet += p.velocity * dt;
        p.on_ground = false;
        return pushes;
    }
    if m.crouch {
        p.duck = (p.duck + dt / TIME_TO_DUCK).min(1.0);
        if p.duck >= 1.0 || !p.on_ground {
            p.crouched = true;
            p.duck = 1.0;
        }
    } else {
        if p.crouched
            && c.trace(p.feet + DVec3::Z * (CROUCH_HEIGHT - 1.0), DVec3::Z, HEIGHT - CROUCH_HEIGHT + 1.0).is_none()
        {
            p.crouched = false;
        }
        if !p.crouched {
            p.duck = (p.duck - dt / TIME_TO_UNDUCK).max(0.0);
        }
    }
    let height = if p.crouched { CROUCH_HEIGHT } else { HEIGHT };
    let speed = if p.crouched {
        CROUCH_SPEED
    } else if m.run {
        RUN_SPEED
    } else {
        WALK_SPEED
    };
    let wish = (fwd * m.forward + right * m.right).normalize_or_zero() * speed;
    let control: f64 = if p.on_ground { 1.0 } else { 0.15 };
    p.velocity.x += (wish.x - p.velocity.x) * control;
    p.velocity.y += (wish.y - p.velocity.y) * control;
    if m.jump && p.on_ground {
        p.velocity.z = JUMP_SPEED;
        p.on_ground = false;
    }
    for axis in [DVec3::X, DVec3::Y] {
        let along = p.velocity.dot(axis) * dt;
        if along.abs() < 1e-9 {
            continue;
        }
        let dir = axis * along.signum();
        match sweep(c, p.feet, height, dir, along.abs()) {
            Some((free, prop)) if free < along.abs() => {
                p.feet += dir * (free - 0.03).max(0.0);
                p.velocity -= axis * p.velocity.dot(axis);
                if let Some(id) = prop {
                    pushes.push((id, dir * (along.abs() - free + 0.5)));
                }
            }
            _ => p.feet += dir * along.abs(),
        }
    }
    let was_on_ground = p.on_ground;
    p.velocity.z -= GRAVITY * dt;
    let dz = p.velocity.z * dt;
    if dz > 0.0 {
        if let Some(d) = c.trace(p.feet + DVec3::Z * (height - 1.0), DVec3::Z, dz + 1.0) {
            p.feet.z += (d - 1.0).max(0.0);
            p.velocity.z = 0.0;
        } else {
            p.feet.z += dz;
        }
        p.on_ground = false;
        return pushes;
    }
    let reach = -dz + if was_on_ground { STEP } else { 0.0 } + 0.5;
    match ground(c, p.feet, reach) {
        Some(z) if z >= p.feet.z + dz - 0.01 => {
            p.feet.z = z;
            p.velocity.z = 0.0;
            p.on_ground = true;
        }
        _ => {
            p.feet.z += dz;
            p.on_ground = false;
        }
    }
    pushes
}

pub struct DocCollider<'a> {
    pub state: &'a AppState,
    pub props: Option<&'a PropBodies>,
    pub except: Option<hp_vmf::Id>,
    pub doors: Option<&'a Doors>,
    pub hidden: Option<&'a std::collections::HashSet<hp_vmf::Id>>,
    pub trains: Option<&'a crate::trains::Trains>,
}

const PROP_RANGE: f64 = 1024.0;

type BodyKey = (String, DVec3, DVec3, f64, glam::DMat4);

#[derive(Clone, Debug)]
struct PropBody {
    key: BodyKey,
    pushable: bool,
    bounds: hp_geom::Aabb,
    tris: Vec<[DVec3; 3]>,
}

#[derive(Clone, Debug, Default)]
pub struct PropBodies {
    map: std::collections::HashMap<hp_vmf::Id, PropBody>,
}

impl PropBodies {
    pub fn update(
        &mut self,
        state: &mut AppState,
        center: DVec3,
        doors: &Doors,
        trains: &crate::trains::Trains,
        hidden: &std::collections::HashSet<hp_vmf::Id>,
    ) {
        let mut wanted: Vec<(hp_vmf::Id, BodyKey, bool)> = Vec::new();
        for n in state.doc.scene().entities().values() {
            let kind = prop_kind(&n.entity);
            if kind == PropKind::None || hidden.contains(&n.entity.id) {
                continue;
            }
            let Some(origin) = n.entity.origin().filter(|o| o.is_finite()) else { continue };
            let moved = trains.child_transform(n.entity.id).unwrap_or_else(|| doors.transform(n.entity.id));
            if moved.transform_point3(origin).distance(center) > PROP_RANGE {
                continue;
            }
            let model = n.entity.get("model").unwrap_or_default().trim().replace('\\', "/").to_ascii_lowercase();
            let angles = n.entity.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
            let scale = n
                .entity
                .get("modelscale")
                .and_then(|s| s.trim().parse::<f64>().ok())
                .filter(|s| s.is_finite() && *s > 0.0)
                .unwrap_or(1.0);
            wanted.push((n.entity.id, (model, origin, angles, scale, moved), kind == PropKind::Pushable));
        }
        self.map.retain(|id, _| wanted.iter().any(|(w, _, _)| w == id));
        for (id, key, pushable) in wanted {
            if self.map.get(&id).is_some_and(|b| b.key == key && b.pushable == pushable) {
                continue;
            }
            match state.assets.collision_triangles(&key.0) {
                Some(tris) => {
                    let m = key.4
                        * glam::DMat4::from_scale_rotation_translation(
                            DVec3::splat(key.3),
                            hp_geom::angles_to_quat(key.2),
                            key.1,
                        );
                    let tris: Vec<[DVec3; 3]> = tris
                        .iter()
                        .flat_map(|t| t.iter())
                        .map(|t| {
                            t.map(|v| m.transform_point3(DVec3::new(f64::from(v[0]), f64::from(v[1]), f64::from(v[2]))))
                        })
                        .collect();
                    let mut bounds = hp_geom::Aabb::EMPTY;
                    for v in tris.iter().flatten() {
                        bounds.extend(*v);
                    }
                    self.map.insert(id, PropBody { key, pushable, bounds, tris });
                }
                None => {
                    self.map.remove(&id);
                }
            }
        }
    }

    pub(crate) fn trace(
        &self,
        ray: &hp_geom::Ray,
        max: f64,
        except: Option<hp_vmf::Id>,
    ) -> Option<(f64, hp_vmf::Id, bool)> {
        let mut best: Option<(f64, hp_vmf::Id, bool)> = None;
        for (id, b) in &self.map {
            if Some(*id) == except || b.tris.is_empty() {
                continue;
            }
            if hp_geom::ray_aabb(ray, &b.bounds).is_none_or(|t| t > max) {
                continue;
            }
            for t in &b.tris {
                if let Some(d) = hp_geom::ray_triangle(ray, t[0], t[1], t[2], false)
                    && d <= max
                    && best.is_none_or(|(x, _, _)| d < x)
                {
                    best = Some((d, *id, b.pushable));
                }
            }
        }
        best
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropKind {
    None,
    Solid,
    Pushable,
}

fn prop_kind(e: &hp_vmf::Entity) -> PropKind {
    let class = e.classname.to_ascii_lowercase();
    if !class.starts_with("prop_") || e.get("model").is_none_or(|m| m.trim().is_empty()) {
        return PropKind::None;
    }
    if e.get("solid").is_some_and(|s| s.trim() == "0") || class == "prop_detail" || class == "prop_ragdoll" {
        return PropKind::None;
    }
    let flags = e.get("spawnflags").and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0);
    if class.starts_with("prop_physics") && flags & 8 == 0 { PropKind::Pushable } else { PropKind::Solid }
}

pub(crate) fn non_solid_class(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    c.starts_with("trigger_")
        || c.starts_with("func_areaportal")
        || matches!(
            c.as_str(),
            "func_illusionary"
                | "func_occluder"
                | "func_dustmotes"
                | "func_dustcloud"
                | "func_precipitation"
                | "func_smokevolume"
                | "func_viscluster"
                | "func_ladder"
                | "func_buyzone"
                | "func_bomb_target"
                | "env_bubbles"
                | "env_embers"
        )
}

pub(crate) fn non_solid_material(m: &str) -> bool {
    let m = m.to_ascii_lowercase().replace('\\', "/");
    matches!(
        m.as_str(),
        "tools/toolstrigger"
            | "tools/toolsskip"
            | "tools/toolshint"
            | "tools/toolsfog"
            | "tools/toolsareaportal"
            | "tools/toolsoccluder"
            | "tools/toolsinvisibleladder"
            | "tools/toolsskybox2d"
            | "tools/toolsorigin"
    )
}

impl Collider for DocCollider<'_> {
    fn trace(&self, from: DVec3, dir: DVec3, max: f64) -> Option<f64> {
        self.trace_prop(from, dir, max).map(|(t, _)| t)
    }

    fn trace_prop(&self, from: DVec3, dir: DVec3, max: f64) -> Option<(f64, Option<hp_vmf::Id>)> {
        let doc = &self.state.doc;
        let scene = doc.scene();
        let solid = |o: hp_editor::ObjRef| match o {
            hp_editor::ObjRef::Solid(id) => scene.solid(id).is_some_and(|n| {
                let owner_ok = match n.owner {
                    hp_editor::Owner::World => true,
                    hp_editor::Owner::Entity(e) => {
                        scene.entity(e).is_none_or(|en| !non_solid_class(&en.entity.classname))
                            && self.doors.is_none_or(|d| !d.moved(e))
                            && self.trains.is_none_or(|t| !t.owns(e))
                            && self.hidden.is_none_or(|h| !h.contains(&e))
                    }
                };
                owner_ok && n.solid.sides.iter().any(|s| !non_solid_material(&s.material))
            }),
            hp_editor::ObjRef::Entity(_) => false,
        };
        let opts = hp_editor::PickOptions {
            include_entities: false,
            max_distance: max,
            filter: Some(&solid),
            ..hp_editor::PickOptions::default()
        };
        let ray = hp_geom::Ray::new(from, dir);
        let brush = doc
            .pick_ray(&ray, &opts, &|e| self.state.entity_bounds(e))
            .map(|h| h.t)
            .filter(|t| t.is_finite() && *t <= max);
        let door = self.doors.and_then(|d| d.trace(&ray, brush.unwrap_or(max)));
        let brush = door.or(brush);
        let train = self.trains.and_then(|t| t.trace(&ray, brush.unwrap_or(max), self.hidden));
        let brush = train.map(|(t, _)| t).or(brush);
        let prop = self.props.and_then(|p| p.trace(&ray, brush.unwrap_or(max), self.except));
        match (brush, prop) {
            (_, Some((t, id, pushable))) => Some((t, pushable.then_some(id))),
            (Some(t), None) => Some((t, None)),
            (None, None) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DoorMove {
    Rotate { pivot: DVec3, axis: DVec3, degrees: f64 },
    Slide { dir: DVec3, distance: f64 },
}

#[derive(Clone, Debug)]
struct Door {
    movement: DoorMove,
    speed: f64,
    open: f64,
    target: f64,
    sign: f64,
    tris: Vec<[DVec3; 3]>,
    moved: Vec<[DVec3; 3]>,
    bounds: hp_geom::Aabb,
    wait: Option<f64>,
    close_in: Option<f64>,
}

impl Door {
    fn transform_at(&self, open: f64, sign: f64) -> glam::DMat4 {
        match self.movement {
            DoorMove::Rotate { pivot, axis, degrees } => {
                glam::DMat4::from_translation(pivot)
                    * glam::DMat4::from_axis_angle(axis, (sign * degrees * open).to_radians())
                    * glam::DMat4::from_translation(-pivot)
            }
            DoorMove::Slide { dir, distance } => glam::DMat4::from_translation(dir * distance * open),
        }
    }

    fn transform(&self) -> glam::DMat4 {
        self.transform_at(self.open, self.sign)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Doors {
    map: std::collections::HashMap<hp_vmf::Id, Door>,
    events: Vec<(hp_vmf::Id, bool)>,
}

const USE_DISTANCE: f64 = 96.0;

pub fn render_none(e: &hp_vmf::Entity) -> bool {
    e.get("rendermode").and_then(|s| s.trim().parse::<f64>().ok()).is_some_and(|m| m as i64 == 10)
}

fn float_key(e: &hp_vmf::Entity, key: &str, default: f64) -> f64 {
    e.get(key).and_then(|s| s.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).unwrap_or(default)
}

fn door_of(state: &AppState, id: hp_vmf::Id) -> Option<Door> {
    let scene = state.doc.scene();
    let e = &scene.entity(id)?.entity;
    let class = e.classname.to_ascii_lowercase();
    let flags = e.get("spawnflags").and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0);
    let mut tris = Vec::new();
    let mut bounds = hp_geom::Aabb::EMPTY;
    for n in scene.solids().values() {
        if n.owner != hp_editor::Owner::Entity(id) {
            continue;
        }
        for f in &n.geom.faces {
            for i in 1..f.vertices.len().saturating_sub(1) {
                tris.push([f.vertices[0], f.vertices[i], f.vertices[i + 1]]);
            }
            for v in &f.vertices {
                bounds.extend(*v);
            }
        }
    }
    let origin = e.origin().filter(|o| o.is_finite());
    let movement = match class.as_str() {
        "prop_door_rotating" => {
            if let Some(b) = state.entity_bounds(e) {
                bounds = b;
            }
            DoorMove::Rotate { pivot: origin?, axis: DVec3::Z, degrees: float_key(e, "distance", 90.0) }
        }
        "func_door_rotating" | "func_rotating_door" => {
            let axis = if flags & 64 != 0 {
                DVec3::X
            } else if flags & 128 != 0 {
                DVec3::Y
            } else {
                DVec3::Z
            };
            let pivot = origin.unwrap_or_else(|| bounds.center());
            DoorMove::Rotate { pivot, axis, degrees: float_key(e, "distance", 90.0) }
        }
        "func_door" => {
            let a = e
                .get("movedir")
                .map(|s| {
                    let v: Vec<f64> = s.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                    DVec3::new(v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0), 0.0)
                })
                .unwrap_or(DVec3::ZERO);
            let (sp, cp) = a.x.to_radians().sin_cos();
            let (sy, cy) = a.y.to_radians().sin_cos();
            let dir = DVec3::new(cp * cy, cp * sy, -sp);
            let size = if bounds.is_empty() { DVec3::ZERO } else { bounds.size() };
            let distance = (size.dot(dir.abs()) - float_key(e, "lip", 0.0)).max(0.0);
            DoorMove::Slide { dir, distance }
        }
        _ => return None,
    };
    if bounds.is_empty() {
        return None;
    }
    let sign = match class.as_str() {
        "prop_door_rotating" if e.get("opendir").map(str::trim) == Some("2") => -1.0,
        _ if class != "prop_door_rotating" && flags & 2 != 0 => -1.0,
        _ => 1.0,
    };
    let speed = float_key(e, "speed", 100.0).max(1.0);
    let wait = if class == "prop_door_rotating" {
        float_key(e, "returndelay", -1.0)
    } else if flags & 32 != 0 {
        -1.0
    } else {
        float_key(e, "wait", 4.0)
    };
    let wait = (wait >= 0.0).then_some(wait);
    Some(Door {
        movement,
        speed,
        open: 0.0,
        target: 0.0,
        sign,
        moved: tris.clone(),
        tris,
        bounds,
        wait,
        close_in: None,
    })
}

fn opens_both_ways(e: &hp_vmf::Entity) -> bool {
    let class = e.classname.to_ascii_lowercase();
    let flags = e.get("spawnflags").and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0);
    match class.as_str() {
        "prop_door_rotating" => e.get("opendir").map(str::trim).is_none_or(|d| d == "0"),
        "func_door_rotating" | "func_rotating_door" => flags & 16 == 0,
        _ => false,
    }
}

#[derive(Default)]
pub struct ObjClusters {
    vis: Option<std::sync::Arc<hp_editor::limits::portals::BspVis>>,
    objects: Vec<(ObjRef, Vec<u32>, hp_geom::Aabb)>,
    areas: Vec<u16>,
}

impl ObjClusters {
    pub fn update(&mut self, state: &AppState) {
        let vis = state.runtime.health.vis.clone();
        let same = match (&self.vis, &vis) {
            (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        self.objects.clear();
        self.vis = vis.clone();
        let Some(vis) = vis else {
            self.areas.clear();
            return;
        };
        self.areas = vis.cluster_areas();
        let scene = state.doc.scene();
        for (id, n) in scene.solids() {
            if !n.bounds.is_empty() {
                self.objects.push((ObjRef::Solid(*id), vis.clusters_in_box(n.bounds.min, n.bounds.max), n.bounds));
            }
        }
        for n in scene.entities().values() {
            if scene.is_brush_entity(n.entity.id) {
                continue;
            }
            if let Some(b) = state.entity_bounds(&n.entity) {
                self.objects.push((ObjRef::Entity(n.entity.id), vis.clusters_in_box(b.min, b.max), b));
            }
        }
    }
}

pub fn view_cone(cam: &hp_app::view::Camera3d, fov_y_deg: f64, aspect: f64) -> ViewCone {
    let tan_y = (fov_y_deg.clamp(1.0, 179.0).to_radians() * 0.5).tan();
    let aspect = if aspect.is_finite() && aspect > 0.01 { aspect } else { 1.0 };
    ViewCone { eye: cam.pos, forward: cam.forward(), right: cam.right(), up: cam.up(), tan_x: tan_y * aspect, tan_y }
}

fn drawn_by_game(
    clusters: &[u32],
    min: DVec3,
    max: DVec3,
    visible: Option<&Vec<bool>>,
    areas: &[u16],
    flow: Option<&AreaFlow>,
) -> bool {
    clusters.is_empty()
        || clusters.iter().any(|&c| {
            visible.is_none_or(|v| v.get(c as usize).copied().unwrap_or(false))
                && flow.is_none_or(|f| f.box_visible(areas.get(c as usize).copied().unwrap_or(0), min, max))
        })
}

pub fn world_groups_drawn(
    world: &hp_editor::limits::world::BspWorld,
    visible: Option<&Vec<bool>>,
    areas: &[u16],
    flow: Option<&AreaFlow>,
) -> Vec<bool> {
    let v = |a: [f32; 3]| DVec3::new(f64::from(a[0]), f64::from(a[1]), f64::from(a[2]));
    world.groups.iter().map(|g| drawn_by_game(&g.clusters, v(g.min), v(g.max), visible, areas, flow)).collect()
}

pub fn game_hidden(state: &AppState, clusters: &ObjClusters) -> Vec<ObjRef> {
    let rt = &state.runtime.health;
    let visible = rt.play_visible.as_deref();
    let flow = rt.play_flow.as_deref().filter(|f| f.own_area.is_some());
    if visible.is_none() && flow.is_none() {
        return Vec::new();
    }
    clusters
        .objects
        .iter()
        .filter(|(_, list, b)| !list.is_empty() && !drawn_by_game(list, b.min, b.max, visible, &clusters.areas, flow))
        .map(|(o, _, _)| *o)
        .collect()
}

pub fn flow_key(flow: Option<&AreaFlow>) -> Option<(Vec<bool>, Vec<Option<ScreenRect>>)> {
    let f = flow.filter(|f| f.own_area.is_some())?;
    let rects = f.rects.iter().enumerate().map(|(a, r)| r.filter(|_| f.own_area != Some(a as u16))).collect();
    Some((f.visible.clone(), rects))
}

impl Doors {
    pub fn open_names(&self, state: &AppState) -> Vec<String> {
        let scene = state.doc.scene();
        let mut names: Vec<String> = self
            .map
            .iter()
            .filter(|(_, d)| d.open > 0.0)
            .filter_map(|(id, _)| scene.entity(*id)?.entity.get("targetname").map(|t| t.trim().to_owned()))
            .filter(|t| !t.is_empty())
            .collect();
        names.sort();
        names
    }

    pub fn moved(&self, id: hp_vmf::Id) -> bool {
        self.map.get(&id).is_some_and(|d| d.open > 0.0 && !d.tris.is_empty())
    }

    pub fn transform(&self, id: hp_vmf::Id) -> glam::DMat4 {
        self.map.get(&id).filter(|d| d.open > 0.0).map_or(glam::DMat4::IDENTITY, Door::transform)
    }

    fn trace(&self, ray: &hp_geom::Ray, max: f64) -> Option<f64> {
        let mut best: Option<f64> = None;
        for d in self.map.values().filter(|d| d.open > 0.0) {
            for t in &d.moved {
                if let Some(h) = hp_geom::ray_triangle(ray, t[0], t[1], t[2], false)
                    && h <= max
                    && best.is_none_or(|b| h < b)
                {
                    best = Some(h);
                }
            }
        }
        best
    }

    pub fn use_pick(
        &self,
        state: &AppState,
        props: &PropBodies,
        trains: &crate::trains::Trains,
        eye: DVec3,
        look: DVec3,
    ) -> Option<hp_vmf::Id> {
        let scene = state.doc.scene();
        let ray = hp_geom::Ray::new(eye, look);
        let mut best: Option<(f64, Option<hp_vmf::Id>)> = None;
        let solid = |o: ObjRef| match o {
            ObjRef::Solid(id) => scene.solid(id).is_some_and(|n| match n.owner {
                hp_editor::Owner::Entity(e) => {
                    !self.moved(e)
                        && !trains.owns(e)
                        && scene.entity(e).is_none_or(|en| !non_solid_class(&en.entity.classname))
                }
                hp_editor::Owner::World => true,
            }),
            ObjRef::Entity(_) => false,
        };
        let opts = hp_editor::PickOptions {
            include_entities: false,
            max_distance: USE_DISTANCE,
            filter: Some(&solid),
            ..hp_editor::PickOptions::default()
        };
        if let Some(h) = state.doc.pick_ray(&ray, &opts, &|e| state.entity_bounds(e))
            && let ObjRef::Solid(id) = h.obj
        {
            let owner = match scene.solid(id).map(|n| n.owner) {
                Some(hp_editor::Owner::Entity(e)) => Some(e),
                _ => None,
            };
            best = Some((h.t, owner));
        }
        for (id, d) in self.map.iter().filter(|(_, d)| d.open > 0.0) {
            for t in &d.moved {
                if let Some(h) = hp_geom::ray_triangle(&ray, t[0], t[1], t[2], false)
                    && h <= USE_DISTANCE
                    && best.is_none_or(|(b, _)| h < b)
                {
                    best = Some((h, Some(*id)));
                }
            }
        }
        if let Some((h, id)) = trains.use_trace(&ray, best.map_or(USE_DISTANCE, |(b, _)| b), None) {
            best = Some((h, Some(id)));
        }
        if let Some((h, id, _)) = props.trace(&ray, USE_DISTANCE, None)
            && best.is_none_or(|(b, _)| h < b)
        {
            best = Some((h, Some(id)));
        }
        best.and_then(|(_, id)| id)
    }

    pub fn set_door(&mut self, state: &AppState, id: hp_vmf::Id, open: Option<bool>, feet: DVec3) -> Option<bool> {
        if let std::collections::hash_map::Entry::Vacant(slot) = self.map.entry(id) {
            slot.insert(door_of(state, id)?);
        }
        let both = state.doc.scene().entity(id).is_some_and(|n| opens_both_ways(&n.entity));
        let d = self.map.get_mut(&id)?;
        let opening = open.unwrap_or(d.target <= 0.5);
        if opening {
            if d.target <= 0.5 && both && d.open <= 0.0 {
                let c = d.bounds.center();
                let away = |s: f64| d.transform_at(1.0, s).transform_point3(c).distance(feet);
                d.sign = if away(-1.0) > away(1.0) { -1.0 } else { 1.0 };
            }
            d.target = 1.0;
        } else {
            d.target = 0.0;
        }
        d.close_in = None;
        Some(opening)
    }

    pub fn start_open(&mut self, state: &AppState, id: hp_vmf::Id) {
        if let std::collections::hash_map::Entry::Vacant(slot) = self.map.entry(id)
            && let Some(mut d) = door_of(state, id)
        {
            d.open = 1.0;
            d.target = 1.0;
            slot.insert(d);
        }
    }

    pub fn is_closed(&self, id: hp_vmf::Id) -> bool {
        self.map.get(&id).is_none_or(|d| d.target <= 0.5)
    }

    pub fn touches(&mut self, state: &AppState, id: hp_vmf::Id, feet: DVec3, height: f64) -> bool {
        if let std::collections::hash_map::Entry::Vacant(slot) = self.map.entry(id) {
            match door_of(state, id) {
                Some(d) => {
                    slot.insert(d);
                }
                None => return false,
            }
        }
        let Some(d) = self.map.get(&id) else { return false };
        let r = 16.0 + 2.0;
        let min = feet - DVec3::new(r, r, 2.0);
        let max = feet + DVec3::new(r, r, height + 2.0);
        let mut b = hp_geom::Aabb::EMPTY;
        for t in &d.moved {
            for v in t {
                b.extend(*v);
            }
        }
        if b.is_empty() {
            b = d.bounds;
        }
        !b.is_empty() && b.min.cmple(max).all() && b.max.cmpge(min).all()
    }

    pub fn take_events(&mut self) -> Vec<(hp_vmf::Id, bool)> {
        std::mem::take(&mut self.events)
    }

    pub fn animate(&mut self, dt: f64) -> Vec<(hp_vmf::Id, glam::DMat4)> {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        let mut out = Vec::new();
        for (id, d) in &mut self.map {
            let span = match d.movement {
                DoorMove::Rotate { degrees, .. } => degrees.abs(),
                DoorMove::Slide { distance, .. } => distance,
            }
            .max(1.0);
            let step = d.speed * dt / span;
            let before = d.open;
            d.open = if d.target > d.open { (d.open + step).min(d.target) } else { (d.open - step).max(d.target) };
            if d.open != before && (d.open >= 1.0 || d.open <= 0.0) {
                self.events.push((*id, d.open >= 1.0));
                if d.open >= 1.0 {
                    d.close_in = d.wait;
                }
            }
            if d.open >= 1.0
                && d.target >= 1.0
                && let Some(t) = d.close_in.as_mut()
            {
                *t -= dt;
                if *t <= 0.0 {
                    d.close_in = None;
                    d.target = 0.0;
                }
            }
            if d.open != before || d.moved.len() != d.tris.len() {
                let m = d.transform();
                d.moved = d.tris.iter().map(|t| t.map(|v| m.transform_point3(v))).collect();
            }
            if d.open > 0.0 {
                out.push((*id, d.transform()));
            }
        }
        out
    }
}

pub fn teleport(state: &AppState, p: &mut PlayerState, trains: Option<&crate::trains::Trains>) -> Option<f64> {
    let scene = state.doc.scene();
    let height = if p.crouched { CROUCH_HEIGHT } else { HEIGHT };
    let hull = hp_geom::Aabb::new(
        p.feet - DVec3::new(HALF_WIDTH, HALF_WIDTH, 0.0),
        p.feet + DVec3::new(HALF_WIDTH, HALF_WIDTH, height),
    );
    let mut inside = None;
    for n in scene.entities().values() {
        let e = &n.entity;
        if !e.classname.eq_ignore_ascii_case("trigger_teleport") || e.get("StartDisabled").map(str::trim) == Some("1") {
            continue;
        }
        let moved = trains.and_then(|t| t.child_transform(e.id));
        let touches = scene.solids().values().any(|s| {
            let b = moved.map_or(s.bounds, |m| crate::io::moved_bounds(s.bounds, &m));
            s.owner == hp_editor::Owner::Entity(e.id) && b.min.cmple(hull.max).all() && b.max.cmpge(hull.min).all()
        });
        if touches {
            inside = Some(e.get("target").unwrap_or_default().trim().to_owned());
            break;
        }
    }
    let Some(target) = inside else {
        p.in_teleport = false;
        return None;
    };
    if p.in_teleport || target.is_empty() {
        return None;
    }
    let dest = scene
        .entities()
        .values()
        .find(|n| n.entity.get("targetname").is_some_and(|t| t.trim().eq_ignore_ascii_case(&target)))?;
    let origin = dest.entity.origin().filter(|o| o.is_finite())?;
    let yaw = dest.entity.angles().map_or(0.0, |a| a.y);
    let moved = trains.and_then(|t| t.child_transform(dest.entity.id));
    p.feet = moved.map_or(origin, |m| m.transform_point3(origin));
    p.velocity = DVec3::ZERO;
    p.in_teleport = true;
    let (sy, cy) = yaw.to_radians().sin_cos();
    let turned = moved.map_or(DVec3::new(cy, sy, 0.0), |m| m.transform_vector3(DVec3::new(cy, sy, 0.0)));
    Some(if moved.is_some() { turned.y.atan2(turned.x).to_degrees() } else { yaw })
}

pub fn push_props(state: &mut AppState, props: &PropBodies, trains: Option<&crate::trains::Trains>, pushes: &[Push]) {
    let mut moves: Vec<(hp_vmf::Id, DVec3)> = Vec::new();
    for &(id, delta) in pushes {
        let len = delta.length();
        let Some(dir) = delta.try_normalize() else { continue };
        if moves.iter().any(|(m, _)| *m == id) {
            continue;
        }
        let Some(node) = state.doc.scene().entity(id) else { continue };
        let Some(b) = state.entity_bounds(&node.entity) else { continue };
        let c = DocCollider { state, props: Some(props), except: Some(id), doors: None, hidden: None, trains };
        let half = (b.size() * 0.5).dot(dir.abs());
        let center = b.center();
        let mut free = len;
        for z in [b.min.z + (b.max.z - b.min.z) * 0.2, center.z, b.max.z - (b.max.z - b.min.z) * 0.1] {
            let from = DVec3::new(center.x, center.y, z);
            if let Some(d) = c.trace(from, dir, half + len) {
                free = free.min((d - half - 0.1).max(0.0));
            }
        }
        if free > 0.01 {
            moves.push((id, dir * free));
        }
    }
    if moves.is_empty() {
        return;
    }
    let assets = &state.assets;
    let doc = &mut state.doc;
    doc.begin_merge("Push props", "player_push");
    for (id, d) in &moves {
        let _ = doc.translate(&[ObjRef::Entity(*id)], *d);
    }
    let objs: Vec<ObjRef> = moves.iter().map(|(id, _)| ObjRef::Entity(*id)).collect();
    let _ = doc.drop_to_surface(&objs, -DVec3::Z, false, &|e| assets.entity_bounds(e));
    doc.commit();
}

fn animation(p: &PlayerState) -> (&'static str, f64) {
    let speed = DVec2::new(p.velocity.x, p.velocity.y).length();
    if p.noclip {
        return if p.velocity.length() > 1.0 { ("swimming_all", 1.0) } else { ("swim_idle_all", 1.0) };
    }
    if !p.on_ground && (p.velocity.z > 0.0 || p.air_time > 0.15) {
        return ("jump_slam", 1.0);
    }
    if p.crouched || p.duck > 0.5 {
        return if speed > 1.0 { ("cwalk_all", (speed / CROUCH_SPEED).clamp(0.5, 2.0)) } else { ("cidle_all", 1.0) };
    }
    if speed > 150.0 {
        ("run_all_01", (speed / WALK_SPEED).clamp(0.5, 2.0))
    } else if speed > 1.0 {
        ("walk_all", (speed / 100.0).clamp(0.5, 2.0))
    } else {
        ("idle_all_01", 1.0)
    }
}

pub fn model_pose(p: &mut PlayerState, yaw: f64, dt: f64) -> PlayerModelPose {
    let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
    p.air_time = if p.on_ground || p.noclip { 0.0 } else { p.air_time + dt };
    let (sequence, rate) = animation(p);
    if p.anim == sequence {
        p.anim_time = (p.anim_time + dt * rate) % 3600.0;
    } else {
        p.anim = sequence.to_owned();
        p.anim_time = 0.0;
    }
    let (sy, cy) = yaw.to_radians().sin_cos();
    let dir = DVec2::new(p.velocity.x, p.velocity.y).normalize_or_zero();
    PlayerModelPose {
        model: p.model.clone(),
        feet: p.feet,
        yaw,
        sequence: sequence.to_owned(),
        move_x: dir.dot(DVec2::new(cy, sy)) as f32,
        move_y: dir.dot(DVec2::new(sy, -cy)) as f32,
        time: p.anim_time as f32,
    }
}

pub fn player_models(state: &AppState) -> Vec<String> {
    let mut list: Vec<String> = state
        .assets
        .models()
        .iter()
        .map(|a| a.name.clone())
        .filter(|n| {
            n.starts_with("models/player/")
                && n.ends_with(".mdl")
                && !n.contains("/c_")
                && !n.contains("arms")
                && !n.contains("hands")
                && !n.contains("_anm")
        })
        .collect();
    list.sort();
    list.dedup();
    list
}

pub fn next_player_model(state: &AppState, current: &str) -> Option<String> {
    let list = player_models(state);
    let i = list.iter().position(|m| m == current).map_or(0, |i| (i + 1) % list.len().max(1));
    list.get(i).cloned()
}

pub fn start_position(state: &AppState) -> (DVec3, f64) {
    const SPAWNS: &[&str] = &[
        "info_player_start",
        "info_player_deathmatch",
        "info_player_combine",
        "info_player_rebel",
        "info_player_terrorist",
        "info_player_counterterrorist",
    ];
    for class in SPAWNS {
        let found = state.doc.scene().entities().values().find(|n| n.entity.classname.eq_ignore_ascii_case(class));
        if let Some(n) = found
            && let Some(o) = n.entity.origin()
        {
            let yaw = n.entity.angles().map_or(0.0, |a| a.y);
            return (o + DVec3::Z, yaw);
        }
    }
    let cam = state.cameras.cam3d;
    let c = DocCollider { state, props: None, except: None, doors: None, hidden: None, trains: None };
    let below = c.trace(cam.pos, -DVec3::Z, 4096.0).map_or(cam.pos - DVec3::Z * EYE, |d| cam.pos - DVec3::Z * d);
    (below + DVec3::Z, cam.yaw_deg)
}

pub fn camera_position(state: &AppState, p: &PlayerState, trains: Option<&crate::trains::Trains>) -> DVec3 {
    let e = eye(p);
    if !p.third_person {
        return e;
    }
    let distance = if p.camera_distance > 0.0 { p.camera_distance } else { DEFAULT_CAMERA_DISTANCE };
    let pivot = e + DVec3::Z * 8.0;
    let back = -state.cameras.cam3d.forward();
    let c = DocCollider { state, props: None, except: None, doors: None, hidden: None, trains };
    let d = c.trace(pivot, back, distance).map_or(distance, |d| (d - 8.0).max(0.0));
    pivot + back * d
}

pub fn zoom_camera(distance: f64, notches: f32) -> f64 {
    let d = if distance > 0.0 { distance } else { DEFAULT_CAMERA_DISTANCE };
    (d * 0.85f64.powf(f64::from(notches))).clamp(CAMERA_DISTANCE_MIN, CAMERA_DISTANCE_MAX)
}

fn num(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

pub fn world_solids(state: &AppState) -> Vec<ObjRef> {
    let scene = state.doc.scene();
    scene
        .solids()
        .iter()
        .filter(|(_, n)| match n.owner {
            hp_editor::Owner::World => true,
            hp_editor::Owner::Entity(e) => {
                scene.entity(e).is_some_and(|en| en.entity.classname.eq_ignore_ascii_case("func_detail"))
            }
        })
        .map(|(id, _)| ObjRef::Solid(*id))
        .collect()
}

pub fn compiled_replaced_entities(state: &AppState, world: &hp_editor::limits::world::BspWorld) -> Vec<ObjRef> {
    let scene = state.doc.scene();
    scene
        .entities()
        .iter()
        .filter(|(id, n)| {
            let e = &n.entity;
            if scene.is_brush_entity(**id) {
                return false;
            }
            if e.classname.trim().eq_ignore_ascii_case("prop_static") {
                return true;
            }
            let model = e.get("model").is_some_and(|m| m.trim().to_ascii_lowercase().ends_with(".mdl"));
            model && !world.entity_ids.is_empty() && world.entity_ids.binary_search(id).is_err()
        })
        .map(|(id, _)| ObjRef::Entity(*id))
        .collect()
}

pub fn world_stale(state: &AppState) -> bool {
    if state.doc.is_dirty() {
        return true;
    }
    let bsp = state.runtime.health.exact.as_ref().map_or(0, |e| e.modified);
    let vmf = state
        .doc
        .path()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    vmf > bsp
}

pub fn world_draw(
    state: &AppState,
    world: &hp_editor::limits::world::BspWorld,
    aspect: f64,
    drawn: &[bool],
) -> hp_editor::limits::world::WorldDraw {
    let cam = &state.cameras.cam3d;
    let (eye, f, r, u) = (cam.pos, cam.forward(), cam.right(), cam.up());
    let tan_y = (cam.fov_y_deg.to_radians() * 0.5).tan();
    let tan_x = tan_y * aspect.max(0.01);
    let planes = [f, f * tan_x - r, f * tan_x + r, f * tan_y - u, f * tan_y + u];
    let in_view = |min: [f32; 3], max: [f32; 3]| {
        let min = DVec3::new(f64::from(min[0]), f64::from(min[1]), f64::from(min[2]));
        let max = DVec3::new(f64::from(max[0]), f64::from(max[1]), f64::from(max[2]));
        let (center, half) = ((min + max) * 0.5 - eye, (max - min) * 0.5);
        planes.iter().all(|n| n.dot(center) + n.abs().dot(half) >= 0.0)
    };
    let eye32 = [eye.x as f32, eye.y as f32, eye.z as f32];
    world.draw_stats(eye32, |g| {
        let Some(group) = world.groups.get(g) else { return false };
        drawn.get(g).copied().unwrap_or(true) && in_view(group.min, group.max)
    })
}

pub struct WorldCounts {
    pub with_portals: hp_editor::limits::world::WorldDraw,
    pub without_portals: hp_editor::limits::world::WorldDraw,
    pub stale: bool,
    pub light: hp_editor::limits::world_light::LightMode,
}

pub fn world_counts(
    state: &AppState,
    world: &hp_editor::limits::world::BspWorld,
    aspect: f64,
    drawn: &[bool],
    stale: bool,
) -> WorldCounts {
    let rt = &state.runtime.health;
    let pvs = rt.play_cluster.zip(rt.vis.as_ref()).and_then(|(c, v)| v.visible_from(c));
    let areas = rt.vis.as_ref().map(|v| v.cluster_areas()).unwrap_or_default();
    let without = world_groups_drawn(world, pvs.as_ref(), &areas, None);
    WorldCounts {
        with_portals: world_draw(state, world, aspect, drawn),
        without_portals: world_draw(state, world, aspect, &without),
        stale,
        light: world.light.mode,
    }
}

pub fn lighting_line(state: &AppState, mode: hp_editor::limits::world_light::LightMode) -> String {
    use hp_editor::limits::world_light::LightMode;
    let light = if !state.settings.render.player_compiled_lighting {
        t!("viewport.player.light_off").to_owned()
    } else {
        match mode {
            LightMode::Fullbright => t!("viewport.player.light_none").to_owned(),
            LightMode::Ldr => tf!("viewport.player.light_on", "LDR"),
            LightMode::Hdr => tf!("viewport.player.light_on", "HDR"),
        }
    };
    match cubemap_text(state.settings.render.cubemaps, state.assets.play_cubemap_sources()) {
        Some(cubes) => tf!("viewport.player.light_and_cubemaps", light, cubes),
        None => light,
    }
}

pub fn cubemap_text(on: bool, sources: Option<(usize, usize)>) -> Option<String> {
    if !on {
        return Some(t!("viewport.player.cubemaps_off").to_owned());
    }
    Some(match sources? {
        (0, 0) => t!("viewport.player.cubemaps_none").to_owned(),
        (built, 0) => tf!("viewport.player.cubemaps_built", built),
        (0, drawn) => tf!("viewport.player.cubemaps_drawn", drawn),
        (built, drawn) => tf!("viewport.player.cubemaps_both", built, drawn),
    })
}

pub fn counter_lines(
    state: &AppState,
    fps: f32,
    view: usize,
    io_log: &[String],
    world: Option<WorldCounts>,
) -> Vec<String> {
    let p = &state.runtime.lab.player;
    if p.hide_counters {
        return vec![t!("viewport.player.hidden").to_owned()];
    }
    let mode = match (p.noclip, p.third_person) {
        (true, _) => t!("viewport.player.mode_noclip"),
        (false, true) => t!("viewport.player.mode_third"),
        (false, false) => t!("viewport.player.mode_first"),
    };
    let name = p.model.rsplit('/').next().unwrap_or(&p.model).trim_end_matches(".mdl");
    let mut lines = vec![tf!("viewport.player.head", t!("viewport.player.title"), mode, format!("{fps:.0}"), name)];
    let speed = DVec2::new(p.velocity.x, p.velocity.y).length();
    lines.push(tf!(
        "viewport.player.state",
        format!("{speed:.0}"),
        format!("{:.0} {:.0} {:.0}", p.feet.x, p.feet.y, p.feet.z)
    ));
    let wire = match p.wireframe {
        1 => t!("viewport.player.wire1"),
        2 => t!("viewport.player.wire2"),
        3 => t!("viewport.player.wire3"),
        _ => t!("viewport.player.wire0"),
    };
    lines.push(tf!("viewport.player.wireframe", p.wireframe, wire));
    if let Some(c) = state.gfx.as_ref().and_then(|g| g.renderer.view_counts(view as u32)) {
        lines.push(tf!("viewport.player.counts_all", c.draw_calls, num(c.triangles), c.material_changes));
        lines.push(tf!(
            "viewport.player.counts_world",
            c.world_draws,
            num(c.world_triangles),
            c.chunks_in_frustum,
            c.chunks_occluded
        ));
        lines.push(tf!(
            "viewport.player.counts_models",
            c.model_draws,
            num(c.model_triangles),
            c.model_instances,
            c.sprites
        ));
    }
    let rt = &state.runtime.health;
    match (rt.play_cluster, rt.play_cost, &rt.play_visible) {
        (Some(c), Some(cost), Some(v)) => {
            let seen = v.iter().filter(|x| **x).count();
            let area = rt.play_area.map_or_else(|| "-".to_owned(), |a| a.to_string());
            lines.push(tf!("viewport.player.game_cluster", c, seen, v.len(), area));
            if let Some(f) = rt.play_flow.as_deref().filter(|f| f.portals > 0) {
                lines.push(tf!(
                    "viewport.player.game_areas",
                    f.portals_open,
                    f.portals,
                    f.portals_seen,
                    f.visible_areas(),
                    f.visible.len()
                ));
            }
            lines.push(tf!(
                "viewport.player.game_counts",
                num(cost.faces as u64),
                num(cost.triangles as u64),
                cost.displacements,
                cost.props,
                cost.overlays
            ));
        }
        _ => lines.push(t!("viewport.player.no_vis").to_owned()),
    }
    match world {
        Some(counts) => {
            let wire =
                |w: hp_editor::limits::world::WorldDraw| if p.wireframe > 0 { w.without_displacements() } else { w };
            let w = wire(counts.with_portals);
            lines.push(tf!(
                "viewport.player.bsp_counts",
                num(u64::from(w.faces)),
                num(u64::from(w.triangles)),
                w.displacements,
                w.materials,
                num(u64::from(w.backfaces))
            ));
            if rt.play_flow.as_deref().is_some_and(|f| f.portals > 0) {
                let o = wire(counts.without_portals);
                lines.push(tf!(
                    "viewport.player.bsp_no_portals",
                    num(u64::from(o.faces)),
                    num(u64::from(o.triangles)),
                    o.displacements
                ));
            }
            if counts.stale {
                lines.push(t!("viewport.player.bsp_stale").to_owned());
            }
            lines.push(lighting_line(state, counts.light));
            lines.extend(style_lines(&p.light_styles));
        }
        None if p.editor_world && state.runtime.health.world.is_some() => {
            lines.push(t!("viewport.player.bsp_editor").to_owned());
        }
        None => lines.push(t!("viewport.player.bsp_none").to_owned()),
    }
    if !p.trains.is_empty() {
        let active = p.trains.iter().filter(|t| t.active).count();
        let moving = p.trains.iter().filter(|t| t.moving).count();
        lines.push(tf!("viewport.player.trains", moving, active));
    }
    if !io_log.is_empty() {
        lines.push(t!("viewport.player.io_title").to_owned());
        lines.extend(io_log.iter().cloned());
    }
    lines.push(t!("viewport.player.keys").to_owned());
    lines
}

pub const LISTED_LIGHTS: usize = 9;
const LISTED_STYLES: usize = 6;

#[derive(Debug, Default)]
pub struct PlayerLights {
    world: usize,
    switches: Vec<LightSwitch>,
    pub time: f64,
}

impl PlayerLights {
    pub fn step(
        &mut self,
        table: &StyleTable,
        world: usize,
        dt: f64,
        cmds: impl IntoIterator<Item = (String, LightCmd)>,
    ) -> [f32; MAX_STYLES] {
        if self.world != world || self.switches.len() != table.lights.len() {
            *self = PlayerLights { world, switches: table.initial(), time: 0.0 };
        }
        if dt.is_finite() && dt > 0.0 {
            self.time += dt.min(1.0);
        }
        for (name, cmd) in cmds {
            if let Some(s) = table.find(&name).and_then(|i| self.switches.get_mut(i)) {
                s.apply(&cmd);
            }
        }
        table.values(self.time, &self.switches)
    }

    pub fn report(&self, table: &StyleTable) -> LightStyleReport {
        let values = table.values(self.time, &self.switches);
        let styles = table
            .used
            .iter()
            .map(|&s| {
                let pattern = table.pattern(usize::from(s), &self.switches).unwrap_or_default().to_owned();
                (s, pattern, values[usize::from(s)])
            })
            .collect();
        let lights = table
            .lights
            .iter()
            .zip(&self.switches)
            .map(|(l, s)| (l.name.clone(), l.style, s.on, s.pattern.clone().unwrap_or_else(|| "m".to_owned())))
            .collect();
        LightStyleReport { time: self.time, styles, lights }
    }
}

pub fn style_lines(r: &LightStyleReport) -> Vec<String> {
    if r.styles.is_empty() && r.lights.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![tf!("viewport.player.styles", r.styles.len(), format!("{:.1}", r.time))];
    let switched: Vec<u8> = r.lights.iter().map(|l| l.1).collect();
    let animated: Vec<&(u8, String, f32)> = r.styles.iter().filter(|s| !switched.contains(&s.0)).collect();
    for (style, pattern, value) in animated.iter().take(LISTED_STYLES) {
        let shown: String = pattern.chars().take(24).collect();
        lines.push(tf!("viewport.player.style", style, shown, format!("{value:.2}")));
    }
    if animated.len() > LISTED_STYLES {
        lines.push(tf!("viewport.player.styles_more", animated.len() - LISTED_STYLES));
    }
    for (i, (name, style, on, pattern)) in r.lights.iter().take(LISTED_LIGHTS).enumerate() {
        let value = r.styles.iter().find(|s| s.0 == *style).map_or(0.0, |s| s.2);
        let state = if *on { t!("viewport.player.light_switch_on") } else { t!("viewport.player.light_switch_off") };
        let shown: String = pattern.chars().take(24).collect();
        lines.push(tf!("viewport.player.style_light", i + 1, name, style, state, shown, format!("{value:.2}")));
    }
    if r.lights.len() > LISTED_LIGHTS {
        lines.push(tf!("viewport.player.styles_more", r.lights.len() - LISTED_LIGHTS));
    }
    if !r.lights.is_empty() {
        lines.push(t!("viewport.player.style_keys").to_owned());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Room;

    impl Collider for Room {
        fn trace(&self, from: DVec3, dir: DVec3, max: f64) -> Option<f64> {
            let mut best: Option<f64> = None;
            let mut hit = |t: f64| {
                if t >= 0.0 && t <= max {
                    best = Some(best.map_or(t, |b: f64| b.min(t)));
                }
            };
            if dir.z < 0.0 {
                let top = if from.x >= 50.0 { 16.0 } else { 0.0 };
                hit((from.z - top) / -dir.z);
            }
            if dir.z > 0.0 {
                hit((200.0 - from.z) / dir.z);
            }
            if dir.x > 0.0 {
                hit((100.0 - from.x) / dir.x);
                if from.z < 16.0 && from.x < 50.0 {
                    hit((50.0 - from.x) / dir.x);
                }
            }
            best
        }
    }

    fn player() -> PlayerState {
        PlayerState { active: true, feet: DVec3::new(0.0, 0.0, 30.0), ..PlayerState::default() }
    }

    #[test]
    fn world_groups_follow_the_pvs_and_the_area_windows() {
        use hp_editor::limits::world::{BspWorld, FaceGroup};
        let group = |clusters: Vec<u32>, x: f32| FaceGroup {
            clusters,
            faces: Vec::new(),
            min: [x, -8.0, -8.0],
            max: [x + 16.0, 8.0, 8.0],
        };
        let world = BspWorld {
            groups: vec![group(vec![0], 100.0), group(vec![1], 100.0), group(vec![2], 100.0), group(vec![], 0.0)],
            ..BspWorld::default()
        };
        let areas = [1u16, 2, 2];
        let pvs = vec![true, true, false];
        assert_eq!(world_groups_drawn(&world, Some(&pvs), &areas, None), [true, true, false, true]);
        assert_eq!(world_groups_drawn(&world, None, &areas, None), [true; 4]);
        let cam = hp_app::view::Camera3d { pos: DVec3::ZERO, yaw_deg: 0.0, pitch_deg: 0.0, ..Default::default() };
        let cone = view_cone(&cam, 90.0, 1.0);
        let window = ScreenRect { left: -1.0, right: -0.5, bottom: -1.0, top: 1.0 };
        let flow = AreaFlow {
            own_area: Some(1),
            visible: vec![false, true, true],
            rects: vec![None, Some(ScreenRect::FULL), Some(window)],
            cone: Some(cone),
            portals: 1,
            portals_open: 1,
            portals_seen: 1,
        };
        assert_eq!(world_groups_drawn(&world, Some(&pvs), &areas, Some(&flow)), [true, false, false, true]);
        let closed = AreaFlow { visible: vec![false, true, false], ..flow.clone() };
        assert_eq!(world_groups_drawn(&world, None, &areas, Some(&closed)), [true, false, false, true]);
        assert_eq!(flow_key(Some(&flow)).map(|k| k.1), Some(vec![None, None, Some(window)]));
        assert_eq!(flow_key(Some(&AreaFlow::default())), None);
    }

    #[test]
    fn falls_lands_walks_steps_up_and_stops_at_walls() {
        let mut p = player();
        for _ in 0..60 {
            let _ = step(&mut p, Move::default(), 0.0, 0.0, 1.0 / 60.0, &Room);
        }
        assert!(p.on_ground && p.feet.z.abs() < 1e-6, "landed: {:?}", p.feet);
        let fwd = Move { forward: 1.0, ..Move::default() };
        for _ in 0..120 {
            let _ = step(&mut p, fwd, 0.0, 0.0, 1.0 / 60.0, &Room);
        }
        assert!((p.feet.z - 16.0).abs() < 1e-6, "on the step: {:?}", p.feet);
        assert!(p.feet.x <= 100.0 - HALF_WIDTH + 1e-6 && p.feet.x > 80.0, "at the wall: {:?}", p.feet);
        let _ = step(&mut p, Move { jump: true, ..Move::default() }, 0.0, 0.0, 1.0 / 60.0, &Room);
        assert!(!p.on_ground && p.velocity.z > 0.0);
        let mut top: f64 = 0.0;
        for _ in 0..120 {
            let _ = step(&mut p, Move::default(), 0.0, 0.0, 1.0 / 60.0, &Room);
            top = top.max(p.feet.z);
        }
        assert!(p.on_ground && (p.feet.z - 16.0).abs() < 1e-6);
        assert!((top - 16.0 - 33.3).abs() < 3.0, "jump height {top}");
        p.noclip = true;
        for _ in 0..60 {
            let _ = step(&mut p, fwd, 0.0, 0.0, 1.0 / 60.0, &Room);
        }
        assert!(p.feet.x > 100.0);
    }

    #[test]
    fn crouching_lowers_the_eyes_and_slows_down() {
        let mut p = player();
        p.feet.z = 0.0;
        p.on_ground = true;
        let _ = step(&mut p, Move { crouch: true, forward: 1.0, ..Move::default() }, 90.0, 0.0, 0.1, &Room);
        assert!(!p.crouched, "crouching takes time");
        assert!(eye(&p).z < EYE && eye(&p).z > CROUCH_EYE, "the eyes are on the way down: {}", eye(&p).z);
        assert!((p.feet.y - WALK_SPEED * 0.1).abs() < 1e-6, "full speed until fully down");
        for _ in 0..3 {
            let _ = step(&mut p, Move { crouch: true, ..Move::default() }, 90.0, 0.0, 0.1, &Room);
        }
        assert!(p.crouched);
        assert_eq!(eye(&p).z, CROUCH_EYE);
        let y = p.feet.y;
        let _ = step(&mut p, Move { crouch: true, forward: 1.0, ..Move::default() }, 90.0, 0.0, 0.1, &Room);
        assert!((p.feet.y - y - CROUCH_SPEED * 0.1).abs() < 1e-6, "walks along +Y at yaw 90: {:?}", p.feet);
        let _ = step(&mut p, Move::default(), 90.0, 0.0, 0.1, &Room);
        assert!(!p.crouched, "stands up with room above");
        assert!(eye(&p).z < EYE, "the eyes take time to come back up");
        let _ = step(&mut p, Move::default(), 90.0, 0.0, 0.1, &Room);
        assert_eq!(eye(&p).z, EYE);
    }

    struct PropRoom;

    impl Collider for PropRoom {
        fn trace(&self, from: DVec3, dir: DVec3, max: f64) -> Option<f64> {
            self.trace_prop(from, dir, max).map(|(t, _)| t)
        }

        fn trace_prop(&self, from: DVec3, dir: DVec3, max: f64) -> Option<(f64, Option<hp_vmf::Id>)> {
            let room = Room.trace(from, dir, max).map(|t| (t, None));
            let prop = (dir.x > 0.0 && from.x <= 60.0 && from.z < 20.0)
                .then(|| (60.0 - from.x) / dir.x)
                .filter(|t| *t <= max)
                .map(|t| (t, Some(7)));
            match (room, prop) {
                (Some(a), Some(b)) => Some(if b.0 < a.0 { b } else { a }),
                (a, b) => a.or(b),
            }
        }
    }

    #[test]
    fn walking_into_a_physics_prop_pushes_it() {
        let mut p = player();
        p.feet = DVec3::new(40.0, 0.0, 0.0);
        p.on_ground = true;
        let pushes = step(&mut p, Move { forward: 1.0, ..Move::default() }, 0.0, 0.0, 0.1, &PropRoom);
        assert!((p.feet.x - (60.0 - HALF_WIDTH)).abs() < 0.1, "stops at the prop: {:?}", p.feet);
        let (id, d) = pushes.first().copied().expect("a push");
        assert_eq!(id, 7);
        assert!(d.x > 15.0 && d.y.abs() < 1e-9, "pushed along +X by the rest of the move: {d:?}");
    }

    #[test]
    fn the_animation_follows_the_movement() {
        let mut p = player();
        p.on_ground = true;
        assert_eq!(model_pose(&mut p, 0.0, 0.1).sequence, "idle_all_01");
        p.velocity = DVec3::new(WALK_SPEED, 0.0, 0.0);
        let pose = model_pose(&mut p, 0.0, 0.1);
        assert_eq!(pose.sequence, "run_all_01");
        assert!((pose.move_x - 1.0).abs() < 1e-6 && pose.move_y.abs() < 1e-6);
        let later = model_pose(&mut p, 0.0, 0.1);
        assert!(later.time > pose.time, "time goes on in the same sequence");
        assert!((model_pose(&mut p, 90.0, 0.1).move_y - 1.0).abs() < 1e-6);
        p.crouched = true;
        assert_eq!(model_pose(&mut p, 0.0, 0.1).sequence, "cwalk_all");
        p.crouched = false;
        p.on_ground = false;
        p.velocity.z = 100.0;
        assert_eq!(model_pose(&mut p, 0.0, 0.1).sequence, "jump_slam");
        p.noclip = true;
        assert_eq!(model_pose(&mut p, 0.0, 0.1).sequence, "swimming_all");
        assert_eq!(zoom_camera(200.0, 100.0), CAMERA_DISTANCE_MIN);
        assert_eq!(zoom_camera(200.0, -100.0), CAMERA_DISTANCE_MAX);
    }

    #[test]
    fn the_compiled_map_stands_for_its_props() {
        let mut state = hp_app::testing::test_state();
        let ent = |id: u32, class: &str, model: Option<&str>| {
            let mut e = hp_vmf::Entity { id, classname: class.into(), ..Default::default() };
            if let Some(m) = model {
                e.set("model", m);
            }
            e
        };
        let map = hp_vmf::Map {
            entities: vec![
                ent(2, "prop_static", Some("models/a.mdl")),
                ent(3, "prop_physics", Some("models/b.mdl")),
                ent(4, "prop_dynamic", Some("models/c.MDL")),
                ent(5, "info_target", None),
                ent(6, "env_sprite", Some("sprites/glow01.vmt")),
            ],
            ..Default::default()
        };
        state.doc = hp_editor::Document::from_map(map);
        let world = hp_editor::limits::world::BspWorld { entity_ids: vec![3, 5, 6], ..Default::default() };
        let mut got = compiled_replaced_entities(&state, &world);
        got.sort_by_key(|o| match o {
            ObjRef::Entity(id) | ObjRef::Solid(id) => *id,
        });
        assert_eq!(got, [ObjRef::Entity(2), ObjRef::Entity(4)]);
        let old = hp_editor::limits::world::BspWorld::default();
        assert_eq!(compiled_replaced_entities(&state, &old), [ObjRef::Entity(2)]);
    }

    #[test]
    fn the_lighting_line_tells_where_the_reflections_come_from() {
        hp_app::register_translations();
        crate::register_translations();
        assert_eq!(cubemap_text(true, None), None, "no compiled world");
        let off = cubemap_text(false, Some((2, 0))).unwrap();
        let none = cubemap_text(true, Some((0, 0))).unwrap();
        let built = cubemap_text(true, Some((3, 0))).unwrap();
        let drawn = cubemap_text(true, Some((0, 4))).unwrap();
        let both = cubemap_text(true, Some((1, 2))).unwrap();
        let all = [&off, &none, &built, &drawn, &both];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert!(built.contains('3') && drawn.contains('4'));
        assert!(both.contains('1') && both.contains('2'));
    }

    #[test]
    fn render_mode_none_is_never_drawn() {
        let ent = |mode: Option<&str>| {
            let mut e = hp_vmf::Entity { classname: "func_brush".into(), ..Default::default() };
            if let Some(m) = mode {
                e.set("rendermode", m);
            }
            e
        };
        assert!(render_none(&ent(Some("10"))));
        assert!(render_none(&ent(Some(" 10.0 "))));
        assert!(!render_none(&ent(Some("2"))));
        assert!(!render_none(&ent(Some("x"))));
        assert!(!render_none(&ent(None)));
    }

    #[test]
    fn light_styles_play_and_switch() {
        use hp_editor::limits::world_light::LightEntity;
        let table = StyleTable::from_entities([
            LightEntity { name: "", style: 10, ..Default::default() },
            LightEntity { name: "lamp", style: 32, pattern: "aaazzz", ..Default::default() },
            LightEntity { name: "dark", style: 33, start_dark: true, ..Default::default() },
        ]);
        let mut l = PlayerLights::default();
        let mut seen = Vec::new();
        for frame in 0..6 {
            let v = l.step(&table, 1, if frame == 0 { 0.0 } else { 0.1 }, []);
            seen.push((v[10], v[32], v[33]));
        }
        let want10 = [1.0, 1.0, 0.0, 1.0, 0.0, 1.0];
        let want32 = [0.0, 0.0, 0.0, 550.0 / 264.0, 550.0 / 264.0, 550.0 / 264.0];
        for (k, (v10, v32, v33)) in seen.iter().enumerate() {
            assert!((v10 - want10[k]).abs() < 1e-6, "style 10 at frame {k}: {v10}");
            assert!((v32 - want32[k]).abs() < 1e-6, "pattern at frame {k}: {v32}");
            assert_eq!(*v33, 0.0, "starts dark");
        }
        let v = l.step(&table, 1, 0.0, [("LAMP".to_owned(), LightCmd::Off), ("dark".to_owned(), LightCmd::Toggle)]);
        assert_eq!((v[32], v[33]), (0.0, 1.0));
        let r = l.report(&table);
        assert_eq!(r.styles.iter().map(|s| s.0).collect::<Vec<_>>(), [10, 32, 33]);
        assert_eq!(r.lights[0], ("lamp".to_owned(), 32, false, "aaazzz".to_owned()));
        assert!(r.lights[1].2, "dark is on now");
        let lines = style_lines(&r);
        assert_eq!(lines.len(), 5, "{lines:?}");
        assert!(lines[1].contains("10") && lines[2].contains("lamp") && lines[3].contains("dark"), "{lines:?}");
        let v = l.step(&table, 1, 0.0, [("nobody".to_owned(), LightCmd::On)]);
        assert_eq!(v[32], 0.0);
        let v = l.step(&table, 2, 0.0, []);
        assert_eq!((l.time, v[33]), (0.0, 0.0));
        assert!(style_lines(&LightStyleReport::default()).is_empty());
    }
}
