use glam::DVec3;
use hp_vmf::{DispInfo, Entity, Side, Solid, TexAxis};
use rayon::prelude::*;

use super::{Snapshot, alloc_bytes, string_bytes, vec_bytes};
use crate::Id;
use crate::ids::IdMap;
use crate::scene::{EntityNode, SolidNode};

const PAR_MIN: usize = 64;

#[derive(Debug)]
pub(crate) struct DispFields {
    start_position: DVec3,
    elevation: f64,
    normals: Option<Vec<DVec3>>,
    distances: Option<Vec<f64>>,
    offsets: Option<Vec<DVec3>>,
    offset_normals: Option<Vec<DVec3>>,
    alphas: Option<Vec<f64>>,
}

#[derive(Debug)]
pub(crate) struct SideFields {
    plane: [DVec3; 3],
    uaxis: TexAxis,
    vaxis: TexAxis,
    rotation: f64,
    lightmap_scale: i32,
    smoothing_groups: u32,
    material: Option<Box<str>>,
    vertices_plus: Option<Box<Option<Vec<DVec3>>>>,
    disp: Option<Box<DispFields>>,
}

#[derive(Debug)]
pub(crate) enum SideDelta {
    Same,
    Fields(Box<SideFields>),
    Whole(Box<Side>),
}

#[derive(Debug)]
pub(crate) struct SolidPatch {
    sides: Vec<SideDelta>,
}

#[derive(Debug)]
pub(crate) struct EntityPatch {
    props: Vec<(u32, String)>,
}

fn changed<T: Clone + PartialEq>(old: &T, new: &T) -> Option<T> {
    (old != new).then(|| old.clone())
}

fn disp_fields(old: &DispInfo, new: &DispInfo) -> Option<DispFields> {
    let same_shape = old.power == new.power
        && old.flags == new.flags
        && old.subdiv == new.subdiv
        && old.triangle_tags == new.triangle_tags
        && old.allowed_verts == new.allowed_verts
        && old.extra_props == new.extra_props
        && old.extra_blocks == new.extra_blocks;
    same_shape.then(|| DispFields {
        start_position: old.start_position,
        elevation: old.elevation,
        normals: changed(&old.normals, &new.normals),
        distances: changed(&old.distances, &new.distances),
        offsets: changed(&old.offsets, &new.offsets),
        offset_normals: changed(&old.offset_normals, &new.offset_normals),
        alphas: changed(&old.alphas, &new.alphas),
    })
}

fn side_delta(old: &Side, new: &Side) -> SideDelta {
    if old == new {
        return SideDelta::Same;
    }
    let disp = match (old.disp.as_deref(), new.disp.as_deref()) {
        (None, None) => None,
        (Some(a), Some(b)) => match disp_fields(a, b) {
            Some(d) => Some(Box::new(d)),
            None => return SideDelta::Whole(Box::new(old.clone())),
        },
        _ => return SideDelta::Whole(Box::new(old.clone())),
    };
    if old.id != new.id || old.extra_props != new.extra_props || old.extra_blocks != new.extra_blocks {
        return SideDelta::Whole(Box::new(old.clone()));
    }
    SideDelta::Fields(Box::new(SideFields {
        plane: old.plane,
        uaxis: old.uaxis,
        vaxis: old.vaxis,
        rotation: old.rotation,
        lightmap_scale: old.lightmap_scale,
        smoothing_groups: old.smoothing_groups,
        material: (old.material != new.material).then(|| old.material.as_str().into()),
        vertices_plus: changed(&old.vertices_plus, &new.vertices_plus).map(Box::new),
        disp,
    }))
}

fn solid_patch(old: &SolidNode, new: &SolidNode) -> Option<SolidPatch> {
    let (a, b) = (&old.solid, &new.solid);
    if old.owner != new.owner
        || old.order != new.order
        || a.id != b.id
        || a.hidden != b.hidden
        || a.sides.len() != b.sides.len()
        || a.editor != b.editor
        || a.extra_props != b.extra_props
        || a.extra_blocks != b.extra_blocks
    {
        return None;
    }
    let sides: Vec<SideDelta> = a.sides.iter().zip(&b.sides).map(|(x, y)| side_delta(x, y)).collect();
    if sides.iter().all(|d| matches!(d, SideDelta::Whole(_))) {
        return None;
    }
    Some(SolidPatch { sides })
}

fn entity_patch(old: &EntityNode, new: &EntityNode) -> Option<EntityPatch> {
    let (a, b) = (&old.entity, &new.entity);
    if old.order != new.order
        || a.id != b.id
        || a.classname != b.classname
        || a.hidden != b.hidden
        || a.props.len() != b.props.len()
        || a.connections != b.connections
        || a.solids != b.solids
        || a.groups != b.groups
        || a.editor != b.editor
        || a.extra_blocks != b.extra_blocks
    {
        return None;
    }
    let mut props = Vec::new();
    for (i, ((ka, va), (kb, vb))) in a.props.iter().zip(&b.props).enumerate() {
        if ka != kb {
            return None;
        }
        if va != vb {
            props.push((i as u32, va.clone()));
        }
    }
    (props.len() * 2 <= a.props.len().max(1)).then_some(EntityPatch { props })
}

fn apply_disp(d: &mut DispInfo, f: &DispFields) {
    d.start_position = f.start_position;
    d.elevation = f.elevation;
    if let Some(v) = &f.normals {
        d.normals.clone_from(v);
    }
    if let Some(v) = &f.distances {
        d.distances.clone_from(v);
    }
    if let Some(v) = &f.offsets {
        d.offsets.clone_from(v);
    }
    if let Some(v) = &f.offset_normals {
        d.offset_normals.clone_from(v);
    }
    if let Some(v) = &f.alphas {
        d.alphas.clone_from(v);
    }
}

fn apply_solid(mut solid: Solid, patch: &SolidPatch) -> Solid {
    for (side, delta) in solid.sides.iter_mut().zip(&patch.sides) {
        match delta {
            SideDelta::Same => {}
            SideDelta::Whole(old) => side.clone_from(old),
            SideDelta::Fields(f) => {
                side.plane = f.plane;
                side.uaxis = f.uaxis;
                side.vaxis = f.vaxis;
                side.rotation = f.rotation;
                side.lightmap_scale = f.lightmap_scale;
                side.smoothing_groups = f.smoothing_groups;
                if let Some(m) = &f.material {
                    m.as_ref().clone_into(&mut side.material);
                }
                if let Some(v) = &f.vertices_plus {
                    side.vertices_plus.clone_from(v);
                }
                if let (Some(d), Some(fields)) = (side.disp.as_deref_mut(), &f.disp) {
                    apply_disp(d, fields);
                }
            }
        }
    }
    solid
}

fn apply_entity(mut entity: Entity, patch: &EntityPatch) -> Entity {
    for (i, value) in &patch.props {
        if let Some((_, v)) = entity.props.get_mut(*i as usize) {
            v.clone_from(value);
        }
    }
    entity
}

fn opt_vec_bytes<T>(v: &Option<Vec<T>>) -> usize {
    v.as_ref().map_or(0, vec_bytes)
}

fn disp_fields_bytes(f: &DispFields) -> usize {
    opt_vec_bytes(&f.normals)
        + opt_vec_bytes(&f.distances)
        + opt_vec_bytes(&f.offsets)
        + opt_vec_bytes(&f.offset_normals)
        + opt_vec_bytes(&f.alphas)
}

pub(crate) fn solid_patch_bytes(p: &SolidPatch) -> usize {
    vec_bytes(&p.sides)
        + p.sides
            .iter()
            .map(|d| match d {
                SideDelta::Same => 0,
                SideDelta::Whole(side) => alloc_bytes::<Side>(1) + super::side_bytes(side),
                SideDelta::Fields(f) => {
                    alloc_bytes::<SideFields>(1)
                        + f.material.as_ref().map_or(0, |m| alloc_bytes::<u8>(m.len()))
                        + f.vertices_plus
                            .as_ref()
                            .map_or(0, |v| alloc_bytes::<Option<Vec<DVec3>>>(1) + opt_vec_bytes(v))
                        + f.disp.as_ref().map_or(0, |d| alloc_bytes::<DispFields>(1) + disp_fields_bytes(d))
                }
            })
            .sum::<usize>()
}

pub(crate) fn entity_patch_bytes(p: &EntityPatch) -> usize {
    vec_bytes(&p.props) + p.props.iter().map(|(_, v)| string_bytes(v)).sum::<usize>()
}

pub(crate) struct Bases<'a> {
    pub solids: &'a (dyn Fn(Id) -> Option<&'a SolidNode> + Sync),
    pub entities: &'a (dyn Fn(Id) -> Option<&'a EntityNode> + Sync),
}

impl Snapshot {
    pub(crate) fn has_patches(&self) -> bool {
        !self.solid_patches.is_empty() || !self.entity_patches.is_empty()
    }

    pub(crate) fn compact(&mut self, bases: &Bases<'_>) -> Vec<SolidNode> {
        let solid_ids: Vec<Id> = self.solids.iter().filter(|(_, n)| n.is_some()).map(|(id, _)| *id).collect();
        let solids = &self.solids;
        let make = |id: &Id| -> Option<(Id, SolidPatch)> {
            let old = solids.get(id)?.as_ref()?;
            let base = (bases.solids)(*id)?;
            let patch = solid_patch(old, base)?;
            Some((*id, patch))
        };
        let patches: Vec<(Id, SolidPatch)> = if solid_ids.len() >= PAR_MIN {
            solid_ids.par_iter().filter_map(make).collect()
        } else {
            solid_ids.iter().filter_map(make).collect()
        };
        let mut dropped = Vec::with_capacity(patches.len());
        for (id, patch) in patches {
            if let Some(Some(node)) = self.solids.swap_remove(&id) {
                dropped.push(node);
            }
            self.solid_patches.insert(id, patch);
        }
        let entity_ids: Vec<Id> = self.entities.iter().filter(|(_, n)| n.is_some()).map(|(id, _)| *id).collect();
        for id in entity_ids {
            let Some(Some(old)) = self.entities.get(&id) else { continue };
            let Some(base) = (bases.entities)(id) else { continue };
            if let Some(patch) = entity_patch(old, base) {
                self.entities.swap_remove(&id);
                self.entity_patches.insert(id, patch);
            }
        }
        self.solids.shrink_to_fit();
        self.entities.shrink_to_fit();
        self.solid_patches.shrink_to_fit();
        self.entity_patches.shrink_to_fit();
        dropped
    }

    pub(crate) fn expand(&mut self, bases: &Bases<'_>) {
        let patches: Vec<(Id, SolidPatch)> = self.solid_patches.drain(..).collect();
        let build = |(id, patch): (Id, SolidPatch)| -> Option<(Id, SolidNode)> {
            let base = (bases.solids)(id)?;
            let solid = apply_solid(base.solid.clone(), &patch);
            Some((id, SolidNode::build(solid, base.owner, base.order)))
        };
        let nodes: Vec<(Id, SolidNode)> = if patches.len() >= PAR_MIN {
            patches.into_par_iter().filter_map(build).collect()
        } else {
            patches.into_iter().filter_map(build).collect()
        };
        for (id, node) in nodes {
            self.solids.insert(id, Some(node));
        }
        let patches: Vec<(Id, EntityPatch)> = self.entity_patches.drain(..).collect();
        for (id, patch) in patches {
            let Some(base) = (bases.entities)(id) else { continue };
            let entity = apply_entity(base.entity.clone(), &patch);
            self.entities.insert(id, Some(EntityNode { entity, order: base.order }));
        }
    }

    pub(crate) fn patch_bytes(&self) -> usize {
        super::map_bytes(&self.solid_patches)
            + self.solid_patches.values().map(solid_patch_bytes).sum::<usize>()
            + super::map_bytes(&self.entity_patches)
            + self.entity_patches.values().map(entity_patch_bytes).sum::<usize>()
    }
}

pub(crate) type SolidPatches = IdMap<SolidPatch>;
pub(crate) type EntityPatches = IdMap<EntityPatch>;
