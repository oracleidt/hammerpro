pub const NONE: u32 = u32::MAX;
pub const MAX_SLOTS: usize = 4;
pub const MAX_PASSES: usize = 2;
pub const MAX_NORMALS: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MaterialKind {
    #[default]
    Opaque,
    Sky,
    Translucent,
    Water,
    Blocker,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub albedo: [f32; 3],
    pub kind: MaterialKind,
    pub alpha: u32,
    pub alpha_cutoff: f32,
    pub transmit: [f32; 3],
    pub two_sided: bool,
}

impl Default for Material {
    fn default() -> Self {
        Material {
            albedo: [0.5; 3],
            kind: MaterialKind::Opaque,
            alpha: NONE,
            alpha_cutoff: 0.5,
            transmit: [1.0; 3],
            two_sided: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlphaTexture {
    pub width: u32,
    pub height: u32,
    pub alpha: Vec<u8>,
}

impl AlphaTexture {
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        if self.width == 0 || self.height == 0 || self.alpha.is_empty() {
            return 1.0;
        }
        let (w, h) = (self.width as i64, self.height as i64);
        let x = ((u * w as f32).floor() as i64).rem_euclid(w);
        let y = ((v * h as f32).floor() as i64).rem_euclid(h);
        f32::from(self.alpha.get((y * w + x) as usize).copied().unwrap_or(255)) * (1.0 / 255.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Triangle {
    pub p: [[f32; 3]; 3],
    pub uv: [[f32; 2]; 3],
    pub lm: [[f32; 2]; 3],
    pub material: u32,
    pub surface: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LightKind {
    Point,
    Spot { stopdot: f32, stopdot2: f32, exponent: f32 },
    Surface,
    Sun,
    SkyAmbient,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub kind: LightKind,
    pub position: [f32; 3],
    pub direction: [f32; 3],
    pub intensity: [[f32; 3]; MAX_PASSES],
    pub style: u8,
    pub constant: f32,
    pub linear: f32,
    pub quadratic: f32,
    pub cap: f32,
    pub fade: (f32, f32),
    pub reach: f32,
    pub size: f32,
}

impl Default for Light {
    fn default() -> Self {
        Light {
            kind: LightKind::Point,
            position: [0.0; 3],
            direction: [0.0, 0.0, -1.0],
            intensity: [[0.0; 3]; MAX_PASSES],
            style: 0,
            constant: 0.0,
            linear: 0.0,
            quadratic: 1.0,
            cap: 1.0e22,
            fade: (0.0, -1.0),
            reach: 1.0e22,
            size: 0.0,
        }
    }
}

impl Light {
    pub fn falloff(&self, to_light: glam::Vec3A, dist: f32) -> f32 {
        let dir = glam::Vec3A::from_array(self.direction);
        if self.kind == LightKind::Surface {
            let dot2 = (-to_light.dot(dir)).max(0.0);
            let f = dot2 / (dist * dist);
            return if f.is_finite() { f } else { 0.0 };
        }
        let (start, end) = self.fade;
        let hard = end > start;
        if hard && dist > end {
            return 0.0;
        }
        let d = dist.max(1.0);
        let e = d.min(self.cap);
        let denom = self.constant + e * (self.linear + e * self.quadratic);
        let mut f = if denom > 0.0 { 1.0 / denom } else { 0.0 };
        if let LightKind::Spot { stopdot, stopdot2, exponent } = self.kind {
            let dot2 = -to_light.dot(dir);
            if dot2 <= stopdot2 {
                return 0.0;
            }
            f *= dot2;
            if dot2 <= stopdot && stopdot > stopdot2 {
                let mut mult = ((dot2 - stopdot2) / (stopdot - stopdot2)).clamp(0.0, 1.0);
                if exponent != 0.0 && exponent != 1.0 {
                    mult = mult.powf(exponent);
                }
                f *= mult;
            }
        }
        if hard {
            let t = 1.0 - ((d - start) / (end - start)).clamp(0.0, 1.0);
            f *= t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
        }
        if f.is_finite() { f.max(0.0) } else { 0.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyCamera {
    pub origin: [f32; 3],
    pub inv_scale: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Surface {
    pub width: u32,
    pub height: u32,
    pub normals: u32,
    pub styles: Vec<u8>,
    pub first_luxel: u32,
    pub lights: (u32, u32),
    pub in_skybox: bool,
    pub minlight: f32,
}

impl Surface {
    pub fn luxels(&self) -> usize {
        self.width as usize * self.height as usize
    }

    pub fn values_len(&self, passes: usize) -> usize {
        passes * self.styles.len().max(1) * self.normals.max(1) as usize * self.luxels()
    }

    pub fn index(&self, pass: usize, slot: usize, n: usize, luxel: usize) -> usize {
        ((pass * self.styles.len().max(1) + slot) * self.normals.max(1) as usize + n) * self.luxels() + luxel
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Luxel {
    pub first_point: u32,
    pub points: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub pos: [f32; 3],
    pub normals: [[f32; 3]; MAX_NORMALS],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub triangles: Vec<Triangle>,
    pub materials: Vec<Material>,
    pub alpha_textures: Vec<AlphaTexture>,
    pub lights: Vec<Light>,
    pub light_lists: Vec<u32>,
    pub sky_cameras: Vec<SkyCamera>,
    pub surfaces: Vec<Surface>,
    pub luxels: Vec<Luxel>,
    pub points: Vec<Point>,
    pub passes: u32,
}

impl Scene {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=MAX_PASSES as u32).contains(&self.passes) {
            return Err(format!("{} lighting passes asked, 1 or 2 expected", self.passes));
        }
        for t in &self.triangles {
            if t.material as usize >= self.materials.len() {
                return Err(format!("triangle material {} out of range", t.material));
            }
            if t.surface != NONE && t.surface as usize >= self.surfaces.len() {
                return Err(format!("triangle surface {} out of range", t.surface));
            }
        }
        for m in &self.materials {
            if m.alpha != NONE && m.alpha as usize >= self.alpha_textures.len() {
                return Err(format!("alpha texture {} out of range", m.alpha));
            }
        }
        for s in &self.surfaces {
            if s.styles.is_empty() || s.styles.len() > MAX_SLOTS {
                return Err(format!("a surface has {} light styles, 1 to {MAX_SLOTS} expected", s.styles.len()));
            }
            if s.normals != 1 && s.normals != 4 {
                return Err(format!("a surface has {} normals, 1 or 4 expected", s.normals));
            }
            if s.first_luxel as usize + s.luxels() > self.luxels.len() {
                return Err("a surface points past the luxel list".to_owned());
            }
            if (s.lights.0 + s.lights.1) as usize > self.light_lists.len() {
                return Err("a surface points past the light lists".to_owned());
            }
        }
        for l in &self.luxels {
            if (l.first_point + l.points) as usize > self.points.len() {
                return Err("a luxel points past the sample points".to_owned());
            }
        }
        if self.light_lists.iter().any(|&i| i as usize >= self.lights.len()) {
            return Err("a light list names a light that does not exist".to_owned());
        }
        Ok(())
    }

    pub fn style_list(&self) -> Vec<u8> {
        let mut out = vec![0u8];
        for l in &self.lights {
            if !out.contains(&l.style) {
                out.push(l.style);
            }
        }
        out
    }
}
