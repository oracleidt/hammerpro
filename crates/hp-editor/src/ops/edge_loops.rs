use std::collections::HashSet;

use glam::DVec3;
use hp_geom::{EdgeNet, EdgeRun, NET_EPS, NetSolid};

use super::bevel::EdgeRef;
use crate::document::Document;
use crate::{EditError, EditResult, Id};

pub const MAX_LOOP_SOLIDS: usize = 256;

pub type LoopPart = (Id, Vec<DVec3>, Vec<Vec<usize>>);

#[derive(Clone, Debug)]
pub struct LoopScope {
    pub ids: Vec<Id>,
    pub net: EdgeNet,
    pub vmap: Vec<Vec<usize>>,
    locals: Vec<Vec<DVec3>>,
    solid_edges: Vec<HashSet<(usize, usize)>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunKind {
    Loop,
    Ring,
}

impl LoopScope {
    pub fn from_parts(parts: &[LoopPart]) -> Self {
        let solids: Vec<NetSolid<'_>> = parts.iter().map(|(_, p, f)| NetSolid { points: p, faces: f }).collect();
        let (net, vmap) = EdgeNet::merged(&solids, NET_EPS);
        let mut solid_edges = Vec::with_capacity(parts.len());
        for (si, (_, _, faces)) in parts.iter().enumerate() {
            let mut set = HashSet::new();
            if let Some(map) = vmap.get(si) {
                for ring in faces {
                    let n = ring.len();
                    for i in 0..n {
                        let (Some(&a), Some(&b)) = (ring.get(i), ring.get((i + 1) % n)) else { continue };
                        if let (Some(&ga), Some(&gb)) = (map.get(a), map.get(b))
                            && ga != gb
                        {
                            set.insert((ga.min(gb), ga.max(gb)));
                        }
                    }
                }
            }
            solid_edges.push(set);
        }
        Self {
            ids: parts.iter().map(|(id, _, _)| *id).collect(),
            net,
            vmap,
            locals: parts.iter().map(|(_, p, _)| p.clone()).collect(),
            solid_edges,
        }
    }

    pub fn global_vertex(&self, solid: usize, local: usize) -> Option<usize> {
        self.vmap.get(solid)?.get(local).copied()
    }

    pub fn vertex_at(&self, p: DVec3) -> Option<usize> {
        self.net
            .points()
            .iter()
            .enumerate()
            .filter(|(_, q)| q.distance(p) <= NET_EPS * 2.0)
            .min_by(|a, b| a.1.distance(p).total_cmp(&b.1.distance(p)))
            .map(|(i, _)| i)
    }

    pub fn edge_at(&self, a: DVec3, b: DVec3) -> Option<usize> {
        let (ga, gb) = (self.vertex_at(a)?, self.vertex_at(b)?);
        self.net.edge_between(ga, gb)
    }

    pub fn run(&self, kind: RunKind, seed: usize, toward: Option<DVec3>) -> EdgeRun {
        match kind {
            RunKind::Loop => self.net.edge_loop(seed, toward),
            RunKind::Ring => self.net.edge_ring(seed),
        }
    }

    pub fn refs(&self, edges: &[usize]) -> Vec<EdgeRef> {
        let mut out = Vec::new();
        for &e in edges {
            let Some(&[ga, gb]) = self.net.edges().get(e) else { continue };
            let k = (ga.min(gb), ga.max(gb));
            for (si, set) in self.solid_edges.iter().enumerate() {
                if !set.contains(&k) {
                    continue;
                }
                let local = |g: usize| {
                    let at = self.vmap.get(si)?.iter().position(|x| *x == g)?;
                    self.locals.get(si)?.get(at).copied()
                };
                if let (Some(&id), Some(a), Some(b)) = (self.ids.get(si), local(ga), local(gb)) {
                    out.push(EdgeRef::new(id, a, b));
                }
            }
        }
        out
    }
}

impl Document {
    pub fn loop_scope(&self, ids: &[Id]) -> LoopScope {
        let parts: Vec<(Id, Vec<DVec3>, Vec<Vec<usize>>)> = ids
            .iter()
            .filter(|id| self.is_solid_visible(**id))
            .filter_map(|id| self.solid_vertices(*id).map(|(p, f)| (*id, p, f)))
            .collect();
        LoopScope::from_parts(&parts)
    }

    pub fn loop_pool(&self, seed: Id, selected: &[Id]) -> Vec<Id> {
        let mut pool: Vec<Id> = vec![seed];
        for id in selected {
            if !pool.contains(id) {
                pool.push(*id);
            }
        }
        let mut next = 0;
        while next < pool.len() && pool.len() < MAX_LOOP_SOLIDS {
            let id = pool[next];
            next += 1;
            for n in self.bevel_neighbors(id) {
                if pool.len() >= MAX_LOOP_SOLIDS {
                    break;
                }
                if !pool.contains(&n) && self.is_solid_visible(n) {
                    pool.push(n);
                }
            }
        }
        pool
    }

    pub fn edge_run_refs(
        &self,
        kind: RunKind,
        seed: &EdgeRef,
        selected: &[Id],
        toward: Option<DVec3>,
    ) -> EditResult<Vec<EdgeRef>> {
        let pool = self.loop_pool(seed.solid, selected);
        let scope = self.loop_scope(&pool);
        let e = scope
            .edge_at(seed.a, seed.b)
            .ok_or_else(|| EditError::Invalid("the edge is not an edge of the solid".to_owned()))?;
        let run = scope.run(kind, e, toward);
        Ok(scope.refs(&run.edges))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_geom::PrimitiveParams;

    fn cube(doc: &mut Document, min: DVec3, max: DVec3) -> Id {
        let params = PrimitiveParams { bounds: hp_geom::Aabb::new(min, max), ..Default::default() };
        doc.create_primitive(&params, crate::scene::Owner::World).unwrap()[0]
    }

    #[test]
    fn a_ring_of_boxes_gives_one_loop_across_the_brushes() {
        let mut doc = Document::new_empty();
        let a = cube(&mut doc, DVec3::ZERO, DVec3::new(64.0, 64.0, 64.0));
        let b = cube(&mut doc, DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 64.0));
        let pool = doc.loop_pool(a, &[]);
        assert_eq!(pool.len(), 2);
        let seed = EdgeRef::new(a, DVec3::new(0.0, 0.0, 64.0), DVec3::new(64.0, 0.0, 64.0));
        let refs = doc.edge_run_refs(RunKind::Loop, &seed, &[], Some(DVec3::Z)).unwrap();
        let on_a = refs.iter().filter(|r| r.solid == a).count();
        let on_b = refs.iter().filter(|r| r.solid == b).count();
        assert_eq!(on_a, 3);
        assert_eq!(on_b, 3);
        let ring = doc.edge_run_refs(RunKind::Ring, &seed, &[], None).unwrap();
        assert_eq!(ring.len(), 4);
        assert!(ring.iter().all(|r| r.solid == a));
    }
}
