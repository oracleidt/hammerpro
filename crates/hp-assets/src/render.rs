use std::sync::Arc;

use ahash::{AHashMap, AHashSet};
use glam::DVec3;
use hp_app::runtime::render::RenderDisplay;
use hp_app::state::CollisionTriangles;
use hp_editor::{Document, Id};
use hp_render::{Environment, Fog, ModelId, Renderer, Sky3d, SkyView, TexId};

use crate::AssetManager;
use crate::material::MatKind;
use crate::producer::DerivedProducer;

pub(crate) fn producers() -> Vec<Box<dyn DerivedProducer>> {
    Vec::new()
}

const PREVIEW_VIEW: u32 = 0;

pub(crate) const SKY_SUFFIXES: [&str; 6] = ["rt", "lf", "bk", "ft", "up", "dn"];
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SkyRoom {
    pub camera: Id,
    pub origin: DVec3,
    pub scale: f32,
    pub solids: hp_editor::IdSet,
    pub entities: hp_editor::IdSet,
    pub bounds: (DVec3, DVec3),
}

#[derive(Default)]
pub(crate) struct EnvState {
    pub sky: Option<(String, [hp_render::MaterialId; 6], Option<[hp_render::MaterialId; 6]>)>,
    pub room: Option<SkyRoom>,
    pub room_dirty: bool,
    pub chosen_camera: Option<Id>,
    pub hdr_sky: HdrSkyState,
    pub compiled_sky: Vec<u32>,
    pub fog_volumes_on: Option<bool>,
}

#[derive(Default)]
pub(crate) struct HdrSkyState {
    key: Option<(String, u64)>,
    rx: Option<crossbeam_channel::Receiver<Option<Vec<HdrFace>>>>,
}

pub(crate) struct HdrFace {
    width: u32,
    height: u32,
    texels: Vec<[f32; 3]>,
}

const MAX_HDR_SKY_DIM: u32 = 2048;

fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((h >> 10) & 0x1f);
    let man = f32::from(h & 0x3ff);
    match exp {
        0 => sign * man * (1.0 / 16_777_216.0),
        31 => 0.0,
        e => sign * (1.0 + man / 1024.0) * 2f32.powi(e - 15),
    }
}

pub(crate) fn hdr_texels(level: &hp_vtf::RawLevel<'_>, compressed: bool) -> Option<Vec<[f32; 3]>> {
    use hp_vtf::ImageFormat as F;
    let n = (level.width as usize) * (level.height as usize);
    let texels: Vec<[f32; 3]> = match level.format {
        F::Rgba16161616F if !compressed => level
            .data
            .chunks_exact(8)
            .map(|p| [0, 1, 2].map(|i| half_to_f32(u16::from_le_bytes([p[2 * i], p[2 * i + 1]]))))
            .collect(),
        F::Rgba16161616 if !compressed => level
            .data
            .chunks_exact(8)
            .map(|p| [0, 1, 2].map(|i| f32::from(u16::from_le_bytes([p[2 * i], p[2 * i + 1]])) / 65535.0 * 16.0))
            .collect(),
        F::Rgb323232F | F::Rgba32323232F if !compressed => {
            let stride = if level.format == F::Rgb323232F { 12 } else { 16 };
            level
                .data
                .chunks_exact(stride)
                .map(|p| [0, 1, 2].map(|i| f32::from_le_bytes([p[4 * i], p[4 * i + 1], p[4 * i + 2], p[4 * i + 3]])))
                .collect()
        }
        _ => {
            let img = level.to_rgba8().ok()?;
            img.pixels
                .chunks_exact(4)
                .map(|p| {
                    let c = [0, 1, 2].map(|i| f32::from(p[i]) / 255.0);
                    if compressed { c.map(|v| v * f32::from(p[3]) / 255.0 * 8.0) } else { c }
                })
                .collect()
        }
    };
    (texels.len() == n).then_some(texels)
}

fn load_hdr_face(fs: &hp_fs::GameFs, material: &str) -> Option<HdrFace> {
    let mat = fs.material(material)?;
    let (texture, compressed) = match mat.texture("$hdrcompressedtexture") {
        Some(t) => (t, true),
        None => (mat.texture("$hdrbasetexture")?, false),
    };
    let data = fs.read(&format!("materials/{texture}.vtf"))?;
    let info = hp_vtf::read_info(&data).ok()?;
    let level = hp_vtf::raw_level(&data, info.mip_for_max_dimension(MAX_HDR_SKY_DIM), 0, 0).ok()?;
    let tint = mat.color_var().unwrap_or([1.0; 3]);
    let texels =
        hdr_texels(&level, compressed)?.into_iter().map(|c| [c[0] * tint[0], c[1] * tint[1], c[2] * tint[2]]).collect();
    Some(HdrFace { width: level.width, height: level.height, texels })
}

fn load_hdr_sky(fs: &hp_fs::GameFs, key: &str) -> Option<Vec<HdrFace>> {
    SKY_SUFFIXES.iter().map(|s| load_hdr_face(fs, &format!("skybox/{key}_hdr{s}"))).collect()
}

fn previewed_camera(doc: &Document, chosen: Option<Id>) -> Option<hp_editor::sky::SkyCamera> {
    let scene = doc.scene();
    chosen.and_then(|id| scene.entity(id)).and_then(|n| hp_editor::sky::sky_camera_of(&n.entity)).or_else(|| {
        let id = hp_editor::sky::game_sky_camera(doc)?;
        scene.entity(id).and_then(|n| hp_editor::sky::sky_camera_of(&n.entity))
    })
}

pub(crate) fn find_shown_room(doc: &Document, chosen: Option<Id>) -> Option<SkyRoom> {
    let cam = previewed_camera(doc, chosen)?;
    let room = hp_editor::sky::find_sky_room(doc, cam.origin)?;
    Some(SkyRoom {
        camera: cam.id,
        origin: cam.origin,
        scale: cam.scale,
        solids: room.solids,
        entities: room.entities,
        bounds: (room.bounds.min, room.bounds.max),
    })
}

pub(crate) fn fog_of(doc: &Document, id: Id) -> Option<Fog> {
    hp_app::runtime::fog::global_fog(doc, Some(id))
}

impl AssetManager {
    pub(crate) fn sprite_material_impl(&mut self, name: &str, additive: bool) -> hp_render::MaterialId {
        let key = crate::util::sprite_material(name).unwrap_or_default();
        let kind = if additive { MatKind::SpriteAdditive } else { MatKind::Sprite };
        self.asset_material(&key, kind)
    }

    fn sky_faces(&mut self, renderer: &Renderer, name: &str) -> Option<SkyView> {
        let key = crate::util::material_key(name);
        if key.is_empty() {
            return None;
        }
        let (ldr, hdr) = match &self.scene.env.sky {
            Some((n, ldr, hdr)) if *n == key => (*ldr, *hdr),
            _ => {
                let ldr = SKY_SUFFIXES.map(|s| self.asset_material(&format!("skybox/{key}{s}"), MatKind::World));
                self.scene.env.sky = Some((key.clone(), ldr, None));
                (ldr, None)
            }
        };
        let texture = |mat| renderer.material(mat).and_then(|d| d.texture);
        let mut faces = [None; 6];
        let mut uv = [SkyView::IDENTITY_UV; 6];
        for (i, (face, mat)) in faces.iter_mut().zip(ldr).enumerate() {
            self.touch_material(mat);
            let mut tex = texture(mat)?;
            let mut face_name = format!("skybox/{key}{}", SKY_SUFFIXES[i]);
            if tex == TexId::MISSING {
                face_name = format!("skybox/{key}_hdr{}", SKY_SUFFIXES[i]);
                let hdr = match hdr {
                    Some(h) => h,
                    None => {
                        let h =
                            SKY_SUFFIXES.map(|s| self.asset_material(&format!("skybox/{key}_hdr{s}"), MatKind::World));
                        if let Some(sky) = self.scene.env.sky.as_mut() {
                            sky.2 = Some(h);
                        }
                        h
                    }
                };
                self.touch_material(hdr[i]);
                tex = texture(hdr[i])?;
            }
            if tex == TexId::LOADING {
                return None;
            }
            *face = (tex != TexId::MISSING).then_some(tex);
            if let crate::material::AnaLook::Ready(a) = self.mats.ana_look(&face_name) {
                uv[i] = a.uv;
            }
        }
        faces.iter().any(Option::is_some).then_some(SkyView { faces, uv, yaw_deg: 0.0 })
    }

    fn sync_hdr_sky(&mut self, renderer: &mut Renderer, want: Option<String>) {
        let generation = self.fs.as_ref().map_or(0, hp_fs::GameFs::generation);
        let state = &mut self.scene.env.hdr_sky;
        if let Some(rx) = &state.rx
            && let Ok(faces) = rx.try_recv()
        {
            state.rx = None;
            let faces: Option<[hp_render::HdrSkyFace<'_>; 6]> = faces.as_ref().and_then(|f| {
                let list: Vec<hp_render::HdrSkyFace<'_>> = f
                    .iter()
                    .map(|f| hp_render::HdrSkyFace { width: f.width, height: f.height, texels: &f.texels })
                    .collect();
                list.try_into().ok()
            });
            renderer.set_hdr_sky(faces.as_ref());
            let name = state.key.as_ref().map_or("", |k| k.0.as_str());
            if renderer.has_hdr_sky() {
                log::info!("player view: HDR sky {name} loaded");
            } else {
                log::info!("player view: sky {name} has no HDR faces, the LDR ones are drawn");
            }
        }
        let key = want.map(|name| (name, generation));
        if state.key == key {
            return;
        }
        state.rx = None;
        if state.key.take().is_some() {
            renderer.set_hdr_sky(None);
        }
        let (Some(key), Some(fs)) = (key, self.fs.clone()) else { return };
        let (tx, rx) = crossbeam_channel::bounded(1);
        state.rx = Some(rx);
        let name = key.0.clone();
        state.key = Some(key);
        rayon::spawn(move || {
            let _ = tx.send(load_hdr_sky(&fs, &name));
        });
    }

    pub(crate) fn sync_environment(
        &mut self,
        doc: &Document,
        renderer: &mut Renderer,
        display: &RenderDisplay,
        map_changed: bool,
    ) -> (Vec<Id>, Vec<Id>) {
        let env = &mut self.scene.env;
        env.room_dirty |= map_changed;
        let mut changed_solids = Vec::new();
        let mut changed_entities = Vec::new();
        if !display.sky_3d {
            if let Some(old) = env.room.take() {
                changed_solids.extend(old.solids);
                changed_entities.extend(old.entities);
            }
        } else if env.room_dirty || env.room.is_none() || env.chosen_camera != display.sky_camera {
            env.room_dirty = false;
            env.chosen_camera = display.sky_camera;
            let new = find_shown_room(doc, display.sky_camera);
            let old = std::mem::replace(&mut env.room, new.clone());
            let (old_s, old_e) = old.map(|r| (r.solids, r.entities)).unwrap_or_default();
            let (new_s, new_e) = new.map(|r| (r.solids, r.entities)).unwrap_or_default();
            changed_solids.extend(old_s.symmetric_difference(&new_s).copied());
            changed_entities.extend(old_e.symmetric_difference(&new_e).copied());
        }
        let skyname = doc.scene().world().get("skyname").map(str::to_owned);
        let sky = match (display.sky, skyname) {
            (true, Some(name)) => self.sky_faces(renderer, &name),
            _ => None,
        };
        let sky_3d =
            self.scene.env.room.as_ref().map(|r| Sky3d { origin: r.origin.as_vec3().to_array(), scale: r.scale });
        let fog = display.fog.and_then(|id| fog_of(doc, id));
        let hdr =
            sky.is_some() && renderer.compiled_lighting().is_some_and(|s| s.mode == hp_render::CompiledLightMode::Hdr);
        let hdr_key = self.scene.env.sky.as_ref().map(|s| s.0.clone()).filter(|_| hdr);
        self.sync_hdr_sky(renderer, hdr_key);
        renderer.set_environment(Environment { sky, sky_3d, fog });
        let room = self.scene.env.room.as_ref().map(|r| r.bounds);
        let objects = self.compiled_sky_objects(room);
        if objects != self.scene.env.compiled_sky {
            renderer.set_compiled_sky(&objects);
            self.scene.env.compiled_sky = objects;
        }
        (changed_solids, changed_entities)
    }
}

pub(crate) type CollisionLines = Arc<Vec<[[f32; 3]; 2]>>;

#[derive(Clone)]
pub(crate) struct CollisionShapes {
    pub lines: CollisionLines,
    pub triangles: CollisionTriangles,
}

pub(crate) struct CollisionState {
    tx: crossbeam_channel::Sender<(String, Option<CollisionShapes>)>,
    rx: crossbeam_channel::Receiver<(String, Option<CollisionShapes>)>,
    loaded: AHashMap<String, Option<CollisionShapes>>,
    pending: AHashSet<String>,
    applied: AHashSet<ModelId>,
    tris_tx: crossbeam_channel::Sender<(String, Option<CollisionTriangles>)>,
    tris_rx: crossbeam_channel::Receiver<(String, Option<CollisionTriangles>)>,
    tris: AHashMap<String, Option<CollisionTriangles>>,
    tris_pending: AHashSet<String>,
}

impl Default for CollisionState {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let (tris_tx, tris_rx) = crossbeam_channel::unbounded();
        Self {
            tx,
            rx,
            loaded: AHashMap::new(),
            pending: AHashSet::new(),
            applied: AHashSet::new(),
            tris_tx,
            tris_rx,
            tris: AHashMap::new(),
            tris_pending: AHashSet::new(),
        }
    }
}

fn load_collision_triangles(fs: &hp_fs::GameFs, path: &str) -> Option<CollisionTriangles> {
    let phy = fs.read(&hp_mdl::phy_path(path))?;
    let model = match fs.read(path) {
        Some(mdl) => hp_mdl::parse_phy_for_model(&phy, &mdl, hp_mdl::Pose::FirstSequence),
        None => hp_mdl::parse_phy(&phy),
    }
    .ok()?;
    let tris: Vec<[[f32; 3]; 3]> = model
        .solids
        .iter()
        .flat_map(|s| {
            s.piece_triangles().filter_map(|t| {
                Some([
                    *s.vertices.get(t[0] as usize)?,
                    *s.vertices.get(t[1] as usize)?,
                    *s.vertices.get(t[2] as usize)?,
                ])
            })
        })
        .collect();
    (!tris.is_empty()).then(|| Arc::new(tris))
}

fn load_collision(fs: &hp_fs::GameFs, key: &str) -> Option<CollisionShapes> {
    let path = key.split_once(crate::models::ANIM_SEPARATOR).map_or(key, |(p, _)| p);
    let phy = fs.read(&hp_mdl::phy_path(path))?;
    let model = match fs.read(path) {
        Some(mdl) => hp_mdl::parse_phy_for_model(&phy, &mdl, hp_mdl::Pose::FirstSequence),
        None => hp_mdl::parse_phy(&phy),
    };
    match model {
        Ok(m) => {
            let triangles: Vec<[[f32; 3]; 3]> = m
                .solids
                .iter()
                .flat_map(|s| {
                    s.piece_triangles().filter_map(|t| {
                        Some([
                            *s.vertices.get(t[0] as usize)?,
                            *s.vertices.get(t[1] as usize)?,
                            *s.vertices.get(t[2] as usize)?,
                        ])
                    })
                })
                .collect();
            Some(CollisionShapes { lines: Arc::new(m.edges()), triangles: Arc::new(triangles) })
        }
        Err(e) => {
            log::debug!("collision model of {path}: {e}");
            None
        }
    }
}

impl AssetManager {
    pub(crate) fn refresh_tool_materials(&mut self) {
        use crate::material::{AnaLook, guess_tool};
        use crate::registry::MatSource;
        let mats = &self.mats;
        let ids: Vec<hp_render::MaterialId> = mats
            .records
            .iter()
            .enumerate()
            .filter(|(_, r)| match &r.source {
                MatSource::Asset { name, .. } => {
                    guess_tool(name).is_some() || matches!(mats.ana_look(name), AnaLook::Ready(a) if a.tool.is_some())
                }
                MatSource::Flat(_) => false,
            })
            .map(|(i, _)| i as hp_render::MaterialId + 1)
            .collect();
        for id in ids {
            self.mats.mark_dirty(id);
        }
    }

    pub(crate) fn collision_triangles_impl(&mut self, path: &str) -> Option<Option<CollisionTriangles>> {
        let c = &mut self.scene.collision;
        while let Ok((key, tris)) = c.tris_rx.try_recv() {
            c.tris_pending.remove(&key);
            c.tris.insert(key, tris);
        }
        if let Some(t) = c.tris.get(path) {
            return Some(t.clone());
        }
        if let Some(fs) = self.fs.clone()
            && c.tris_pending.insert(path.to_owned())
        {
            let tx = c.tris_tx.clone();
            let path = path.to_owned();
            rayon::spawn(move || {
                let tris = load_collision_triangles(&fs, &path);
                let _ = tx.send((path, tris));
            });
        }
        None
    }

    fn poll_collision(&mut self) {
        let c = &mut self.scene.collision;
        while let Ok((key, lines)) = c.rx.try_recv() {
            c.pending.remove(&key);
            c.loaded.insert(key, lines);
        }
    }

    fn collision_shapes(&mut self, key: &str) -> Option<Option<CollisionShapes>> {
        self.poll_collision();
        let c = &mut self.scene.collision;
        if let Some(lines) = c.loaded.get(key) {
            return Some(lines.clone());
        }
        if let Some(fs) = self.fs.clone()
            && c.pending.insert(key.to_owned())
        {
            let tx = c.tx.clone();
            let key = key.to_owned();
            rayon::spawn(move || {
                let lines = load_collision(&fs, &key);
                let _ = tx.send((key, lines));
            });
        }
        None
    }

    pub(crate) fn preview_collision(&mut self, key: &str, physics: bool) {
        let display = hp_app::runtime::render::render_display();
        let show = physics || display.collision;
        let lines = if show { self.collision_shapes(key) } else { None };
        let Some(p) = self.preview.as_mut() else { return };
        let opts =
            hp_render::ViewOptions { collision_models: show, normal_maps: display.normal_maps, ..Default::default() };
        if p.renderer.view_options(PREVIEW_VIEW) != opts {
            p.renderer.set_view_options(PREVIEW_VIEW, opts);
            p.needs_render = true;
        }
        let Some(id) = p.model_id() else { return };
        if let Some(Some(shapes)) = lines
            && !p.renderer.has_model_collision(id)
        {
            p.renderer.set_model_collision(id, &shapes.lines);
            p.needs_render = true;
        }
    }

    pub(crate) fn sync_collision(&mut self, renderer: &mut Renderer, display: &RenderDisplay) {
        self.poll_collision();
        if !display.collision && !display.collision_only {
            return;
        }
        let models: Vec<(String, ModelId)> = self
            .models
            .render
            .iter()
            .filter_map(|((path, _), m)| Some((path.clone(), m.id?)))
            .chain(self.models.variants.iter().filter_map(|((key, _, _), (m, _))| Some((key.clone(), m.id?))))
            .filter(|(_, id)| !self.scene.collision.applied.contains(id))
            .collect();
        for (key, id) in models {
            if let Some(shapes) = self.collision_shapes(&key) {
                match shapes {
                    Some(shapes) => {
                        renderer.set_model_collision(id, &shapes.lines);
                        renderer.set_model_collision_hull(id, &shapes.triangles);
                    }
                    None => renderer.set_model_no_collision(id),
                }
                self.scene.collision.applied.insert(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_vmf::{Entity, Map, Side, Solid, TexAxis};

    fn block(id: Id, lo: DVec3, hi: DVec3, material: &str) -> Solid {
        let p = DVec3::new;
        let planes = [
            [p(lo.x, hi.y, hi.z), p(hi.x, hi.y, hi.z), p(hi.x, lo.y, hi.z)],
            [p(lo.x, lo.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, hi.y, lo.z)],
            [p(lo.x, hi.y, hi.z), p(lo.x, lo.y, hi.z), p(lo.x, lo.y, lo.z)],
            [p(hi.x, hi.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, lo.y, hi.z)],
            [p(hi.x, hi.y, hi.z), p(lo.x, hi.y, hi.z), p(lo.x, hi.y, lo.z)],
            [p(hi.x, lo.y, lo.z), p(lo.x, lo.y, lo.z), p(lo.x, lo.y, hi.z)],
        ];
        let sides = planes
            .iter()
            .enumerate()
            .map(|(k, plane)| Side {
                id: id * 10 + k as Id,
                plane: *plane,
                material: material.to_owned(),
                uaxis: TexAxis { axis: DVec3::X, shift: 0.0, scale: 0.25 },
                vaxis: TexAxis { axis: DVec3::NEG_Y, shift: 0.0, scale: 0.25 },
                ..Default::default()
            })
            .collect();
        Solid { id, sides, ..Default::default() }
    }

    fn room(first: Id, lo: DVec3, hi: DVec3, material: &str) -> Vec<Solid> {
        let t = 16.0;
        let v = DVec3::new;
        vec![
            block(first, v(lo.x - t, lo.y - t, lo.z - t), v(hi.x + t, hi.y + t, lo.z), "nature/dirt"),
            block(first + 1, v(lo.x - t, lo.y - t, hi.z), v(hi.x + t, hi.y + t, hi.z + t), material),
            block(first + 2, v(lo.x - t, lo.y - t, lo.z), v(lo.x, hi.y + t, hi.z), material),
            block(first + 3, v(hi.x, lo.y - t, lo.z), v(hi.x + t, hi.y + t, hi.z), material),
            block(first + 4, v(lo.x, lo.y - t, lo.z), v(hi.x, lo.y, hi.z), material),
            block(first + 5, v(lo.x, hi.y, lo.z), v(hi.x, hi.y + t, hi.z), material),
        ]
    }

    fn ent(id: Id, class: &str, props: &[(&str, &str)]) -> Entity {
        let mut e = Entity { id, classname: class.to_owned(), ..Default::default() };
        for (k, v) in props {
            e.set(k, *v);
        }
        e
    }

    fn map() -> Document {
        let mut m = Map::new_empty();
        let v = DVec3::new;
        m.world.solids.extend(room(100, v(-512.0, -512.0, 2000.0), v(512.0, 512.0, 2512.0), "TOOLS/TOOLSSKYBOX"));
        m.world.solids.push(block(200, v(-64.0, -64.0, 2000.0), v(64.0, 64.0, 2200.0), "brick/a"));
        m.world.solids.push(block(300, v(-256.0, -256.0, -16.0), v(256.0, 256.0, 0.0), "brick/a"));
        m.entities.push(ent(1, "sky_camera", &[("origin", "0 0 2300"), ("scale", "16")]));
        m.entities.push(ent(2, "prop_static", &[("origin", "100 100 2001"), ("model", "models/a.mdl")]));
        m.entities.push(ent(3, "prop_static", &[("origin", "0 0 1"), ("model", "models/a.mdl")]));
        m.entities.push(ent(
            4,
            "env_fog_controller",
            &[
                ("fogenable", "1"),
                ("fogcolor", "255 0 0"),
                ("fogstart", "100"),
                ("fogend", "50"),
                ("fogmaxdensity", "2"),
                ("fogblend", "1"),
            ],
        ));
        Document::from_map(m)
    }

    #[test]
    fn the_sky_room_holds_the_skybox_only() {
        let doc = map();
        let room = find_shown_room(&doc, None).expect("sealed room");
        assert_eq!((room.camera, room.scale), (1, 16.0));
        assert!(room.solids.contains(&200), "the building is in the skybox");
        assert!(!room.solids.contains(&300), "the playable floor is not");
        assert!(room.entities.contains(&2) && room.entities.contains(&1) && !room.entities.contains(&3));
        assert_eq!(find_shown_room(&doc, Some(4)).map(|r| r.camera), Some(1));
        let mut open = Map::new_empty();
        open.world.solids.push(block(
            1,
            DVec3::new(-64.0, -64.0, -16.0),
            DVec3::new(64.0, 64.0, 0.0),
            "tools/toolsskybox",
        ));
        open.entities.push(ent(1, "sky_camera", &[("origin", "0 0 64")]));
        assert!(find_shown_room(&Document::from_map(open), None).is_none());
    }

    #[test]
    fn fog_keys_are_read_and_sanitized() {
        let doc = map();
        let f = fog_of(&doc, 0).expect("first controller");
        assert_eq!(f.color, [1.0, 0.0, 0.0]);
        assert_eq!(f.color2, f.color, "no secondary color: the primary one");
        assert_eq!((f.start, f.end), (100.0, 101.0), "end after start");
        assert_eq!(f.max_density, 1.0);
        assert!(f.blend);
        assert!(fog_of(&doc, 4).is_some() && fog_of(&doc, 999).is_some(), "unknown ids fall back to the first");
        assert!(fog_of(&Document::from_map(Map::new_empty()), 0).is_none());
        let mut off = map();
        off.edit_entity(4, |e| e.set("fogenable", "0")).expect("edit");
        assert!(fog_of(&off, 4).is_none(), "Fog Enable No: no fog");
    }

    #[test]
    fn hdr_sky_texels_are_read_like_the_engine() {
        assert_eq!(half_to_f32(0x3c00), 1.0);
        assert_eq!(half_to_f32(0x4400), 4.0);
        assert_eq!(half_to_f32(0xb800), -0.5);
        assert_eq!(half_to_f32(1), 1.0 / 16_777_216.0);
        let level = |format, data: &'static [u8]| hp_vtf::RawLevel { format, width: 1, height: 1, data };
        let c = hdr_texels(&level(hp_vtf::ImageFormat::Bgra8888, &[0, 255, 51, 128]), true).expect("compressed");
        let a = 128.0 / 255.0 * 8.0;
        assert!((c[0][0] - 0.2 * a).abs() < 1e-4 && (c[0][1] - a).abs() < 1e-4 && c[0][2] == 0.0, "{c:?}");
        let f = hdr_texels(&level(hp_vtf::ImageFormat::Rgba16161616F, &[0, 0x3c, 0, 0x44, 0, 0x38, 0, 0x3c]), false);
        assert_eq!(f, Some(vec![[1.0, 4.0, 0.5]]));
        let i = hdr_texels(&level(hp_vtf::ImageFormat::Rgba16161616, &[255, 255, 0, 0, 0, 0, 0, 0]), false);
        assert_eq!(i, Some(vec![[16.0, 0.0, 0.0]]));
        assert!(hdr_texels(&level(hp_vtf::ImageFormat::Rgba16161616F, &[0, 0]), false).is_none());
    }
}
