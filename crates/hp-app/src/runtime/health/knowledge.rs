use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use hp_editor::limits::checks::{AssetKey, CheckKnowledge, MaterialFacts, ToolUse};
use hp_editor::limits::snapshot::MapSnapshot;
use hp_fgd::{PropertyType, ResolvedFgd};
use hp_fs::GameFs;
use hp_fs::deps::{DepKind, DepRoot, DepsCache, ResolveOptions, RootKind, sound_path};
use parking_lot::Mutex;

use crate::state::MaterialInfo;

#[derive(Clone, Debug, Default)]
pub(crate) struct ClassFacts {
    pub solid: bool,
    pub keys: HashMap<String, bool>,
    pub assets: HashMap<String, AssetKey>,
    pub inputs: HashSet<String>,
}

pub(crate) fn class_facts(fgd: &ResolvedFgd, class: &str) -> Option<ClassFacts> {
    let c = fgd.class(class)?;
    let mut keys = HashMap::new();
    let mut assets = HashMap::new();
    for p in &c.props {
        let name = p.name.to_ascii_lowercase();
        keys.insert(name.clone(), matches!(p.kind, PropertyType::TargetDestination | PropertyType::TargetNameOrClass));
        let kind = match p.kind {
            PropertyType::Studio => Some(AssetKey::Model),
            PropertyType::Sound => Some(AssetKey::Sound),
            PropertyType::ParticleSystem => Some(AssetKey::Particle),
            PropertyType::Material | PropertyType::Sprite | PropertyType::Decal => Some(AssetKey::Material),
            _ => None,
        };
        if let Some(k) = kind {
            assets.insert(name, k);
        }
    }
    let inputs = c.inputs.iter().map(|i| i.name.to_ascii_lowercase()).collect();
    Some(ClassFacts { solid: c.is_solid(), keys, assets, inputs })
}

pub(crate) fn material_facts(info: &MaterialInfo) -> MaterialFacts {
    use hp_vmt::ToolKind;
    let tool = info.tool.map(|t| match t {
        ToolKind::Nodraw => ToolUse::Nodraw,
        ToolKind::Clip | ToolKind::PlayerClip | ToolKind::NpcClip => ToolUse::Clip,
        ToolKind::Trigger => ToolUse::Trigger,
        ToolKind::Skybox | ToolKind::Skybox2d => ToolUse::Sky,
        ToolKind::Hint => ToolUse::Hint,
        ToolKind::Skip => ToolUse::Skip,
        ToolKind::AreaPortal => ToolUse::AreaPortal,
        ToolKind::Occluder => ToolUse::Occluder,
        ToolKind::Origin => ToolUse::Origin,
        ToolKind::Invisible | ToolKind::BlockLight | ToolKind::BlockBullets | ToolKind::Fog | ToolKind::Ladder => {
            ToolUse::NonSolid
        }
        ToolKind::Other => MaterialFacts::guess(&info.name).tool.unwrap_or(ToolUse::NonSolid),
    });
    MaterialFacts {
        missing: info.missing,
        tool,
        water: info.is_water && tool.is_none(),
        translucent: (info.translucent || info.non_solid) && tool.is_none(),
    }
}

#[derive(Debug, Default)]
pub(crate) struct AssetAnswers {
    pub generation: u64,
    pub sounds: HashMap<String, bool>,
    pub particles: HashMap<String, bool>,
}

pub(crate) struct Knowledge {
    pub classes: Option<Arc<HashMap<String, Option<ClassFacts>>>>,
    pub materials: Arc<HashMap<String, MaterialFacts>>,
    pub fs: Option<GameFs>,
    pub answers: Arc<Mutex<AssetAnswers>>,
    pub deps_cache: Arc<DepsCache>,
    pub instance_names: Arc<Vec<String>>,
}

fn normalize(name: &str) -> String {
    let mut s = name.trim().replace('\\', "/").to_ascii_lowercase();
    if let Some(rest) = s.strip_prefix("materials/") {
        s = rest.to_owned();
    }
    if let Some(rest) = s.strip_suffix(".vmt") {
        s = rest.to_owned();
    }
    s
}

impl Knowledge {
    fn class(&self, class: &str) -> Option<&ClassFacts> {
        self.classes.as_ref()?.get(&class.trim().to_ascii_lowercase())?.as_ref()
    }

    pub(crate) fn resolve_assets(&self, snap: &MapSnapshot, cancel: &std::sync::atomic::AtomicBool) {
        let Some(fs) = &self.fs else { return };
        let mut sounds: HashSet<String> = HashSet::new();
        let mut particles: HashSet<String> = HashSet::new();
        for r in &snap.entities {
            let e = &r.entity;
            for (key, v) in &e.props {
                let v = v.trim();
                if v.is_empty() || v.len() > 260 {
                    continue;
                }
                match self.asset_key(&e.classname, key) {
                    Some(AssetKey::Sound) => {
                        sounds.insert(v.to_owned());
                    }
                    Some(AssetKey::Particle) => {
                        particles.insert(v.to_owned());
                    }
                    _ => {}
                }
            }
        }
        let todo: Vec<DepRoot> = {
            let mut a = self.answers.lock();
            if a.generation != fs.generation() {
                *a = AssetAnswers { generation: fs.generation(), ..AssetAnswers::default() };
            }
            let s = sounds.iter().filter(|n| !a.sounds.contains_key(*n)).map(|n| (RootKind::Sound, n.clone()));
            let p = particles.iter().filter(|n| !a.particles.contains_key(*n)).map(|n| (RootKind::Particle, n.clone()));
            s.chain(p).map(|(kind, name)| DepRoot { kind, name, why: String::new() }).collect()
        };
        if todo.is_empty() {
            return;
        }
        let graph = fs.resolve_dependencies(
            &todo,
            &ResolveOptions { map_name: "", cancel: Some(cancel), ..ResolveOptions::default() },
            &self.deps_cache,
        );
        if graph.cancelled {
            return;
        }
        let missing: HashSet<(DepKind, String)> =
            graph.missing.iter().map(|m| (m.kind, m.name.to_ascii_lowercase())).collect();
        let mut a = self.answers.lock();
        for root in todo {
            let name = root.name.trim().to_ascii_lowercase();
            match root.kind {
                RootKind::Sound => {
                    let file = sound_path(&root.name).map(|p| p.to_ascii_lowercase());
                    let gone = missing.contains(&(DepKind::Sound, name.clone()))
                        || file.is_some_and(|f| missing.contains(&(DepKind::Sound, f)));
                    a.sounds.insert(root.name, !gone);
                }
                RootKind::Particle => {
                    let gone = missing.contains(&(DepKind::Particle, name));
                    a.particles.insert(root.name, !gone);
                }
                _ => {}
            }
        }
    }
}

impl CheckKnowledge for Knowledge {
    fn class_exists(&self, class: &str) -> Option<bool> {
        let classes = self.classes.as_ref()?;
        Some(classes.get(&class.trim().to_ascii_lowercase()).is_some_and(Option::is_some))
    }

    fn is_brush_class(&self, class: &str) -> Option<bool> {
        self.class(class).map(|c| c.solid)
    }

    fn is_target_key(&self, class: &str, key: &str) -> Option<bool> {
        self.class(class)?.keys.get(&key.to_ascii_lowercase()).copied()
    }

    fn class_has_input(&self, class: &str, input: &str) -> Option<bool> {
        let c = self.class(class)?;
        (!c.inputs.is_empty()).then(|| c.inputs.contains(&input.trim().to_ascii_lowercase()))
    }

    fn asset_key(&self, class: &str, key: &str) -> Option<AssetKey> {
        self.class(class)?.assets.get(&key.to_ascii_lowercase()).copied()
    }

    fn material(&self, name: &str) -> Option<MaterialFacts> {
        self.materials.get(&normalize(name)).copied()
    }

    fn model_exists(&self, path: &str) -> Option<bool> {
        Some(self.fs.as_ref()?.exists(&path.trim().replace('\\', "/").to_ascii_lowercase()))
    }

    fn sound_exists(&self, name: &str) -> Option<bool> {
        self.fs.as_ref()?;
        self.answers.lock().sounds.get(name.trim()).copied()
    }

    fn particle_exists(&self, name: &str) -> Option<bool> {
        self.fs.as_ref()?;
        self.answers.lock().particles.get(name.trim()).copied()
    }

    fn extra_target_names(&self) -> &[String] {
        &self.instance_names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_facts_from_infos() {
        let info = |tool: Option<hp_vmt::ToolKind>| MaterialInfo { name: "x".into(), tool, ..MaterialInfo::default() };
        assert_eq!(material_facts(&info(Some(hp_vmt::ToolKind::Nodraw))).tool, Some(ToolUse::Nodraw));
        assert!(material_facts(&info(Some(hp_vmt::ToolKind::Skybox2d))).sky());
        assert!(!material_facts(&info(Some(hp_vmt::ToolKind::PlayerClip))).seals());
        let glass = MaterialInfo { translucent: true, ..MaterialInfo::default() };
        assert!(!material_facts(&glass).seals());
        let grate = MaterialInfo { alpha_test: true, non_solid: true, ..MaterialInfo::default() };
        assert!(!material_facts(&grate).seals(), "alpha tested brushes do not seal, like in the compiler");
        let water = MaterialInfo { is_water: true, ..MaterialInfo::default() };
        assert!(material_facts(&water).water);
        assert_eq!(normalize("Materials\\Brick/Wall.VMT"), "brick/wall");
    }

    #[test]
    fn class_facts_from_the_fgd() {
        let src = r#"
@BaseClass = Targetname [ targetname(target_source) : "Name" input Kill(void) : "" ]
@PointClass base(Targetname) = ambient_generic : "Sound" [ message(sound) : "Sound" target(target_destination) : "T" ]
@SolidClass base(Targetname) = func_door : "Door" [ input Open(void) : "" ]
"#;
        let fgd = hp_fgd::Fgd::parse(src, None).resolve();
        let amb = class_facts(&fgd, "AMBIENT_GENERIC").expect("class");
        assert!(!amb.solid);
        assert_eq!(amb.keys.get("target"), Some(&true));
        assert_eq!(amb.keys.get("message"), Some(&false));
        assert_eq!(amb.assets.get("message"), Some(&AssetKey::Sound));
        let door = class_facts(&fgd, "func_door").expect("door");
        assert!(door.solid);
        assert!(door.inputs.contains("open") && door.inputs.contains("kill"));
        assert!(class_facts(&fgd, "nothing").is_none());
        let mut classes = HashMap::new();
        classes.insert("func_door".to_owned(), Some(door));
        classes.insert("weird".to_owned(), None);
        let k = Knowledge {
            classes: Some(Arc::new(classes)),
            materials: Arc::new(HashMap::new()),
            fs: None,
            answers: Arc::default(),
            deps_cache: Arc::new(DepsCache::new()),
            instance_names: Arc::new(vec!["a-door".to_owned()]),
        };
        assert_eq!(k.extra_target_names(), ["a-door".to_owned()]);
        assert_eq!(k.class_exists("Func_Door"), Some(true));
        assert_eq!(k.class_exists("weird"), Some(false));
        assert_eq!(k.class_has_input("func_door", "Open"), Some(true));
        assert_eq!(k.class_has_input("func_door", "Explode"), Some(false));
        assert_eq!(k.model_exists("models/x.mdl"), None);
        assert_eq!(k.sound_exists("x.wav"), None);
    }
}
