#[derive(Clone, Debug, PartialEq)]
pub struct CompileProgress {
    pub step: String,
    pub fraction: f32,
    pub elapsed_s: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileBadge {
    Leak,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompileBrowse {
    GameExe,
    MapsDir,
    ModelCompiler,
}

#[derive(Debug, Default)]
pub struct CompileRuntime {
    pub progress: Option<CompileProgress>,
    pub background: Option<CompileProgress>,
    pub badge: Option<CompileBadge>,
    pub auto_label: Option<String>,
    pub show_history: bool,
    pub show_batch: bool,
    pub browse_request: Option<CompileBrowse>,
}
