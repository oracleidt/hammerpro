use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VtfRequest {
    pub texture: Option<String>,
    pub file: Option<PathBuf>,
    pub external: Option<bool>,
    pub materials: Vec<String>,
    pub param: Option<String>,
}

#[derive(Debug, Default)]
pub struct VtfRuntime {
    pub show: bool,
    pub request: Option<VtfRequest>,
}

impl VtfRuntime {
    pub fn open(&mut self, request: VtfRequest) {
        self.request = Some(request);
    }

    pub fn open_window(&mut self) {
        self.show = true;
    }
}
