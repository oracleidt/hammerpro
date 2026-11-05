use std::fmt;

use glam::DVec3;

pub const MAX_PIECES: usize = 4096;
pub const MAX_MOVES: usize = 256;
pub const MAX_RADIUS: f64 = 2048.0;
pub const MAX_LENGTH: f64 = 65536.0;
pub const MIN_SIDES: u32 = 5;
pub const MAX_SIDES: u32 = 48;
pub const MAX_SEGMENTS: u32 = 256;
pub const MAX_SLICES: u32 = 32;
pub const MAX_COLLAR_EVERY: u32 = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    Left,
    Right,
    Up,
    Down,
}

impl Turn {
    pub const ALL: [Turn; 4] = [Turn::Left, Turn::Right, Turn::Up, Turn::Down];

    pub fn name(self) -> &'static str {
        match self {
            Turn::Left => "left",
            Turn::Right => "right",
            Turn::Up => "up",
            Turn::Down => "down",
        }
    }

    pub fn letter(self) -> char {
        match self {
            Turn::Left => 'L',
            Turn::Right => 'R',
            Turn::Up => 'U',
            Turn::Down => 'D',
        }
    }

    pub fn parse(text: &str) -> Option<Turn> {
        let t = text.trim().to_ascii_lowercase();
        let t = t.strip_suffix("90").unwrap_or(&t).trim_end_matches(['-', '_', ' ']);
        match t {
            "l" | "left" => Some(Turn::Left),
            "r" | "right" => Some(Turn::Right),
            "u" | "up" => Some(Turn::Up),
            "d" | "down" => Some(Turn::Down),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PipeMove {
    Forward(f64),
    Turn(Turn),
}

pub fn parse_moves(text: &str) -> Result<Vec<PipeMove>, String> {
    let mut out = Vec::new();
    for token in text.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':')) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if out.len() >= MAX_MOVES {
            return Err(format!("a route holds at most {MAX_MOVES} moves"));
        }
        out.push(parse_move(token)?);
    }
    Ok(out)
}

pub fn parse_move(token: &str) -> Result<PipeMove, String> {
    let lower = token.trim().to_ascii_lowercase();
    if let Ok(n) = lower.parse::<f64>() {
        return forward(n, token);
    }
    if let Some(turn) = Turn::parse(&lower) {
        return Ok(PipeMove::Turn(turn));
    }
    for prefix in ["forward", "fwd", "f", "s"] {
        if let Some(rest) = lower.strip_prefix(prefix)
            && let Ok(n) = rest.trim_start_matches(['=', '-', '_']).parse::<f64>()
        {
            return forward(n, token);
        }
    }
    Err(format!("\"{token}\" is not a move: use a length (128), left, right, up or down (L R U D)"))
}

fn forward(n: f64, token: &str) -> Result<PipeMove, String> {
    if n.is_finite() && n > 0.0 && n <= MAX_LENGTH {
        Ok(PipeMove::Forward(n))
    } else {
        Err(format!("\"{token}\": a length must be a number from 0 to {MAX_LENGTH}"))
    }
}

pub fn format_moves(moves: &[PipeMove]) -> String {
    moves
        .iter()
        .map(|m| match m {
            PipeMove::Forward(l) => crate::common::fmt_num(*l),
            PipeMove::Turn(t) => t.letter().to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Square,
    Round,
}

impl Shape {
    pub fn name(self) -> &'static str {
        match self {
            Shape::Square => "square",
            Shape::Round => "round",
        }
    }

    pub fn parse(text: &str) -> Option<Shape> {
        match text.trim().to_ascii_lowercase().as_str() {
            "square" | "rect" | "box" => Some(Shape::Square),
            "round" | "polygon" | "circle" => Some(Shape::Round),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    Straight,
    Bend,
    Collar,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PipeSpec {
    pub start: DVec3,
    pub direction: DVec3,
    pub moves: Vec<PipeMove>,
    pub shape: Shape,
    pub sides: u32,
    pub radius: f64,
    pub hollow: bool,
    pub wall: f64,
    pub segments: u32,
    pub segment_length: f64,
    pub bend_radius: f64,
    pub bend_slices: u32,
    pub collar_every: u32,
    pub collar_ends: bool,
    pub collar_bends: bool,
    pub collar_size: f64,
    pub collar_length: f64,
    pub snap: bool,
    pub material: String,
    pub collar_material: Option<String>,
    pub tex_scale: f64,
}

impl Default for PipeSpec {
    fn default() -> Self {
        Self {
            start: DVec3::ZERO,
            direction: DVec3::X,
            moves: vec![PipeMove::Forward(256.0)],
            shape: Shape::Square,
            sides: 12,
            radius: 32.0,
            hollow: true,
            wall: 8.0,
            segments: 4,
            segment_length: 0.0,
            bend_radius: 64.0,
            bend_slices: 4,
            collar_every: 0,
            collar_ends: false,
            collar_bends: false,
            collar_size: 8.0,
            collar_length: 8.0,
            snap: true,
            material: "dev/dev_measuregeneric01b".to_owned(),
            collar_material: None,
            tex_scale: hp_geom::DEFAULT_TEXTURE_SCALE,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PipeError {
    Invalid(String),
    TooShort { leg: usize, length: f64, needed: f64 },
    TooMany(usize),
    Geometry(String),
}

impl fmt::Display for PipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PipeError::Invalid(s) | PipeError::Geometry(s) => f.write_str(s),
            PipeError::TooShort { leg, length, needed } => write!(
                f,
                "run {leg} is {} long but its bends need {} (bend radius on each bent end): lengthen it or reduce the bend radius",
                crate::common::fmt_num(*length),
                crate::common::fmt_num(*needed)
            ),
            PipeError::TooMany(n) => write!(f, "the pipe would need {n} brushes (at most {MAX_PIECES})"),
        }
    }
}

impl std::error::Error for PipeError {}

#[derive(Clone, Debug, PartialEq)]
pub struct PipePiece {
    pub kind: PieceKind,
    pub points: Vec<DVec3>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PipePlan {
    pub spec: PipeSpec,
    pub pieces: Vec<PipePiece>,
    pub wire: Vec<[DVec3; 2]>,
    pub centerline: Vec<DVec3>,
    pub end: DVec3,
    pub end_direction: DVec3,
    pub length: f64,
    pub runs: usize,
    pub bends: usize,
    pub collars: usize,
    pub bounds: hp_geom::Aabb,
}

impl PipePlan {
    pub fn count(&self, kind: PieceKind) -> usize {
        self.pieces.iter().filter(|p| p.kind == kind).count()
    }
}
