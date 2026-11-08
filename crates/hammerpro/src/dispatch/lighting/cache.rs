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

fn scene_fingerprint(doc: &Document) -> u64 {
    let scene = doc.scene();
    let mut hash = Fingerprint(DefaultHasher::new());
    let entity = |hash: &mut Fingerprint, e: &hp_vmf::Entity| {
        hash_debug(hash, &(e.id, &e.classname, &e.props, &e.connections, e.hidden, &e.extra_blocks));
    };
    entity(&mut hash, scene.world());
    let mut solids: Vec<_> = scene.solids().iter().collect();
    solids.sort_unstable_by_key(|(id, _)| **id);
    solids.len().hash(&mut hash.0);
    for (id, node) in solids {
        let solid = &node.solid;
        hash_debug(
            &mut hash,
            &(
                id,
                node.owner,
                &solid.sides,
                solid.hidden,
                doc.is_solid_visible(*id),
                &solid.extra_props,
                &solid.extra_blocks,
            ),
        );
    }
    let mut entities: Vec<_> = scene.entities().iter().collect();
    entities.sort_unstable_by_key(|(id, _)| **id);
    entities.len().hash(&mut hash.0);
    for (id, node) in entities {
        doc.is_entity_visible(*id).hash(&mut hash.0);
        entity(&mut hash, &node.entity);
    }
    hash_debug(&mut hash, scene.cordons());
    for block in scene.extra_blocks() {
        let presentation = [
            hp_editor::luxels::LIGHT_PLUS,
            hp_editor::luxels::COLORCORRECTION_PLUS,
            "postprocess_plus",
            "palette_plus",
            "bgimages_plus",
        ];
        if !presentation.iter().any(|name| block.name.eq_ignore_ascii_case(name)) {
            hash_debug(&mut hash, block);
        }
    }
    hash.0.finish()
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
    input: [Input; 2],
}

impl BakeCache {
    pub(super) fn current_geometry_key(&self, state: &AppState) -> Option<u64> {
        let (_, geometry) = self.geometry.filter(|(serial, _)| *serial == state.doc.content_serial())?;
        let mut hash = Fingerprint(DefaultHasher::new());
        geometry.hash(&mut hash.0);
        settings_fingerprint(state, false).hash(&mut hash.0);
        Some(hash.0.finish())
    }
    pub(super) fn geometry_key(&mut self, state: &AppState) -> u64 {
        let doc = &state.doc;
        let serial = doc.content_serial();
        let geometry = if let Some((_, key)) = self.geometry.filter(|(revision, _)| *revision == serial) {
            key
        } else {
            let mut hash = Fingerprint(DefaultHasher::new());
            let scene = doc.scene();
            hash_debug(&mut hash, &(scene.world(), scene.cordons()));
            let mut solids: Vec<_> = scene.solids().iter().collect();
            solids.sort_unstable_by_key(|(id, _)| **id);
            for (id, node) in solids {
                hash_debug(&mut hash, &(id, node.owner, &node.solid));
                doc.is_solid_visible(*id).hash(&mut hash.0);
            }
            let mut entities: Vec<_> = scene
                .entities()
                .iter()
                .filter(|(_, node)| {
                    let entity = &node.entity;
                    !entity.solids.is_empty()
                        || entity.get("model").is_some_and(|name| name.to_ascii_lowercase().ends_with(".mdl"))
                        || entity.classname.eq_ignore_ascii_case("func_instance")
                        || entity.classname.eq_ignore_ascii_case("light_environment")
                })
                .collect();
            entities.sort_unstable_by_key(|(id, _)| **id);
            for (id, node) in entities {
                let entity = &node.entity;
                hash_debug(&mut hash, &(id, &entity.classname, entity.hidden));
                for property in entity.props.iter().filter(|(name, _)| !name.eq_ignore_ascii_case("lightingorigin")) {
                    hash_debug(&mut hash, property);
                }
                doc.is_entity_visible(*id).hash(&mut hash.0);
            }
            let key = hash.0.finish();
            self.geometry = Some((serial, key));
            key
        };
        let mut hash = Fingerprint(DefaultHasher::new());
        geometry.hash(&mut hash.0);
        settings_fingerprint(state, false).hash(&mut hash.0);
        hash.0.finish()
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
                let key = scene_fingerprint(doc);
                self.scene = Some((stamp.content, key));
                key
            }
        };
        let key = (scene, stamp.settings);
        let mut hash = DefaultHasher::new();
        key.hash(&mut hash);
        let serial = hash.finish();
        input.resolved = Some((stamp, serial));
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
        assert_ne!(cache.geometry_key(&state), initial);
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
