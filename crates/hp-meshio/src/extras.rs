use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextureSource {
    pub path: Option<PathBuf>,
    pub embedded: Option<(Vec<u8>, String)>,
}

impl TextureSource {
    pub fn is_some(&self) -> bool {
        self.path.is_some() || self.embedded.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NormalKind {
    #[default]
    OpenGl,
    DirectX,
    Bump,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialExtras {
    pub normal: Option<(TextureSource, NormalKind)>,
    pub metallic: Option<f64>,
    pub roughness: Option<f64>,
    pub metal_rough: Option<TextureSource>,
    pub specular: Option<f64>,
    pub shininess: Option<f64>,
    pub emissive: Option<[f64; 3]>,
    pub emissive_texture: Option<TextureSource>,
    pub uv_scale: [f64; 2],
    pub uv_offset: [f64; 2],
    pub ignored: Vec<String>,
}

impl Default for MaterialExtras {
    fn default() -> Self {
        Self {
            normal: None,
            metallic: None,
            roughness: None,
            metal_rough: None,
            specular: None,
            shininess: None,
            emissive: None,
            emissive_texture: None,
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            ignored: Vec::new(),
        }
    }
}

impl MaterialExtras {
    pub fn has_uv_transform(&self) -> bool {
        self.uv_scale.iter().any(|s| (s - 1.0).abs() > 1e-6) || self.uv_offset.iter().any(|o| o.abs() > 1e-6)
    }

    pub fn ignore(&mut self, id: &str) {
        if !self.ignored.iter().any(|i| i == id) {
            self.ignored.push(id.to_owned());
        }
    }

    pub fn set_flipped_v_transform(&mut self, scale: [f64; 2], offset: [f64; 2]) {
        self.uv_scale = scale;
        self.uv_offset = [offset[0], 1.0 - scale[1] - offset[1]];
    }
}

impl crate::Model {
    pub fn bake_uv_transforms(&mut self) {
        for t in &mut self.triangles {
            let Some(m) = self.materials.get(t.material) else { continue };
            let e = &m.extras;
            if !e.has_uv_transform() {
                continue;
            }
            for uv in &mut t.uv {
                uv.x = uv.x * e.uv_scale[0] + e.uv_offset[0];
                uv.y = uv.y * e.uv_scale[1] + e.uv_offset[1];
            }
        }
        for m in &mut self.materials {
            m.extras.uv_scale = [1.0, 1.0];
            m.extras.uv_offset = [0.0, 0.0];
        }
    }
}
