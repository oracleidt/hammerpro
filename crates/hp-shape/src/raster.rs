use glam::{DVec2, DVec3};

use crate::font;
use crate::mesh::{TriMesh, round_to};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewKind {
    Top,
    Bottom,
    Front,
    Back,
    Side,
    OtherSide,
    Iso,
}

impl ViewKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "top" => Self::Top,
            "bottom" => Self::Bottom,
            "front" => Self::Front,
            "back" => Self::Back,
            "side" => Self::Side,
            "other_side" => Self::OtherSide,
            "iso" => Self::Iso,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Front => "front",
            Self::Back => "back",
            Self::Side => "side",
            Self::OtherSide => "other_side",
            Self::Iso => "iso",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Top => "TOP (X RIGHT, Y UP)",
            Self::Bottom => "BOTTOM (X RIGHT, -Y UP)",
            Self::Front => "FRONT FROM +X (Y RIGHT, Z UP)",
            Self::Back => "BACK FROM -X (-Y RIGHT, Z UP)",
            Self::Side => "SIDE FROM -Y (X RIGHT, Z UP)",
            Self::OtherSide => "SIDE FROM +Y (-X RIGHT, Z UP)",
            Self::Iso => "ISO",
        }
    }

    fn basis(self) -> (DVec3, DVec3, DVec3, &'static str, &'static str) {
        match self {
            Self::Top => (DVec3::X, DVec3::Y, DVec3::NEG_Z, "X", "Y"),
            Self::Bottom => (DVec3::X, DVec3::NEG_Y, DVec3::Z, "X", "Y"),
            Self::Front => (DVec3::Y, DVec3::Z, DVec3::NEG_X, "Y", "Z"),
            Self::Back => (DVec3::NEG_Y, DVec3::Z, DVec3::X, "Y", "Z"),
            Self::Side => (DVec3::X, DVec3::Z, DVec3::Y, "X", "Z"),
            Self::OtherSide => (DVec3::NEG_X, DVec3::Z, DVec3::NEG_Y, "X", "Z"),
            Self::Iso => {
                let f = DVec3::new(-1.0, 1.0, -0.8).normalize();
                let r = f.cross(DVec3::Z).normalize();
                let u = r.cross(f).normalize();
                (r, u, f, "", "")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub mesh: TriMesh,
    pub color: [u8; 3],
    pub wire: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub pos: DVec3,
    pub color: [u8; 3],
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SheetOptions {
    pub width: u32,
    pub height: u32,
    pub views: Vec<ViewKind>,
    pub dimensions: bool,
    pub grid: bool,
    pub title: String,
    pub ground: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

const BG: [u8; 3] = [30, 32, 36];
const PANEL: [u8; 3] = [42, 45, 50];
const GRID: [u8; 3] = [58, 62, 70];
const TEXT: [u8; 3] = [230, 232, 236];
const DIM: [u8; 3] = [255, 196, 64];
const GROUND: [u8; 3] = [90, 200, 120];

struct Canvas {
    w: i32,
    h: i32,
    rgba: Vec<u8>,
}

impl Canvas {
    fn put(&mut self, x: i32, y: i32, c: [u8; 3]) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        let i = ((y * self.w + x) * 4) as usize;
        self.rgba[i..i + 3].copy_from_slice(&c);
        self.rgba[i + 3] = 255;
    }

    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: [u8; 3]) {
        for y in y0.max(0)..y1.min(self.h) {
            for x in x0.max(0)..x1.min(self.w) {
                self.put(x, y, c);
            }
        }
    }

    fn line(&mut self, a: DVec2, b: DVec2, c: [u8; 3], dash: i32) {
        let n = (b - a).abs().max_element().ceil().clamp(1.0, 8192.0) as i32;
        for i in 0..=n {
            if dash > 0 && (i / dash) % 2 == 1 {
                continue;
            }
            let p = a.lerp(b, f64::from(i) / f64::from(n));
            self.put(p.x.round() as i32, p.y.round() as i32, c);
        }
    }

    fn text(&mut self, x: i32, y: i32, s: &str, c: [u8; 3], scale: i32) {
        let mut cx = x;
        for ch in s.chars() {
            if let Some(rows) = font::glyph(ch) {
                for (ry, row) in rows.iter().enumerate() {
                    for rx in 0..5 {
                        if row & (0x10 >> rx) != 0 {
                            for sy in 0..scale {
                                for sx in 0..scale {
                                    self.put(cx + rx * scale + sx, y + ry as i32 * scale + sy, c);
                                }
                            }
                        }
                    }
                }
            }
            cx += font::WIDTH * scale;
        }
    }

    fn text_boxed(&mut self, x: i32, y: i32, s: &str, c: [u8; 3], scale: i32) {
        let w = font::text_width(s, scale);
        self.rect(x - 2, y - 2, x + w + 1, y + font::HEIGHT * scale, BG);
        self.text(x, y, s, c, scale);
    }
}

fn fmt_len(v: f64) -> String {
    let r = round_to(v, 0.1);
    if (r - r.round()).abs() < 1e-9 { format!("{}", r.round() as i64) } else { format!("{r:.1}") }
}

fn grid_step(span: f64) -> f64 {
    let mut s = 1.0;
    while span / s > 12.0 && s < 65536.0 {
        s *= 2.0;
    }
    s
}

struct Panel {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[allow(clippy::too_many_arguments)]
fn draw_panel(
    cv: &mut Canvas,
    zbuf: &mut [f64],
    p: &Panel,
    view: ViewKind,
    layers: &[Layer],
    markers: &[Marker],
    opts: &SheetOptions,
    scale_text: i32,
) {
    cv.rect(p.x, p.y, p.x + p.w, p.y + p.h, PANEL);
    let (r, u, f, rname, uname) = view.basis();
    let mut lo = DVec2::INFINITY;
    let mut hi = DVec2::NEG_INFINITY;
    let mut zlo = f64::INFINITY;
    let mut zhi = f64::NEG_INFINITY;
    let solid_only = layers.iter().any(|l| !l.wire);
    for l in layers.iter().filter(|l| !solid_only || !l.wire) {
        for v in l.mesh.used_vertices() {
            let q = DVec2::new(v.dot(r), v.dot(u));
            lo = lo.min(q);
            hi = hi.max(q);
            zlo = zlo.min(v.z);
            zhi = zhi.max(v.z);
        }
    }
    if !lo.is_finite() {
        cv.text(p.x + 6, p.y + 6, "NOTHING TO DRAW", TEXT, scale_text);
        return;
    }
    let th = font::HEIGHT * scale_text;
    let margin_x = 16 + font::text_width("-0000.0", scale_text);
    let margin_top = th * 2 + 10;
    let margin_bottom = th * 2 + 14;
    let avail = DVec2::new(f64::from(p.w - margin_x * 2), f64::from(p.h - margin_top - margin_bottom)).max(DVec2::ONE);
    let span = (hi - lo).max(DVec2::splat(1e-3));
    let s = (avail.x / span.x).min(avail.y / span.y);
    let center = (lo + hi) * 0.5;
    let pc = DVec2::new(f64::from(p.x) + f64::from(p.w) * 0.5, f64::from(p.y + margin_top) + avail.y * 0.5);
    let to_px = |q: DVec2| DVec2::new(pc.x + (q.x - center.x) * s, pc.y - (q.y - center.y) * s);
    if opts.grid && view != ViewKind::Iso {
        let step = grid_step(span.x.max(span.y));
        let (gx0, gx1) = ((lo.x / step).floor() as i64, (hi.x / step).ceil() as i64);
        let (gy0, gy1) = ((lo.y / step).floor() as i64, (hi.y / step).ceil() as i64);
        for gx in gx0..=gx1 {
            let a = to_px(DVec2::new(gx as f64 * step, gy0 as f64 * step));
            let b = to_px(DVec2::new(gx as f64 * step, gy1 as f64 * step));
            cv.line(a, b, GRID, 0);
        }
        for gy in gy0..=gy1 {
            let a = to_px(DVec2::new(gx0 as f64 * step, gy as f64 * step));
            let b = to_px(DVec2::new(gx1 as f64 * step, gy as f64 * step));
            cv.line(a, b, GRID, 0);
        }
        cv.text(
            p.x + 6,
            p.y + p.h - th - 4,
            &format!("GRID {}", fmt_len(step)),
            GRID.map(|c| c.saturating_add(60)),
            scale_text,
        );
    }
    let light = (-f + r * 0.35 + u * 0.55).normalize();
    for z in zbuf.iter_mut() {
        *z = f64::INFINITY;
    }
    for l in layers.iter().filter(|l| !l.wire) {
        for i in 0..l.mesh.tris.len() {
            let t = l.mesh.tri(i);
            let n = l.mesh.normals[i];
            let shade = 0.30 + 0.70 * n.dot(light).abs();
            let col = l.color.map(|c| (f64::from(c) * shade).clamp(0.0, 255.0) as u8);
            let sp = t.map(|v| to_px(DVec2::new(v.dot(r), v.dot(u))));
            let dz = t.map(|v| v.dot(f));
            fill_tri(cv, zbuf, p, sp, dz, col);
        }
    }
    for l in layers.iter().filter(|l| l.wire) {
        for [a, b] in l.mesh.edges() {
            let (va, vb) = (l.mesh.verts[a as usize], l.mesh.verts[b as usize]);
            cv.line(to_px(DVec2::new(va.dot(r), va.dot(u))), to_px(DVec2::new(vb.dot(r), vb.dot(u))), l.color, 0);
        }
    }
    if let Some(gz) = opts.ground
        && u.z.abs() > 0.99
        && view != ViewKind::Iso
    {
        let y = to_px(DVec2::new(0.0, gz * u.z)).y;
        cv.line(DVec2::new(f64::from(p.x + 4), y), DVec2::new(f64::from(p.x + p.w - 4), y), GROUND, 6);
    }
    for m in markers {
        let q = to_px(DVec2::new(m.pos.dot(r), m.pos.dot(u)));
        let (x, y) = (q.x.round() as i32, q.y.round() as i32);
        for k in -5..=5 {
            cv.put(x + k, y, m.color);
            cv.put(x, y + k, m.color);
        }
        if !m.label.is_empty() {
            cv.text_boxed(x + 7, y - font::HEIGHT - 2, &m.label, m.color, 1);
        }
    }
    if opts.dimensions && view != ViewKind::Iso {
        let a = to_px(lo);
        let b = to_px(hi);
        let ybar = a.y + 8.0;
        cv.line(DVec2::new(a.x, ybar), DVec2::new(b.x, ybar), DIM, 0);
        for x in [a.x, b.x] {
            cv.line(DVec2::new(x, ybar - 4.0), DVec2::new(x, ybar + 4.0), DIM, 0);
        }
        let label = format!("{} {}", rname, fmt_len(span.x));
        let lw = font::text_width(&label, scale_text);
        cv.text_boxed(((a.x + b.x) * 0.5) as i32 - lw / 2, ybar as i32 + 6, &label, DIM, scale_text);
        let xbar = a.x - 8.0;
        cv.line(DVec2::new(xbar, a.y), DVec2::new(xbar, b.y), DIM, 0);
        for y in [a.y, b.y] {
            cv.line(DVec2::new(xbar - 4.0, y), DVec2::new(xbar + 4.0, y), DIM, 0);
        }
        let label = format!("{} {}", uname, fmt_len(span.y));
        let lw = font::text_width(&label, scale_text);
        cv.text_boxed(
            (xbar as i32 - lw - 4).max(p.x + 2),
            ((a.y + b.y) * 0.5) as i32 - th / 2,
            &label,
            DIM,
            scale_text,
        );
        let sgn = u.x + u.y + u.z;
        let lo_lbl = fmt_len(lo.y * sgn);
        let hi_lbl = fmt_len(hi.y * sgn);
        cv.text_boxed(
            (xbar as i32 - font::text_width(&lo_lbl, scale_text) - 4).max(p.x + 2),
            a.y as i32 - th / 2,
            &lo_lbl,
            TEXT,
            scale_text,
        );
        cv.text_boxed(
            (xbar as i32 - font::text_width(&hi_lbl, scale_text) - 4).max(p.x + 2),
            b.y as i32 - th / 2,
            &hi_lbl,
            TEXT,
            scale_text,
        );
    } else if opts.dimensions {
        let label = format!("H {} (Z {} TO {})", fmt_len(zhi - zlo), fmt_len(zlo), fmt_len(zhi));
        cv.text_boxed(p.x + 6, p.y + p.h - th - 4, &label, DIM, scale_text);
    }
    cv.text_boxed(p.x + 6, p.y + 6, view.label(), TEXT, scale_text);
}

fn fill_tri(cv: &mut Canvas, zbuf: &mut [f64], p: &Panel, sp: [DVec2; 3], dz: [f64; 3], col: [u8; 3]) {
    let lo = sp[0].min(sp[1]).min(sp[2]);
    let hi = sp[0].max(sp[1]).max(sp[2]);
    let x0 = (lo.x.floor() as i32).max(p.x);
    let y0 = (lo.y.floor() as i32).max(p.y);
    let x1 = (hi.x.ceil() as i32).min(p.x + p.w - 1);
    let y1 = (hi.y.ceil() as i32).min(p.y + p.h - 1);
    let area = (sp[1] - sp[0]).perp_dot(sp[2] - sp[0]);
    if area.abs() < 1e-9 {
        return;
    }
    for y in y0..=y1 {
        for x in x0..=x1 {
            let q = DVec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            let w0 = (sp[2] - sp[1]).perp_dot(q - sp[1]) / area;
            let w1 = (sp[0] - sp[2]).perp_dot(q - sp[2]) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-6 || w1 < -1e-6 || w2 < -1e-6 {
                continue;
            }
            let z = w0 * dz[0] + w1 * dz[1] + w2 * dz[2];
            let k = (y * cv.w + x) as usize;
            if z < zbuf[k] {
                zbuf[k] = z;
                cv.put(x, y, col);
            }
        }
    }
}

pub fn render_sheet(layers: &[Layer], markers: &[Marker], opts: &SheetOptions) -> Image {
    let w = opts.width.clamp(64, 4096) as i32;
    let h = opts.height.clamp(64, 4096) as i32;
    let mut cv = Canvas { w, h, rgba: vec![0; (w * h * 4) as usize] };
    cv.rect(0, 0, w, h, BG);
    let scale_text = if w.min(h) >= 700 { 2 } else { 1 };
    let th = font::HEIGHT * scale_text;
    let title_h = if opts.title.is_empty() { 0 } else { th + 10 };
    if !opts.title.is_empty() {
        cv.text(8, 6, &opts.title, TEXT, scale_text);
    }
    let views = if opts.views.is_empty() { vec![ViewKind::Iso] } else { opts.views.clone() };
    let n = views.len() as i32;
    let cols = (f64::from(n).sqrt().ceil() as i32).max(1);
    let rows = (n + cols - 1) / cols;
    let pw = w / cols;
    let ph = (h - title_h) / rows;
    let mut zbuf = vec![f64::INFINITY; (w * h) as usize];
    for (i, v) in views.iter().enumerate() {
        let (cx, cy) = (i as i32 % cols, i as i32 / cols);
        let panel = Panel { x: cx * pw + 2, y: title_h + cy * ph + 2, w: pw - 4, h: ph - 4 };
        draw_panel(&mut cv, &mut zbuf, &panel, *v, layers, markers, opts, scale_text);
    }
    Image { width: w as u32, height: h as u32, rgba: cv.rgba }
}
