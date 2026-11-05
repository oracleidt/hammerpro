use std::sync::Arc;

#[derive(Debug, Default)]
pub struct SoundColorRuntime {
    pub sound_open: bool,
    pub color_open: bool,
    pub preview_lut: Option<Arc<Vec<u8>>>,
    pub preview_label: String,
    pub lut_serial: u64,
    pub zones: Vec<hp_colorgrade::ZoneReport>,
    pub zone_applied: bool,
    pub eye: [f64; 3],
    pub open_sound: Option<String>,
    pub open_lut: Option<String>,
}

impl SoundColorRuntime {
    pub fn set_preview(&mut self, rgba: Option<Vec<u8>>, label: &str) {
        self.preview_lut = rgba.filter(|d| d.len() == hp_colorgrade::RGBA_BYTES).map(Arc::new);
        label.clone_into(&mut self.preview_label);
        if self.preview_lut.is_none() {
            self.preview_label.clear();
        }
    }
}
