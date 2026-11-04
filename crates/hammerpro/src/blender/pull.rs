use std::path::PathBuf;

use glam::DVec3;
use hp_app::runtime::{BlenderTarget, DispSent};
use hp_app::settings::UnpropShape;
use hp_app::{AppState, Severity, t, tf};
use hp_editor::mesh_disp::{AxisChoice, ConformOptions, MeshDispOptions, Origin};
use hp_editor::{FaceRef, ObjRef, Owner};
use hp_ui_entities::model_import::batch::{ConvertOptions, Converted, convert_model};
use hp_ui_entities::model_import::{clean_name, content_root, lod_options, remount_after_write};
use hp_ui_mapping::propper::{PropBrushes, PropSource, build_brushes};

use super::disp;

const MAX_KNOWN: usize = 4096;

pub struct PullJob {
    pub file: PathBuf,
    pub target: BlenderTarget,
    pub root: Option<PathBuf>,
    pub fs: Option<hp_fs::GameFs>,
    pub convert: ConvertOptions,
    pub stem: String,
}

pub enum Outcome {
    Prop { converted: Box<Converted>, name: String },
    Brushes { made: Box<PropBrushes> },
    BrushesAsProp { converted: Box<Converted>, reason: String },
    Displacements { plan: Box<disp::Plan> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub kind: &'static str,
    pub text: String,
    pub model: Option<String>,
    pub solids: Vec<u32>,
    pub props: Vec<u32>,
    pub warnings: Vec<String>,
}

pub fn job_of(state: &AppState) -> Result<PullJob, String> {
    let link = state.runtime.blender.link.clone().ok_or_else(|| t!("blender.no_link").to_owned())?;
    let mut convert = ConvertOptions {
        lod: lod_options(state.settings.lab.model_lod),
        keep_params: state.settings.lab.model_keep_params,
        ..ConvertOptions::default()
    };
    convert.import.center = false;
    if let BlenderTarget::Prop { props, .. } = &link.target {
        let scene = state.doc.scene();
        let any_physics = props
            .iter()
            .filter_map(|id| scene.entity(*id))
            .any(|n| !n.entity.classname.eq_ignore_ascii_case("prop_static"));
        convert.static_prop = !any_physics;
    }
    let stem = link.file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(PullJob {
        file: link.file,
        target: link.target,
        root: content_root(state),
        fs: state.assets.fs().cloned(),
        convert,
        stem,
    })
}

pub fn clean_material(name: &str) -> String {
    let trimmed = name.trim();
    if let Some((head, tail)) = trimmed.rsplit_once('.')
        && tail.len() == 3
        && tail.chars().all(|c| c.is_ascii_digit())
        && !head.is_empty()
    {
        return head.to_owned();
    }
    trimmed.to_owned()
}

fn brush_source(model: &hp_meshio::Model, fs: Option<&hp_fs::GameFs>) -> PropSource {
    let tris: Vec<hp_editor::unprop::ModelTri> = model
        .triangles
        .iter()
        .map(|t| hp_editor::unprop::ModelTri {
            material: clean_material(model.materials.get(t.material).map_or("", |m| m.name.as_str())),
            pos: t.pos,
            uv: t.uv,
        })
        .collect();
    let mut sizes = std::collections::HashMap::new();
    for t in &tris {
        if !sizes.contains_key(&t.material) {
            let size = fs.and_then(|f| f.material_texture_size(&t.material)).filter(|s| s.0 > 0 && s.1 > 0);
            sizes.insert(t.material.clone(), size.unwrap_or((512, 512)));
        }
    }
    PropSource { model: String::new(), tris, collision: Vec::new(), sizes }
}

pub fn compute(job: PullJob) -> Result<Outcome, String> {
    let model = hp_meshio::read(&job.file).map_err(|e| tf!("blender.read_failed", e.to_string()))?;
    match &job.target {
        BlenderTarget::Prop { name, .. } => {
            let root = job.root.clone().ok_or_else(|| t!("blender.no_content").to_owned())?;
            let converted = convert_model(&model, name, &job.convert, &root)?;
            Ok(Outcome::Prop { converted: Box::new(converted), name: name.clone() })
        }
        BlenderTarget::Displacements { faces } => {
            let source = model.to_source(&job.convert.import);
            Ok(Outcome::Displacements { plan: Box::new(disp::plan(&source, faces)) })
        }
        BlenderTarget::Brushes { .. } => {
            let source = model.to_source(&job.convert.import);
            let pieces = brush_source(&source, job.fs.as_ref());
            match build_brushes(&pieces, UnpropShape::Render) {
                Ok(made) => Ok(Outcome::Brushes { made: Box::new(made) }),
                Err(reason) => {
                    let root = job.root.clone().ok_or_else(|| reason.clone())?;
                    let name = clean_name(&format!("blender_{}", job.stem));
                    let converted = convert_model(&model, &name, &job.convert, &root)?;
                    Ok(Outcome::BrushesAsProp { converted: Box::new(converted), reason })
                }
            }
        }
    }
}

fn existing_solids(state: &AppState, ids: &[u32]) -> Vec<u32> {
    ids.iter().copied().filter(|id| state.doc.scene().solid(*id).is_some()).collect()
}

fn common_owner(state: &AppState, ids: &[u32]) -> Owner {
    let scene = state.doc.scene();
    let mut owners = ids.iter().filter_map(|id| scene.solid(*id)).map(|n| n.owner);
    match owners.next() {
        Some(first @ Owner::Entity(_)) if owners.all(|o| o == first) => first,
        _ => Owner::World,
    }
}

fn set_link_target(state: &mut AppState, target: BlenderTarget) {
    if let Some(link) = state.runtime.blender.link.as_mut() {
        link.target = target;
        link.pulls += 1;
    }
}

pub fn apply(state: &mut AppState, outcome: Outcome) -> Result<Report, String> {
    let target = state
        .runtime
        .blender
        .link
        .as_ref()
        .map(|l| l.target.clone())
        .ok_or_else(|| t!("blender.no_link").to_owned())?;
    match (outcome, target) {
        (Outcome::Prop { converted, name }, BlenderTarget::Prop { props, .. }) => {
            remount_after_write(state);
            let live: Vec<u32> = props.iter().copied().filter(|id| state.doc.scene().entity(*id).is_some()).collect();
            let model = converted.model.clone();
            if !live.is_empty() {
                state
                    .doc
                    .transaction("Update from Blender", |doc| doc.set_entity_key(&live, "model", &model))
                    .map_err(|e| e.to_string())?;
            }
            set_link_target(state, BlenderTarget::Prop { props: live.clone(), model: model.clone(), name });
            let text = tf!("blender.pulled_prop", model.clone(), live.len(), converted.triangles);
            state.notify(Severity::Success, text.clone());
            Ok(Report {
                kind: "prop",
                text,
                model: Some(model),
                solids: Vec::new(),
                props: live,
                warnings: converted.warnings.clone(),
            })
        }
        (Outcome::Brushes { made }, BlenderTarget::Brushes { solids }) => {
            let old = existing_solids(state, &solids);
            let owner = common_owner(state, &old);
            let new_sets = made.brushes.solids.clone();
            let refs: Vec<ObjRef> = old.iter().map(|id| ObjRef::Solid(*id)).collect();
            let ids = state
                .doc
                .transaction("Update from Blender", |doc| {
                    let ids = doc.create_solids(new_sets, owner)?;
                    if owner == Owner::World && ids.len() > 1 {
                        let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Solid(*id)).collect();
                        doc.group(&objs)?;
                    }
                    if !refs.is_empty() {
                        doc.delete(&refs)?;
                    }
                    let select: Vec<ObjRef> = match owner {
                        Owner::World => ids.iter().map(|id| ObjRef::Solid(*id)).collect(),
                        Owner::Entity(e) => vec![ObjRef::Entity(e)],
                    };
                    doc.select(&select, hp_editor::SelectOp::Replace);
                    Ok::<Vec<u32>, hp_editor::EditError>(ids)
                })
                .map_err(|e| e.to_string())?;
            let mut known = solids.clone();
            known.extend(ids.iter().copied());
            known.sort_unstable();
            known.dedup();
            let skip = known.len().saturating_sub(MAX_KNOWN);
            known.drain(..skip);
            set_link_target(state, BlenderTarget::Brushes { solids: known });
            let text = tf!("blender.pulled_brushes", ids.len(), made.brushes.textured_faces, old.len());
            state.notify(Severity::Success, text.clone());
            let mut warnings = Vec::new();
            if made.approximate > 0 {
                warnings.push(tf!("propper.approximate", made.approximate));
            }
            if made.brushes.skipped > 0 {
                warnings.push(tf!("propper.skipped", made.brushes.skipped));
            }
            Ok(Report { kind: "brushes", text, model: None, solids: ids, props: Vec::new(), warnings })
        }
        (Outcome::BrushesAsProp { converted, reason }, BlenderTarget::Brushes { solids }) => {
            remount_after_write(state);
            let old = existing_solids(state, &solids);
            let refs: Vec<ObjRef> = old.iter().map(|id| ObjRef::Solid(*id)).collect();
            let model = converted.model.clone();
            let kv = vec![("model".to_owned(), model.clone()), ("solid".to_owned(), "6".to_owned())];
            let id = state
                .doc
                .transaction("Update from Blender", |doc| {
                    if !refs.is_empty() {
                        doc.delete(&refs)?;
                    }
                    doc.create_point_entity("prop_static", DVec3::ZERO, &kv)
                })
                .map_err(|e| e.to_string())?;
            let name = converted.model.rsplit('/').next().unwrap_or("").trim_end_matches(".mdl").to_owned();
            set_link_target(state, BlenderTarget::Prop { props: vec![id], model: model.clone(), name });
            let text = tf!("blender.pulled_as_prop", model.clone(), reason);
            state.notify(Severity::Warning, text.clone());
            Ok(Report {
                kind: "prop",
                text,
                model: Some(model),
                solids: Vec::new(),
                props: vec![id],
                warnings: converted.warnings.clone(),
            })
        }
        (Outcome::Displacements { plan }, BlenderTarget::Displacements { faces }) => {
            apply_displacements(state, *plan, faces)
        }
        _ => Err(t!("blender.link_changed").to_owned()),
    }
}

fn current_positions(state: &AppState, solid: u32, side: u32) -> Vec<[f32; 3]> {
    state
        .doc
        .scene()
        .solid(solid)
        .and_then(|n| {
            n.disp_mesh(side as usize)
                .map(|m| m.positions.iter().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect())
        })
        .unwrap_or_default()
}

fn apply_displacements(state: &mut AppState, plan: disp::Plan, sent: Vec<DispSent>) -> Result<Report, String> {
    let mut infos: Vec<(FaceRef, hp_vmf::DispInfo)> = Vec::new();
    let mut moved = 0usize;
    let mut skipped = 0usize;
    let mut next_sent = sent.clone();
    let mut touched: Vec<FaceRef> = Vec::new();
    for u in &plan.updates {
        let face = FaceRef::new(u.solid, u.side as usize);
        let Some(node) = state.doc.scene().solid(u.solid) else {
            skipped += 1;
            continue;
        };
        let (Some(mesh), Some(old)) =
            (node.disp_mesh(u.side as usize), node.solid.sides.get(u.side as usize).and_then(|s| s.disp.as_deref()))
        else {
            skipped += 1;
            continue;
        };
        let Some(snapshot) = sent.iter().find(|s| s.solid == u.solid && s.side == u.side) else {
            skipped += 1;
            continue;
        };
        let Some((mut changed, count)) =
            disp::rebuild(old, &mesh.base_positions, mesh.face_normal, &snapshot.positions, &u.positions)
        else {
            skipped += 1;
            continue;
        };
        if let Some(slot) = next_sent.iter_mut().find(|s| s.solid == u.solid && s.side == u.side) {
            slot.positions = u.positions.iter().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect();
        }
        touched.push(face);
        if count == 0 {
            continue;
        }
        if let Some(poly) = node.geom.face_for_side(u.side as usize).map(|f| f.vertices.clone()) {
            hp_geom::update_disp_tags(&mut changed, &poly);
        }
        moved += count;
        infos.push((face, changed));
    }
    let updated = infos.len();
    let conform_faces: Vec<FaceRef> = plan
        .conform
        .iter()
        .filter(|(s, side)| state.doc.scene().solid(*s).and_then(|n| n.disp_mesh(*side as usize)).is_some())
        .map(|(s, side)| FaceRef::new(*s, *side as usize))
        .collect();
    skipped += plan.conform.len() - conform_faces.len();
    touched.extend(conform_faces.iter().copied());
    let before_objects = state.doc.selected_objects();
    let before_faces = state.doc.selected_faces();
    let record = state.doc.records_selection();
    state.doc.set_record_selection(false);
    state.doc.begin("Update from Blender");
    let mut conformed = 0usize;
    let mut created = 0usize;
    let result: Result<(), String> = (|| {
        state.doc.set_disp_data("Update from Blender", infos).map_err(|e| e.to_string())?;
        if !conform_faces.is_empty() {
            let opts = ConformOptions { axis: AxisChoice::Best, origin: Origin::Keep, ..ConformOptions::default() };
            let r = state
                .doc
                .conform_displacements(&conform_faces, &plan.conform_tris, &opts)
                .map_err(|e| e.to_string())?;
            conformed = r.faces;
        }
        if !plan.created.is_empty() {
            let opts = MeshDispOptions {
                origin: Origin::Keep,
                material: state.current_material.clone(),
                ..MeshDispOptions::default()
            };
            let tiles = hp_editor::mesh_disp::plan(&plan.created, &opts).map_err(|e| e.to_string())?;
            let r = state.doc.create_mesh_displacements(&tiles, &opts).map_err(|e| e.to_string())?;
            created = r.tiles;
        }
        if touched.len() >= 2 {
            state.doc.clear_selection();
            state.doc.select_faces(&touched, hp_editor::SelectOp::Replace);
            hp_tools_mesh::DisplacementTool::new().sew(state);
        }
        Ok(())
    })();
    match &result {
        Ok(()) => {
            state.doc.commit();
        }
        Err(_) => state.doc.cancel(),
    }
    state.doc.clear_selection();
    state.doc.select(&before_objects, hp_editor::SelectOp::Replace);
    state.doc.select_faces(&before_faces, hp_editor::SelectOp::Add);
    state.doc.set_record_selection(record);
    result?;
    for f in &conform_faces {
        if let Some(slot) = next_sent.iter_mut().find(|s| s.solid == f.solid && s.side as usize == f.side) {
            slot.positions = current_positions(state, f.solid, f.side as u32);
        }
    }
    if let Some(link) = state.runtime.blender.link.as_mut() {
        link.target = BlenderTarget::Displacements { faces: next_sent };
        link.pulls += 1;
    }
    let text = tf!("blender.pulled_disp", updated, moved, conformed, created, skipped);
    state.notify(Severity::Success, text.clone());
    Ok(Report { kind: "displacements", text, model: None, solids: Vec::new(), props: Vec::new(), warnings: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blender_duplicate_suffixes_are_removed_from_material_names() {
        assert_eq!(clean_material("dev/wall.001"), "dev/wall");
        assert_eq!(clean_material("dev/wall.12"), "dev/wall.12");
        assert_eq!(clean_material(" brick "), "brick");
        assert_eq!(clean_material(".001"), ".001");
        assert_eq!(clean_material("a.b.002"), "a.b");
    }

    fn cube(size: f64) -> hp_meshio::Model {
        use glam::{DVec2, DVec3};
        let h = size / 2.0;
        let corner = |i: usize| {
            DVec3::new(
                if i & 1 == 0 { -h } else { h },
                if i & 2 == 0 { -h } else { h },
                if i & 4 == 0 { 0.0 } else { size },
            )
        };
        let quads: [([usize; 4], DVec3); 6] = [
            ([0, 2, 6, 4], -DVec3::X),
            ([1, 5, 7, 3], DVec3::X),
            ([0, 4, 5, 1], -DVec3::Y),
            ([2, 3, 7, 6], DVec3::Y),
            ([0, 1, 3, 2], -DVec3::Z),
            ([4, 6, 7, 5], DVec3::Z),
        ];
        let mut triangles = Vec::new();
        for (q, n) in quads {
            for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                triangles.push(hp_meshio::Triangle {
                    pos: t.map(corner),
                    normal: [n; 3],
                    uv: [DVec2::ZERO, DVec2::X, DVec2::ONE],
                    material: 0,
                });
            }
        }
        hp_meshio::Model {
            triangles,
            materials: vec![hp_meshio::Material::named("dev/wall.001")],
            up: hp_meshio::UpAxis::Z,
            meters_per_unit: 1.0 / hp_meshio::UNITS_PER_METER,
            objects: Vec::new(),
        }
    }

    #[test]
    fn a_pull_replaces_the_linked_brushes_in_one_undo_step() {
        use hp_app::runtime::BlenderLink;
        let mut state = hp_app::testing::test_state();
        let first = *state.doc.scene().solids().keys().next().expect("a solid");
        state.runtime.blender.link = Some(BlenderLink {
            file: PathBuf::from("x.glb"),
            format: hp_app::settings::BlenderFormat::Gltf,
            target: BlenderTarget::Brushes { solids: vec![first] },
            baseline: None,
            candidate: None,
            pulls: 0,
            live: true,
        });
        let source = brush_source(&cube(128.0), None);
        assert_eq!(source.tris[0].material, "dev/wall");
        let made = build_brushes(&source, UnpropShape::Render).expect("brushes");
        let before = state.doc.scene().solids().len();
        let steps = state.doc.history_len();
        let report = apply(&mut state, Outcome::Brushes { made: Box::new(made) }).expect("apply");
        assert_eq!(report.kind, "brushes");
        assert!(state.doc.scene().solid(first).is_none(), "the old brush is gone");
        assert!(!report.solids.is_empty());
        assert_eq!(state.doc.scene().solids().len(), before - 1 + report.solids.len());
        assert_eq!(state.doc.history_len().0, steps.0 + 1, "one undo step");
        assert!(state.doc.undo());
        assert!(state.doc.scene().solid(first).is_some(), "undo brings it back");
        assert_eq!(state.doc.scene().solids().len(), before);
        let link = state.runtime.blender.link.clone().expect("link");
        let BlenderTarget::Brushes { solids } = link.target else { panic!("brushes") };
        assert!(solids.contains(&first) && report.solids.iter().all(|id| solids.contains(id)));
        assert_eq!(link.pulls, 1);
        let made = build_brushes(&source, UnpropShape::Render).expect("brushes");
        apply(&mut state, Outcome::Brushes { made: Box::new(made) }).expect("second pull");
        assert!(state.doc.scene().solid(first).is_none(), "a pull after an undo replaces it again");
    }

    #[test]
    fn a_pull_without_a_link_or_with_another_kind_is_refused() {
        let mut state = hp_app::testing::test_state();
        let source = brush_source(&cube(64.0), None);
        let made = build_brushes(&source, UnpropShape::Render).expect("brushes");
        assert!(apply(&mut state, Outcome::Brushes { made: Box::new(made) }).is_err());
        assert!(job_of(&state).is_err());
    }
}
