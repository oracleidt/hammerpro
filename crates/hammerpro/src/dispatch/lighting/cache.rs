use std::collections::HashMap;
use std::fmt::{Debug, Write};
use std::hash::{DefaultHasher, Hash, Hasher};

use hp_app::AppState;
use hp_editor::Document;
use hp_editor::luxels::LightPlus;

use super::{Scope, uses_hdr};

struct Fingerprint(DefaultHasher);

impl Write for Fingerprint {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        self.0.write(value.as_bytes());
        Ok(())
    }
}

fn hash_debug(hash: &mut Fingerprint, value: &impl Debug) {
    write!(hash, "{value:?}").expect("fingerprint formatting");
}

const PRESENTATION: [&str; 5] = [
    hp_editor::luxels::LIGHT_PLUS,
    hp_editor::luxels::COLORCORRECTION_PLUS,
    "postprocess_plus",
    "palette_plus",
    "bgimages_plus",
];

fn entity_hash(e: &hp_vmf::Entity, visible: bool, connections: bool, skip: &dyn Fn(&str) -> bool) -> u64 {
    let mut h = DefaultHasher::new();
    (e.id, e.classname.as_str(), e.hidden, visible).hash(&mut h);
    for (k, v) in e.props.iter().filter(|(k, _)| !skip(k)) {
        (k.as_str(), v.as_str()).hash(&mut h);
    }
    if connections {
        e.connections.len().hash(&mut h);
        for c in &e.connections {
            (c.output.as_str(), c.target.as_str(), c.input.as_str(), c.param.as_str()).hash(&mut h);
            (c.delay.to_bits(), c.times, c.separator).hash(&mut h);
        }
    }
    if e.extra_blocks.is_empty() {
        return h.finish();
    }
    let mut f = Fingerprint(h);
    hash_debug(&mut f, &e.extra_blocks);
    f.0.finish()
}

#[cfg(test)]
fn scene_fingerprint(doc: &Document) -> u64 {
    BakeCache::default().scene_key(doc)
}

fn physical_options(options: LightPlus) -> LightPlus {
    LightPlus {
        brightness_scale: 1.0,
        incremental_delay: 0.0,
        bake_dist: 1024.0,
        incremental: true,
        keep_bake: true,
        debug: false,
        ..options
    }
}

fn settings_fingerprint(state: &AppState, lit: bool) -> u64 {
    let mut hash = Fingerprint(DefaultHasher::new());
    let s = state.settings.lighting.sanitized();
    let hdr = if lit {
        state.settings.render.preview_hdr(state.settings.compile.lighting)
    } else {
        uses_hdr(state.settings.compile.lighting)
    };
    hash_debug(
        &mut hash,
        &(
            lit,
            s.quality,
            s.max_luxels_millions,
            hdr,
            state.settings.compile.static_prop_shadows,
            state.settings.compile.static_prop_lighting,
            physical_options(state.doc.light_plus()),
            state.assets.lighting_generation(),
        ),
    );
    if let Some(fs) = state.assets.fs() {
        hash_debug(&mut hash, &(fs.root(), fs.game_dir(), fs.generation()));
    }
    hash.0.finish()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    content: u64,
    settings: u64,
}

#[derive(Default)]
struct Input {
    observed: Option<(Stamp, f64)>,
    resolved: Option<(Stamp, u64)>,
}

#[derive(Default)]
pub(super) struct BakeCache {
    scene: Option<(u64, u64)>,
    geometry: Option<(u64, u64)>,
    solids: HashMap<u32, (usize, usize, u64)>,
    input: [Input; 2],
}

fn mix(parts: &[u64]) -> u64 {
    let mut hash = DefaultHasher::new();
    parts.hash(&mut hash);
    hash.finish()
}

pub(super) fn geometry_entity(entity: &hp_vmf::Entity) -> bool {
    entity.get("model").is_some_and(|name| name.to_ascii_lowercase().ends_with(".mdl"))
        || entity.classname.eq_ignore_ascii_case("func_instance")
        || entity.classname.eq_ignore_ascii_case("sky_camera")
}

fn compile_settings(state: &AppState) -> u64 {
    let mut hash = Fingerprint(DefaultHasher::new());
    hash_debug(
        &mut hash,
        &(state.settings.lighting.sanitized().max_luxels_millions, state.assets.lighting_generation()),
    );
    if let Some(fs) = state.assets.fs() {
        hash_debug(&mut hash, &(fs.root(), fs.game_dir(), fs.generation()));
    }
    hash.0.finish()
}

impl BakeCache {
    pub(super) fn current_geometry_key(&self, state: &AppState) -> Option<u64> {
        let (_, geometry) = self.geometry.filter(|(serial, _)| *serial == state.doc.content_serial())?;
        Some(mix(&[geometry, compile_settings(state)]))
    }

    fn scene_geometry(&mut self, doc: &Document) -> u64 {
        let scene = doc.scene();
        let mut world = Fingerprint(DefaultHasher::new());
        hash_debug(&mut world, &(scene.world(), scene.cordons()));
        let mut total = world.0.finish();
        let mut owners = std::collections::HashSet::new();
        for (id, node) in scene.solids().iter() {
            let ptr = node.solid.sides.as_ptr() as usize;
            let len = node.solid.sides.len();
            let solid = match self.solids.get(id) {
                Some(&(p, l, h)) if p == ptr && l == len => h,
                _ => {
                    let mut hash = Fingerprint(DefaultHasher::new());
                    hash_debug(&mut hash, &node.solid);
                    let h = hash.0.finish();
                    self.solids.insert(*id, (ptr, len, h));
                    h
                }
            };
            let owner = match node.owner {
                hp_editor::Owner::World => 0,
                hp_editor::Owner::Entity(e) => {
                    owners.insert(e);
                    u64::from(e) + 1
                }
            };
            let flags = u64::from(node.solid.hidden) | (u64::from(doc.is_solid_visible(*id)) << 1);
            total = total.wrapping_add(mix(&[u64::from(*id), solid, owner, flags]));
        }
        if self.solids.len() > scene.solids().len() * 2 + 64 {
            self.solids.retain(|id, _| scene.solid(*id).is_some());
        }
        let lighting_origin = |k: &str| k.eq_ignore_ascii_case("lightingorigin");
        for (id, node) in scene.entities().iter() {
            let entity = &node.entity;
            if !owners.contains(id) && !geometry_entity(entity) {
                continue;
            }
            total = total.wrapping_add(entity_hash(entity, doc.is_entity_visible(*id), false, &lighting_origin));
        }
        total
    }

    fn raw_geometry(&mut self, doc: &Document) -> u64 {
        let serial = doc.content_serial();
        match self.geometry.filter(|(revision, _)| *revision == serial) {
            Some((_, key)) => key,
            None => {
                let key = self.scene_geometry(doc);
                self.geometry = Some((serial, key));
                key
            }
        }
    }

    fn scene_key(&mut self, doc: &Document) -> u64 {
        let geometry = self.raw_geometry(doc);
        let scene = doc.scene();
        let all = |_: &str| false;
        let entities = scene.entities().iter().fold(scene.entities().len() as u64, |total, (id, node)| {
            total.wrapping_add(entity_hash(&node.entity, doc.is_entity_visible(*id), true, &all))
        });
        let mut blocks = Fingerprint(DefaultHasher::new());
        for block in scene.extra_blocks() {
            if !PRESENTATION.iter().any(|name| block.name.eq_ignore_ascii_case(name)) {
                hash_debug(&mut blocks, block);
            }
        }
        mix(&[geometry, entities, blocks.0.finish()])
    }

    pub(super) fn geometry_key(&mut self, state: &AppState) -> u64 {
        let geometry = self.raw_geometry(&state.doc);
        mix(&[geometry, compile_settings(state)])
    }

    pub(super) fn resolve(&mut self, state: &AppState, lit: bool, delay: f64, force: bool) -> Option<u64> {
        let stamp = Stamp { content: state.doc.content_serial(), settings: settings_fingerprint(state, lit) };
        self.resolve_with(&state.doc, lit, stamp, state.frame.time, delay, force)
    }

    fn resolve_with(
        &mut self,
        doc: &Document,
        lit: bool,
        stamp: Stamp,
        now: f64,
        delay: f64,
        force: bool,
    ) -> Option<u64> {
        let input = &mut self.input[usize::from(lit)];
        if input.observed.is_none_or(|(seen, _)| seen != stamp) {
            input.observed = Some((stamp, now));
        }
        if let Some((resolved, serial)) = input.resolved
            && resolved == stamp
        {
            return Some(serial);
        }
        if !force && input.observed.is_some_and(|(_, at)| now - at < delay) {
            return None;
        }
        let scene = match self.scene {
            Some((content, key)) if content == stamp.content => key,
            _ => {
                let key = self.scene_key(doc);
                self.scene = Some((stamp.content, key));
                key
            }
        };
        let key = (scene, stamp.settings);
        let mut hash = DefaultHasher::new();
        key.hash(&mut hash);
        let serial = hash.finish();
        self.input[usize::from(lit)].resolved = Some((stamp, serial));
        Some(serial)
    }

    pub(super) fn current(&self, state: &AppState, lit: bool, serial: Option<u64>) -> bool {
        let stamp = Stamp { content: state.doc.content_serial(), settings: settings_fingerprint(state, lit) };
        self.input[usize::from(lit)].resolved.is_some_and(|(seen, resolved)| seen == stamp && Some(resolved) == serial)
    }

    pub(super) fn pending(&self, lit: bool) -> bool {
        let input = &self.input[usize::from(lit)];
        input.observed.is_some_and(|(observed, _)| input.resolved.is_none_or(|(resolved, _)| resolved != observed))
    }
}

pub(super) fn scope_serial(serial: u64, scope: &Scope) -> u64 {
    let mut hash = Fingerprint(DefaultHasher::new());
    serial.hash(&mut hash.0);
    hash_debug(&mut hash, scope);
    hash.0.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_light_changes_and_prop_lighting_origins_keep_the_geometry_bake_key() {
        let mut state = hp_app::testing::test_state();
        let mut cache = BakeCache::default();
        let initial = cache.geometry_key(&state);
        let id = state
            .doc
            .create_point_entity(
                "light",
                glam::DVec3::new(32.0, 0.0, 128.0),
                &[("_light".into(), "255 128 64 200".into())],
            )
            .unwrap();
        assert_eq!(cache.geometry_key(&state), initial);
        state.doc.set_entity_key(&[id], "_light", "32 64 255 600").unwrap();
        assert_eq!(cache.geometry_key(&state), initial);
        state
            .doc
            .create_point_entity("light_environment", glam::DVec3::ZERO, &[("_light".into(), "255 255 255 300".into())])
            .unwrap();
        assert_eq!(cache.geometry_key(&state), initial, "the sun is relit without compiling");
        state
            .doc
            .create_point_entity("prop_static", glam::DVec3::ZERO, &[("model".into(), "models/a.mdl".into())])
            .unwrap();
        let prop = cache.geometry_key(&state);
        assert_ne!(prop, initial, "props cast shadows");
        let solid = *state.doc.scene().solids().keys().next().unwrap();
        state.doc.apply_material_to_objects(&[hp_editor::ObjRef::Solid(solid)], "lights/white").unwrap();
        assert_ne!(cache.geometry_key(&state), prop, "materials are part of the geometry");
        assert!(state.doc.undo());
        assert_eq!(cache.geometry_key(&state), prop, "undo restores the key");
    }
    use hp_app::settings::{LightingQuality, LightingSetting};
    use hp_editor::luxels::{ColorCorrectionPlus, ColorCorrectionSlot};

    fn ready(cache: &mut BakeCache, state: &AppState, lit: bool) -> u64 {
        cache.resolve(state, lit, 0.0, true).unwrap()
    }

    #[test]
    fn presentation_and_camera_reuse_the_bake() {
        let mut state = hp_app::testing::test_state();
        let mut cache = BakeCache::default();
        let original = ready(&mut cache, &state, false);
        let scene = scene_fingerprint(&state.doc);
        state.cameras.cam3d.pos += glam::DVec3::splat(1000.0);
        state.settings.view.render_mode = hp_app::settings::RenderModeSetting::Textured;
        state.settings.lighting.threads = 8;
        state.settings.lighting.show_budget = false;
        state.settings.lighting.auto_scope = true;
        assert_eq!(ready(&mut cache, &state, false), original);
        let mut options = state.doc.light_plus();
        options.brightness_scale = 4.0;
        options.incremental_delay = 30.0;
        options.bake_dist = 8192.0;
        options.incremental = false;
        options.debug = true;
        state.doc.set_light_plus(&options).unwrap();
        let mut cc = ColorCorrectionPlus::default();
        cc.slots[0] = ColorCorrectionSlot { name: "test.raw".to_owned(), weight: 1.0 };
        state.doc.set_color_correction_plus(&cc).unwrap();
        for name in ["postprocess_plus", "palette_plus", "bgimages_plus"] {
            let mut block = hp_kv::Block::new(name);
            block.set("enable", "1");
            state.doc.set_extra_block(name, block).unwrap();
        }
        assert_eq!(scene_fingerprint(&state.doc), scene);
        assert_eq!(ready(&mut cache, &state, false), original);
    }

    #[test]
    fn physical_settings_invalidate_their_own_cache() {
        let mut state = hp_app::testing::test_state();
        let mut cache = BakeCache::default();
        let first = ready(&mut cache, &state, false);
        state.settings.lighting.quality = LightingQuality::Final;
        let quality = ready(&mut cache, &state, false);
        assert_ne!(first, quality);
        state.settings.compile.lighting = LightingSetting::Ldr;
        let ldr = ready(&mut cache, &state, false);
        state.settings.compile.lighting = LightingSetting::Hdr;
        let hdr = ready(&mut cache, &state, false);
        assert_ne!(ldr, hdr);
        state.settings.compile.static_prop_shadows = !state.settings.compile.static_prop_shadows;
        let shadows = ready(&mut cache, &state, false);
        assert_ne!(hdr, shadows);
        state.settings.compile.static_prop_lighting = !state.settings.compile.static_prop_lighting;
        let props = ready(&mut cache, &state, false);
        assert_ne!(shadows, props);
        state.settings.lighting.max_luxels_millions += 1;
        let budget = ready(&mut cache, &state, false);
        assert_ne!(props, budget);
        let indirect = ready(&mut cache, &state, true);
        state.settings.render.light_preview_match_compile = false;
        state.settings.render.light_preview_hdr = !state.settings.render.light_preview_hdr;
        assert_ne!(indirect, ready(&mut cache, &state, true));
        assert_eq!(budget, ready(&mut cache, &state, false));
        let mut options = state.doc.light_plus();
        options.samples_sun += 1;
        options.bounced = !options.bounced;
        state.doc.set_light_plus(&options).unwrap();
        assert_ne!(budget, ready(&mut cache, &state, false));
    }

    #[test]
    fn changed_scene_is_debounced_and_old_results_are_not_current() {
        let mut state = hp_app::testing::test_state();
        let mut cache = BakeCache::default();
        let old = ready(&mut cache, &state, false);
        let scene = cache.scene;
        state.doc.create_point_entity("light", glam::DVec3::ZERO, &[]).unwrap();
        state.frame.time = 5.0;
        assert_eq!(cache.resolve(&state, false, 1.0, false), None);
        assert_eq!(cache.scene, scene, "debounce does not scan geometry");
        assert!(!cache.current(&state, false, Some(old)));
        state.frame.time = 6.1;
        let new = cache.resolve(&state, false, 1.0, false).unwrap();
        assert_ne!(old, new);
        assert!(!cache.current(&state, false, Some(old)));
        assert!(cache.current(&state, false, Some(new)));
        assert!(!cache.pending(false));
        let scene = cache.scene;
        for i in 0..100 {
            state.frame.time += 1.0;
            state.cameras.cam3d.pos.x += f64::from(i);
            assert_eq!(cache.resolve(&state, false, 1.0, false), Some(new));
            assert_eq!(cache.scene, scene);
        }
    }

    #[test]
    fn geometry_and_light_changes_invalidate_the_scene_fingerprint() {
        let mut state = hp_app::testing::test_state();
        let original = scene_fingerprint(&state.doc);
        let solid = *state.doc.scene().solids().keys().next().unwrap();
        state.doc.translate(&[hp_editor::ObjRef::Solid(solid)], glam::DVec3::X * 64.0).unwrap();
        assert_ne!(scene_fingerprint(&state.doc), original);
        assert!(state.doc.undo());
        assert_eq!(scene_fingerprint(&state.doc), original);
        state.doc.apply_material_to_objects(&[hp_editor::ObjRef::Solid(solid)], "lights/white").unwrap();
        assert_ne!(scene_fingerprint(&state.doc), original);
        let light = state.doc.create_point_entity("light", glam::DVec3::ZERO, &[]).unwrap();
        let before = scene_fingerprint(&state.doc);
        state.doc.set_entity_key(&[light], "_light", "255 0 0 500").unwrap();
        assert_ne!(scene_fingerprint(&state.doc), before);
    }

    #[test]
    fn remounted_sources_invalidate_without_asset_loading() {
        let dir = std::env::temp_dir().join(format!("hp-lighting-cache-remount-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fs = hp_fs::GameFs::empty();
        let mut state = hp_app::testing::test_state();
        state.assets = Box::new(hp_app::testing::MockAssets::with_fs(fs.clone()));
        let mut cache = BakeCache::default();
        let before = ready(&mut cache, &state, false);
        fs.add_game_dir(&dir).unwrap();
        assert_ne!(ready(&mut cache, &state, false), before);
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn regional_compiles_do_not_reuse_another_region() {
        let local = Scope::Around { center: glam::DVec3::ZERO, radius: 1024.0 };
        assert_ne!(scope_serial(1, &Scope::All), scope_serial(1, &local));
        assert_ne!(scope_serial(1, &local), scope_serial(2, &local));
        let moved = Scope::Around { center: glam::DVec3::X * 512.0, radius: 1024.0 };
        assert_ne!(scope_serial(1, &local), scope_serial(1, &moved));
    }
}
