use std::collections::{HashMap, HashSet};
use std::io::Cursor;

use glam::{DVec2, DVec3};
use hp_app::runtime::BlenderTarget;
use hp_app::{AppState, t, tf};
use hp_editor::ObjRef;
use hp_meshio::{Material, Model, Triangle, UNITS_PER_METER, UpAxis};
use hp_ui_entities::model_import::{IMPORT_FOLDER, clean_name};

use crate::export::{ObjInput, ObjLayout};

pub const TEXTURE_SIDE: u32 = 512;
pub const MAX_DISPLACEMENTS: usize = 1024;

#[derive(Debug)]
pub struct Source {
    pub model: Model,
    pub target: BlenderTarget,
    pub stem: String,
}

pub fn png_of(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    use image::ImageEncoder;
    if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        return None;
    }
    let mut out = Cursor::new(Vec::new());
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .ok()?;
    Some(out.into_inner())
}

pub fn embed_textures(fs: &hp_fs::GameFs, model: &mut Model) {
    let mut cache: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    for m in &mut model.materials {
        if m.embedded.is_some() || m.texture.is_some() {
            continue;
        }
        let key = m.name.to_ascii_lowercase();
        let png = cache
            .entry(key)
            .or_insert_with(|| {
                let texture = fs.material(&m.name)?.preview_texture()?;
                let loaded = fs.load_texture(&texture, Some(TEXTURE_SIDE)).ok()?;
                png_of(loaded.width, loaded.height, &loaded.rgba)
            })
            .clone();
        if let Some(png) = png {
            m.embedded = Some((png, "png".to_owned()));
        }
    }
}

fn int_key(e: &hp_vmf::Entity, key: &str) -> Option<u32> {
    e.get(key).and_then(|s| s.trim().parse::<f64>().ok()).filter(|v| v.is_finite() && *v >= 0.0).map(|v| v as u32)
}

pub fn model_stem(model: &str) -> String {
    let path = model.replace('\\', "/");
    let file = path.rsplit('/').next().unwrap_or(&path);
    file.strip_suffix(".mdl").or_else(|| file.strip_suffix(".MDL")).unwrap_or(file).to_owned()
}

pub fn blender_model_name(model: &str) -> String {
    let lower = model.replace('\\', "/").to_ascii_lowercase();
    let stem = model_stem(&lower);
    if lower.starts_with(&format!("models/{IMPORT_FOLDER}/")) && !stem.is_empty() {
        return clean_name(&stem);
    }
    clean_name(&format!("{stem}_blender"))
}

pub fn prop_model(fs: &hp_fs::GameFs, entity: &hp_vmf::Entity) -> Result<Model, String> {
    let model_path = hp_mdl::normalize_material_name(entity.get("model").unwrap_or_default());
    let model_path = if model_path.ends_with(".mdl") { model_path } else { format!("{model_path}.mdl") };
    if fs.read(&model_path).is_none() {
        return Err(tf!("blender.model_missing", model_path));
    }
    let body = int_key(entity, "body").unwrap_or(0);
    let opts = hp_mdl::LoadOptions { body_groups: hp_mdl::BodyGroupSelect::Body(body), ..Default::default() };
    let loaded = hp_mdl::load_from(&model_path, |p| fs.read(p), &opts)
        .map_err(|e| tf!("blender.model_bad", model_path.clone(), e.to_string()))?;
    let skin = int_key(entity, "skin").unwrap_or(0) as usize;
    let mut slots: HashMap<usize, usize> = HashMap::new();
    let mut materials: Vec<Material> = Vec::new();
    let mut triangles = Vec::new();
    for mesh in &loaded.meshes {
        let slot = *slots.entry(mesh.skin_ref).or_insert_with(|| {
            let candidates = loaded.material_candidates(mesh.skin_ref, skin);
            let name = candidates
                .iter()
                .find(|c| fs.exists(&format!("materials/{c}.vmt")))
                .or_else(|| candidates.first())
                .cloned()
                .unwrap_or_default();
            materials.push(Material::named(name));
            materials.len() - 1
        });
        for tri in mesh.indices.chunks_exact(3) {
            let Some(v) = tri.iter().map(|&i| mesh.vertices.get(i as usize)).collect::<Option<Vec<_>>>() else {
                continue;
            };
            let vec = |a: [f32; 3]| DVec3::new(f64::from(a[0]), f64::from(a[1]), f64::from(a[2]));
            let pos = [v[0], v[1], v[2]].map(|x| vec(x.pos));
            let normal = [v[0], v[1], v[2]].map(|x| vec(x.normal));
            let uv = [v[0], v[1], v[2]].map(|x| DVec2::new(f64::from(x.uv[0]), f64::from(x.uv[1])));
            triangles.push(Triangle { pos, normal, uv, material: slot });
        }
    }
    if triangles.is_empty() {
        return Err(t!("blender.empty").to_owned());
    }
    Ok(Model { triangles, materials, up: UpAxis::Z, meters_per_unit: 1.0 / UNITS_PER_METER, objects: Vec::new() })
}

pub fn selected_model_props(state: &AppState) -> Vec<u32> {
    let scene = state.doc.scene();
    state
        .doc
        .selection()
        .objects
        .iter()
        .filter_map(|o| match *o {
            ObjRef::Entity(id) => scene.entity(id).filter(|n| hp_app::actions::is_model_prop(&n.entity)).map(|_| id),
            ObjRef::Solid(_) => None,
        })
        .collect()
}

pub fn selected_solids(state: &AppState) -> Vec<u32> {
    let objs: Vec<ObjRef> = state.doc.selection().objects.iter().copied().collect();
    let mut ids = state.doc.scene().expand_solids(&objs);
    if ids.is_empty() {
        let mut seen = HashSet::new();
        ids = state.doc.selection().faces.iter().map(|f| f.solid).filter(|s| seen.insert(*s)).collect();
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub fn selection_source(state: &mut AppState) -> Result<Source, String> {
    let props = selected_model_props(state);
    if let Some(first) = props.first().copied() {
        let entity = state
            .doc
            .scene()
            .entity(first)
            .map(|n| n.entity.clone())
            .ok_or_else(|| t!("blender.nothing").to_owned())?;
        let Some(fs) = state.assets.fs().cloned() else {
            return Err(t!("blender.no_content").to_owned());
        };
        let mut model = prop_model(&fs, &entity)?;
        embed_textures(&fs, &mut model);
        let path = entity.get("model").unwrap_or_default().to_owned();
        let same: Vec<u32> = props
            .iter()
            .copied()
            .filter(|id| {
                state.doc.scene().entity(*id).is_some_and(|n| {
                    n.entity
                        .get("model")
                        .unwrap_or_default()
                        .replace('\\', "/")
                        .eq_ignore_ascii_case(&path.replace('\\', "/"))
                })
            })
            .collect();
        let name = blender_model_name(&path);
        return Ok(Source {
            model,
            target: BlenderTarget::Prop { props: same, model: path, name: name.clone() },
            stem: name,
        });
    }
    let disp_faces = hp_tools_mesh::disp::commands::targets(&state.doc);
    if !disp_faces.is_empty() {
        if disp_faces.len() > MAX_DISPLACEMENTS {
            return Err(tf!("blender.too_many_disps", disp_faces.len(), MAX_DISPLACEMENTS));
        }
        let wanted: HashSet<(u32, usize)> = disp_faces.iter().map(|f| (f.solid, f.side)).collect();
        let input = ObjInput::build(state, Some(&wanted), ObjLayout::default());
        let (mut model, sent) = input.to_displacement_model();
        if model.triangles.is_empty() {
            return Err(t!("blender.empty").to_owned());
        }
        if let Some(fs) = state.assets.fs().cloned() {
            embed_textures(&fs, &mut model);
        }
        return Ok(Source {
            model,
            target: BlenderTarget::Displacements { faces: sent },
            stem: "hammerpro_displacements".to_owned(),
        });
    }
    let solids = selected_solids(state);
    if solids.is_empty() {
        return Err(t!("blender.nothing").to_owned());
    }
    let mut wanted: HashSet<(u32, usize)> = HashSet::new();
    let faces: HashSet<(u32, usize)> = if state.doc.selection().objects.is_empty() {
        state.doc.selection().faces.iter().map(|f| (f.solid, f.side)).collect()
    } else {
        HashSet::new()
    };
    for id in &solids {
        let Some(node) = state.doc.scene().solid(*id) else { continue };
        for side in 0..node.solid.sides.len() {
            if faces.is_empty() || faces.contains(&(*id, side)) {
                wanted.insert((*id, side));
            }
        }
    }
    let input = ObjInput::build(state, Some(&wanted), ObjLayout::default());
    let mut model = input.to_model();
    if model.triangles.is_empty() {
        return Err(t!("blender.empty").to_owned());
    }
    if let Some(fs) = state.assets.fs().cloned() {
        embed_textures(&fs, &mut model);
    }
    Ok(Source { model, target: BlenderTarget::Brushes { solids }, stem: "hammerpro_brushes".to_owned() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_of_blender_models() {
        assert_eq!(model_stem("models/props_c17/Chair01a.mdl"), "Chair01a");
        assert_eq!(model_stem("models\\a\\b.mdl"), "b");
        assert_eq!(blender_model_name("models/props_c17/chair01a.mdl"), "chair01a_blender");
        assert_eq!(blender_model_name("models/hammerpro/my_chair.mdl"), "my_chair");
        assert_eq!(blender_model_name("models/hammerpro/Weird Name!.mdl"), "weird_name");
    }

    #[test]
    fn png_bytes_start_with_the_signature() {
        let png = png_of(2, 2, &[255; 16]).expect("png");
        assert_eq!(&png[..4], b"\x89PNG");
        assert!(png_of(2, 2, &[255; 4]).is_none());
    }

    #[test]
    fn selected_brushes_become_a_model() {
        let mut state = hp_app::testing::test_state();
        let ids: Vec<ObjRef> = state.doc.scene().solids().keys().take(2).map(|id| ObjRef::Solid(*id)).collect();
        assert!(!ids.is_empty());
        state.doc.select(&ids, hp_editor::SelectOp::Replace);
        let source = selection_source(&mut state).expect("source");
        assert!(!source.model.triangles.is_empty());
        let BlenderTarget::Brushes { solids } = &source.target else { panic!("brushes") };
        assert_eq!(solids.len(), ids.len());
        let (lo, hi) = source.model.bounds().expect("bounds");
        assert!(hi.x > lo.x && hi.z > lo.z);
        state.doc.select(&[], hp_editor::SelectOp::Replace);
        assert!(selection_source(&mut state).is_err());
    }
}
