pub type PreviewTri = ([[f32; 3]; 3], [f32; 4]);
pub type PreviewLine = ([f32; 3], [f32; 3], [f32; 4]);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoadPreview {
    pub tris: Vec<PreviewTri>,
    pub lines: Vec<PreviewLine>,
    pub faces: usize,
    pub vertices: usize,
    pub truncated: bool,
}

impl RoadPreview {
    pub fn is_empty(&self) -> bool {
        self.tris.is_empty() && self.lines.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct RoadsideRuntime {
    pub preview: RoadPreview,
    pub revision: u64,
}

impl RoadsideRuntime {
    pub fn set_preview(&mut self, preview: RoadPreview) {
        if self.preview != preview {
            self.preview = preview;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub fn clear_preview(&mut self) {
        self.set_preview(RoadPreview::default());
    }
}
