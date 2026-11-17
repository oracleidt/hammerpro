use ahash::AHashMap;
use glam::DVec3;

use crate::materials::MaterialFacts;

pub const DEFAULT_LUXEL: f64 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexInfo {
    pub texture: [[f32; 4]; 2],
    pub lightmap: [[f32; 4]; 2],
    pub flags: u32,
    pub texdata: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TexData {
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub reflectivity: [f32; 3],
    pub surfaceprop: String,
    pub has_surfaceprop: bool,
}

#[derive(Clone, Debug, Default)]
pub struct TexInfos {
    pub infos: Vec<TexInfo>,
    pub data: Vec<TexData>,
    pub bottom: AHashMap<i32, i32>,
    by_name: AHashMap<String, i32>,
    by_bits: AHashMap<[u32; 18], i32>,
}

#[derive(Clone, Copy, Debug)]
pub struct SideMapping {
    pub u_axis: DVec3,
    pub u_shift: f64,
    pub u_scale: f64,
    pub v_axis: DVec3,
    pub v_shift: f64,
    pub v_scale: f64,
    pub lightmap_scale: f64,
}

impl TexInfos {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn texdata(&mut self, name: &str, facts: &MaterialFacts) -> i32 {
        let key = name.to_ascii_lowercase().replace('\\', "/");
        if let Some(i) = self.by_name.get(&key) {
            return *i;
        }
        let i = self.data.len() as i32;
        self.data.push(TexData {
            name: name.replace('\\', "/"),
            width: facts.width,
            height: facts.height,
            reflectivity: facts.reflectivity,
            surfaceprop: facts.surfaceprop.to_ascii_lowercase(),
            has_surfaceprop: facts.has_surfaceprop,
        });
        self.by_name.insert(key, i);
        i
    }

    pub fn texdata_index(&self, name: &str) -> Option<i32> {
        self.by_name.get(&name.to_ascii_lowercase().replace('\\', "/")).copied()
    }

    pub fn clone_texdata_of(&mut self, texdata: i32, name: &str) -> i32 {
        let key = name.to_ascii_lowercase().replace('\\', "/");
        if let Some(i) = self.by_name.get(&key) {
            return *i;
        }
        let Some(src) = usize::try_from(texdata).ok().and_then(|d| self.data.get(d)).cloned() else { return -1 };
        let i = self.data.len() as i32;
        self.data.push(TexData { name: name.replace('\\', "/"), ..src });
        self.by_name.insert(key, i);
        i
    }

    pub fn has_texdata(&self, name: &str) -> bool {
        self.by_name.contains_key(&name.to_ascii_lowercase().replace('\\', "/"))
    }

    pub fn clone_texdata(&mut self, ti: i32, name: &str) -> i32 {
        let key = name.to_ascii_lowercase().replace('\\', "/");
        if let Some(i) = self.by_name.get(&key) {
            return *i;
        }
        let Some(src) = usize::try_from(ti)
            .ok()
            .and_then(|t| self.infos.get(t))
            .and_then(|i| usize::try_from(i.texdata).ok())
            .and_then(|d| self.data.get(d))
            .cloned()
        else {
            return -1;
        };
        let i = self.data.len() as i32;
        self.data.push(TexData { name: name.replace('\\', "/"), ..src });
        self.by_name.insert(key, i);
        i
    }

    pub fn add(&mut self, info: TexInfo) -> i32 {
        let mut bits = [0u32; 18];
        for (k, v) in info.texture.iter().chain(info.lightmap.iter()).flatten().enumerate() {
            bits[k] = v.to_bits();
        }
        bits[16] = info.flags;
        bits[17] = info.texdata as u32;
        if let Some(i) = self.by_bits.get(&bits) {
            return *i;
        }
        let i = self.infos.len() as i32;
        self.infos.push(info);
        self.by_bits.insert(bits, i);
        i
    }

    pub fn set_bottom(&mut self, top: i32, bottom: i32) -> i32 {
        let Some(info) = self.infos.get(top as usize).copied() else { return top };
        let b = self.add(TexInfo { texdata: bottom, ..info });
        self.bottom.insert(top, b);
        b
    }

    pub fn for_side(&mut self, m: &SideMapping, offset: DVec3, flags: u32, texdata: i32) -> i32 {
        let lm = if m.lightmap_scale.trunc() == 0.0 {
            DEFAULT_LUXEL as f32
        } else {
            (m.lightmap_scale.trunc() as f32).max(1.0)
        };
        let origin = [offset.x as f32, offset.y as f32, offset.z as f32];
        let add_dot = |start: f32, b: [f32; 3]| (origin[0] * b[0] + origin[1] * b[1]) + (origin[2] * b[2] + start);
        let row = |axis: DVec3, scale: f64, shift: f64| -> ([f32; 4], [f32; 4]) {
            let a = [axis.x as f32, axis.y as f32, axis.z as f32];
            let scale = if scale as f32 == 0.0 { 1.0 } else { scale as f32 };
            let shift = shift as f32;
            let (rs, rl) = (1.0 / scale, 1.0 / lm);
            let t = [a[0] * rs, a[1] * rs, a[2] * rs];
            let l = [a[0] * rl, a[1] * rl, a[2] * rl];
            let shift_scale = scale * rl;
            ([t[0], t[1], t[2], add_dot(shift, t)], [l[0], l[1], l[2], add_dot(shift_scale * shift, l)])
        };
        let (tu, lu) = row(m.u_axis, m.u_scale, m.u_shift);
        let (tv, lv) = row(m.v_axis, m.v_scale, m.v_shift);
        self.add(TexInfo { texture: [tu, tv], lightmap: [lu, lv], flags, texdata })
    }
}
