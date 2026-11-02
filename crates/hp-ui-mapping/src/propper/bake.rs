use std::collections::HashMap;

use glam::{DVec2, DVec3};
use hp_editor::prop_export::{DispFace, PropGeometry, PropTri};
use hp_fs::GameFs;
use rayon::prelude::*;

type BakedRow = (i64, Vec<([u8; 4], [u8; 4])>);

pub const MAX_ATLAS: u32 = 4096;
const MIN_CELL: f64 = 16.0;
const PAD: u32 = 4;

struct Layer {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
}

fn to_linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.040_45 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

impl Layer {
    fn load(fs: &GameFs, texture: &str, color: bool) -> Option<Layer> {
        let t = fs.load_texture(texture, None).ok()?;
        if t.width == 0 || t.height == 0 || t.rgba.len() < (t.width * t.height * 4) as usize {
            return None;
        }
        let lut: Vec<f32> = (0..=255u8).map(|c| if color { to_linear(c) } else { f32::from(c) / 255.0 }).collect();
        let px = t
            .rgba
            .chunks_exact(4)
            .map(|p| [lut[p[0] as usize], lut[p[1] as usize], lut[p[2] as usize], f32::from(p[3]) / 255.0])
            .collect();
        Some(Layer { w: t.width as usize, h: t.height as usize, px })
    }

    fn sample(&self, uv: DVec2) -> [f32; 4] {
        let x = uv.x * self.w as f64 - 0.5;
        let y = uv.y * self.h as f64 - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
        let wrap = |v: f64, n: usize| (v as i64).rem_euclid(n as i64) as usize;
        let (xa, xb) = (wrap(x0, self.w), wrap(x0 + 1.0, self.w));
        let (ya, yb) = (wrap(y0, self.h), wrap(y0 + 1.0, self.h));
        let p = |x: usize, y: usize| self.px[y * self.w + x];
        let (a, b, c, d) = (p(xa, ya), p(xb, ya), p(xa, yb), p(xb, yb));
        std::array::from_fn(|i| {
            let top = a[i] + (b[i] - a[i]) * fx;
            let bottom = c[i] + (d[i] - c[i]) * fx;
            top + (bottom - top) * fy
        })
    }
}

struct Blend {
    base: Layer,
    base2: Layer,
    bump: Option<Layer>,
    bump2: Option<Layer>,
    ssbump: bool,
    modulate: Option<Layer>,
    detail: Option<Detail>,
}

struct Detail {
    layer: Layer,
    scale: DVec2,
    factor: f32,
    tint: [f32; 3],
}

impl Detail {
    fn load(fs: &GameFs, m: &hp_vmt::Material) -> Option<Detail> {
        if m.get_i32("$detailblendmode").unwrap_or(0) != 0 {
            return None;
        }
        let layer = Layer::load(fs, &m.texture("$detail")?, false)?;
        let nums: Vec<f64> = m
            .get("$detailscale")
            .unwrap_or("4")
            .split(|c: char| c.is_whitespace() || c == '[' || c == ']')
            .filter_map(|x| x.trim().parse::<f64>().ok())
            .collect();
        let scale = match nums.as_slice() {
            [x, y, ..] => DVec2::new(*x, *y),
            [x] => DVec2::splat(*x),
            [] => DVec2::splat(4.0),
        };
        let factor = m.get_f32("$detailblendfactor").unwrap_or(1.0).clamp(0.0, 1.0);
        let tint = m.get_vec3("$detailtint").unwrap_or([1.0; 3]);
        Some(Detail { layer, scale, factor, tint })
    }

    fn apply(&self, uv: DVec2, c: [f32; 3]) -> [f32; 3] {
        let d = self.layer.sample(uv * self.scale);
        std::array::from_fn(|i| c[i] * (1.0 + (2.0 * d[i] * self.tint[i] - 1.0) * self.factor))
    }
}

impl Blend {
    fn load(fs: &GameFs, name: &str) -> Option<Blend> {
        let m = fs.material(name)?;
        if !hp_vmt::ShaderKind::from_shader_name(&m.shader).is_blend() {
            return None;
        }
        let base = Layer::load(fs, &m.base_texture()?, true)?;
        let base2 = Layer::load(fs, &m.base_texture2()?, true)?;
        let bump = m.texture("$bumpmap").and_then(|t| Layer::load(fs, &t, false));
        let bump2 = m.texture("$bumpmap2").and_then(|t| Layer::load(fs, &t, false));
        let modulate = m.texture("$blendmodulatetexture").and_then(|t| Layer::load(fs, &t, false));
        let detail = Detail::load(fs, &m);
        Some(Blend { base, base2, bump, bump2, ssbump: m.flag("$ssbump"), modulate, detail })
    }

    fn factor(&self, uv: DVec2, alpha: f32) -> f32 {
        let a = alpha.clamp(0.0, 1.0);
        match &self.modulate {
            Some(m) => {
                let t = m.sample(uv);
                let lo = (t[1] - t[0]).clamp(0.0, 1.0);
                let hi = (t[1] + t[0]).clamp(0.0, 1.0);
                if hi - lo < 1e-4 {
                    if a < lo { 0.0 } else { 1.0 }
                } else {
                    let x = ((a - lo) / (hi - lo)).clamp(0.0, 1.0);
                    x * x * (3.0 - 2.0 * x)
                }
            }
            None => a,
        }
    }
}

fn ssbump_normal(t: [f32; 4]) -> DVec3 {
    let (s6, s2, s3) = (6f64.sqrt(), 2f64.sqrt(), 3f64.sqrt());
    let basis = [
        DVec3::new(2.0 / s6, 0.0, 1.0 / s3),
        DVec3::new(-1.0 / s6, 1.0 / s2, 1.0 / s3),
        DVec3::new(-1.0 / s6, -1.0 / s2, 1.0 / s3),
    ];
    let n = basis[0] * f64::from(t[0]) + basis[1] * f64::from(t[1]) + basis[2] * f64::from(t[2]);
    let n = n.normalize_or_zero();
    if n == DVec3::ZERO { DVec3::Z } else { n }
}

#[derive(Clone, Copy)]
struct GridTri {
    grid: [DVec2; 3],
    uv: [DVec2; 3],
    alpha: [f32; 3],
}

impl GridTri {
    fn bary(&self, p: DVec2) -> [f64; 3] {
        let [a, b, c] = self.grid;
        let d = (b - a).perp_dot(c - a);
        if d.abs() < 1e-15 {
            return [1.0, -1.0, -1.0];
        }
        let w1 = (p - a).perp_dot(c - a) / d;
        let w2 = (b - a).perp_dot(p - a) / d;
        [1.0 - w1 - w2, w1, w2]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Cell {
    face: usize,
    want: (f64, f64),
    at: (u32, u32),
    size: (u32, u32),
}

fn pack(cells: &mut [Cell], scale: f64, w: u32, max_h: u32) -> Option<u32> {
    let mut order: Vec<usize> = (0..cells.len()).collect();
    for c in cells.iter_mut() {
        c.size = (
            (c.want.0 * scale).max(MIN_CELL).round().min(f64::from(w - 2 * PAD)) as u32,
            (c.want.1 * scale).max(MIN_CELL).round().min(f64::from(max_h - 2 * PAD)) as u32,
        );
    }
    order.sort_by_key(|&i| std::cmp::Reverse(cells[i].size.1));
    let (mut x, mut y, mut shelf) = (0u32, 0u32, 0u32);
    for i in order {
        let (cw, ch) = (cells[i].size.0 + 2 * PAD, cells[i].size.1 + 2 * PAD);
        if x + cw > w {
            x = 0;
            y += shelf;
            shelf = 0;
        }
        if y + ch > max_h {
            return None;
        }
        cells[i].at = (x + PAD, y + PAD);
        x += cw;
        shelf = shelf.max(ch);
    }
    Some(y + shelf)
}

fn layout(cells: &mut [Cell]) -> (u32, u32) {
    let area: f64 = cells.iter().map(|c| (c.want.0.max(MIN_CELL) + 8.0) * (c.want.1.max(MIN_CELL) + 8.0)).sum();
    let mut scale = 1.0;
    loop {
        let side = ((area * scale * scale * 1.15).sqrt() as u32).next_power_of_two().clamp(64, MAX_ATLAS);
        for w in [side, (side * 2).min(MAX_ATLAS)] {
            if let Some(h) = pack(cells, scale, w, MAX_ATLAS) {
                return (w, h.next_power_of_two().max(16));
            }
        }
        scale *= 0.85;
        if scale < 1e-3 {
            let _ = pack(cells, scale, MAX_ATLAS, u32::MAX / 4);
            return (MAX_ATLAS, MAX_ATLAS);
        }
    }
}

fn tangent_matrix(face: &DispFace, size: (usize, usize)) -> [[f64; 2]; 2] {
    let b = face.base;
    let px = |uv: [f64; 2]| DVec2::new(uv[0] * size.0 as f64, uv[1] * size.1 as f64);
    let uvs = face.base_uv.map(px);
    let dp_ds = ((b[1] - b[0]) + (b[2] - b[3])) * 0.5;
    let dp_dt = ((b[3] - b[0]) + (b[2] - b[1])) * 0.5;
    let duv_ds = ((uvs[1] - uvs[0]) + (uvs[2] - uvs[3])) * 0.5;
    let duv_dt = ((uvs[3] - uvs[0]) + (uvs[2] - uvs[1])) * 0.5;
    let det = duv_ds.x * duv_dt.y - duv_dt.x * duv_ds.y;
    if det.abs() < 1e-12 {
        return [[1.0, 0.0], [0.0, 1.0]];
    }
    let dp_du = ((dp_ds * duv_dt.y - dp_dt * duv_ds.y) / det).normalize_or_zero();
    let dp_dv = ((dp_dt * duv_ds.x - dp_ds * duv_dt.x) / det).normalize_or_zero();
    let (bu, bv) = (dp_ds.normalize_or_zero(), dp_dt.normalize_or_zero());
    [[dp_du.dot(bu), dp_dv.dot(bu)], [dp_du.dot(bv), dp_dv.dot(bv)]]
}

#[derive(Clone, Debug, PartialEq)]
pub struct Baked {
    pub material: String,
    pub files: Vec<(String, Vec<u8>)>,
    pub faces: usize,
    pub size: (u32, u32),
    pub bump: bool,
}

pub fn bake_blends(fs: &GameFs, geo: &mut PropGeometry, folder: &str, surface_prop: &str) -> Option<Baked> {
    let mut blends: HashMap<String, Option<Blend>> = HashMap::new();
    let mut faces: Vec<usize> = Vec::new();
    for (i, face) in geo.disps.iter().enumerate() {
        let blend = blends.entry(face.material.clone()).or_insert_with(|| Blend::load(fs, &face.material));
        if blend.is_some() {
            faces.push(i);
        }
    }
    if faces.is_empty() {
        return None;
    }
    let mut cells: Vec<Cell> = faces
        .iter()
        .map(|&f| {
            let face = &geo.disps[f];
            let b = blends[&face.material].as_ref().expect("loaded");
            let px = |uv: [f64; 2]| DVec2::new(uv[0] * b.base.w as f64, uv[1] * b.base.h as f64);
            let uv = face.base_uv.map(px);
            let ws = ((uv[1] - uv[0]).length() + (uv[2] - uv[3]).length()) * 0.5;
            let wt = ((uv[3] - uv[0]).length() + (uv[2] - uv[1]).length()) * 0.5;
            Cell {
                face: f,
                want: (ws.min(f64::from(MAX_ATLAS)), wt.min(f64::from(MAX_ATLAS))),
                at: (0, 0),
                size: (0, 0),
            }
        })
        .collect();
    let (w, h) = layout(&mut cells);
    let mut grids: HashMap<usize, HashMap<(usize, usize), Vec<GridTri>>> = HashMap::new();
    for t in &geo.tris {
        let Some(d) = t.disp else { continue };
        let Some(n) = geo.disps.get(d.face).map(|f| f.row_len.saturating_sub(1).max(1)) else { continue };
        let tri = GridTri {
            grid: d.grid.map(|g| DVec2::new(g[0], g[1])),
            uv: t.uv.map(|u| DVec2::new(u[0], u[1])),
            alpha: d.alpha.map(|a| a as f32),
        };
        let lo = tri.grid.iter().fold(DVec2::splat(f64::MAX), |a, g| a.min(*g));
        let key = (((lo.x * n as f64) + 1e-6) as usize, ((lo.y * n as f64) + 1e-6) as usize);
        grids.entry(d.face).or_default().entry(key).or_default().push(tri);
    }
    let any_bump = faces.iter().any(|f| blends[&geo.disps[*f].material].as_ref().is_some_and(|b| b.bump.is_some()));
    let mut color = vec![[128u8, 128, 128, 255]; (w * h) as usize];
    let mut normal = vec![[128u8, 128, 255, 255]; if any_bump { (w * h) as usize } else { 0 }];
    for cell in &cells {
        let face = &geo.disps[cell.face];
        let b = blends[&face.material].as_ref().expect("loaded");
        let n = face.row_len.saturating_sub(1).max(1);
        let Some(tris) = grids.get(&cell.face) else { continue };
        let m = tangent_matrix(face, (b.base.w, b.base.h));
        let (x0, y0) = (i64::from(cell.at.0), i64::from(cell.at.1));
        let (cw, ch) = (f64::from(cell.size.0), f64::from(cell.size.1));
        let pad = i64::from(PAD);
        let rows: Vec<BakedRow> = ((y0 - pad)..(y0 + i64::from(cell.size.1) + pad))
            .into_par_iter()
            .map(|py| {
                let t = ((py - y0) as f64 + 0.5) / ch;
                let row = ((x0 - pad)..(x0 + i64::from(cell.size.0) + pad))
                    .map(|px| {
                        let s = ((px - x0) as f64 + 0.5) / cw;
                        let g = DVec2::new(s.clamp(0.0, 1.0), t.clamp(0.0, 1.0));
                        let key = (((g.x * n as f64) as usize).min(n - 1), ((g.y * n as f64) as usize).min(n - 1));
                        let best = tris.get(&key).and_then(|list| {
                            list.iter().map(|tri| (tri, tri.bary(g))).max_by(|a, b| {
                                let ma = a.1.iter().copied().fold(f64::MAX, f64::min);
                                let mb = b.1.iter().copied().fold(f64::MAX, f64::min);
                                ma.total_cmp(&mb)
                            })
                        });
                        let Some((tri, w3)) = best else { return ([128, 128, 128, 255], [128, 128, 255, 255]) };
                        let uv = tri.uv[0] * w3[0] + tri.uv[1] * w3[1] + tri.uv[2] * w3[2];
                        let alpha =
                            tri.alpha[0] * w3[0] as f32 + tri.alpha[1] * w3[1] as f32 + tri.alpha[2] * w3[2] as f32;
                        let f = b.factor(uv, alpha);
                        let (c1, c2) = (b.base.sample(uv), b.base2.sample(uv));
                        let mixed: [f32; 3] = std::array::from_fn(|i| c1[i] + (c2[i] - c1[i]) * f);
                        let mixed = b.detail.as_ref().map_or(mixed, |d| d.apply(uv, mixed));
                        let rgb: [u8; 4] = [to_srgb(mixed[0]), to_srgb(mixed[1]), to_srgb(mixed[2]), 255];
                        let nrm = match &b.bump {
                            Some(bump) => {
                                let decode = |t: [f32; 4]| {
                                    if b.ssbump {
                                        ssbump_normal(t)
                                    } else {
                                        DVec3::new(
                                            f64::from(t[0]) * 2.0 - 1.0,
                                            f64::from(t[1]) * 2.0 - 1.0,
                                            f64::from(t[2]) * 2.0 - 1.0,
                                        )
                                    }
                                };
                                let n1 = decode(bump.sample(uv));
                                let n2 = b.bump2.as_ref().map_or(n1, |b2| decode(b2.sample(uv)));
                                let v = (n1 + (n2 - n1) * f64::from(f)).normalize_or_zero();
                                let v = if v == DVec3::ZERO { DVec3::Z } else { v };
                                let x = m[0][0] * v.x + m[0][1] * v.y;
                                let y = m[1][0] * v.x + m[1][1] * v.y;
                                let out = DVec3::new(x, y, v.z.max(0.05)).normalize();
                                let enc = |c: f64| ((c * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
                                [enc(out.x), enc(out.y), enc(out.z), 255]
                            }
                            None => [128, 128, 255, 255],
                        };
                        (rgb, nrm)
                    })
                    .collect();
                (py, row)
            })
            .collect();
        for (py, row) in rows {
            if py < 0 || py >= i64::from(h) {
                continue;
            }
            for (k, (c, nm)) in row.into_iter().enumerate() {
                let px = x0 - pad + k as i64;
                if px < 0 || px >= i64::from(w) {
                    continue;
                }
                let i = (py * i64::from(w) + px) as usize;
                color[i] = c;
                if any_bump {
                    normal[i] = nm;
                }
            }
        }
    }
    let material = format!("{}/blend", folder.trim_matches('/'));
    let by_face: HashMap<usize, Cell> = cells.iter().map(|c| (c.face, *c)).collect();
    for t in &mut geo.tris {
        let Some(d) = t.disp else { continue };
        let Some(cell) = by_face.get(&d.face) else { continue };
        t.material.clone_from(&material);
        for k in 0..3 {
            t.uv[k] = [
                (f64::from(cell.at.0) + d.grid[k][0] * f64::from(cell.size.0)) / f64::from(w),
                (f64::from(cell.at.1) + d.grid[k][1] * f64::from(cell.size.1)) / f64::from(h),
            ];
        }
    }
    let encode = |pixels: &[[u8; 4]], flags: u32| {
        let img = hp_vtf::RgbaImage { width: w, height: h, pixels: pixels.iter().flatten().copied().collect() };
        let opts = hp_vtf::EncodeOptions {
            format: hp_vtf::EncodeFormat::Dxt1,
            mipmaps: true,
            flags,
            ..hp_vtf::EncodeOptions::default()
        };
        hp_vtf::encode(&img, &opts).ok()
    };
    let mut files = Vec::new();
    files.push((format!("materials/{material}.vtf"), encode(&color, 0)?));
    let mut vmt = format!("\"VertexLitGeneric\"\n{{\n\t\"$basetexture\" \"{material}\"\n");
    if any_bump {
        files.push((format!("materials/{material}_normal.vtf"), encode(&normal, hp_vtf::flags::NORMAL)?));
        vmt.push_str(&format!("\t\"$bumpmap\" \"{material}_normal\"\n"));
    }
    let surface = if surface_prop.trim().is_empty() { "default" } else { surface_prop.trim() };
    vmt.push_str(&format!("\t\"$surfaceprop\" \"{surface}\"\n\t\"%keywords\" \"hammerpro,propper\"\n}}\n"));
    files.push((format!("materials/{material}.vmt"), vmt.into_bytes()));
    Some(Baked { material, files, faces: faces.len(), size: (w, h), bump: any_bump })
}

pub fn dominant_surface_prop(fs: &GameFs, tris: &[PropTri]) -> Option<String> {
    let mut props: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    let mut area: HashMap<String, f64> = HashMap::new();
    for t in tris {
        let (s1, s2) = props
            .entry(t.material.clone())
            .or_insert_with(|| {
                fs.material(&t.material).map_or((None, None), |m| {
                    (m.surface_prop().map(str::to_owned), m.surface_prop2().map(str::to_owned))
                })
            })
            .clone();
        let a = (t.pos[1] - t.pos[0]).cross(t.pos[2] - t.pos[0]).length() * 0.5;
        let alpha = t.disp.map_or(0.0, |d| (d.alpha[0] + d.alpha[1] + d.alpha[2]) / 3.0);
        match (s1, s2) {
            (Some(p1), Some(p2)) => {
                *area.entry(p1).or_default() += a * (1.0 - alpha);
                *area.entry(p2).or_default() += a * alpha;
            }
            (Some(p), None) | (None, Some(p)) => *area.entry(p).or_default() += a,
            (None, None) => {}
        }
    }
    area.into_iter().max_by(|a, b| a.1.total_cmp(&b.1)).map(|(p, _)| p)
}

#[cfg(test)]
mod tests;
