use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::DVec3;
use hp_app::AppState;
use hp_editor::mesh_disp::Tri;
use hp_meshio::{ImportOptions, Model, UpAxis};

pub(crate) const MESH_EXTENSIONS: [&str; 4] = ["obj", "fbx", "gltf", "glb"];
pub(crate) const MAX_TRIANGLES: usize = 3_000_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum UpChoice {
    #[default]
    File,
    Y,
    Z,
}

impl UpChoice {
    pub(crate) fn id(self) -> &'static str {
        match self {
            UpChoice::File => "file",
            UpChoice::Y => "y",
            UpChoice::Z => "z",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        [UpChoice::File, UpChoice::Y, UpChoice::Z].into_iter().find(|u| u.id().eq_ignore_ascii_case(s.trim()))
    }

    fn axis(self) -> Option<UpAxis> {
        match self {
            UpChoice::File => None,
            UpChoice::Y => Some(UpAxis::Y),
            UpChoice::Z => Some(UpAxis::Z),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Kind {
    File(Arc<Model>),
    World(Arc<Vec<Tri>>),
}

#[derive(Clone, Debug)]
pub(crate) struct SourceMesh {
    pub label: String,
    pub kind: Kind,
    pub triangles: usize,
}

impl SourceMesh {
    pub(crate) fn is_file(&self) -> bool {
        matches!(self.kind, Kind::File(_))
    }

    pub(crate) fn tris(&self, up: UpChoice, convert_units: bool) -> Vec<Tri> {
        match &self.kind {
            Kind::World(t) => t.as_ref().clone(),
            Kind::File(model) => {
                let opts = ImportOptions { up: up.axis(), scale: 1.0, convert_units, center: false };
                model.to_source(&opts).triangles.iter().map(|t| t.pos).collect()
            }
        }
    }

    pub(crate) fn size(&self, up: UpChoice, convert_units: bool) -> DVec3 {
        let tris = self.tris(up, convert_units);
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        for p in tris.iter().flatten() {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        if lo.is_finite() && hi.is_finite() { hi - lo } else { DVec3::ZERO }
    }
}

pub(crate) fn is_mesh_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| MESH_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

pub(crate) fn read_file(path: &Path) -> Result<SourceMesh, String> {
    if !is_mesh_file(path) {
        return Err(format!("{} is not an OBJ, FBX, glTF or GLB file", path.display()));
    }
    let model = hp_meshio::read(path).map_err(|e| e.to_string())?;
    if model.triangles.len() > MAX_TRIANGLES {
        return Err(format!("{} triangles are too many (at most {MAX_TRIANGLES})", model.triangles.len()));
    }
    let label = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    let triangles = model.triangles.len();
    Ok(SourceMesh { label, kind: Kind::File(Arc::new(model)), triangles })
}

fn from_entity(state: &AppState, entity: &hp_vmf::Entity) -> Result<SourceMesh, String> {
    let Some(fs) = state.assets.fs() else {
        return Err(hp_app::t!("propper.no_content").to_owned());
    };
    let source = hp_ui_mapping::propper::read_prop_source(fs, entity)?;
    let tris: Vec<Tri> = source.tris.iter().map(|t| t.pos).collect();
    if tris.is_empty() {
        return Err(hp_meshio::MeshError::Empty.to_string());
    }
    if tris.len() > MAX_TRIANGLES {
        return Err(format!("{} triangles are too many (at most {MAX_TRIANGLES})", tris.len()));
    }
    let triangles = tris.len();
    Ok(SourceMesh { label: source.model, kind: Kind::World(Arc::new(tris)), triangles })
}

pub(crate) fn from_prop(state: &AppState, id: u32) -> Result<SourceMesh, String> {
    match state.doc.scene().entity(id) {
        Some(n) if hp_app::actions::is_model_prop(&n.entity) => from_entity(state, &n.entity),
        Some(_) => Err(format!("entity {id} is not a model prop")),
        None => Err(format!("no entity has the id {id}")),
    }
}

pub(crate) fn from_model(state: &AppState, model: &str) -> Result<SourceMesh, String> {
    let model = model.trim().replace('\\', "/");
    if model.is_empty() || !model.to_ascii_lowercase().ends_with(".mdl") || model.contains("..") || model.len() > 255 {
        return Err("the model must be a path ending in .mdl, like models/props_c17/oildrum001.mdl".to_owned());
    }
    let mut entity = hp_vmf::Entity { classname: "prop_static".to_owned(), ..hp_vmf::Entity::default() };
    entity.set("model", model);
    from_entity(state, &entity)
}

pub(crate) fn selected_prop(state: &AppState) -> Option<u32> {
    state.doc.selected_objects().into_iter().find_map(|o| match o {
        hp_editor::ObjRef::Entity(id)
            if state.doc.scene().entity(id).is_some_and(|n| hp_app::actions::is_model_prop(&n.entity)) =>
        {
            Some(id)
        }
        _ => None,
    })
}

pub(crate) fn resolve_path(state: &AppState, given: &str) -> Result<PathBuf, String> {
    let path = Path::new(given.trim());
    if given.trim().is_empty() || given.contains('\0') {
        return Err("path is empty".to_owned());
    }
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("path must not contain ..".to_owned());
    }
    let dir = state
        .doc
        .path()
        .or_else(|| state.doc.opened_from())
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .ok_or_else(|| "save the map first or give a full path".to_owned())?;
    Ok(dir.join(path))
}
