use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Count {
    output: Field,
    input: [i32; 2],
    range: [f32; 2],
    multiply: bool,
    within: bool,
}

fn value(input: f32, range: [f32; 2], output: [f32; 2]) -> f32 {
    if range[0] == range[1] {
        output[usize::from(input - range[1] >= 0.0)]
    } else {
        lerp(output[0], output[1], ((input - range[0]) / (range[1] - range[0])).clamp(0.0, 1.0))
    }
}

impl Count {
    pub(super) fn compile(op: &Operator) -> Option<Self> {
        let output = Field::from_index(i(op, "output field", 3).clamp(0, 21));
        if output == Field::Other {
            return None;
        }
        let mut range = [f(op, "output minimum", 0.0), f(op, "output maximum", 1.0)];
        if matches!(output, Field::Alpha | Field::Alpha2) {
            range = range.map(|v| v.clamp(0.0, 1.0));
        }
        Some(Self {
            output,
            input: [i(op, "input minimum", 0), i(op, "input maximum", 10)],
            range,
            multiply: b(op, "output is scalar of initial random range", false),
            within: b(op, "only active within specified input range", false),
        })
    }

    pub(super) fn initialize(&self, particles: &mut [Particle], counter: &mut i32) {
        if self.within && *counter > self.input[1] {
            return;
        }
        for p in particles {
            let count = *counter;
            *counter = counter.wrapping_add(1);
            if self.within && (count < self.input[0] || count > self.input[1]) {
                continue;
            }
            let mut result = value(count as f32, self.input.map(|v| v as f32), self.range);
            if self.multiply {
                result *= get_field(p, self.output);
            }
            match self.output {
                Field::Alpha => p.base_alpha = result,
                Field::Alpha2 => p.alpha2 = result,
                _ => set_field(p, self.output, result, false),
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Vector {
    input: Field,
    output: i32,
    range: [f32; 2],
    values: [V3; 2],
    window: [f32; 2],
    cp: i32,
    local: bool,
    multiply: bool,
}

impl Vector {
    pub(super) fn compile(op: &Operator) -> Option<Self> {
        let input = Field::from_index(i(op, "input field", 8).clamp(0, 21));
        let output = i(op, "output field", 0).clamp(0, 21);
        if input == Field::Other || !matches!(output, 0 | 2 | 6 | 15 | 21) {
            return None;
        }
        Some(Self {
            input,
            output,
            range: [f(op, "input minimum", 0.0), f(op, "input maximum", 1.0)],
            values: [v(op, "output minimum", [0.0; 3]), v(op, "output maximum", [1.0; 3])],
            window: [
                f(op, "emitter lifetime start time (seconds)", -1.0),
                f(op, "emitter lifetime end time (seconds)", -1.0),
            ],
            cp: i(op, "control_point_number", 0).clamp(0, 63),
            local: b(op, "use local system", true),
            multiply: b(op, "output is scalar of initial random range", false),
        })
    }

    pub(super) fn initialize(&self, p: &mut Particle, positions: &control::Positions<'_>) {
        if self.window[0] != -1.0 && self.window[1] != -1.0 && !(p.born >= self.window[0] && p.born < self.window[1]) {
            return;
        }
        let input = get_field(p, self.input);
        let mut result =
            std::array::from_fn(|axis| value(input, self.range, [self.values[0][axis], self.values[1][axis]]));
        if self.output == 0 {
            let origin = positions.at(self.cp, p.born);
            if self.local {
                let axes = positions.current.orientation(self.cp);
                result = std::array::from_fn(|axis| {
                    (result[1] * axes[1][axis] + result[0] * axes[0][axis]) + (result[2] * axes[2][axis] + origin[axis])
                });
            } else {
                result = add(result, origin);
            }
            p.pos = if self.multiply { mul(p.pos, result) } else { result };
            p.previous_pos = if self.multiply && !self.local { mul(p.previous_pos, result) } else { p.pos };
        } else {
            let slot = match self.output {
                2 => &mut p.previous_pos,
                6 => &mut p.base_color,
                15 => &mut p.hitbox_relative,
                21 => &mut p.normal,
                _ => unreachable!(),
            };
            *slot = if self.multiply { mul(*slot, result) } else { result };
        }
    }
}

#[cfg(test)]
mod tests;
