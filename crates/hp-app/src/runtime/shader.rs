use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShaderRequest {
    pub path: Option<PathBuf>,
    pub source: Option<String>,
    pub preset: Option<String>,
    pub graph_json: Option<String>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveGraph {
    pub json: String,
    pub path: Option<PathBuf>,
    pub dirty: bool,
}

#[derive(Debug, Default)]
pub struct ShaderRuntime {
    pub show: bool,
    pub request: Option<ShaderRequest>,
    pub live: Option<LiveGraph>,
}

impl ShaderRuntime {
    pub fn open(&mut self, request: ShaderRequest) {
        self.request = Some(request);
        self.show = true;
    }
}
