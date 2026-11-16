pub const NO_GPU_MESSAGE: &str = "No GPU detected, compiling with Ray Tracing will take longer.";
pub const NO_GPU_MESSAGE_FR: &str = "Aucun GPU détecté, la compilation avec Ray Tracing prendra plus de temps.";
pub const NO_GPU_MESSAGE_RU: &str = "GPU не обнаружен, компиляция с Ray Tracing займёт больше времени.";

pub fn no_gpu_message(lang: &str) -> &'static str {
    let lang = lang.trim().to_ascii_lowercase();
    if lang.starts_with("fr") {
        NO_GPU_MESSAGE_FR
    } else if lang.starts_with("ru") {
        NO_GPU_MESSAGE_RU
    } else {
        NO_GPU_MESSAGE
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DevicePreference {
    #[default]
    Auto,
    Gpu,
    Cpu,
}

impl DevicePreference {
    pub fn name(self) -> &'static str {
        match self {
            DevicePreference::Auto => "auto",
            DevicePreference::Gpu => "gpu",
            DevicePreference::Cpu => "cpu",
        }
    }

    pub fn parse(text: &str) -> Option<DevicePreference> {
        match text.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(DevicePreference::Auto),
            "gpu" => Some(DevicePreference::Gpu),
            "cpu" => Some(DevicePreference::Cpu),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuInfo {
    pub name: String,
    pub backend: String,
    pub hardware_ray_query: bool,
    pub vram_budget: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceInfo {
    pub gpu: Option<GpuInfo>,
    pub threads: usize,
    pub no_gpu: bool,
    pub note: Option<String>,
}

impl DeviceInfo {
    pub fn cpu(threads: usize, no_gpu: bool, note: Option<String>) -> DeviceInfo {
        DeviceInfo { gpu: None, threads, no_gpu, note }
    }

    pub fn describe(&self) -> String {
        match &self.gpu {
            Some(g) => format!(
                "GPU {} ({}, {}, {} MB budget)",
                g.name,
                g.backend,
                if g.hardware_ray_query { "hardware ray query" } else { "compute BVH traversal" },
                g.vram_budget >> 20
            ),
            None => format!("CPU, {} threads", self.threads),
        }
    }
}

pub fn env_budget() -> Option<u64> {
    let mb: u64 = std::env::var("HAMMERPRO_VRAM_BUDGET_MB").ok()?.trim().parse().ok()?;
    (mb > 0).then(|| mb.clamp(64, 1 << 20) << 20)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_languages() {
        assert_eq!(no_gpu_message("en"), NO_GPU_MESSAGE);
        assert_eq!(no_gpu_message("fr-FR"), NO_GPU_MESSAGE_FR);
        assert_eq!(no_gpu_message("ru"), NO_GPU_MESSAGE_RU);
        assert_eq!(no_gpu_message(""), NO_GPU_MESSAGE);
    }
}
