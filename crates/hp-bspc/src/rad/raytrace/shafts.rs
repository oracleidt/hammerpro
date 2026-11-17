use super::Effects;

pub const SHAFT_MATERIAL: &str = "hammerpro/raytrace/light_shaft";

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShaftVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shaft {
    pub origin: [f32; 3],
    pub direction: [f32; 3],
    pub length: f32,
    pub width: f32,
    pub side: [f32; 3],
    pub depth: f32,
    pub color: [f32; 3],
    pub spot: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShaftVolume {
    pub center: [f32; 3],
    pub vertices: Vec<[f32; 3]>,
    pub weights: Vec<f32>,
    pub layers: Vec<u8>,
    pub indices: Vec<u32>,
    pub shells: usize,
    pub color: [f32; 3],
    pub spot: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShaftMesh {
    pub shafts: Vec<Shaft>,
    pub volumes: Vec<ShaftVolume>,
    pub vertices: Vec<ShaftVertex>,
    pub indices: Vec<u32>,
    pub material: String,
}

impl ShaftMesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

pub const VOLUME_MAGIC: &[u8; 4] = b"HPV2";

pub fn encode_volume(vertices: &[[f32; 3]], alphas: &[f32], indices: &[u32], color: [f32; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(28 + vertices.len() * 12 + indices.len() * 4);
    out.extend_from_slice(VOLUME_MAGIC);
    for c in color {
        out.extend_from_slice(&c.to_le_bytes());
    }
    out.extend_from_slice(&(vertices.len() as u32).to_le_bytes());
    for v in vertices {
        for c in v {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    for k in 0..vertices.len() {
        out.extend_from_slice(&alphas.get(k).copied().unwrap_or(1.0).to_le_bytes());
    }
    out.extend_from_slice(&(indices.len() as u32).to_le_bytes());
    for i in indices {
        out.extend_from_slice(&i.to_le_bytes());
    }
    out
}

pub type DecodedVolume = (Vec<[f32; 3]>, Vec<f32>, Vec<u32>, [f32; 4]);

pub fn decode_volume(data: &[u8]) -> Option<DecodedVolume> {
    let mut at = 4usize;
    if data.get(..4)? != VOLUME_MAGIC {
        return None;
    }
    let f32_at = |at: &mut usize| -> Option<f32> {
        let v = f32::from_le_bytes(data.get(*at..*at + 4)?.try_into().ok()?);
        *at += 4;
        Some(v)
    };
    let color = [f32_at(&mut at)?, f32_at(&mut at)?, f32_at(&mut at)?, f32_at(&mut at)?];
    let u32_at = |at: &mut usize| -> Option<u32> {
        let v = u32::from_le_bytes(data.get(*at..*at + 4)?.try_into().ok()?);
        *at += 4;
        Some(v)
    };
    let n = u32_at(&mut at)? as usize;
    if n > data.len() / 12 {
        return None;
    }
    let mut vertices = Vec::with_capacity(n);
    for _ in 0..n {
        vertices.push([f32_at(&mut at)?, f32_at(&mut at)?, f32_at(&mut at)?]);
    }
    let mut alphas = Vec::with_capacity(n);
    for _ in 0..n {
        alphas.push(f32_at(&mut at)?);
    }
    let m = u32_at(&mut at)? as usize;
    if m > data.len() / 4 {
        return None;
    }
    let mut indices = Vec::with_capacity(m);
    for _ in 0..m {
        let i = u32_at(&mut at)?;
        if i as usize >= n {
            return None;
        }
        indices.push(i);
    }
    Some((vertices, alphas, indices, color))
}

pub fn light_shafts(bsp: &[u8], effects: &Effects, log: &mut dyn FnMut(&str)) -> Result<ShaftMesh, String> {
    let rt = super::RayTracing {
        effects: Effects { shafts: true, ..*effects },
        min_samples: Some(1),
        max_samples: Some(1),
        ..super::RayTracing::with_quality(super::Quality::Draft)
    };
    let opts = super::super::RadOptions { raytracing: Some(rt), ..super::super::RadOptions::default() };
    let ctl = super::super::session::Control { cancelled: &|| false, progress: &|_, _| {}, partial: None };
    let p =
        super::Progressive::new(bsp.to_vec(), &opts, &crate::props::NoModels, log, &ctl).map_err(|e| e.to_string())?;
    Ok(p.light_shafts())
}
