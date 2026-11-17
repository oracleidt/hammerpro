use glam::{DVec3, Vec3A};

use super::data::{MapData, i32_at};
use super::lights::Light;
use super::surface::{disp_surface, phong_normal, smoothed_normals};
use crate::emit::lump;
use crate::materials::surf;

const MAX_TEXLIGHTS: usize = 128;
const DLIGHT_THRESHOLD: f32 = 0.1;
const DIRECT_SCALE: f64 = 100.0 * 100.0;
const ON_EPSILON: f64 = 0.1;
const LINE: usize = 1023;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightFile {
    pub path: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TexLightOptions {
    pub files: Vec<LightFile>,
    pub map_name: String,
    pub minchop: f32,
    pub maxchop: f32,
    pub dispchop: f32,
    pub fast: bool,
}

impl Default for TexLightOptions {
    fn default() -> Self {
        Self { files: Vec::new(), map_name: String::new(), minchop: 4.0, maxchop: 4.0, dispchop: 8.0, fast: false }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TexLight {
    pub name: String,
    pub value: Vec3A,
    pub file: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightFileData {
    pub texlights: Vec<TexLight>,
    pub noshadow: Vec<String>,
    pub force_texture_shadow: Vec<String>,
}

fn starts_with_ci(s: &[u8], p: &[u8]) -> bool {
    s.len() >= p.len() && s[..p.len()].eq_ignore_ascii_case(p)
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

fn first_word(s: &[u8]) -> Option<&[u8]> {
    let start = s.iter().position(|c| !is_space(*c))?;
    let len = s[start..].iter().position(|c| is_space(*c)).unwrap_or(s.len() - start);
    Some(&s[start..start + len])
}

fn keyword_arg<'a>(s: &'a [u8], keyword: &[u8]) -> Option<&'a [u8]> {
    s.strip_prefix(keyword).and_then(first_word)
}

fn scan_doubles(s: &[u8], max: usize) -> Vec<f64> {
    let mut out = Vec::new();
    let mut rest = s;
    while out.len() < max {
        let Some(start) = rest.iter().position(|c| !is_space(*c)) else { break };
        rest = &rest[start..];
        let (v, used) = strtod(rest);
        if used == 0 {
            break;
        }
        out.push(v);
        rest = &rest[used..];
    }
    out
}

fn strtod(s: &[u8]) -> (f64, usize) {
    let mut i = 0;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        i += 1;
    }
    let negative = s.first() == Some(&b'-');
    for (word, v) in [(&b"infinity"[..], f64::INFINITY), (b"inf", f64::INFINITY), (b"nan", f64::NAN)] {
        if starts_with_ci(&s[i..], word) {
            return (if negative { -v } else { v }, i + word.len());
        }
    }
    let digits = |s: &[u8], mut i: usize| {
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
        }
        i
    };
    let int_end = digits(s, i);
    let mut end = int_end;
    let mut any = int_end > i;
    if end < s.len() && s[end] == b'.' {
        let frac_end = digits(s, end + 1);
        any |= frac_end > end + 1;
        end = frac_end;
    }
    if !any {
        return (0.0, 0);
    }
    if end < s.len() && (s[end] == b'e' || s[end] == b'E') {
        let mut j = end + 1;
        if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
            j += 1;
        }
        let exp_end = digits(s, j);
        if exp_end > j {
            end = exp_end;
        }
    }
    let text = std::str::from_utf8(&s[..end]).unwrap_or("0");
    (text.parse::<f64>().unwrap_or(0.0), end)
}

pub(crate) fn light_for_string(s: &[u8], hdr: bool) -> Vec3A {
    let v = scan_doubles(s, 8);
    let get = |k: usize| v.get(k).copied().unwrap_or(0.0);
    let (mut r, mut g, mut b, mut scaler) = (get(0), get(1), get(2), get(3));
    let mut count = v.len();
    if count == 8 {
        if hdr {
            (r, g, b, scaler) = (get(4), get(5), get(6), get(7));
        }
        count = 4;
    }
    if r < 0.0 || g < 0.0 || b < 0.0 || scaler < 0.0 {
        return Vec3A::ZERO;
    }
    let lin = |c: f64| (c / 255.0).powf(2.2) * 255.0;
    let mut out = [lin(r), 0.0, 0.0];
    match count {
        1 => out = [out[0]; 3],
        3 | 4 => {
            out[1] = lin(g);
            out[2] = lin(b);
            if count == 4 {
                for c in &mut out {
                    *c *= scaler / 255.0;
                }
            }
        }
        _ => {}
    }
    let out = Vec3A::new(out[0] as f32, out[1] as f32, out[2] as f32);
    if out.is_finite() { out } else { Vec3A::ZERO }
}

fn clean_model_name(name: &str) -> &str {
    let name = if starts_with_ci(name.as_bytes(), b"models/") { &name[7..] } else { name };
    name.split('.').next().unwrap_or(name)
}

fn lines(text: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let end = rest.iter().position(|c| *c == b'\n').map_or(rest.len(), |p| p + 1).min(LINE);
        let (line, tail) = rest.split_at(end);
        rest = tail;
        Some(line)
    })
}

impl LightFileData {
    pub fn read(files: &[LightFile], hdr: bool, log: &mut dyn FnMut(&str)) -> Result<LightFileData, String> {
        let mut out = LightFileData::default();
        for index in 0..files.len() {
            out.read_file(index, files, hdr, log)?;
        }
        Ok(out)
    }

    fn read_file(
        &mut self,
        index: usize,
        files: &[LightFile],
        hdr: bool,
        log: &mut dyn FnMut(&str),
    ) -> Result<(), String> {
        let file = &files[index];
        log(&format!("[Reading texlights from '{}']", file.path));
        let mut count = 0usize;
        for line in lines(file.text.as_bytes()) {
            let mut scan = line;
            if starts_with_ci(scan, b"hdr:") {
                scan = &scan[4..];
                if !hdr {
                    continue;
                }
            }
            if starts_with_ci(scan, b"ldr:") {
                scan = &scan[4..];
                if hdr {
                    continue;
                }
            }
            let skip = scan.iter().position(|c| *c != b' ' && *c != b'\t').unwrap_or(scan.len());
            scan = &scan[skip..];
            if let Some(name) = keyword_arg(scan, b"noshadow") {
                let name = String::from_utf8_lossy(name);
                let name = name.split('.').next().unwrap_or_default().to_owned();
                self.noshadow.push(name);
            } else if let Some(name) = keyword_arg(scan, b"forcetextureshadow") {
                let name = String::from_utf8_lossy(name);
                let clean = clean_model_name(&name).to_owned();
                if !self.force_texture_shadow.contains(&clean) {
                    self.force_texture_shadow.push(clean);
                }
            } else {
                if self.texlights.len() == MAX_TEXLIGHTS {
                    return Err(format!("Too many texlights, max = {MAX_TEXLIGHTS}"));
                }
                let Some(word) = first_word(scan) else {
                    if scan.len() > 4 {
                        log(&format!(
                            "ignoring bad texlight '{}' in {}",
                            String::from_utf8_lossy(scan).trim_end(),
                            file.path
                        ));
                    }
                    continue;
                };
                let name = String::from_utf8_lossy(word).into_owned();
                let value = light_for_string(scan.get(word.len() + 1..).unwrap_or_default(), hdr);
                let j = match self.texlights.iter().position(|t| t.name == name) {
                    Some(j) => {
                        let old = &self.texlights[j];
                        let old_path = &files[old.file].path;
                        if old.file == index || *old_path == file.path {
                            log(&format!("ERROR: Duplication of '{name}' in file '{old_path}'!"));
                        } else if old.value != value {
                            log(&format!("Warning: Overriding '{name}' from '{old_path}' with '{}'!", file.path));
                        } else {
                            log(&format!("Warning: Redundant '{name}' def in '{old_path}' AND '{}'!", file.path));
                        }
                        j
                    }
                    None => {
                        self.texlights.push(TexLight { name: name.clone(), value, file: index });
                        self.texlights.len() - 1
                    }
                };
                self.texlights[j] = TexLight { name, value, file: index };
                count += 1;
            }
        }
        log(&format!("[{count} texlights parsed from '{}']", file.path));
        Ok(())
    }

    pub fn light_for_texture(&self, name: &str, map_name: &str) -> Vec3A {
        let mut name = name;
        let base;
        if let Some(rest) = name.strip_prefix("maps/").and_then(|r| r.strip_prefix(map_name))
            && let Some(rest) = rest.strip_prefix('/')
        {
            let mut cut = rest;
            let mut found = true;
            for _ in 0..3 {
                match cut.rfind('_') {
                    Some(k) => cut = &cut[..k],
                    None => found = false,
                }
            }
            if found {
                base = cut.to_owned();
                name = &base;
            }
        }
        self.texlights.iter().find(|t| t.name.eq_ignore_ascii_case(name)).map_or(Vec3A::ZERO, |t| t.value)
    }

    pub fn casts_no_shadow(&self, material: &str) -> bool {
        let m = material.to_ascii_lowercase();
        self.noshadow.iter().any(|n| m.contains(&n.to_ascii_lowercase()))
    }

    pub fn texture_shadow_forced(&self, model: &str) -> bool {
        let clean = clean_model_name(model).replace('\\', "/");
        self.force_texture_shadow.iter().any(|m| m.replace('\\', "/").eq_ignore_ascii_case(&clean))
    }
}

pub(crate) fn texdata(src: &hp_bsp::Bsp) -> Vec<(String, i32, i32)> {
    let get = |i: usize| src.lump(i).map(|c| c.into_owned()).unwrap_or_default();
    let (data, table, strings) = (get(lump::TEXDATA), get(lump::TEXDATA_STRING_TABLE), get(lump::TEXDATA_STRING_DATA));
    (0..data.len() / 32)
        .map(|i| {
            let o = i * 32;
            let id = i32_at(&data, o + 12);
            let name = usize::try_from(id)
                .ok()
                .filter(|id| id * 4 + 4 <= table.len())
                .and_then(|id| usize::try_from(i32_at(&table, id * 4)).ok())
                .and_then(|at| strings.get(at..))
                .map(|s| String::from_utf8_lossy(&s[..s.iter().position(|c| *c == 0).unwrap_or(s.len())]).into_owned())
                .unwrap_or_default();
            (name, i32_at(&data, o + 16), i32_at(&data, o + 20))
        })
        .collect()
}

#[derive(Clone, Debug)]
struct Patch {
    face: usize,
    winding: Vec<DVec3>,
    origin: DVec3,
    normal: DVec3,
    area: f64,
    mins: DVec3,
    maxs: DVec3,
    chop: f64,
    luxscale: f64,
    scale: f64,
    sky: bool,
    indices: Option<[usize; 3]>,
    disp: bool,
    children: bool,
}

fn bounds(w: &[DVec3]) -> (DVec3, DVec3) {
    w.iter().fold((DVec3::splat(99999.0), DVec3::splat(-99999.0)), |(lo, hi), p| (lo.min(*p), hi.max(*p)))
}

fn winding_area(w: &[DVec3]) -> f64 {
    (2..w.len()).map(|i| (w[i - 1] - w[0]).cross(w[i] - w[0]).length()).sum::<f64>() * 0.5
}

fn area_and_balance(w: &[DVec3]) -> (f64, DVec3) {
    let mut total = 0.0;
    let mut center = DVec3::ZERO;
    for i in 2..w.len() {
        let area = (w[i - 1] - w[0]).cross(w[i] - w[0]).length();
        total += area;
        center += (w[i - 1] + w[i] + w[0]) * (area / 3.0);
    }
    if total != 0.0 {
        center /= total;
    }
    (total * 0.5, center)
}

fn remove_colinear(w: Vec<DVec3>) -> Vec<DVec3> {
    let n = w.len();
    (0..n)
        .filter(|&i| {
            let (j, k) = ((i + 1) % n, (i + n - 1) % n);
            let v1 = (w[j] - w[i]).normalize_or_zero();
            let v2 = (w[i] - w[k]).normalize_or_zero();
            v1.dot(v2) < 0.999
        })
        .map(|i| w[i])
        .collect()
}

fn clip_axis(w: &[DVec3], axis: usize, dist: f64) -> (Option<Vec<DVec3>>, Option<Vec<DVec3>>) {
    let dists: Vec<f64> = w.iter().map(|p| p[axis] - dist).collect();
    let sides: Vec<u8> = dists
        .iter()
        .map(|d| {
            if *d > ON_EPSILON {
                0
            } else if *d < -ON_EPSILON {
                1
            } else {
                2
            }
        })
        .collect();
    if !sides.contains(&0) {
        return (None, Some(w.to_vec()));
    }
    if !sides.contains(&1) {
        return (Some(w.to_vec()), None);
    }
    let (mut f, mut b) = (Vec::new(), Vec::new());
    let n = w.len();
    for i in 0..n {
        let p1 = w[i];
        if sides[i] == 2 {
            f.push(p1);
            b.push(p1);
            continue;
        }
        if sides[i] == 0 {
            f.push(p1);
        } else {
            b.push(p1);
        }
        let next = sides[(i + 1) % n];
        if next == 2 || next == sides[i] {
            continue;
        }
        let p2 = w[(i + 1) % n];
        let dot = dists[i] / (dists[i] - dists[(i + 1) % n]);
        let mut mid = p1 + (p2 - p1) * dot;
        mid[axis] = dist;
        f.push(mid);
        b.push(mid);
    }
    (Some(f), Some(b))
}

struct Patches<'a> {
    list: Vec<Patch>,
    minchop: f64,
    dispchop: f64,
    grids: std::collections::HashMap<usize, (Vec<DVec3>, usize, u32, f64)>,
    vnormals: &'a [Option<Vec<DVec3>>],
    map: &'a MapData,
    offsets: &'a [DVec3],
}

impl Patches<'_> {
    fn subdivide(&mut self, i: usize) {
        let p = &self.list[i];
        if p.sky {
            return;
        }
        let total = (p.maxs - p.mins) * p.luxscale;
        let (mut widest, mut axis) = (-1.0, None);
        let mut cut = false;
        for k in 0..3 {
            if total[k] > widest {
                axis = Some(k);
                widest = total[k];
            }
            if total[k] >= p.chop && total[k] >= self.minchop {
                cut = true;
            }
        }
        let Some(axis) = axis else { return };
        if !cut
            && total[axis] > total[(axis + 1) % 3] * 2.0
            && total[axis] > total[(axis + 2) % 3] * 2.0
            && p.chop > self.minchop
        {
            cut = true;
            self.list[i].chop = self.minchop.max(p.chop / 2.0);
        }
        if !cut {
            return;
        }
        let p = &self.list[i];
        let dist = (p.mins[axis] + p.maxs[axis]) * 0.5;
        let (front, back) = clip_axis(&p.winding, axis, dist);
        let (a1, c1) = front.as_deref().map_or((0.0, DVec3::ZERO), area_and_balance);
        let (a2, c2) = back.as_deref().map_or((0.0, DVec3::ZERO), area_and_balance);
        if a1 == 0.0 || a2 == 0.0 {
            return;
        }
        let c1i = self.child(i, front.unwrap_or_default(), a1, c1);
        let c2i = self.child(i, back.unwrap_or_default(), a2, c2);
        self.list[i].children = true;
        self.subdivide(c1i);
        self.subdivide(c2i);
    }

    fn child(&mut self, parent: usize, winding: Vec<DVec3>, area: f64, origin: DVec3) -> usize {
        let mut c = self.list[parent].clone();
        let (mins, maxs) = bounds(&winding);
        c.winding = winding;
        c.area = area;
        c.origin = origin;
        c.normal = self.phong(c.face, origin);
        c.mins = mins;
        c.maxs = maxs;
        c.children = false;
        self.list.push(c);
        self.list.len() - 1
    }

    fn phong(&self, face: usize, p: DVec3) -> DVec3 {
        let f = &self.map.faces[face];
        let normal = self.map.face_plane(f).map_or(DVec3::Z, |pl| pl.normal);
        let Some(Some(vn)) = self.vnormals.get(face) else { return normal };
        let poly = self.map.face_points(f);
        let off = self.offsets.get(face).copied().unwrap_or(DVec3::ZERO);
        let w = remove_colinear(poly.iter().map(|q| *q + off).collect());
        let center = w.iter().copied().sum::<DVec3>() / w.len().max(1) as f64 - off;
        phong_normal(&poly, center, normal, vn, p)
    }

    fn disp_triangle(&mut self, parent: usize, pts: [DVec3; 3], indices: Option<[usize; 3]>) -> usize {
        let (e0, e1) = (pts[1] - pts[0], pts[2] - pts[0]);
        let n = e1.cross(e0);
        let area = n.length() * 0.5;
        let (mins, maxs) = pts
            .iter()
            .fold((DVec3::splat(f64::MAX), DVec3::splat(f64::MIN_POSITIVE)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let mut c = self.list[parent].clone();
        c.winding = pts.to_vec();
        c.origin = (pts[0] + pts[1] + pts[2]) / 3.0;
        c.normal = n.normalize_or_zero();
        c.area = area;
        c.mins = mins;
        c.maxs = maxs;
        c.indices = indices;
        c.children = false;
        self.list.push(c);
        self.list.len() - 1
    }

    fn disp_children(&mut self, i: usize, level: u32) {
        let face = self.list[i].face;
        let Some((verts, width, power, sample)) = self.grids.get(&face).cloned() else { return };
        let max_len = sample;
        let min_edge = max_len * self.dispchop;
        let w = self.list[i].winding.clone();
        if w.len() == 4 {
            let edges = [w[1] - w[0], w[2] - w[1], w[3] - w[2], w[3] - w[0]];
            let longest = edges.iter().map(|e| e.length()).fold(0.0, f64::max);
            if longest < min_edge {
                return;
            }
            let min_area = (self.dispchop * max_len) * (self.dispchop * max_len);
            if edges[3].cross(edges[0]).length() < min_area {
                return;
            }
            let n = width;
            let tri = |a: usize, b: usize, c: usize| ([verts[a], verts[b], verts[c]], [a, b, c]);
            let (p0, i0) = tri(n * n - 1, 0, n * (n - 1));
            let (p1, i1) = tri(0, n * n - 1, n - 1);
            let c0 = self.disp_triangle(i, p0, Some(i0));
            let c1 = self.disp_triangle(i, p1, Some(i1));
            self.list[i].children = true;
            self.disp_children(c0, 0);
            self.disp_children(c1, 0);
            return;
        }
        if w.len() != 3 {
            return;
        }
        let edges = [w[1] - w[0], w[2] - w[0], w[2] - w[1]];
        let longest = edges.iter().map(|e| e.length()).fold(0.0, f64::max);
        if longest < min_edge {
            return;
        }
        let min_area = (self.dispchop * max_len) * (self.dispchop * max_len) * 0.5;
        if edges[1].cross(edges[0]).length() * 0.5 < min_area {
            return;
        }
        let Some(idx) = self.list[i].indices.filter(|_| level < power * 2) else {
            self.disp_children_sub(i);
            return;
        };
        let mid = (idx[1] + idx[0]) / 2;
        let kids = [[idx[2], idx[0], mid], [idx[1], idx[2], mid]];
        let mut made = [0usize; 2];
        for (k, ids) in kids.iter().enumerate() {
            let pts = ids.map(|v| verts.get(v).copied().unwrap_or(DVec3::ZERO));
            made[k] = self.disp_triangle(i, pts, Some(*ids));
        }
        self.list[i].children = true;
        self.disp_children(made[0], level + 1);
        self.disp_children(made[1], level + 1);
    }

    fn disp_children_sub(&mut self, i: usize) {
        let face = self.list[i].face;
        let Some(&(_, _, _, sample)) = self.grids.get(&face) else { return };
        let w = self.list[i].winding.clone();
        if w.len() != 3 {
            return;
        }
        let min_edge = sample * self.dispchop;
        let edges = [w[1] - w[0], w[2] - w[1], w[0] - w[2]];
        let (mut longest, mut which) = (0.0, None);
        for (k, e) in edges.iter().enumerate() {
            if longest < e.length() {
                longest = e.length();
                which = Some(k);
            }
        }
        if longest < min_edge {
            return;
        }
        let min_area = (self.dispchop * sample) * (self.dispchop * sample) * 0.5;
        if edges[1].cross(edges[0]).length() * 0.5 < min_area {
            return;
        }
        let kids = match which {
            Some(0) => {
                let m = (w[0] + w[1]) * 0.5;
                [[w[0], m, w[2]], [m, w[1], w[2]]]
            }
            Some(1) => {
                let m = (w[1] + w[2]) * 0.5;
                [[w[0], w[1], m], [m, w[2], w[0]]]
            }
            _ => {
                let m = (w[0] + w[2]) * 0.5;
                [[w[0], w[1], m], [m, w[1], w[2]]]
            }
        };
        let c0 = self.disp_triangle(i, kids[0], None);
        let c1 = self.disp_triangle(i, kids[1], None);
        self.list[i].children = true;
        self.disp_children_sub(c0);
        self.disp_children_sub(c1);
    }
}

#[derive(Debug, Default)]
pub(crate) struct Emission {
    pub lights: Vec<Light>,
    pub light_texinfos: Vec<usize>,
    pub faces: usize,
}

fn model_origins(map: &MapData) -> Vec<DVec3> {
    let vec3 = |s: Option<&str>| -> DVec3 {
        let v = scan_doubles(s.unwrap_or("").as_bytes(), 3);
        let g = |k: usize| v.get(k).copied().filter(|x| x.is_finite()).unwrap_or(0.0);
        DVec3::new(g(0), g(1), g(2))
    };
    (0..map.models.len())
        .map(|i| {
            let name = format!("*{i}");
            let ent = map.entities.iter().find(|e| e.get("model") == Some(name.as_str())).or(map.entities.first());
            vec3(ent.and_then(|e| e.get("origin")))
        })
        .collect()
}

pub(crate) fn emitters(
    src: &hp_bsp::Bsp,
    map: &MapData,
    tables: &[LightFileData],
    opts: &TexLightOptions,
    subdivide: bool,
) -> Emission {
    let mut out = Emission::default();
    if tables.iter().all(|t| t.texlights.is_empty()) {
        return out;
    }
    let texdata = texdata(src);
    let base: Vec<Vec<Vec3A>> = texdata
        .iter()
        .map(|(name, _, _)| tables.iter().map(|t| t.light_for_texture(name, &opts.map_name)).collect())
        .collect();
    let lit = |td: i32| {
        usize::try_from(td).ok().and_then(|t| base.get(t)).is_some_and(|b| b.iter().any(|v| *v != Vec3A::ZERO))
    };
    let texinfo_of = |face: usize| usize::try_from(map.faces[face].texinfo).ok().filter(|t| *t < map.texinfo.len());
    let emitting: Vec<usize> =
        (0..map.faces.len()).filter(|&f| texinfo_of(f).is_some_and(|t| lit(map.texinfo[t].texdata))).collect();
    if emitting.is_empty() {
        return out;
    }
    let origins = model_origins(map);
    let mut offsets = vec![DVec3::ZERO; map.faces.len()];
    let mut in_model = vec![false; map.faces.len()];
    let mut order = Vec::new();
    for (mi, m) in map.models.iter().enumerate() {
        let end = m.first_face.saturating_add(m.num_faces).min(map.faces.len());
        for f in m.first_face.min(end)..end {
            offsets[f] = origins[mi];
            in_model[f] = true;
            order.push(f);
        }
    }
    let any_brush = emitting.iter().any(|&f| map.faces[f].disp < 0 && in_model[f]);
    let vnormals =
        if any_brush && subdivide { smoothed_normals(map, super::surface::SMOOTHING_THRESHOLD) } else { Vec::new() };
    let mut flags: Vec<u32> = map.texinfo.iter().map(|t| t.flags).collect();
    let mut patches = Patches {
        list: Vec::new(),
        minchop: f64::from(opts.minchop),
        dispchop: f64::from(opts.dispchop),
        grids: std::collections::HashMap::new(),
        vnormals: &vnormals,
        map,
        offsets: &offsets,
    };
    let is_lit = |f: usize| texinfo_of(f).is_some_and(|t| lit(map.texinfo[t].texdata));
    for &fi in &order {
        let f = &map.faces[fi];
        if f.disp >= 0 || !is_lit(fi) {
            continue;
        }
        let Some(ti) = texinfo_of(fi) else { continue };
        let tex = &map.texinfo[ti];
        let winding = remove_colinear(map.face_points(f).into_iter().map(|p| p + offsets[fi]).collect());
        let area = winding_area(&winding);
        if area.is_nan() || area <= 0.0 {
            continue;
        }
        flags[ti] |= surf::LIGHT;
        let len = |v: [f32; 4]| f64::from((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt());
        let (mins, maxs) = bounds(&winding);
        let origin = winding.iter().copied().sum::<DVec3>() / winding.len() as f64;
        patches.list.push(Patch {
            face: fi,
            origin,
            normal: map.face_plane(f).map_or(DVec3::Z, |p| p.normal),
            area,
            mins,
            maxs,
            chop: f64::from(opts.maxchop),
            luxscale: (len(tex.lm[0]) + len(tex.lm[1])) / 2.0,
            scale: len(tex.tex[0]) * len(tex.tex[1]),
            sky: tex.flags & surf::SKY != 0,
            indices: None,
            disp: false,
            children: false,
            winding,
        });
    }
    let mut disps: Vec<(i32, usize)> =
        emitting.iter().filter(|&&f| map.faces[f].disp >= 0).map(|&f| (map.faces[f].disp, f)).collect();
    disps.sort_unstable();
    for (_, fi) in disps {
        let f = &map.faces[fi];
        let Some(ti) = texinfo_of(fi) else { continue };
        let Some(d) = disp_surface(map, f) else { continue };
        let power = map.face_disp(f).map_or(0, |d| d.power);
        let n = d.n;
        let pts = [d.pos[0], d.pos[n * (n - 1)], d.pos[n * n - 1], d.pos[n - 1]];
        let normal = (pts[3] - pts[0]).cross(pts[1] - pts[0]);
        let lm = map.texinfo[ti].lm[0];
        let sample = 1.0 / f64::from((lm[0] * lm[0] + lm[1] * lm[1] + lm[2] * lm[2]).sqrt());
        let (mins, maxs) = pts
            .iter()
            .fold((DVec3::splat(f64::MAX), DVec3::splat(f64::MIN_POSITIVE)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        patches.grids.insert(fi, (d.pos.clone(), n, power, sample));
        patches.list.push(Patch {
            face: fi,
            winding: pts.to_vec(),
            origin: (pts[0] + pts[1] + pts[2] + pts[3]) / 4.0,
            normal: normal.normalize_or_zero(),
            area: normal.length(),
            mins,
            maxs,
            chop: f64::from(opts.dispchop),
            luxscale: 1.0,
            scale: 1.0,
            sky: false,
            indices: None,
            disp: true,
            children: false,
        });
    }
    out.faces = patches.list.len();
    if subdivide {
        for i in 0..patches.list.len() {
            let p = &patches.list[i];
            let fl = texinfo_of(p.face).map_or(0, |t| flags[t]);
            if fl & surf::NOCHOP != 0 || (fl & surf::NOLIGHT != 0 && fl & surf::LIGHT == 0) {
                continue;
            }
            if p.disp {
                patches.disp_children(i, 0);
            } else {
                patches.subdivide(i);
            }
        }
    }
    for p in patches.list.iter().filter(|p| !p.children) {
        let Some(ti) = texinfo_of(p.face) else { continue };
        let td = map.texinfo[ti].texdata;
        let Some((_, w, h)) = usize::try_from(td).ok().and_then(|t| texdata.get(t)) else { continue };
        let basearea = f64::from(*h) * f64::from(*w);
        if basearea < 1e-6 {
            continue;
        }
        let bl = &base[td as usize];
        let intensity: Vec<Vec3A> = bl
            .iter()
            .map(|b| {
                if (b.x + b.y + b.z) / 3.0 >= DLIGHT_THRESHOLD {
                    *b * (p.area * p.scale / basearea * DIRECT_SCALE) as f32
                } else {
                    Vec3A::ZERO
                }
            })
            .collect();
        if intensity.iter().all(|v| *v == Vec3A::ZERO) || !p.origin.is_finite() {
            continue;
        }
        out.lights.push(Light::surface(p.origin, p.normal, intensity, map));
    }
    out.lights.reverse();
    out.light_texinfos =
        (0..flags.len()).filter(|&t| flags[t] & surf::LIGHT != 0 && map.texinfo[t].flags & surf::LIGHT == 0).collect();
    out
}

#[cfg(test)]
mod tests {
    use glam::DVec3;

    use super::*;

    fn file(path: &str, text: &str) -> LightFile {
        LightFile { path: path.to_owned(), text: text.to_owned() }
    }

    #[test]
    fn light_strings_like_vrad() {
        let v = light_for_string(b"255 255 255 400", false);
        assert!((v.x - 400.0).abs() < 1e-3 && (v.z - 400.0).abs() < 1e-3);
        let gray = light_for_string(b" 128\n", false);
        assert!(gray.x > 0.0 && gray.x == gray.z);
        let two = light_for_string(b"255 255", false);
        assert!(two.x > 254.0 && two.y == 0.0);
        let both = b"255 255 255 100 255 0 0 50";
        assert!((light_for_string(both, false).y - 100.0).abs() < 1e-3);
        assert_eq!(light_for_string(both, true).y, 0.0);
        assert!((light_for_string(both, true).x - 50.0).abs() < 1e-3);
        assert_eq!(light_for_string(b"-1 2 3 4", false), Vec3A::ZERO);
        assert_eq!(light_for_string(b"", false), Vec3A::ZERO);
        let cut = light_for_string(b"255 128,0 9", false);
        assert!(cut.x > 254.0 && cut.y == 0.0);
        assert_eq!(strtod(b"1e3x"), (1000.0, 3));
        assert_eq!(strtod(b".5"), (0.5, 2));
        assert_eq!(strtod(b"x").1, 0);
    }

    #[test]
    fn light_files_like_vrad() {
        let game = file(
            "lights.rad",
            "xeno/crys_4b\t\t\t255 160 128 40000\r\n\r\nldr: lights/white001 255 255 255 200\r\nhdr: lights/white001 255 255 255 100\r\n\
             noshadow models/props/glass.vmt\r\nforcetextureshadow models/trees/tree01.mdl\r\nnoshadowfoo\r\n  \r\nlights/Red 255 0 0\r\n",
        );
        let level = file("map.rad", "LIGHTS/WHITE001 255 255 255 999\nlights/Red 255 0 0\n");
        let mut lines = Vec::new();
        let ldr = LightFileData::read(&[game.clone(), level], false, &mut |l| lines.push(l.to_owned())).unwrap();
        let hdr = LightFileData::read(std::slice::from_ref(&game), true, &mut |_| {}).unwrap();
        assert!((ldr.light_for_texture("xeno/CRYS_4B", "").x - 40000.0).abs() < 1.0);
        assert!((hdr.light_for_texture("lights/white001", "").x - 100.0).abs() < 1e-3);
        assert!((ldr.light_for_texture("lights/white001", "").x - 200.0).abs() < 1e-3);
        assert_eq!(ldr.texlights.len(), 4);
        assert!(lines.iter().any(|l| l.starts_with("Warning: Redundant 'lights/Red'")), "{lines:?}");
        assert!(lines.iter().any(|l| l == "[2 texlights parsed from 'map.rad']"));
        assert_eq!(ldr.noshadow, vec!["models/props/glass".to_owned(), "foo".to_owned()]);
        assert!(ldr.casts_no_shadow("MODELS/PROPS/GLASS_PANE"));
        assert!(ldr.texture_shadow_forced("models/trees/tree01.mdl") && ldr.texture_shadow_forced("trees/tree01"));
        assert!(ldr.light_for_texture("maps/gm_x/lights/white001_1_-2_3", "gm_x").x > 0.0);
        assert_eq!(ldr.light_for_texture("maps/gm_y/lights/white001_1_-2_3", "gm_x"), Vec3A::ZERO);
        assert_eq!(ldr.light_for_texture("maps/gm_x/lights/white001_3", "gm_x"), Vec3A::ZERO);
        let many: String = (0..129).map(|i| format!("t{i} 1 1 1 1\n")).collect();
        assert!(LightFileData::read(&[file("x", &many)], false, &mut |_| {}).is_err());
    }

    #[test]
    fn patch_cuts() {
        let sq = vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(0.0, 128.0, 0.0),
            DVec3::new(128.0, 128.0, 0.0),
            DVec3::new(128.0, 0.0, 0.0),
        ];
        let (f, b) = clip_axis(&sq, 0, 64.0);
        let (fa, fc) = area_and_balance(&f.unwrap());
        let (ba, bc) = area_and_balance(&b.unwrap());
        assert!((fa - 8192.0).abs() < 1e-9 && (ba - 8192.0).abs() < 1e-9);
        assert!((fc.x - 96.0).abs() < 1e-9 && (bc.x - 32.0).abs() < 1e-9);
        assert!(clip_axis(&sq, 0, 200.0).0.is_none());
        let mut with_extra = sq.clone();
        with_extra.insert(1, DVec3::new(0.0, 64.0, 0.0));
        assert_eq!(remove_colinear(with_extra).len(), 4);
        assert!((winding_area(&sq) - 16384.0).abs() < 1e-9);
    }

    use super::super::tests::{compare, compile_vmf, luxel_at, pct, room_with};
    use super::super::{RadOptions, RadQuality, light_bsp};
    use crate::emit::lump as l;

    fn world_lights(bsp: &[u8], hdr: bool) -> Vec<u8> {
        let b = hp_bsp::Bsp::from_vec(bsp.to_vec()).unwrap();
        b.lump(if hdr { 54 } else { 15 }).unwrap().into_owned()
    }

    fn texinfo_flags(bsp: &[u8]) -> Vec<i32> {
        let b = hp_bsp::Bsp::from_vec(bsp.to_vec()).unwrap();
        let t = b.lump(l::TEXINFO).unwrap().into_owned();
        (0..t.len() / 72).map(|i| i32_at(&t, i * 72 + 64)).collect()
    }

    #[test]
    fn ceiling_texture_light() {
        let bsp = compile_vmf(&room_with("DEV/DEV_MEASUREGENERIC01B", "LIGHTS/TESTPANEL", ""));
        let files = vec![file(
            "test.rad",
            "ldr: lights/testpanel 255 255 255 100
hdr: lights/testpanel 255 255 255 50
",
        )];
        let texlights = TexLightOptions { files, ..TexLightOptions::default() };
        let opts = RadOptions {
            bounces: 0,
            quality: RadQuality::Draft,
            threads: Some(2),
            texlights: texlights.clone(),
            ..RadOptions::default()
        };
        let lit = light_bsp(&bsp, &opts, &mut |_| {}).unwrap();
        for hdr in [false, true] {
            let wl = world_lights(&lit, hdr);
            assert_eq!(wl.len(), 88, "one emit_surface light");
            assert_eq!(i32_at(&wl, 40), 0, "emit_surface");
            assert!((super::super::data::f32_at(&wl, 32) + 1.0).abs() < 1e-6);
            let want = if hdr { 50.0 } else { 100.0 } * 512.0 * 10000.0 / 255.0;
            assert!((super::super::data::f32_at(&wl, 12) / want - 1.0).abs() < 1e-4);
        }
        assert!(texinfo_flags(&lit).iter().any(|f| *f as u32 & surf::LIGHT != 0));
        let floor = luxel_at(&lit, DVec3::new(128.0, 128.0, 0.0), DVec3::Z, false).unwrap();
        assert!(floor.x > 10.0, "{floor}");
        let hdr_floor = luxel_at(&lit, DVec3::new(128.0, 128.0, 0.0), DVec3::Z, true).unwrap();
        assert!((hdr_floor.x / floor.x - 0.5).abs() < 0.05);
        let opts = RadOptions { bounces: 1, texlights, ..opts };
        let lit = light_bsp(&bsp, &opts, &mut |_| {}).unwrap();
        assert_eq!(world_lights(&lit, false).len() / 88, 16 * 8);
        let plain = light_bsp(
            &bsp,
            &RadOptions { bounces: 0, quality: RadQuality::Draft, ..RadOptions::default() },
            &mut |_| {},
        )
        .unwrap();
        assert!(world_lights(&plain, false).is_empty());
    }

    const GOLDEN: &str = "C:/Users/oracle/Desktop/gmodmapping/test_maps/.valve_ref/texlight";

    fn golden(name: &str, rad: &[&str], bounces: u32, chop: (f32, f32), floor: f32, bounds: (f32, f32, f32)) {
        let Ok(valve) = std::fs::read(format!("{GOLDEN}/{name}.bsp")) else {
            eprintln!("{GOLDEN}/{name}.bsp is not here: skipped");
            return;
        };
        let files = rad
            .iter()
            .map(|r| file(r, &std::fs::read_to_string(format!("{GOLDEN}/{r}")).expect("light file")))
            .collect();
        let texlights = TexLightOptions { files, minchop: chop.0, maxchop: chop.1, ..TexLightOptions::default() };
        let opts = RadOptions { bounces, texlights, ..RadOptions::default() };
        let ours = light_bsp(&valve, &opts, &mut |_| {}).expect("light");
        assert!(super::super::check_lighting(&ours).is_empty());
        assert_eq!(texinfo_flags(&ours), texinfo_flags(&valve), "{name}: SURF_LIGHT");
        let has_hdr = !world_lights(&valve, true).is_empty();
        for hdr in [false, true] {
            if hdr && !has_hdr {
                continue;
            }
            let (a, b) = (world_lights(&ours, hdr), world_lights(&valve, hdr));
            assert_eq!(a.len(), b.len(), "{name}: world light count");
            for k in 0..a.len() / 4 {
                let (x, y) = (super::super::data::f32_at(&a, k * 4), super::super::data::f32_at(&b, k * 4));
                let same = if matches!(k % 22, 9..=11 | 19..=21) {
                    a[k * 4..k * 4 + 4] == b[k * 4..k * 4 + 4]
                } else {
                    (x - y).abs() <= 1e-3 * y.abs().max(1.0)
                };
                assert!(same, "{name}: world light {} word {}: {x} vs {y}", k / 22, k % 22);
            }
            let (r, faces) = compare(&ours, &valve, hdr, floor);
            let (p10, median, p90) = (pct(&r, 0.1), pct(&r, 0.5), pct(&r, 0.9));
            eprintln!(
                "{name} {}: {faces} faces, {} luxels, ours/VRAD p10 {p10:.3} median {median:.3} p90 {p90:.3}",
                if hdr { "HDR" } else { "LDR" },
                r.len()
            );
            assert!(r.len() > 40);
            assert!(
                (median - 1.0).abs() < bounds.0 && p10 > bounds.1 && p90 < bounds.2,
                "{name}: p10 {p10} median {median} p90 {p90}"
            );
        }
    }

    #[test]
    #[ignore]
    fn time_map() {
        let Ok(path) = std::env::var("TEXLIGHT_MAP") else { return };
        let bsp = std::fs::read(&path).expect("map");
        let files = std::env::var("TEXLIGHT_RAD")
            .unwrap_or_default()
            .split(';')
            .filter(|p| !p.is_empty())
            .map(|p| file(p, &std::fs::read_to_string(p).expect("light file")))
            .collect();
        let opts =
            RadOptions { texlights: TexLightOptions { files, ..TexLightOptions::default() }, ..RadOptions::default() };
        let t = std::time::Instant::now();
        let out = light_bsp(&bsp, &opts, &mut |l| eprintln!("  {l}")).expect("light");
        eprintln!("{path}: {:.1} s", t.elapsed().as_secs_f64());
        if let Ok(save) = std::env::var("TEXLIGHT_SAVE") {
            std::fs::write(save, out).unwrap();
        }
    }

    #[test]
    fn golden_room() {
        golden("texlight_room", &["lights.rad"], 100, (4.0, 4.0), 2.0, (0.03, 0.85, 1.1));
    }

    #[test]
    fn golden_room_without_bounces() {
        golden("texlight_room_b0", &["lights.rad"], 0, (4.0, 4.0), 2.0, (0.01, 0.95, 1.03));
    }

    #[test]
    fn golden_custom() {
        golden("texlight_custom", &["lights.rad", "texlight_custom.rad"], 100, (4.0, 4.0), 2.0, (0.03, 0.85, 1.1));
        golden("texlight_custom_b0", &["lights.rad", "texlight_custom.rad"], 0, (4.0, 4.0), 2.0, (0.01, 0.93, 1.03));
    }

    #[test]
    fn golden_misc() {
        let rad = ["lights.rad", "designer.rad", "texlight_misc.rad"];
        golden("texlight_misc", &rad, 100, (4.0, 4.0), 2.0, (0.03, 0.85, 1.1));
        golden("texlight_misc_chop", &rad, 100, (2.0, 8.0), 2.0, (0.03, 0.85, 1.1));
        golden("texlight_dim", &["lights.rad", "texlight_dim.rad"], 0, (4.0, 4.0), 0.01, (0.03, 0.9, 1.1));
    }
}
