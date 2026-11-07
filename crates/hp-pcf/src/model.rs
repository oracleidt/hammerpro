use crate::dmx::{Dmx, Element};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Operator {
    pub function: String,
    pub params: Element,
}

impl Operator {
    fn native_value(&self, name: &str) -> bool {
        crate::parameters::expected(&self.function, name)
            .is_none_or(|kind| self.params.get(name).is_some_and(|value| kind.accepts(value)))
    }

    pub fn float(&self, name: &str) -> Option<f32> {
        self.native_value(name).then(|| self.params.float(name)).flatten()
    }

    pub fn int(&self, name: &str) -> Option<i32> {
        self.native_value(name).then(|| self.params.int(name)).flatten()
    }

    pub fn bool(&self, name: &str) -> Option<bool> {
        self.native_value(name).then(|| self.params.bool(name)).flatten()
    }

    pub fn vec3(&self, name: &str) -> Option<[f32; 3]> {
        self.native_value(name).then(|| self.params.vec3(name)).flatten()
    }

    pub fn color(&self, name: &str) -> Option<[u8; 4]> {
        self.native_value(name).then(|| self.params.color(name)).flatten()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Child {
    pub system: String,
    pub delay: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SystemDef {
    pub name: String,
    pub material: String,
    pub group_id: i32,
    pub max_particles: u32,
    pub initial_particles: u32,
    pub radius: f32,
    /// The native system constant is radians; rotation initializers use degrees.
    pub rotation: f32,
    /// Native angular speed constant, in radians per second.
    pub rotation_speed: f32,
    /// Primary and secondary integer animation sequence constants.
    pub sequences: [i32; 2],
    /// Default direction used by model-particle renderers (attribute 21).
    pub normal: [f32; 3],
    pub color: [u8; 4],
    pub emitters: Vec<Operator>,
    pub initializers: Vec<Operator>,
    pub operators: Vec<Operator>,
    pub forces: Vec<Operator>,
    pub constraints: Vec<Operator>,
    pub renderers: Vec<Operator>,
    pub children: Vec<Child>,
}

impl Default for SystemDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            material: String::new(),
            group_id: 0,
            max_particles: 1000,
            initial_particles: 0,
            radius: 5.0,
            rotation: 0.0,
            rotation_speed: 0.0,
            sequences: [0; 2],
            normal: [0.0, 0.0, 1.0],
            color: [255, 255, 255, 255],
            emitters: Vec::new(),
            initializers: Vec::new(),
            operators: Vec::new(),
            forces: Vec::new(),
            constraints: Vec::new(),
            renderers: Vec::new(),
            children: Vec::new(),
        }
    }
}

fn operators(dmx: &Dmx, e: &Element, key: &str) -> Vec<Operator> {
    e.elements(key)
        .into_iter()
        .filter_map(|i| dmx.elements.get(i))
        .map(|op| Operator {
            function: op.str("functionName").unwrap_or(&op.name).to_owned(),
            params: Element {
                kind: op.kind.clone(),
                name: op.name.clone(),
                attrs: op.attrs.iter().filter(|(k, _)| !k.eq_ignore_ascii_case("functionName")).cloned().collect(),
            },
        })
        .collect()
}

pub fn systems(dmx: &Dmx) -> Vec<SystemDef> {
    let mut out = Vec::new();
    for e in &dmx.elements {
        if e.kind != "DmeParticleSystemDefinition" {
            continue;
        }
        let d = SystemDef::default();
        let children = e
            .elements("children")
            .into_iter()
            .filter_map(|i| dmx.elements.get(i))
            .filter_map(|c| {
                let child = c.elements("child").first().and_then(|&i| dmx.elements.get(i))?;
                Some(Child { system: child.name.clone(), delay: c.float("delay").unwrap_or(0.0).clamp(0.0, 60.0) })
            })
            .collect();
        out.push(SystemDef {
            name: e.name.clone(),
            material: e.str("material").unwrap_or("").to_owned(),
            group_id: match e.get("group id") {
                Some(crate::dmx::Value::Int(value)) => *value,
                _ => 0,
            },
            max_particles: e.int("max_particles").map_or(d.max_particles, |v| v.clamp(0, 100_000) as u32),
            initial_particles: e.int("initial_particles").map_or(0, |v| v.clamp(0, 100_000) as u32),
            radius: e.float("radius").map_or(d.radius, |v| v.clamp(0.0, 100_000.0)),
            rotation: match e.get("rotation") {
                Some(crate::dmx::Value::Float(value)) if value.is_finite() => *value,
                _ => d.rotation,
            },
            rotation_speed: match e.get("rotation_speed") {
                Some(crate::dmx::Value::Float(value)) if value.is_finite() => *value,
                _ => d.rotation_speed,
            },
            sequences: ["sequence_number", "sequence_number 1"].map(|name| match e.get(name) {
                Some(crate::dmx::Value::Int(value)) => *value,
                _ => 0,
            }),
            normal: match e.get("normal") {
                Some(crate::dmx::Value::Vec3(value)) if value.iter().all(|v| v.is_finite()) => *value,
                _ => d.normal,
            },
            color: e.color("color").unwrap_or(d.color),
            emitters: operators(dmx, e, "emitters"),
            initializers: operators(dmx, e, "initializers"),
            operators: operators(dmx, e, "operators"),
            forces: operators(dmx, e, "forces"),
            constraints: operators(dmx, e, "constraints"),
            renderers: operators(dmx, e, "renderers"),
            children,
        });
    }
    out
}
