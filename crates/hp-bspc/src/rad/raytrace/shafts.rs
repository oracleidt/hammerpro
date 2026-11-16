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
    pub color: [f32; 3],
    pub spot: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShaftMesh {
    pub shafts: Vec<Shaft>,
    pub vertices: Vec<ShaftVertex>,
    pub indices: Vec<u32>,
    pub material: String,
}

impl ShaftMesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
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
