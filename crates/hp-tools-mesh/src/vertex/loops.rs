use std::collections::HashSet;

use egui::Pos2;
use glam::DVec3;
use hp_app::ViewInfo;
use hp_editor::{Id, LoopScope};
use hp_geom::EdgeRun;

use super::model::EditSolid;
use super::segment_distance;

pub(crate) const LOOP_HIT_PX: f32 = 9.0;
const PICK_TIE_PX: f32 = 0.75;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeCmd {
    Loop,
    Ring,
    Grow,
    Shrink,
    Boundary,
    Set,
}

impl EdgeCmd {
    pub fn name(self) -> &'static str {
        match self {
            EdgeCmd::Loop => "loop",
            EdgeCmd::Ring => "ring",
            EdgeCmd::Grow => "grow",
            EdgeCmd::Shrink => "shrink",
            EdgeCmd::Boundary => "boundary",
            EdgeCmd::Set => "set",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text.trim().to_ascii_lowercase().as_str() {
            "loop" => EdgeCmd::Loop,
            "ring" => EdgeCmd::Ring,
            "grow" => EdgeCmd::Grow,
            "shrink" => EdgeCmd::Shrink,
            "boundary" => EdgeCmd::Boundary,
            "set" => EdgeCmd::Set,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EdgeSummary {
    pub edges: usize,
    pub vertices: usize,
    pub closed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeIssue {
    NothingSelected,
}

pub type Seed = (Id, DVec3, DVec3);

pub(crate) fn scope_of(solids: &[EditSolid]) -> LoopScope {
    let parts: Vec<(Id, Vec<DVec3>, Vec<Vec<usize>>)> =
        solids.iter().map(|s| (s.id, s.points.clone(), s.faces.clone())).collect();
    LoopScope::from_parts(&parts)
}

pub(crate) fn selection_in(scope: &LoopScope, solids: &[EditSolid]) -> (HashSet<usize>, HashSet<usize>) {
    let mut verts = HashSet::new();
    let mut edges = HashSet::new();
    for (si, s) in solids.iter().enumerate() {
        for (i, sel) in s.selected.iter().enumerate() {
            if *sel && let Some(g) = scope.global_vertex(si, i) {
                verts.insert(g);
            }
        }
        for (e, sel) in s.edge_selected.iter().enumerate() {
            if !*sel {
                continue;
            }
            let Some([a, b]) = s.edges.get(e).copied() else { continue };
            if let (Some(ga), Some(gb)) = (scope.global_vertex(si, a), scope.global_vertex(si, b))
                && let Some(ge) = scope.net.edge_between(ga, gb)
            {
                edges.insert(ge);
            }
        }
    }
    (verts, edges)
}

pub(crate) fn apply_selection(
    scope: &LoopScope,
    solids: &mut [EditSolid],
    verts: &HashSet<usize>,
    edges: &HashSet<usize>,
    additive: bool,
) {
    for (si, s) in solids.iter_mut().enumerate() {
        if !additive {
            s.selected.iter_mut().for_each(|f| *f = false);
            s.edge_selected.iter_mut().for_each(|f| *f = false);
        }
        for i in 0..s.points.len() {
            if scope.global_vertex(si, i).is_some_and(|g| verts.contains(&g))
                && let Some(flag) = s.selected.get_mut(i)
            {
                *flag = true;
            }
        }
        for (e, [a, b]) in s.edges.clone().into_iter().enumerate() {
            let (Some(ga), Some(gb)) = (scope.global_vertex(si, a), scope.global_vertex(si, b)) else { continue };
            if scope.net.edge_between(ga, gb).is_some_and(|ge| edges.contains(&ge))
                && let Some(flag) = s.edge_selected.get_mut(e)
            {
                *flag = true;
            }
        }
    }
}

fn run_edges(scope: &LoopScope, cmd: EdgeCmd, seeds: &[usize], toward: Option<DVec3>) -> (Vec<usize>, bool) {
    let kind = if cmd == EdgeCmd::Ring { hp_editor::RunKind::Ring } else { hp_editor::RunKind::Loop };
    let mut out: Vec<usize> = Vec::new();
    let mut closed = false;
    for &s in seeds {
        if out.contains(&s) && seeds.len() > 1 {
            continue;
        }
        let EdgeRun { edges, closed: c } = scope.run(kind, s, toward);
        closed = c;
        for e in edges {
            if !out.contains(&e) {
                out.push(e);
            }
        }
    }
    (out, closed && seeds.len() == 1)
}

pub(crate) fn run_edge_command(
    solids: &mut [EditSolid],
    cmd: EdgeCmd,
    seeds: &[Seed],
    toward: Option<DVec3>,
    additive: bool,
) -> Result<EdgeSummary, EdgeIssue> {
    let scope = scope_of(solids);
    let (sel_v, sel_e) = selection_in(&scope, solids);
    let explicit = !seeds.is_empty();
    let mut seed_edges: Vec<usize> = if explicit {
        seeds.iter().filter_map(|(_, a, b)| scope.edge_at(*a, *b)).collect()
    } else {
        let mut v: Vec<usize> = sel_e.iter().copied().collect();
        v.sort_unstable();
        v
    };
    if !explicit && seed_edges.is_empty() && sel_v.len() >= 2 {
        seed_edges = scope.net.induced_edges(&sel_v);
        seed_edges.sort_unstable();
    }
    let base: HashSet<usize> = if explicit {
        seed_edges.iter().flat_map(|e| scope.net.edges().get(*e).copied().into_iter().flatten()).collect()
    } else {
        sel_v.clone()
    };
    let (verts, edges, closed): (HashSet<usize>, HashSet<usize>, bool) = match cmd {
        EdgeCmd::Loop | EdgeCmd::Ring => {
            if seed_edges.is_empty() {
                return Err(EdgeIssue::NothingSelected);
            }
            let (list, closed) = run_edges(&scope, cmd, &seed_edges, toward);
            let verts: HashSet<usize> = scope.net.run_vertices(&list).into_iter().collect();
            (verts, list.into_iter().collect(), closed)
        }
        EdgeCmd::Set => {
            if seed_edges.is_empty() {
                return Err(EdgeIssue::NothingSelected);
            }
            let verts: HashSet<usize> = scope.net.run_vertices(&seed_edges).into_iter().collect();
            (verts, seed_edges.iter().copied().collect(), false)
        }
        EdgeCmd::Grow | EdgeCmd::Shrink | EdgeCmd::Boundary => {
            if base.is_empty() {
                return Err(EdgeIssue::NothingSelected);
            }
            match cmd {
                EdgeCmd::Grow => {
                    let g = scope.net.grow(&base);
                    let e = scope.net.induced_edges(&g).into_iter().collect();
                    (g, e, false)
                }
                EdgeCmd::Shrink => {
                    let s = scope.net.shrink(&base);
                    let e = scope.net.induced_edges(&s).into_iter().collect();
                    (s, e, false)
                }
                _ => {
                    let (v, e) = scope.net.boundary(&base);
                    (v, e.into_iter().collect(), false)
                }
            }
        }
    };
    apply_selection(&scope, solids, &verts, &edges, additive);
    Ok(EdgeSummary { edges: edges.len(), vertices: verts.len(), closed })
}

pub(crate) fn pick_edge(solids: &[EditSolid], view: &ViewInfo, pos: Pos2) -> Option<(usize, usize)> {
    let mut best: Option<(f32, f64, usize, usize)> = None;
    for (si, s) in solids.iter().enumerate() {
        for (ei, [a, b]) in s.edges.iter().enumerate() {
            let (Some(&pa), Some(&pb)) = (s.points.get(*a), s.points.get(*b)) else { continue };
            let (Some(sa), Some(sb)) = (view.world_to_screen(pa), view.world_to_screen(pb)) else { continue };
            let d = if sa.distance(sb) < 1.0 { sa.distance(pos) } else { segment_distance(pos, sa, sb) };
            if d > LOOP_HIT_PX {
                continue;
            }
            let mid = (pa + pb) * 0.5;
            let z = if view.is_2d() {
                -mid.dot(view.kind.toward_viewer())
            } else {
                (mid - view.cam3d.pos).dot(view.cam3d.forward())
            };
            let better = match best {
                None => true,
                Some((bd, bz, _, _)) => d + PICK_TIE_PX < bd || ((d - bd).abs() <= PICK_TIE_PX && z < bz),
            };
            if better {
                best = Some((d, z, si, ei));
            }
        }
    }
    best.map(|(_, _, si, ei)| (si, ei))
}

pub(crate) fn toward_for(view: &ViewInfo, at: DVec3) -> DVec3 {
    if view.is_2d() { view.kind.toward_viewer() } else { (view.cam3d.pos - at).normalize_or_zero() }
}

pub(crate) fn seed_of(solids: &[EditSolid], pick: (usize, usize)) -> Option<Seed> {
    let s = solids.get(pick.0)?;
    let [a, b] = *s.edges.get(pick.1)?;
    Some((s.id, *s.points.get(a)?, *s.points.get(b)?))
}

pub(crate) fn run_segments(solids: &[EditSolid], cmd: EdgeCmd, seed: Seed, toward: Option<DVec3>) -> Vec<[DVec3; 2]> {
    let scope = scope_of(solids);
    let Some(e) = scope.edge_at(seed.1, seed.2) else { return Vec::new() };
    let (list, _) = run_edges(&scope, cmd, &[e], toward);
    list.into_iter()
        .filter_map(|e| {
            let [a, b] = *scope.net.edges().get(e)?;
            Some([*scope.net.points().get(a)?, *scope.net.points().get(b)?])
        })
        .collect()
}

pub(crate) fn counts(solids: &[EditSolid]) -> (usize, usize) {
    let key = |p: DVec3| [(p.x * 16.0).round() as i64, (p.y * 16.0).round() as i64, (p.z * 16.0).round() as i64];
    let mut edges: HashSet<([i64; 3], [i64; 3])> = HashSet::new();
    let mut verts: HashSet<[i64; 3]> = HashSet::new();
    for s in solids {
        for (i, sel) in s.selected.iter().enumerate() {
            if *sel && let Some(p) = s.points.get(i) {
                verts.insert(key(*p));
            }
        }
        for (e, sel) in s.edge_selected.iter().enumerate() {
            if !*sel {
                continue;
            }
            let Some([a, b]) = s.edges.get(e).copied() else { continue };
            if let (Some(pa), Some(pb)) = (s.points.get(a), s.points.get(b)) {
                let (ka, kb) = (key(*pa), key(*pb));
                edges.insert((ka.min(kb), ka.max(kb)));
            }
        }
    }
    (edges.len(), verts.len())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EdgeSet {
    pub edges: Vec<hp_editor::EdgeRef>,
    pub vertices: Vec<DVec3>,
    pub closed: bool,
}

pub fn compute_edges(
    doc: &hp_editor::Document,
    ids: &[Id],
    cmd: EdgeCmd,
    seeds: &[Seed],
    toward: Option<DVec3>,
    times: usize,
) -> Result<EdgeSet, String> {
    let scope = doc.loop_scope(ids);
    let mut seed_edges: Vec<usize> = Vec::new();
    for (_, a, b) in seeds {
        let e = scope.edge_at(*a, *b).ok_or("a seed is not an edge of the given solids")?;
        if !seed_edges.contains(&e) {
            seed_edges.push(e);
        }
    }
    if seed_edges.is_empty() {
        return Err("give at least one seed edge".to_owned());
    }
    let (verts, edges, closed): (HashSet<usize>, Vec<usize>, bool) = match cmd {
        EdgeCmd::Loop | EdgeCmd::Ring => {
            let (list, closed) = run_edges(&scope, cmd, &seed_edges, toward);
            (scope.net.run_vertices(&list).into_iter().collect(), list, closed)
        }
        EdgeCmd::Set => (scope.net.run_vertices(&seed_edges).into_iter().collect(), seed_edges.clone(), false),
        EdgeCmd::Grow | EdgeCmd::Shrink | EdgeCmd::Boundary => {
            let mut set: HashSet<usize> = scope.net.run_vertices(&seed_edges).into_iter().collect();
            let mut edges = scope.net.induced_edges(&set);
            for _ in 0..times.max(1) {
                match cmd {
                    EdgeCmd::Grow => set = scope.net.grow(&set),
                    EdgeCmd::Shrink => set = scope.net.shrink(&set),
                    _ => {
                        let (v, e) = scope.net.boundary(&set);
                        set = v;
                        edges = e;
                        continue;
                    }
                }
                edges = scope.net.induced_edges(&set);
            }
            (set, edges, false)
        }
    };
    let vertices = verts.iter().filter_map(|v| scope.net.points().get(*v).copied()).collect();
    Ok(EdgeSet { edges: scope.refs(&edges), vertices, closed })
}

pub const TR: hp_app::i18n::TrTable = &[
    (
        "vertex.loop.counts",
        "{0} edge(s), {1} vertex(es) selected. Alt+click: edge loop, Ctrl+Alt+click: edge ring, Shift adds",
        "{0} arête(s), {1} sommet(s) sélectionné(s). Alt+clic : boucle d'arêtes, Ctrl+Alt+clic : anneau d'arêtes, Maj ajoute",
    ),
    (
        "vertex.loop.need_edge",
        "Select an edge first: Alt+click an edge selects its loop.",
        "Sélectionnez d'abord une arête : Alt+clic sur une arête sélectionne sa boucle.",
    ),
    (
        "vertex.loop.need_vertices",
        "Select some vertices or edges first.",
        "Sélectionnez d'abord des sommets ou des arêtes.",
    ),
    (
        "vertex.loop.done_loop",
        "Edge loop: {0} edge(s), {1} vertex(es)",
        "Boucle d'arêtes : {0} arête(s), {1} sommet(s)",
    ),
    (
        "vertex.loop.done_closed",
        "Closed edge loop: {0} edge(s), {1} vertex(es)",
        "Boucle d'arêtes fermée : {0} arête(s), {1} sommet(s)",
    ),
    (
        "vertex.loop.done_ring",
        "Edge ring: {0} edge(s), {1} vertex(es)",
        "Anneau d'arêtes : {0} arête(s), {1} sommet(s)",
    ),
    ("vertex.loop.done_set", "Selection: {0} edge(s), {1} vertex(es)", "Sélection : {0} arête(s), {1} sommet(s)"),
    ("vertex.loop.title", "Edge selection", "Sélection d'arêtes"),
    ("vertex.loop.selected_counts", "{0} edge(s), {1} vertex(es)", "{0} arête(s), {1} sommet(s)"),
    (
        "vertex.loop.loop_tip",
        "Extend the selected edges to their edge loops: the line of edges that continues straight through 4 way vertices and follows the boundary of the face at the corners of a brush (Alt+click an edge)",
        "Étend les arêtes sélectionnées à leurs boucles d'arêtes : la ligne d'arêtes qui continue tout droit aux sommets à 4 arêtes et suit le contour de la face aux coins d'un brush (Alt+clic sur une arête)",
    ),
    (
        "vertex.loop.ring_tip",
        "Extend the selected edges to their edge rings: the parallel edges across the four sided faces (Ctrl+Alt+click an edge)",
        "Étend les arêtes sélectionnées à leurs anneaux d'arêtes : les arêtes parallèles à travers les faces à quatre côtés (Ctrl+Alt+clic sur une arête)",
    ),
    (
        "vertex.loop.grow_tip",
        "Add every vertex next to the selected vertices",
        "Ajoute tous les sommets voisins des sommets sélectionnés",
    ),
    (
        "vertex.loop.shrink_tip",
        "Deselect the vertices that touch an unselected vertex",
        "Désélectionne les sommets qui touchent un sommet non sélectionné",
    ),
    (
        "vertex.loop.boundary_tip",
        "Keep only the boundary of the selected region: the edges and vertices on its border",
        "Ne garde que le contour de la région sélectionnée : les arêtes et sommets de sa bordure",
    ),
    (
        "vertex.loop.slide_tip",
        "Slide the selected edge loops along the neighbouring faces, keeping the shape, with a live preview",
        "Fait glisser les boucles d'arêtes sélectionnées le long des faces voisines en gardant la forme, avec aperçu en direct",
    ),
    (
        "vertex.loop.rotate_tip",
        "Rotate the selected vertices around a center, with a live preview",
        "Fait tourner les sommets sélectionnés autour d'un centre, avec aperçu en direct",
    ),
    ("vertex.loop.no_edge", "No edge under the cursor.", "Aucune arête sous le curseur."),
    (
        "vertex.loop.hint_pick",
        "Alt+click an edge: select its loop. Ctrl+Alt+click: its ring. Shift adds to the selection",
        "Alt+clic sur une arête : sélectionne sa boucle. Ctrl+Alt+clic : son anneau. Maj ajoute à la sélection",
    ),
    ("vertex.rotate.title", "Rotate vertices", "Rotation des sommets"),
    ("vertex.rotate.angle", "Angle", "Angle"),
    (
        "vertex.rotate.angle_tip",
        "Degrees, from -360 to 360. Alt + mouse wheel changes it by 5 degrees (Ctrl+Alt: 1 degree)",
        "Degrés, de -360 à 360. Alt + molette le change de 5 degrés (Ctrl+Alt : 1 degré)",
    ),
    ("vertex.rotate.axis", "Axis", "Axe"),
    (
        "vertex.rotate.axis_tip",
        "Rotation axis: a world axis, or the normal of the plane of the selected vertices (a loop turns in its own plane)",
        "Axe de rotation : un axe du monde, ou la normale du plan des sommets sélectionnés (une boucle tourne dans son propre plan)",
    ),
    ("vertex.rotate.normal", "Loop normal", "Normale de la boucle"),
    ("vertex.rotate.center", "Center", "Centre"),
    (
        "vertex.rotate.center_tip",
        "Point the vertices turn around (default: the middle of the selected vertices)",
        "Point autour duquel les sommets tournent (par défaut : le milieu des sommets sélectionnés)",
    ),
    ("vertex.rotate.need", "Select some vertices to rotate.", "Sélectionnez des sommets à faire tourner."),
    (
        "vertex.hint_rotate",
        "Rotate {0} degrees: {1} applies, {2} cancels, Alt + wheel changes the angle",
        "Rotation de {0} degrés : {1} applique, {2} annule, Alt + molette change l'angle",
    ),
    ("vertex.slide.title", "Slide edge loop", "Glisser la boucle d'arêtes"),
    ("vertex.slide.amount", "Slide", "Glissement"),
    (
        "vertex.slide.amount_tip",
        "-1 to 1: each vertex of the loop slides along its edge on one side (positive) or the other (negative) of the loop, by that fraction of the edge. Alt + mouse wheel changes it by 0.05",
        "-1 à 1 : chaque sommet de la boucle glisse le long de son arête d'un côté (positif) ou de l'autre (négatif) de la boucle, de cette fraction de l'arête. Alt + molette le change de 0,05",
    ),
    (
        "vertex.slide.need",
        "Select an edge loop to slide (Alt+click an edge).",
        "Sélectionnez une boucle d'arêtes à faire glisser (Alt+clic sur une arête).",
    ),
    (
        "vertex.slide.branching",
        "The selected edges branch: select separate edge loops to slide them.",
        "Les arêtes sélectionnées se ramifient : sélectionnez des boucles d'arêtes séparées pour les faire glisser.",
    ),
    (
        "vertex.hint_slide",
        "Slide {0}: {1} applies, {2} cancels, Alt + wheel changes the amount",
        "Glissement {0} : {1} applique, {2} annule, Alt + molette change la valeur",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translations_are_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty());
        for (k, _, _) in TR {
            assert!(k.starts_with("vertex."));
        }
    }

    #[test]
    fn command_names_round_trip() {
        for c in [EdgeCmd::Loop, EdgeCmd::Ring, EdgeCmd::Grow, EdgeCmd::Shrink, EdgeCmd::Boundary] {
            assert_eq!(EdgeCmd::parse(c.name()), Some(c));
        }
        assert_eq!(EdgeCmd::parse("x"), None);
    }
}
