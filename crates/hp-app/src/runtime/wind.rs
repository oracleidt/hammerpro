use std::cell::RefCell;

use glam::DVec3;
use hp_editor::Id;
use hp_pcf::random::UniformRandomStream;
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
    random: UniformRandomStream,
    variation: UniformRandomStream,
    yaw: f32,
    speed: f32,
    target: f32,
    gusting: bool,
    switch: f64,
    time: f64,
    variation_time: f64,
    angle_variation: f32,
    speed_variation: f32,
    velocity: DVec3,
    visible: bool,
}

fn angle_mod(degrees: f32) -> f32 {
    ((degrees * (65536.0 / 360.0)) as i32 & 65535) as f32 * (360.0 / 65536.0)
}

impl Controller {
    fn new(config: Config, time: f64) -> Self {
        let yaw = config.yaw as i32 as f32;
        let mut value = Self {
            config,
            random: UniformRandomStream::new(0),
            variation: UniformRandomStream::new(0),
            yaw,
            speed: 0.0,
            target: 0.0,
            gusting: true,
            switch: time,
            time,
            variation_time: time,
            angle_variation: 1.0,
            speed_variation: 1.0,
            velocity: DVec3::ZERO,
            visible: true,
        };
        value.advance(time);
        value
    }

    fn approach(&mut self, duration: f32) {
        let rate = if self.target >= self.speed { 150.0 } else { 15.0 };
        let difference = self.target - self.speed;
        self.speed += difference.signum() * difference.abs().min(duration.max(0.0) * rate);
    }

    fn advance(&mut self, time: f64) {
        if !time.is_finite() || time < self.time {
            return;
        }
        while time >= self.variation_time {
            self.angle_variation = self.variation.float(-10.0, 10.0);
            self.speed_variation = 1.0 + self.variation.float(-0.2, 0.2);
            let next = self.variation_time + f64::from(0.1f32);
            if next <= self.variation_time {
                break;
            }
            self.variation_time = next;
        }
        while time > self.switch {
            self.approach((self.switch - self.time) as f32);
            self.time = self.switch;
            if self.gusting {
                self.target = self.random.int(self.config.normal[0] as i32, self.config.normal[1] as i32) as f32;
                self.gusting = false;
                self.switch +=
                    f64::from(self.config.delay[0] as f32 + self.random.float(0.0, self.config.delay[1] as f32));
            } else {
                self.target = self.random.int(self.config.gust[0] as i32, self.config.gust[1] as i32) as f32;
                let turn = self.config.turn as i32;
                self.yaw = angle_mod(self.yaw + self.random.int(-turn, turn) as f32).trunc();
                self.gusting = true;
                self.switch += f64::from(self.config.duration as f32);
            }
            if self.switch <= self.time {
                self.switch = self.time + 0.01;
                if self.switch <= self.time {
                    break;
                }
            }
        }
        self.approach((time - self.time) as f32);
        self.time = time;
        let yaw = (self.yaw + self.angle_variation).to_radians();
        let speed = self.speed * self.speed_variation;
        self.velocity = DVec3::new(f64::from(yaw.cos() * speed), f64::from(yaw.sin() * speed), 0.0);
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

pub fn global_velocity() -> DVec3 {
    FIELD.with(|field| field.borrow().controllers.iter().find(|c| c.visible).map_or(DVec3::ZERO, |c| c.velocity))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            id: 1,
            origin: DVec3::ZERO,
            radius: -1.0,
            yaw: 90.0,
            normal: [400.0; 2],
            gust: [800.0; 2],
            delay: [1000.0; 2],
            duration: 5.0,
            turn: 20.0,
        }
    }

    #[test]
    fn wind_starts_at_rest_and_accelerates_toward_the_target() {
        let mut controller = Controller::new(config(), 0.0);
        assert_eq!(controller.velocity, DVec3::ZERO);
        controller.advance(0.5);
        assert_eq!(controller.speed, 75.0);
        controller.advance(2.75);
        assert_eq!(controller.speed, 400.0);
    }

    #[test]
    fn steady_and_gusting_wind_match_captured_garrys_mod_samples() {
        #[derive(serde::Deserialize)]
        struct Sample {
            time: f64,
            frametime: f64,
            velocity: [f64; 3],
        }
        #[derive(serde::Deserialize)]
        struct Case {
            normal: [f64; 2],
            gust: [f64; 2],
            delay: [f64; 2],
            duration: f64,
            turn: f64,
            start: f64,
            samples: Vec<Sample>,
        }
        #[derive(serde::Deserialize)]
        struct Capture {
            cases: Vec<Case>,
        }
        let capture: Capture = serde_json::from_str(include_str!("wind_native_samples.json")).unwrap();
        for case in capture.cases {
            let config = Config {
                normal: case.normal,
                gust: case.gust,
                delay: case.delay,
                duration: case.duration,
                turn: case.turn,
                ..config()
            };
            let mut controller = Controller::new(config, case.start);
            for sample in case.samples {
                controller.advance(sample.time - sample.frametime);
                let expected = DVec3::from_array(sample.velocity);
                assert!(
                    controller.velocity.distance(expected) < 0.002,
                    "{}: actual {:?}, native {expected:?}",
                    sample.time,
                    controller.velocity
                );
            }
        }
    }

    #[test]
    fn gust_transitions_and_variations_do_not_depend_on_frame_rate_or_entity_id() {
        let mut config = config();
        config.normal = [20.0, 50.0];
        config.gust = [100.0, 250.0];
        config.delay = [0.5, 1.5];
        config.duration = 0.75;
        let mut coarse = Controller::new(config.clone(), 0.0);
        config.id = 73;
        let mut fine = Controller::new(config, 0.0);
        for tick in 1..=3600 {
            let time = f64::from(tick) / 60.0;
            fine.advance(time);
            if tick % 60 == 0 {
                coarse.advance(time);
                assert!(fine.velocity.distance(coarse.velocity) < 0.002);
                assert_eq!(fine.gusting, coarse.gusting);
                assert_eq!(fine.yaw, coarse.yaw);
                assert_eq!(fine.switch, coarse.switch);
            }
        }
    }
}
