use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Source {
    Particle,
    Control { point: i32, movement: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Color {
    first: V3,
    second: V3,
    source: Source,
    bias: f32,
    minimum: V3,
    maximum: V3,
    amplification: f32,
    blend: i32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Cached {
    position: V3,
    light: V3,
}

pub(super) fn compile(op: &Operator) -> Option<Color> {
    let particle = op.function.eq_ignore_ascii_case("Color Lit Per Particle");
    if !particle && (!op.function.eq_ignore_ascii_case("Color Random") || i(op, "output field", 6) != 6) {
        return None;
    }
    let bias = f(op, if particle { "light bias" } else { "tint_perc" }, 0.0);
    let blend = i(op, "tint blend mode", 0);
    if bias != 0.0 && !(0..=3).contains(&blend) {
        return None;
    }
    Some(Color {
        first: rgb(op.color("color1").unwrap_or([255; 4])),
        second: rgb(op.color("color2").unwrap_or([255; 4])),
        source: if particle {
            Source::Particle
        } else {
            Source::Control {
                point: i(op, "tint control point", 0),
                movement: f(op, "tint update movement threshold", 32.0).max(0.0),
            }
        },
        bias,
        minimum: rgb(op.color("tint clamp min").unwrap_or([0; 4])),
        maximum: rgb(op.color("tint clamp max").unwrap_or([255; 4])),
        amplification: f(op, "light amplification amount", 1.0),
        blend,
    })
}

pub(super) fn initialize(
    color: &Color,
    particle: &mut Particle,
    points: &Points,
    world: &dyn Environment,
    random: &mut Rng,
    cached: &mut Option<Cached>,
) {
    let base = lerp3(color.first, color.second, random.next());
    if color.bias == 0.0 {
        particle.base_color = base;
        return;
    }
    let light = match color.source {
        Source::Particle => world.lighting(particle.pos),
        Source::Control { point, movement } => {
            let position = points.get(point);
            if cached.is_none_or(|cached| {
                dot(sub(position, cached.position), sub(position, cached.position)) > movement * movement
            }) {
                *cached = Some(Cached { position, light: world.lighting(position) });
            }
            cached.as_ref().map_or([1.0; 3], |cached| cached.light)
        }
    };
    let light: V3 =
        std::array::from_fn(|axis| light[axis].max(color.minimum[axis]).min(color.maximum[axis]) * color.amplification);
    let mixed = std::array::from_fn(|axis| match color.blend {
        1 if light[axis] < 0.5 => 2.0 * base[axis] * light[axis],
        1 => 1.0 - 2.0 * (1.0 - base[axis]) * (1.0 - light[axis]),
        2 => base[axis].min(light[axis]),
        3 => base[axis].max(light[axis]),
        _ => light[axis],
    });
    particle.base_color = lerp3(base, mixed, color.bias);
}
