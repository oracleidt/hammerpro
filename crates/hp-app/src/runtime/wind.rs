use std::cell::RefCell;

use glam::DVec3;
use hp_editor::Id;
use hp_vmf::Entity;

use crate::AppState;

#[derive(Clone, Debug, PartialEq)]
struct Config {
    id: Id,
    origin: DVec3,
    radius: f64,
    yaw: f64,
    normal: [f64; 2],
    gust: [f64; 2],
    delay: [f64; 2],
    duration: f64,
    turn: f64,
}

fn number(e: &Entity, key: &str, default: f64) -> f64 {
    e.get(key).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).unwrap_or(default)
}

fn range(e: &Entity, min: &str, max: &str, defaults: [f64; 2]) -> [f64; 2] {
    let a = number(e, min, defaults[0]).clamp(0.0, 100_000.0);
    let b = number(e, max, defaults[1]).clamp(0.0, 100_000.0);
    [a.min(b), a.max(b)]
}

impl Config {
    fn of(id: Id, e: &Entity) -> Option<Self> {
        if !e.classname.eq_ignore_ascii_case("env_wind") {
            return None;
        }
        Some(Self {
            id,
            origin: e.origin().filter(|p| p.is_finite()).unwrap_or(DVec3::ZERO),
            radius: number(e, "windradius", -1.0).clamp(-1.0, 1_000_000.0),
            yaw: e.angles().filter(|p| p.is_finite()).map_or(0.0, |p| p.y).rem_euclid(360.0),
            normal: range(e, "minwind", "maxwind", [20.0, 50.0]),
            gust: range(e, "mingust", "maxgust", [100.0, 250.0]),
            delay: range(e, "mingustdelay", "maxgustdelay", [10.0, 20.0]),
            duration: number(e, "gustduration", 5.0).clamp(0.05, 100_000.0),
            turn: number(e, "gustdirchange", 20.0).clamp(0.0, 180.0),
        })
    }
}

struct Controller {
    config: Config,
    random: u64,
    yaw: f64,
    speed: f64,
    target: f64,
    gusting: bool,
    switch: f64,
    time: f64,
    velocity: DVec3,
    visible: bool,
}

impl Controller {
    fn new(config: Config, time: f64) -> Self {
        let yaw = config.yaw;
        let speed = (config.normal[0] + config.normal[1]) * 0.5;
        let random = u64::from(config.id).wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = Self {
            config,
            random,
            yaw,
            speed,
            target: speed,
            gusting: false,
            switch: time,
            time,
            velocity: DVec3::ZERO,
            visible: true,
        };
        value.switch = time + value.choose(value.config.delay).max(0.05);
        value.advance(time);
        value
    }

    fn choose(&mut self, range: [f64; 2]) -> f64 {
        self.random = self.random.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        range[0] + (range[1] - range[0]) * ((self.random >> 11) as f64 / (1u64 << 53) as f64)
    }

    fn approach(&mut self, duration: f64) {
        let rate = if self.target >= self.speed { 150.0 } else { 15.0 };
        let difference = self.target - self.speed;
        self.speed += difference.signum() * difference.abs().min(duration.max(0.0) * rate);
    }

    fn advance(&mut self, time: f64) {
        if !time.is_finite() || time < self.time {
            return;
        }
        let mut events = 0;
        while self.switch <= time && events < 256 {
            self.approach(self.switch - self.time);
            self.time = self.switch;
            self.gusting = !self.gusting;
            if self.gusting {
                self.target = self.choose(self.config.gust);
                self.yaw = (self.yaw + self.choose([-self.config.turn, self.config.turn])).rem_euclid(360.0);
                self.switch += self.config.duration;
            } else {
                self.target = self.choose(self.config.normal);
                self.switch += self.choose(self.config.delay).max(0.05);
            }
            events += 1;
        }
        if events == 256 && self.switch <= time {
            self.switch = time + self.config.duration;
        }
        self.approach(time - self.time);
        self.time = time;
        let phase = time * 1.7 + f64::from(self.config.id);
        let yaw = (self.yaw + phase.sin() * 10.0).to_radians();
        let speed = self.speed * (1.0 + (phase * 1.31).sin() * 0.2);
        self.velocity = DVec3::new(yaw.cos(), yaw.sin(), 0.0) * speed;
    }
}

#[derive(Default)]
struct Field {
    key: Option<(u64, u64)>,
    controllers: Vec<Controller>,
    eye: DVec3,
}

thread_local! {
    static FIELD: RefCell<Field> = RefCell::new(Field::default());
}

pub fn update(state: &AppState) {
    FIELD.with(|field| {
        let mut field = field.borrow_mut();
        let time = state.frame.time;
        field.eye = state.cameras.cam3d.pos;
        let key = (state.sidecar.serial(), state.doc.revision());
        if field.key != Some(key) {
            let same_map = field.key.is_some_and(|old| old.0 == key.0);
            let mut old = std::mem::take(&mut field.controllers);
            for (id, node) in state.doc.scene().entities() {
                let Some(config) = Config::of(*id, &node.entity) else { continue };
                let controller =
                    if same_map { old.iter().position(|c| c.config == config).map(|i| old.remove(i)) } else { None };
                field.controllers.push(controller.unwrap_or_else(|| Controller::new(config, time)));
            }
            field.key = Some(key);
        }
        for controller in &mut field.controllers {
            controller.visible = state.doc.is_entity_visible(controller.config.id);
            controller.advance(time);
        }
    });
}

pub fn velocity_at(pos: DVec3) -> Option<DVec3> {
    FIELD.with(|field| {
        let field = field.borrow();
        let local = field
            .controllers
            .iter()
            .find(|c| c.visible && c.config.radius >= 0.0 && pos.distance(c.config.origin) < c.config.radius);
        local.or_else(|| field.controllers.iter().find(|c| c.visible && c.config.radius < 0.0)).map(|c| c.velocity)
    })
}

pub fn active() -> bool {
    FIELD.with(|field| field.borrow().controllers.iter().any(|c| c.visible && c.config.radius != 0.0))
}

pub fn rope_velocity_at(pos: DVec3) -> DVec3 {
    let near = FIELD.with(|field| field.borrow().eye.distance_squared(pos) < 1_000_000.0);
    if near { velocity_at(pos).unwrap_or(DVec3::ZERO) } else { DVec3::ZERO }
}

#[derive(Clone, Copy)]
pub struct Sample {
    pub id: Id,
    pub origin: DVec3,
    pub velocity: DVec3,
    pub gusting: bool,
}

pub fn samples() -> Vec<Sample> {
    FIELD.with(|field| {
        field
            .borrow()
            .controllers
            .iter()
            .filter(|c| c.visible)
            .map(|c| Sample { id: c.config.id, origin: c.config.origin, velocity: c.velocity, gusting: c.gusting })
            .collect()
    })
}
