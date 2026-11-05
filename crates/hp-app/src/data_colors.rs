use egui::Color32;

pub const CHANNEL_MASTER: Color32 = Color32::from_rgb(225, 225, 225);
pub const CHANNEL_RED: Color32 = Color32::from_rgb(240, 90, 90);
pub const CHANNEL_GREEN: Color32 = Color32::from_rgb(100, 215, 110);
pub const CHANNEL_BLUE: Color32 = Color32::from_rgb(100, 150, 245);

pub const PIN_COLOR: Color32 = Color32::from_rgb(240, 190, 70);
pub const PIN_FLOAT: Color32 = Color32::from_rgb(110, 205, 125);
pub const PIN_UV: Color32 = Color32::from_rgb(185, 125, 235);

pub const NODE_INPUT: Color32 = Color32::from_rgb(50, 98, 158);
pub const NODE_UV: Color32 = Color32::from_rgb(112, 74, 158);
pub const NODE_MATH: Color32 = Color32::from_rgb(46, 124, 92);
pub const NODE_COLOR: Color32 = Color32::from_rgb(176, 104, 44);
pub const NODE_FILTER: Color32 = Color32::from_rgb(40, 124, 146);
pub const NODE_PROCEDURAL: Color32 = Color32::from_rgb(158, 64, 76);
pub const NODE_CHANNELS: Color32 = Color32::from_rgb(150, 66, 124);
pub const NODE_OTHER: Color32 = Color32::from_rgb(70, 70, 70);

pub const TILE_GRID: Color32 = Color32::from_rgb(255, 200, 0);
pub const PREVIEW_BACKDROP: Color32 = Color32::from_rgb(23, 25, 31);
pub const CARVE: Color32 = Color32::from_rgb(255, 96, 64);

pub fn node_category(cat: &str) -> Color32 {
    match cat {
        "input" => NODE_INPUT,
        "uv" => NODE_UV,
        "math" => NODE_MATH,
        "color" => NODE_COLOR,
        "filter" => NODE_FILTER,
        "procedural" => NODE_PROCEDURAL,
        "channels" => NODE_CHANNELS,
        _ => NODE_OTHER,
    }
}

pub fn channel(i: usize) -> Color32 {
    match i {
        1 => CHANNEL_RED,
        2 => CHANNEL_GREEN,
        3 => CHANNEL_BLUE,
        _ => CHANNEL_MASTER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_are_distinct_and_hued() {
        let cats = ["input", "uv", "math", "color", "filter", "procedural", "channels"];
        for (i, a) in cats.iter().enumerate() {
            let ca = node_category(a);
            assert!(!(ca.r() == ca.g() && ca.g() == ca.b()), "{a} is hued");
            for b in &cats[i + 1..] {
                assert_ne!(ca, node_category(b), "{a} vs {b}");
            }
        }
    }

    #[test]
    fn channels_follow_rgb() {
        assert!(channel(1).r() > channel(1).g());
        assert!(channel(2).g() > channel(2).r());
        assert!(channel(3).b() > channel(3).r());
    }
}
