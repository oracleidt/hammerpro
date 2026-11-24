use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AdapterKind {
    Dedicated,
    Integrated,
    Virtual,
    Software,
    #[default]
    Other,
}

impl AdapterKind {
    pub const fn id(self) -> &'static str {
        match self {
            AdapterKind::Dedicated => "dedicated",
            AdapterKind::Integrated => "integrated",
            AdapterKind::Virtual => "virtual",
            AdapterKind::Software => "software",
            AdapterKind::Other => "other",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdapterEntry {
    pub name: String,
    pub kind: AdapterKind,
    pub backend: String,
    pub driver: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdapterNotice {
    Software,
    Integrated { dedicated: String },
}

impl AdapterNotice {
    pub const fn id(&self) -> &'static str {
        match self {
            AdapterNotice::Software => "software",
            AdapterNotice::Integrated { .. } => "integrated_while_dedicated_exists",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdapterChoice {
    pub chosen: AdapterEntry,
    pub notice: Option<AdapterNotice>,
    pub all: Vec<AdapterEntry>,
    pub wanted: Option<String>,
    pub wanted_found: bool,
    pub fallback_from: Option<String>,
}

impl AdapterChoice {
    pub fn notice_key(&self) -> Option<String> {
        self.notice.as_ref().map(|n| format!("{}:{}", n.id(), self.chosen.name))
    }

    pub fn names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for a in &self.all {
            if !out.iter().any(|n| n.eq_ignore_ascii_case(&a.name)) {
                out.push(a.name.clone());
            }
        }
        out
    }
}

static CHOICE: Mutex<Option<AdapterChoice>> = Mutex::new(None);

pub fn set_choice(choice: AdapterChoice) {
    let mut slot = CHOICE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    *slot = Some(choice);
}

pub fn choice() -> Option<AdapterChoice> {
    CHOICE.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
}

const SOFTWARE_NAMES: &[&str] = &["microsoft basic render", "warp", "llvmpipe", "lavapipe", "swiftshader", "softpipe"];

pub fn is_software_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SOFTWARE_NAMES.iter().any(|s| lower.contains(s))
}

pub fn kind_of(name: &str, kind: AdapterKind) -> AdapterKind {
    if is_software_name(name) { AdapterKind::Software } else { kind }
}

pub fn notice_for(chosen: &AdapterEntry, all: &[AdapterEntry], named_by_user: bool) -> Option<AdapterNotice> {
    if chosen.kind == AdapterKind::Software {
        return Some(AdapterNotice::Software);
    }
    if named_by_user || chosen.kind != AdapterKind::Integrated {
        return None;
    }
    all.iter()
        .find(|a| a.kind == AdapterKind::Dedicated)
        .map(|a| AdapterNotice::Integrated { dedicated: a.name.clone() })
}

pub fn name_matches(wanted: &str, name: &str, kind: AdapterKind) -> bool {
    let w = wanted.trim().to_ascii_lowercase();
    if w.is_empty() {
        return false;
    }
    match w.as_str() {
        "integrated" => kind == AdapterKind::Integrated,
        "dedicated" | "discrete" => kind == AdapterKind::Dedicated,
        "cpu" | "software" => kind == AdapterKind::Software,
        _ => name.to_ascii_lowercase().contains(&w),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, kind: AdapterKind) -> AdapterEntry {
        AdapterEntry {
            name: name.to_owned(),
            kind: kind_of(name, kind),
            backend: "Vulkan".to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn software_adapters_are_recognised_by_name() {
        assert!(is_software_name("Microsoft Basic Render Driver"));
        assert!(is_software_name("llvmpipe (LLVM 15.0.7, 256 bits)"));
        assert!(!is_software_name("NVIDIA GeForce RTX 3070"));
        assert_eq!(kind_of("Microsoft Basic Render Driver", AdapterKind::Other), AdapterKind::Software);
        assert_eq!(kind_of("AMD Radeon RX 6600", AdapterKind::Dedicated), AdapterKind::Dedicated);
    }

    #[test]
    fn notices_only_when_a_better_card_is_left_unused() {
        let nvidia = entry("NVIDIA GeForce RTX 3060 Laptop GPU", AdapterKind::Dedicated);
        let intel = entry("Intel(R) UHD Graphics", AdapterKind::Integrated);
        let warp = entry("Microsoft Basic Render Driver", AdapterKind::Other);
        let all = vec![nvidia.clone(), intel.clone(), warp.clone()];
        assert_eq!(notice_for(&nvidia, &all, false), None);
        assert_eq!(notice_for(&intel, &all, false), Some(AdapterNotice::Integrated { dedicated: nvidia.name.clone() }));
        assert_eq!(notice_for(&intel, &all, true), None, "a card chosen in the settings is not questioned");
        assert_eq!(notice_for(&intel, std::slice::from_ref(&intel), false), None, "integrated alone is fine");
        assert_eq!(notice_for(&warp, &all, true), Some(AdapterNotice::Software));
        let choice =
            AdapterChoice { chosen: intel.clone(), notice: notice_for(&intel, &all, false), all, ..Default::default() };
        assert_eq!(choice.notice_key().as_deref(), Some("integrated_while_dedicated_exists:Intel(R) UHD Graphics"));
        assert_eq!(choice.names().len(), 3);
    }

    #[test]
    fn wanted_adapter_matches_names_and_kinds() {
        assert!(name_matches("rtx", "NVIDIA GeForce RTX 3070", AdapterKind::Dedicated));
        assert!(name_matches("integrated", "Intel(R) UHD Graphics", AdapterKind::Integrated));
        assert!(name_matches("dedicated", "AMD Radeon RX 6600", AdapterKind::Dedicated));
        assert!(!name_matches("", "AMD Radeon RX 6600", AdapterKind::Dedicated));
        assert!(!name_matches("intel", "AMD Radeon RX 6600", AdapterKind::Dedicated));
    }
}
