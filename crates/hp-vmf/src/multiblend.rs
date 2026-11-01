use hp_kv::Block;

use crate::DispInfo;
use crate::num::{num_tokens, parse_f64, push_f64};

pub const CHANNELS: usize = 4;
pub const MULTIBLEND: &str = "multiblend";
pub const ALPHABLEND: &str = "alphablend";
pub const COLOR_BLOCKS: [&str; CHANNELS] =
    ["multiblend_color_0", "multiblend_color_1", "multiblend_color_2", "multiblend_color_3"];

#[derive(Clone, Debug, PartialEq)]
pub struct MultiBlend {
    pub weights: Vec<[f64; CHANNELS]>,
    pub alphas: Vec<[f64; CHANNELS]>,
    pub colors: [Vec<[f64; 3]>; CHANNELS],
}

impl MultiBlend {
    pub fn new(count: usize) -> Self {
        Self {
            weights: vec![[1.0, 0.0, 0.0, 0.0]; count],
            alphas: vec![[0.0; CHANNELS]; count],
            colors: std::array::from_fn(|_| vec![[1.0; 3]; count]),
        }
    }

    pub fn len(&self) -> usize {
        self.weights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.weights.is_empty()
    }

    pub fn of(disp: &DispInfo) -> Option<Self> {
        if !has_multiblend(disp) {
            return None;
        }
        let n = grid_size(disp)?;
        let count = n * n;
        let mut out = Self::new(count);
        if let Some(b) = find(disp, MULTIBLEND) {
            read_rows(b, n, &mut out.weights);
        }
        if let Some(b) = find(disp, ALPHABLEND) {
            read_rows(b, n, &mut out.alphas);
        }
        for (name, colors) in COLOR_BLOCKS.iter().zip(out.colors.iter_mut()) {
            if let Some(b) = find(disp, name) {
                read_rows(b, n, colors);
            }
        }
        Some(out)
    }

    pub fn from_alphas(disp: &DispInfo) -> Option<Self> {
        let n = grid_size(disp)?;
        let mut out = Self::new(n * n);
        for (w, a) in out.weights.iter_mut().zip(&disp.alphas) {
            let t = if a.is_finite() { (a / 255.0).clamp(0.0, 1.0) } else { 0.0 };
            *w = [1.0 - t, t, 0.0, 0.0];
        }
        Some(out)
    }

    pub fn of_or_alphas(disp: &DispInfo) -> Option<Self> {
        Self::of(disp).or_else(|| Self::from_alphas(disp))
    }

    pub fn write_to(&self, disp: &mut DispInfo) -> bool {
        let Some(n) = grid_size(disp) else { return false };
        let count = n * n;
        if self.weights.len() != count || self.alphas.len() != count || self.colors.iter().any(|c| c.len() != count) {
            return false;
        }
        let mut blocks: Vec<Block> = Vec::with_capacity(2 + CHANNELS);
        blocks.push(rows_block(MULTIBLEND, n, &self.weights));
        blocks.push(rows_block(ALPHABLEND, n, &self.alphas));
        for (name, colors) in COLOR_BLOCKS.iter().zip(&self.colors) {
            blocks.push(rows_block(name, n, colors));
        }
        for block in blocks {
            match disp.extra_blocks.iter_mut().find(|b| b.name.eq_ignore_ascii_case(&block.name)) {
                Some(slot) => *slot = block,
                None => disp.extra_blocks.push(block),
            }
        }
        true
    }

    pub fn normalize(&mut self) {
        for w in &mut self.weights {
            normalize_weights(w);
        }
    }

    pub fn paint(&mut self, k: usize, channel: usize, amount: f64) -> bool {
        let Some(w) = self.weights.get_mut(k) else { return false };
        if channel >= CHANNELS || !amount.is_finite() {
            return false;
        }
        let old = *w;
        normalize_weights(w);
        let t = amount.clamp(-1.0, 1.0);
        let target = if t >= 0.0 { 1.0 } else { 0.0 };
        let cur = w[channel];
        let new = cur + (target - cur) * t.abs();
        let rest = 1.0 - cur;
        let new_rest = 1.0 - new;
        for (i, v) in w.iter_mut().enumerate() {
            if i == channel {
                *v = new;
            } else if rest > 1e-12 {
                *v *= new_rest / rest;
            } else {
                *v = new_rest / (CHANNELS - 1) as f64;
            }
        }
        normalize_weights(w);
        *w != old
    }
}

pub fn has_multiblend(disp: &DispInfo) -> bool {
    find(disp, MULTIBLEND).is_some()
}

pub fn remove_multiblend(disp: &mut DispInfo) -> bool {
    let before = disp.extra_blocks.len();
    disp.extra_blocks.retain(|b| {
        !(b.name.eq_ignore_ascii_case(MULTIBLEND)
            || b.name.eq_ignore_ascii_case(ALPHABLEND)
            || COLOR_BLOCKS.iter().any(|c| b.name.eq_ignore_ascii_case(c)))
    });
    disp.extra_blocks.len() != before
}

fn normalize_weights(w: &mut [f64; CHANNELS]) {
    for v in w.iter_mut() {
        *v = if v.is_finite() { v.max(0.0) } else { 0.0 };
    }
    let sum: f64 = w.iter().sum();
    if sum > 1e-12 && sum.is_finite() {
        for v in w.iter_mut() {
            *v /= sum;
        }
    } else {
        *w = [1.0, 0.0, 0.0, 0.0];
    }
}

fn grid_size(disp: &DispInfo) -> Option<usize> {
    (disp.power <= crate::MAX_DISP_POWER).then(|| disp.row_len())
}

fn find<'a>(disp: &'a DispInfo, name: &str) -> Option<&'a Block> {
    disp.extra_blocks.iter().find(|b| b.name.eq_ignore_ascii_case(name))
}

fn read_rows<const M: usize>(block: &Block, n: usize, out: &mut [[f64; M]]) {
    for (key, value) in &block.props {
        let Some(row) =
            key.strip_prefix("row").or_else(|| key.strip_prefix("ROW")).and_then(|r| r.parse::<usize>().ok())
        else {
            continue;
        };
        if row >= n {
            continue;
        }
        let mut tokens = num_tokens(value);
        'cols: for col in 0..n {
            let Some(slot) = out.get_mut(row * n + col) else { break };
            for v in slot.iter_mut() {
                let Some(t) = tokens.next() else { break 'cols };
                if let Some(x) = parse_f64(t) {
                    *v = x;
                }
            }
        }
    }
}

fn rows_block<const M: usize>(name: &str, n: usize, values: &[[f64; M]]) -> Block {
    let mut block = Block::new(name);
    for row in 0..n {
        let mut text = String::with_capacity(n * M * 4);
        for col in 0..n {
            let v = values.get(row * n + col).copied().unwrap_or([0.0; M]);
            for (i, x) in v.iter().enumerate() {
                if col > 0 || i > 0 {
                    text.push(' ');
                }
                push_f64(&mut text, round6(*x));
            }
        }
        block.props.push((format!("row{row}"), text));
    }
    block
}

fn round6(x: f64) -> f64 {
    if x.is_finite() { (x * 1e6).round() / 1e6 + 0.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

    use glam::DVec3;

    use super::*;

    fn disp() -> DispInfo {
        DispInfo::new_flat(2, DVec3::ZERO, DVec3::Z)
    }

    #[test]
    fn round_trip_and_order() {
        let mut d = disp();
        d.extra_blocks.push(Block::new("custom_plus"));
        assert!(MultiBlend::of(&d).is_none());
        let mut mb = MultiBlend::new(25);
        assert!(mb.paint(6, 2, 0.5));
        assert!(mb.write_to(&mut d));
        let names: Vec<&str> = d.extra_blocks.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "custom_plus",
                "multiblend",
                "alphablend",
                "multiblend_color_0",
                "multiblend_color_1",
                "multiblend_color_2",
                "multiblend_color_3"
            ]
        );
        let back = MultiBlend::of(&d).expect("data");
        assert_eq!(back, mb);
        assert_eq!(back.weights[6], [0.5, 0.0, 0.5, 0.0]);
        mb.paint(6, 2, 1.0);
        mb.write_to(&mut d);
        assert_eq!(d.extra_blocks.len(), 7);
        assert_eq!(MultiBlend::of(&d).expect("data").weights[6], [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(d.extra_blocks[1].get("row1"), Some("1 0 0 0 0 0 1 0 1 0 0 0 1 0 0 0 1 0 0 0"));
        assert!(remove_multiblend(&mut d));
        assert_eq!(d.extra_blocks.len(), 1);
        assert!(!remove_multiblend(&mut d));
        assert!(!mb.write_to(&mut DispInfo { power: 3, ..disp() }), "wrong size");
    }

    #[test]
    fn the_vertex_alpha_seeds_texture_2() {
        let mut d = disp();
        d.alphas[0] = 255.0;
        d.alphas[1] = 127.5;
        d.alphas[2] = f64::NAN;
        let mb = MultiBlend::of_or_alphas(&d).expect("from alphas");
        assert_eq!((mb.weights[0], mb.weights[1], mb.weights[2]), ([0.0, 1.0, 0.0, 0.0], [0.5, 0.5, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]));
        let mut painted = MultiBlend::new(25);
        painted.paint(0, 3, 1.0);
        painted.write_to(&mut d);
        assert_eq!(MultiBlend::of_or_alphas(&d).expect("rows").weights[0], [0.0, 0.0, 0.0, 1.0], "rows win");
        assert!(MultiBlend::from_alphas(&DispInfo { power: 40, ..disp() }).is_none());
    }

    #[test]
    fn hammer_plus_plus_rows_survive_load_and_save_untouched() {
        let rows = |name: &str, per: usize| {
            let mut s = format!("				{name}
				{{
");
            for r in 0..5 {
                let vals: Vec<String> = (0..5 * per).map(|k| format!("{}", ((r * 7 + k) % 9) as f64 / 8.0)).collect();
                s += &format!("					\"row{r}\" \"{}\"
", vals.join(" "));
            }
            s + "				}
"
        };
        let mut blocks = rows("multiblend", 4) + &rows("alphablend", 4);
        for k in 0..4 {
            blocks += &rows(&format!("multiblend_color_{k}"), 3);
        }
        let text = format!(
            "world
{{
	\"id\" \"1\"
	\"classname\" \"worldspawn\"
	solid
	{{
		\"id\" \"2\"
		side
		{{
			\"id\" \"1\"
			\"plane\" \"(0 128 64) (128 128 64) (128 0 64)\"
			dispinfo
			{{
				\"power\" \"2\"
				\"startposition\" \"[0 0 64]\"
{blocks}			}}
		}}
	}}
}}
"
        );
        let map = crate::parse(&text).expect("map");
        let out = crate::write(&map);
        for line in text.lines().filter(|l| l.contains("\"row")) {
            assert!(out.contains(line.trim()), "kept: {line}");
        }
        let d = map.world.solids[0].sides[0].disp.as_ref().expect("disp");
        let mb = MultiBlend::of(d).expect("4-way rows");
        assert_eq!(mb.weights[1], [0.5, 0.625, 0.75, 0.875]);
        assert_eq!(mb.colors[3][24], [0.5, 0.625, 0.75]);
        let again = crate::write(&crate::parse(&out).expect("reparse"));
        assert_eq!(again, out, "stable");
    }

    #[test]
    fn painting_keeps_weights_normalized() {
        let mut mb = MultiBlend::new(1);
        for (ch, amount) in [(1, 0.3), (3, 0.7), (0, 0.2), (1, -0.5), (2, 1.0), (2, -1.0)] {
            mb.paint(0, ch, amount);
            let w = mb.weights[0];
            assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-12, "{w:?}");
            assert!(w.iter().all(|v| (0.0..=1.0).contains(v)));
        }
        assert!(!mb.paint(0, 9, 1.0));
        assert!(!mb.paint(5, 0, 1.0));
        assert!(!mb.paint(0, 0, f64::NAN));
        let mut solo = MultiBlend::new(1);
        solo.paint(0, 0, -0.6);
        assert_eq!(solo.weights[0].map(|v| (v * 10.0).round() / 10.0), [0.4, 0.2, 0.2, 0.2]);
        solo.weights[0] = [f64::NAN, -1.0, 0.0, f64::INFINITY];
        solo.normalize();
        assert_eq!(solo.weights[0], [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn malformed_blocks_never_panic_or_grow() {
        let mut d = disp();
        let mut b = Block::new("MultiBlend");
        b.props.push(("row0".to_owned(), "0 1 0 0 x 2".to_owned()));
        b.props.push(("row99".to_owned(), "1 1 1 1".to_owned()));
        b.props.push(("rowx".to_owned(), "1".to_owned()));
        b.props.push(("row4".to_owned(), "0 0 0 1 ".repeat(100)));
        d.extra_blocks.push(b);
        let mb = MultiBlend::of(&d).expect("data");
        assert_eq!(mb.len(), 25);
        assert_eq!(mb.weights[0], [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(mb.weights[1], [1.0, 2.0, 0.0, 0.0], "unparsable values keep the default");
        assert_eq!(mb.weights[24], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(mb.colors[3][0], [1.0, 1.0, 1.0]);
        let mut huge = disp();
        huge.power = 40;
        huge.extra_blocks.push(Block::new("multiblend"));
        assert!(MultiBlend::of(&huge).is_none());
    }

    #[test]
    fn written_by_the_vmf_writer() {
        let text = "world\n{\n\t\"id\" \"1\"\n\t\"classname\" \"worldspawn\"\n\tsolid\n\t{\n\t\t\"id\" \"2\"\n\t\tside\n\t\t{\n\t\t\t\"id\" \"1\"\n\t\t\t\"plane\" \"(0 128 64) (128 128 64) (128 0 64)\"\n\t\t\tdispinfo\n\t\t\t{\n\t\t\t\t\"power\" \"2\"\n\t\t\t\t\"startposition\" \"[0 0 64]\"\n\t\t\t}\n\t\t}\n\t}\n}\n";
        let mut map = crate::parse(text).expect("map");
        let d = map.world.solids[0].sides[0].disp.as_mut().expect("disp");
        let mut mb = MultiBlend::new(25);
        mb.paint(0, 3, 1.0);
        assert!(mb.write_to(d));
        let out = crate::write(&map);
        assert!(out.contains("multiblend\n"));
        assert!(out.contains("\"row0\" \"0 0 0 1 1 0 0 0"));
        let back = crate::parse(&out).expect("reparse");
        let d2 = back.world.solids[0].sides[0].disp.as_ref().expect("disp");
        assert_eq!(MultiBlend::of(d2).expect("data"), mb);
    }
}
