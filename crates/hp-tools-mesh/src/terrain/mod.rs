pub mod export;
pub mod generate;
pub mod image_io;
pub mod import;
pub mod masks;
pub mod masks_ui;
pub mod tr;
pub mod ui;

use glam::{DVec2, DVec3};
use hp_app::state::AppState;
use hp_editor::terrain_ops::{MAX_TERRAIN_TILES, TerrainLayout, TerrainSurface};
use hp_editor::{EditError, FaceRef};
use hp_geom::{AutoTexture, HeightField, auto_weights, slope_degrees};

use crate::common::edit_error_text;
use crate::disp::seams::{UNDO_SEW, plan_sew};

pub const UNDO_IMPORT: &str = "Import heightmap";
pub const UNDO_GENERATE: &str = "Generate terrain";
pub const MAP_LIMIT: f64 = 16384.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexMode {
    None,
    Auto,
    Splat,
}

impl TexMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Auto => "auto",
            Self::Splat => "splat",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::None, Self::Auto, Self::Splat].into_iter().find(|m| m.name() == name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Texturing {
    pub mode: TexMode,
    pub auto: AutoTexture,
    pub splat: String,
    pub channels: [String; 4],
}

impl Default for Texturing {
    fn default() -> Self {
        Self { mode: TexMode::None, auto: AutoTexture::default(), splat: String::new(), channels: Default::default() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TileSetup {
    pub size: DVec2,
    pub origin: DVec3,
    pub centered: bool,
    pub tile_power: u32,
    pub tile_size: f64,
    pub h_min: f64,
    pub h_max: f64,
    pub skirt: f64,
    pub thickness: f64,
    pub material: String,
    pub lightmap_scale: i32,
}

impl Default for TileSetup {
    fn default() -> Self {
        Self {
            size: DVec2::new(4096.0, 4096.0),
            origin: DVec3::ZERO,
            centered: true,
            tile_power: 3,
            tile_size: 512.0,
            h_min: 0.0,
            h_max: 512.0,
            skirt: 0.0,
            thickness: 16.0,
            material: "DEV/DEV_BLENDMEASURE".to_owned(),
            lightmap_scale: 16,
        }
    }
}

impl TileSetup {
    pub fn tiles(&self) -> (u32, u32) {
        let count = |size: f64| {
            let n = (size / self.tile_size.max(1.0)).round();
            if n.is_finite() { n.clamp(1.0, 4096.0) as u32 } else { 1 }
        };
        (count(self.size.x), count(self.size.y))
    }

    pub fn min(&self) -> DVec2 {
        let base = DVec2::new(self.origin.x, self.origin.y);
        if self.centered { base - self.size * 0.5 } else { base }
    }

    pub fn cells(&self) -> (usize, usize) {
        let (tx, ty) = self.tiles();
        let per = 1usize << self.tile_power.clamp(2, 4);
        (tx as usize * per + 1, ty as usize * per + 1)
    }

    pub fn spacing(&self) -> DVec2 {
        let (cx, cy) = self.cells();
        DVec2::new(self.size.x / (cx - 1) as f64, self.size.y / (cy - 1) as f64)
    }

    pub fn plane_z(&self) -> f64 {
        self.origin.z + self.h_min
    }

    pub fn validate(&self) -> Result<(), String> {
        let ok = |v: f64| v.is_finite();
        if !(ok(self.size.x)
            && ok(self.size.y)
            && self.size.min_element() >= 16.0
            && self.size.max_element() <= 32768.0)
        {
            return Err("the terrain size must be between 16 and 32768 units on each side".to_owned());
        }
        if !(2..=4).contains(&self.tile_power) {
            return Err("the tile power must be 2, 3 or 4".to_owned());
        }
        if !(ok(self.tile_size) && (16.0..=16384.0).contains(&self.tile_size)) {
            return Err("the tile size must be between 16 and 16384 units".to_owned());
        }
        if !(ok(self.h_min) && ok(self.h_max) && self.h_max >= self.h_min && self.h_max - self.h_min <= 32768.0) {
            return Err("the height range must have min at most max and span at most 32768 units".to_owned());
        }
        if !(ok(self.skirt) && (0.0..=4096.0).contains(&self.skirt)) {
            return Err("the skirt must be between 0 and 4096 units".to_owned());
        }
        if !(ok(self.thickness) && (1.0..=4096.0).contains(&self.thickness)) {
            return Err("the thickness must be between 1 and 4096 units".to_owned());
        }
        if !self.origin.is_finite() || self.origin.z.abs() > MAP_LIMIT {
            return Err("the origin is not a valid position".to_owned());
        }
        let min = self.min();
        let max = min + self.size;
        if min.abs().max_element() > MAP_LIMIT || max.abs().max_element() > MAP_LIMIT {
            return Err(format!("the terrain must stay within +-{MAP_LIMIT} units of the map center"));
        }
        let (tx, ty) = self.tiles();
        if (tx as usize) * (ty as usize) > MAX_TERRAIN_TILES {
            return Err(format!(
                "{} tiles is too many, the limit is {MAX_TERRAIN_TILES}",
                (tx as usize) * (ty as usize)
            ));
        }
        if self.material.trim().is_empty() {
            return Err("the material is empty".to_owned());
        }
        if !(1..=1024).contains(&self.lightmap_scale) {
            return Err("the lightmap scale must be between 1 and 1024".to_owned());
        }
        Ok(())
    }

    pub fn layout(&self) -> TerrainLayout {
        TerrainLayout {
            min: self.min(),
            size: self.size,
            tiles: self.tiles(),
            power: self.tile_power,
            plane_z: self.plane_z(),
            thickness: self.thickness,
            material: self.material.trim().to_owned(),
            lightmap_scale: self.lightmap_scale,
        }
    }
}

pub enum Source {
    None,
    Auto(AutoTexture),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PixelGrid {
    #[default]
    Corner,
    Center,
}

impl PixelGrid {
    pub const NAMES: [&'static str; 2] = ["corner", "center"];

    pub fn name(self) -> &'static str {
        match self {
            Self::Corner => "corner",
            Self::Center => "center",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Corner, Self::Center].into_iter().find(|g| g.name() == name)
    }
}

pub fn sample_field(field: &HeightField, u: f64, v: f64, grid: PixelGrid, bilinear: bool) -> f64 {
    let (w, h) = (field.width as f64, field.height as f64);
    let (x, y) = match grid {
        PixelGrid::Corner => (u * (w - 1.0), v * (h - 1.0)),
        PixelGrid::Center => (u * w - 0.5, v * h - 0.5),
    };
    if bilinear { field.sample_linear(x, y) } else { field.sample(x, y) }
}

pub struct Terrain {
    pub field: HeightField,
    pub min: DVec2,
    pub size: DVec2,
    pub h_min: f64,
    pub h_max: f64,
    pub skirt: f64,
    pub spacing: f64,
    pub bilinear: bool,
    pub grid: PixelGrid,
    pub source: Source,
}

impl Terrain {
    fn uv(&self, x: f64, y: f64) -> (f64, f64) {
        (((x - self.min.x) / self.size.x).clamp(0.0, 1.0), ((y - self.min.y) / self.size.y).clamp(0.0, 1.0))
    }

    pub fn norm(&self, x: f64, y: f64) -> f64 {
        let (u, v) = self.uv(x, y);
        sample_field(&self.field, u, v, self.grid, self.bilinear)
    }

    pub fn world(&self, x: f64, y: f64) -> f64 {
        self.h_min + (self.h_max - self.h_min) * self.norm(x, y)
    }

    fn on_edge(&self, x: f64, y: f64) -> bool {
        let eps = 1e-4;
        (x - self.min.x).abs() < eps
            || (x - self.min.x - self.size.x).abs() < eps
            || (y - self.min.y).abs() < eps
            || (y - self.min.y - self.size.y).abs() < eps
    }

    pub fn height(&self, x: f64, y: f64) -> f64 {
        let h = (self.h_max - self.h_min) * self.norm(x, y);
        if self.skirt > 0.0 && self.on_edge(x, y) { h - self.skirt } else { h }
    }

    pub fn has_weights(&self) -> bool {
        !matches!(self.source, Source::None)
    }

    pub fn weights(&self, x: f64, y: f64) -> [f64; 4] {
        match &self.source {
            Source::None => [1.0, 0.0, 0.0, 0.0],
            Source::Auto(tex) => {
                let d = self.spacing.max(1e-6);
                let dx = (self.world(x + d, y) - self.world(x - d, y)) / (2.0 * d);
                let dy = (self.world(x, y + d) - self.world(x, y - d)) / (2.0 * d);
                auto_weights(slope_degrees(dx, dy), self.norm(x, y).clamp(0.0, 1.0), tex)
            }
        }
    }

    fn run<R>(&self, f: impl FnOnce(&TerrainSurface<'_>) -> R) -> R {
        let height = |x: f64, y: f64| self.height(x, y);
        let weights = |x: f64, y: f64| self.weights(x, y);
        let surface = TerrainSurface { height: &height, weights: self.has_weights().then_some(&weights) };
        f(&surface)
    }
}

fn edit_failed(e: &EditError) -> String {
    edit_error_text(e)
}

pub fn create(state: &mut AppState, setup: &TileSetup, terrain: &Terrain, label: &str) -> Result<usize, String> {
    setup.validate()?;
    let layout = setup.layout();
    terrain
        .run(|surface| state.doc.create_terrain(&layout, surface, label))
        .map(|ids| ids.len())
        .map_err(|e| edit_failed(&e))
}

pub fn replace(state: &mut AppState, terrain: &Terrain, faces: &[FaceRef], label: &str) -> Result<usize, String> {
    let result = state.doc.transaction(label, |doc| {
        let n = terrain.run(|surface| doc.set_terrain_surface(faces, surface, label))?;
        if n > 0 {
            let plan = plan_sew(doc, faces, &[]);
            doc.set_disp_data(UNDO_SEW, plan.updates)?;
        }
        Ok::<usize, EditError>(n)
    });
    result.map_err(|e| edit_failed(&e))
}

pub fn orient(mut field: HeightField, flip_x: bool, flip_y: bool) -> HeightField {
    field.flip_y();
    if flip_x {
        field.flip_x();
    }
    if flip_y {
        field.flip_y();
    }
    field
}

pub fn resolve_path(state: &AppState, text: &str) -> Result<std::path::PathBuf, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("no file given".to_owned());
    }
    let path = std::path::PathBuf::from(text);
    if path.is_absolute() {
        return Ok(path);
    }
    let base = state
        .doc
        .path()
        .or_else(|| state.doc.opened_from())
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .ok_or_else(|| format!("\"{text}\" is relative and the map is not saved: give a full path"))?;
    Ok(base.join(path))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

    use super::*;

    #[test]
    fn setup_counts_tiles_and_vertices() {
        let s = TileSetup { size: DVec2::new(2048.0, 1024.0), tile_size: 512.0, tile_power: 3, ..TileSetup::default() };
        assert_eq!(s.tiles(), (4, 2));
        assert_eq!(s.cells(), (33, 17));
        assert_eq!(s.spacing(), DVec2::new(64.0, 64.0));
        assert_eq!(s.min(), DVec2::new(-1024.0, -512.0));
        assert!(s.validate().is_ok());
        let far = TileSetup { origin: DVec3::new(20000.0, 0.0, 0.0), centered: false, ..s.clone() };
        assert!(far.validate().is_err());
        let many = TileSetup { size: DVec2::new(32768.0, 32768.0), tile_size: 256.0, ..TileSetup::default() };
        assert!(many.validate().is_err());
        assert!(TileSetup { tile_power: 5, ..s }.validate().is_err());
    }

    #[test]
    fn pixel_grids_put_samples_on_corners_or_centers() {
        let f = HeightField::new(4, 1, vec![0.0, 1.0, 2.0, 3.0]).unwrap();
        assert_eq!(sample_field(&f, 0.0, 0.0, PixelGrid::Corner, true), 0.0);
        assert_eq!(sample_field(&f, 1.0, 0.0, PixelGrid::Corner, false), 3.0);
        assert_eq!(sample_field(&f, 0.125, 0.0, PixelGrid::Center, true), 0.0);
        assert_eq!(sample_field(&f, 0.375, 0.0, PixelGrid::Center, false), 1.0);
        assert_eq!(sample_field(&f, 0.5, 0.0, PixelGrid::Center, true), 1.5);
        assert_eq!(PixelGrid::from_name("center"), Some(PixelGrid::Center));
    }
}
