use hp_vmf::Id;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssetKind {
    Material,
    Decal,
    Sprite,
    Sound,
    Particle,
}

impl AssetKind {
    pub fn is_material(self) -> bool {
        matches!(self, AssetKind::Material | AssetKind::Decal | AssetKind::Sprite)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssetPick {
    pub kind: AssetKind,
    pub entities: Vec<Id>,
    pub world: bool,
    pub key: String,
    pub label: String,
    pub current: String,
    pub result: Option<String>,
}

impl AssetPick {
    pub fn pending(pick: &Option<AssetPick>, kinds: &[AssetKind]) -> bool {
        pick.as_ref().is_some_and(|p| p.result.is_none() && kinds.contains(&p.kind))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyvalueClip {
    pub class: String,
    pub keys: Vec<(String, String)>,
    pub targetname: Option<String>,
}

#[derive(Debug, Default)]
pub struct EntitiesRuntime {
    pub asset_pick: Option<AssetPick>,
    pub clipboard: Option<KeyvalueClip>,
    pub show_find_replace: bool,
    pub help_class: Option<String>,
    pub edit_color: bool,
    pub display_color_window: bool,
    pub show_soundscapes: bool,
    pub sound_window: bool,
    pub particle_window: bool,
    pub set_max_distance: bool,
}
