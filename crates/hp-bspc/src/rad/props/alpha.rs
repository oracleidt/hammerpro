use std::collections::HashMap;

use super::model::Studio;

#[derive(Clone, Debug)]
struct AlphaTexture {
    width: usize,
    height: usize,
    alpha: Vec<u8>,
}

#[derive(Default)]
pub(crate) struct Textures {
    list: Vec<AlphaTexture>,
    by_name: HashMap<String, usize>,
}

impl Textures {
    fn find_or_load(
        &mut self,
        vmt_path: &str,
        read: &dyn Fn(&str) -> Option<Vec<u8>>,
        log: &mut dyn FnMut(&str),
    ) -> (bool, Option<usize>) {
        let key = vmt_path.to_ascii_lowercase();
        if let Some(i) = self.by_name.get(&key) {
            return (true, Some(*i));
        }
        let Some(bytes) = read(vmt_path) else { return (false, None) };
        let Ok(root) = hp_kv::parse_bytes(&bytes) else { return (false, None) };
        let Some(vmt) = root.children.first() else { return (false, None) };
        let has = |k: &str| vmt.get(k).is_some() || vmt.children.iter().any(|c| c.name.eq_ignore_ascii_case(k));
        if !(has("$translucent") || has("$alphatest")) {
            return (true, None);
        }
        let Some(base) = vmt.get("$basetexture") else { return (true, None) };
        let path = format!("materials/{}.vtf", base.replace('\\', "/"));
        let Some(vtf) = read(&path) else { return (true, None) };
        let Some(img) = hp_vtf::raw_level(&vtf, 0, 0, 0).ok().and_then(|l| l.to_rgba8().ok()) else {
            return (true, None);
        };
        log(&format!("Loaded alpha texture {path}"));
        let (width, height) = (img.width as usize, img.height as usize);
        let alpha = img.pixels.chunks_exact(4).map(|p| p[3]).collect();
        self.list.push(AlphaTexture { width, height, alpha });
        self.by_name.insert(key, self.list.len() - 1);
        (true, Some(self.list.len() - 1))
    }

    pub(crate) fn load_for_model(
        &mut self,
        s: &Studio,
        read: &dyn Fn(&str) -> Option<Vec<u8>>,
        log: &mut dyn FnMut(&str),
    ) -> Vec<Option<usize>> {
        s.textures
            .iter()
            .map(|t| {
                for cd in &s.cdtextures {
                    let path = format!("materials/{cd}{t}.vmt").replace('\\', "/");
                    let (found, index) = self.find_or_load(&path, read, log);
                    if found {
                        return index;
                    }
                }
                None
            })
            .collect()
    }

    pub(crate) fn export(&self, tex: usize) -> Option<hp_raytrace::AlphaTexture> {
        let t = self.list.get(tex)?;
        Some(hp_raytrace::AlphaTexture { width: t.width as u32, height: t.height as u32, alpha: t.alpha.clone() })
    }

    pub(crate) fn coverage_for_triangle(&self, tex: usize, uv: &[[f32; 2]; 3]) -> f32 {
        let Some(t) = self.list.get(tex) else { return 1.0 };
        let lo = |c: usize| uv[0][c].min(uv[1][c]).min(uv[2][c]).clamp(0.0, 1.0);
        let hi = |c: usize| uv[0][c].max(uv[1][c]).max(uv[2][c]).clamp(0.0, 1.0);
        let (u0, u1) = ((lo(0) * (t.width - 1) as f32) as usize, (hi(0) * (t.width - 1) as f32) as usize);
        let (v0, v1) = ((lo(1) * (t.height - 1) as f32) as usize, (hi(1) * (t.height - 1) as f32) as usize);
        let (mut total, mut count) = (0u64, 0u64);
        for v in v0..=v1 {
            for u in u0..=u1 {
                total += u64::from(t.alpha.get(v * t.width + u).copied().unwrap_or(255));
                count += 1;
            }
        }
        if count == 0 { 1.0 } else { total as f32 / (count as f32 * 255.0) }
    }

    pub(crate) fn sample(&self, tex: usize, uv: &[[f32; 2]; 3], b: [f32; 3]) -> u8 {
        let Some(t) = self.list.get(tex) else { return 0 };
        let u = b[0] * uv[0][0] + b[1] * uv[1][0] + b[2] * uv[2][0];
        let v = b[0] * uv[0][1] + b[1] * uv[1][1] + b[2] * uv[2][1];
        let ui = ((u * t.width as f32).round_ties_even() as i32 & (t.width as i32 - 1)) as usize;
        let vi = ((v * t.height as f32).round_ties_even() as i32 & (t.height as i32 - 1)) as usize;
        t.alpha.get(vi * t.width + ui).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_and_sample() {
        let mut t = Textures::default();
        t.list.push(AlphaTexture { width: 2, height: 2, alpha: vec![255, 0, 255, 0] });
        let full = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        assert!((t.coverage_for_triangle(0, &full) - 0.5).abs() < 1e-6);
        let left = [[0.0, 0.0], [0.0, 0.5], [0.0, 1.0]];
        assert_eq!(t.coverage_for_triangle(0, &left), 1.0);
        assert_eq!(t.sample(0, &full, [1.0, 0.0, 0.0]), 255);
        assert_eq!(t.sample(0, &full, [0.5, 0.5, 0.0]), 0);
        assert_eq!(t.sample(0, &full, [0.0, 1.0, 0.0]), 255);
    }
}
