use glam::DVec3;
use hp_app::state::AppState;
use hp_app::tool::ToolOption;
use hp_app::{Severity, t, tf};
use hp_editor::{Document, FaceRef, ObjRef, Owner};
use hp_geom::{BLEND_CHANNELS, BlendFalloff, BlendMode, BlendParams, BlendPart, MAX_BLEND_WIDTH, MIN_BLEND_WIDTH};
use hp_vmf::DispInfo;
use hp_vmf::multiblend::MultiBlend;

use super::commands::reshape_doc;
use super::smooth::{SmoothPart, set_position};
use crate::common::edit_error_text;

pub const UNDO_BLEND: &str = "Blend displacements";

#[derive(Clone, Debug, PartialEq)]
pub struct BlendOptions {
    pub width: f64,
    pub falloff: BlendFalloff,
    pub mode: BlendMode,
    pub match_edges: bool,
    pub alpha: bool,
    pub match_powers: bool,
}

impl Default for BlendOptions {
    fn default() -> Self {
        Self {
            width: 128.0,
            falloff: BlendFalloff::Smooth,
            mode: BlendMode::Average,
            match_edges: false,
            alpha: true,
            match_powers: false,
        }
    }
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_owned() } else { s.to_owned() }
}

pub fn falloff_name(f: BlendFalloff) -> &'static str {
    match f {
        BlendFalloff::Linear => "linear",
        BlendFalloff::Smooth => "smooth",
        BlendFalloff::EaseIn => "ease_in",
        BlendFalloff::EaseOut => "ease_out",
    }
}

pub fn mode_name(m: BlendMode) -> &'static str {
    match m {
        BlendMode::Average => "average",
        BlendMode::AOverB => "a_over_b",
        BlendMode::BOverA => "b_over_a",
        BlendMode::Max => "max",
        BlendMode::Min => "min",
    }
}

pub const FALLOFF_NAMES: [&str; 4] = ["linear", "smooth", "ease_in", "ease_out"];
pub const MODE_NAMES: [&str; 5] = ["average", "a_over_b", "b_over_a", "max", "min"];

impl BlendOptions {
    pub fn params(&self) -> BlendParams {
        BlendParams {
            width: self.width,
            falloff: self.falloff,
            mode: self.mode,
            match_edges: self.match_edges,
            alpha: self.alpha,
        }
    }

    pub fn values(&self) -> Vec<ToolOption> {
        vec![
            ToolOption::new(
                "blend_width",
                num(self.width),
                "height blending: width of the transition zone in units (0.01 to 65536)",
            ),
            ToolOption::new(
                "blend_falloff",
                falloff_name(self.falloff),
                "height blending: curve of the transition: linear, smooth, ease_in (slow start) or ease_out (slow end)",
            ),
            ToolOption::new(
                "blend_mode",
                mode_name(self.mode),
                "height blending: average, a_over_b (A wins, B follows it), b_over_a, max (the higher surface wins) or min",
            ),
            ToolOption::new(
                "blend_match_edges",
                on_off(self.match_edges),
                "height blending: shift each side by the height gap at the meeting line, so it keeps its own relief and the sides meet without a step",
            ),
            ToolOption::new(
                "blend_alpha",
                on_off(self.alpha),
                "height blending: blend the texture alpha (and 4-way blend rows) across the same zone",
            ),
            ToolOption::new(
                "blend_match_powers",
                on_off(self.match_powers),
                "height blending: first resample the lower power displacements to the highest power of the selection",
            ),
        ]
    }

    pub fn set(&mut self, key: &str, value: &str) -> bool {
        let v = value.trim().to_ascii_lowercase();
        let flag = || match v.as_str() {
            "true" | "on" | "1" => Some(true),
            "false" | "off" | "0" => Some(false),
            _ => None,
        };
        match key {
            "blend_width" => v
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && (MIN_BLEND_WIDTH..=MAX_BLEND_WIDTH).contains(n))
                .map(|n| self.width = n)
                .is_some(),
            "blend_falloff" => {
                BlendFalloff::ALL.into_iter().find(|f| falloff_name(*f) == v).map(|f| self.falloff = f).is_some()
            }
            "blend_mode" => BlendMode::ALL.into_iter().find(|m| mode_name(*m) == v).map(|m| self.mode = m).is_some(),
            "blend_match_edges" => flag().map(|b| self.match_edges = b).is_some(),
            "blend_alpha" => flag().map(|b| self.alpha = b).is_some(),
            "blend_match_powers" => flag().map(|b| self.match_powers = b).is_some(),
            _ => false,
        }
    }

    pub fn from_values(values: &[ToolOption]) -> Self {
        let mut out = Self::default();
        for o in values {
            out.set(o.key, &o.value);
        }
        out
    }
}

fn disp_faces_of(doc: &Document, faces: &[FaceRef]) -> Vec<FaceRef> {
    let mut out: Vec<FaceRef> = Vec::new();
    for f in faces {
        let ok = doc.scene().solid(f.solid).is_some_and(|n| n.disp_mesh(f.side).is_some());
        if ok && !out.contains(f) {
            out.push(*f);
        }
    }
    out
}

pub fn partition(doc: &Document, faces: &[FaceRef]) -> Vec<Vec<FaceRef>> {
    let faces = disp_faces_of(doc, faces);
    let scene = doc.scene();
    let group_key = |f: &FaceRef| -> (u8, hp_editor::Id) {
        if let Some(g) = scene.object_group(ObjRef::Solid(f.solid)) {
            return (0, scene.group_root(g));
        }
        match scene.solid(f.solid).map(|n| n.owner) {
            Some(Owner::Entity(e)) => (1, e),
            _ => (2, f.solid),
        }
    };
    let mut keys: Vec<(u8, hp_editor::Id)> = Vec::new();
    for f in &faces {
        let k = group_key(f);
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    let by_group = keys.len() >= 2;
    let key_of = |f: &FaceRef| if by_group { group_key(f) } else { (2, f.solid) };
    let mut order: Vec<(u8, hp_editor::Id)> = Vec::new();
    let mut sets: Vec<Vec<FaceRef>> = Vec::new();
    for f in faces {
        let k = key_of(&f);
        match order.iter().position(|o| *o == k) {
            Some(i) => sets[i].push(f),
            None => {
                order.push(k);
                sets.push(vec![f]);
            }
        }
    }
    sets
}

fn uses_blend_rows(disps: &[&DispInfo]) -> bool {
    disps.iter().any(|d| MultiBlend::of(d).is_some())
}

fn part_values(disp: &DispInfo, count: usize, rows: bool) -> Vec<[f64; BLEND_CHANNELS]> {
    if rows {
        return MultiBlend::of_or_alphas(disp)
            .filter(|b| b.len() == count)
            .map_or_else(|| vec![[1.0, 0.0, 0.0, 0.0]; count], |b| b.weights);
    }
    (0..count)
        .map(|k| {
            let a = disp.alphas.get(k).copied().filter(|a| a.is_finite()).unwrap_or(0.0);
            [a.clamp(0.0, 255.0), 0.0, 0.0, 0.0]
        })
        .collect()
}

fn build_part(mesh: &hp_geom::DispMesh, disp: &DispInfo, set: usize, rows: bool) -> BlendPart {
    let count = mesh.positions.len();
    BlendPart {
        set,
        points: mesh.positions.clone(),
        values: part_values(disp, count, rows),
        tris: mesh.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect(),
    }
}

fn blend_parts(
    valid: &[FaceRef],
    parts: &mut [SmoothPart<'_>],
    sets: &[Vec<FaceRef>],
    params: &BlendParams,
) -> Vec<usize> {
    let mut changed = vec![0usize; parts.len()];
    let rows = uses_blend_rows(&parts.iter().map(|p| &*p.disp).collect::<Vec<_>>());
    let inputs: Vec<BlendPart> = valid
        .iter()
        .zip(parts.iter())
        .map(|(f, p)| {
            let set = sets.iter().position(|s| s.contains(f)).unwrap_or(0);
            build_part(p.mesh, p.disp, set, rows)
        })
        .collect();
    let results = hp_geom::blend_sets(&inputs, params);
    for ((pi, part), result) in parts.iter_mut().enumerate().zip(results) {
        let Some(input) = inputs.get(pi) else { continue };
        let mut moved = 0usize;
        for (k, (old, new)) in input.points.iter().zip(&result.heights).enumerate() {
            if (new - old.z).abs() > 1e-9 && set_position(part, k, DVec3::new(old.x, old.y, *new)) {
                moved += 1;
            }
        }
        if params.alpha {
            let differs = input
                .values
                .iter()
                .zip(&result.values)
                .any(|(a, b)| a.iter().zip(b).any(|(x, y)| (x - y).abs() > 1e-9));
            if differs {
                let count = result.values.len();
                if rows && let Some(mut mb) = MultiBlend::of(part.disp) {
                    mb.weights.clone_from(&result.values);
                    mb.normalize();
                    if mb.write_to(part.disp) {
                        moved += 1;
                    }
                } else if rows {
                    part.disp.alphas.resize(count, 0.0);
                    for (a, v) in part.disp.alphas.iter_mut().zip(&result.values) {
                        *a = (v[1] * 255.0).clamp(0.0, 255.0);
                    }
                    moved += 1;
                } else {
                    part.disp.alphas.resize(count, 0.0);
                    for (a, v) in part.disp.alphas.iter_mut().zip(&result.values) {
                        *a = v[0].clamp(0.0, 255.0);
                    }
                    moved += 1;
                }
            }
        }
        if let Some(c) = changed.get_mut(pi) {
            *c = moved;
        }
    }
    changed
}

pub fn apply(state: &mut AppState, sets: &[Vec<FaceRef>], opts: &BlendOptions) -> usize {
    let sets: Vec<Vec<FaceRef>> = sets.iter().map(|s| disp_faces_of(&state.doc, s)).filter(|s| !s.is_empty()).collect();
    if sets.len() < 2 {
        state.notify(Severity::Info, t!("disp.blend_need_sets"));
        return 0;
    }
    let faces: Vec<FaceRef> = sets.iter().flatten().copied().fold(Vec::new(), |mut acc, f| {
        if !acc.contains(&f) {
            acc.push(f);
        }
        acc
    });
    let power = faces
        .iter()
        .filter_map(|f| state.doc.face_side(*f).and_then(|s| s.disp.as_ref()).map(|d| d.power))
        .max()
        .unwrap_or(3);
    state.doc.begin(UNDO_BLEND);
    if opts.match_powers {
        let low: Vec<FaceRef> = faces
            .iter()
            .copied()
            .filter(|f| state.doc.face_side(*f).and_then(|s| s.disp.as_ref()).is_some_and(|d| d.power != power))
            .collect();
        if !low.is_empty()
            && let Err(e) = state.doc.resample_displacements(&low, power)
        {
            state.doc.cancel();
            state.notify(Severity::Warning, tf!("disp.failed", edit_error_text(&e)));
            return 0;
        }
    }
    let params = opts.params();
    let outcome = reshape_doc(&mut state.doc, &faces, UNDO_BLEND, |parts| blend_parts(&faces, parts, &sets, &params));
    state.doc.commit();
    match outcome {
        Ok(moved) if moved > 0 => {
            state.notify(Severity::Success, tf!("disp.blended", faces.len(), sets.len()));
            moved
        }
        Ok(_) => {
            state.notify(Severity::Info, t!("disp.blend_nothing"));
            0
        }
        Err(e) => {
            state.notify(Severity::Warning, tf!("disp.failed", edit_error_text(&e)));
            0
        }
    }
}

pub fn preview(doc: &Document, sets: &[Vec<FaceRef>], opts: &BlendOptions) -> Vec<(DVec3, f64)> {
    let sets: Vec<Vec<FaceRef>> = sets.iter().map(|s| disp_faces_of(doc, s)).filter(|s| !s.is_empty()).collect();
    if sets.len() < 2 {
        return Vec::new();
    }
    let mut inputs: Vec<BlendPart> = Vec::new();
    for (si, set) in sets.iter().enumerate() {
        for f in set {
            let Some(node) = doc.scene().solid(f.solid) else { continue };
            let (Some(mesh), Some(disp)) =
                (node.disp_mesh(f.side), node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()))
            else {
                continue;
            };
            inputs.push(build_part(mesh, disp, si, false));
        }
    }
    let mut params = opts.params();
    params.alpha = false;
    let results = hp_geom::blend_sets(&inputs, &params);
    let mut out = Vec::new();
    for (input, result) in inputs.iter().zip(results) {
        for (p, w) in input.points.iter().zip(result.influence) {
            if w > 0.0 {
                out.push((*p, w));
            }
        }
    }
    out
}

pub const TR: hp_app::i18n::TrTable = &[
    ("disp.blend", "Height blending", "Fusion des hauteurs"),
    (
        "disp.blend_hint",
        "Blends the heights of the selected displacements or groups (set A and set B, or one set per group) across a transition zone",
        "Fusionne les hauteurs des displacements ou groupes sélectionnés (ensemble A et ensemble B, ou un ensemble par groupe) sur une zone de transition",
    ),
    ("disp.blend_set_a", "Set A = selection", "A = sélection"),
    ("disp.blend_set_b", "Set B = selection", "B = sélection"),
    ("disp.blend_clear_sets", "Clear sets", "Effacer les ensembles"),
    (
        "disp.blend_set_tip",
        "Remember the selected displacements as this set. Without both sets, each selected group (or solid) is its own set",
        "Mémorise les displacements sélectionnés comme cet ensemble. Sans les deux ensembles, chaque groupe (ou solide) sélectionné est son propre ensemble",
    ),
    (
        "disp.blend_sets_info",
        "Set A: {0} face(s), set B: {1} face(s)",
        "Ensemble A : {0} face(s), ensemble B : {1} face(s)",
    ),
    ("disp.blend_sets_auto", "{0} set(s) from the selection", "{0} ensemble(s) issus de la sélection"),
    ("disp.blend_width", "Width", "Largeur"),
    ("disp.blend_width_tip", "Width of the transition zone, in units", "Largeur de la zone de transition, en unités"),
    ("disp.blend_falloff", "Curve", "Courbe"),
    ("disp.blend_falloff_linear", "Linear", "Linéaire"),
    ("disp.blend_falloff_smooth", "Smooth", "Douce"),
    ("disp.blend_falloff_ease_in", "Ease in", "Entrée douce"),
    ("disp.blend_falloff_ease_out", "Ease out", "Sortie douce"),
    (
        "disp.blend_falloff_tip",
        "How the blend grows from the edge of the zone to the meeting line",
        "Comment la fusion croît du bord de la zone jusqu'à la ligne de rencontre",
    ),
    ("disp.blend_mode", "Mode", "Mode"),
    ("disp.blend_mode_average", "Average", "Moyenne"),
    ("disp.blend_mode_a_over_b", "A over B", "A sur B"),
    ("disp.blend_mode_b_over_a", "B over A", "B sur A"),
    ("disp.blend_mode_max", "Maximum", "Maximum"),
    ("disp.blend_mode_min", "Minimum", "Minimum"),
    (
        "disp.blend_mode_tip",
        "Average: both sides meet in the middle. A over B: A stays, B follows it with a soft edge. Maximum or minimum: the higher or lower surface wins",
        "Moyenne : les deux côtés se rejoignent au milieu. A sur B : A reste, B le suit avec un bord doux. Maximum ou minimum : la surface la plus haute ou la plus basse l'emporte",
    ),
    ("disp.blend_match_edges", "Match edge heights", "Raccorder les hauteurs de bord"),
    (
        "disp.blend_match_edges_tip",
        "Shift each side by the height gap at the meeting line instead of copying the other side, so every side keeps its relief and the two meet without a step",
        "Décale chaque côté de l'écart de hauteur à la ligne de rencontre au lieu de copier l'autre côté : chaque côté garde son relief et les deux se raccordent sans marche",
    ),
    ("disp.blend_alpha", "Blend texture alpha", "Fusionner l'alpha des textures"),
    (
        "disp.blend_alpha_tip",
        "Blend the vertex alpha (and the 4-way blend rows) across the same zone",
        "Fusionne l'alpha des sommets (et les lignes du 4-way blend) sur la même zone",
    ),
    ("disp.blend_match_powers", "Match powers first", "Aligner d'abord les puissances"),
    (
        "disp.blend_match_powers_tip",
        "Resample the lower power displacements to the highest power so the blend keeps all the detail. Off: each keeps its own power",
        "Rééchantillonne les displacements de puissance plus faible à la puissance la plus haute pour garder tout le détail. Désactivé : chacun garde sa puissance",
    ),
    ("disp.blend_preview", "Show the zone in the 3D view", "Afficher la zone dans la vue 3D"),
    (
        "disp.blend_preview_tip",
        "Draws the vertices the blend will move, brighter where it acts more",
        "Dessine les sommets que la fusion va déplacer, plus lumineux là où elle agit le plus",
    ),
    ("disp.blend_run", "Blend heights", "Fusionner les hauteurs"),
    (
        "disp.blend_run_tip",
        "Blend the sets across the zone. Both sides stay sewn. One undo step",
        "Fusionne les ensembles sur la zone. Les deux côtés restent cousus. Une seule étape d'annulation",
    ),
    (
        "disp.blend_need_sets",
        "Blending needs two sets: select displacements from two groups or solids, or set A and B",
        "La fusion demande deux ensembles : sélectionnez des displacements de deux groupes ou solides, ou définissez A et B",
    ),
    ("disp.blended", "{0} displacement(s) blended in {1} sets.", "{0} displacement(s) fusionné(s) en {1} ensembles."),
    (
        "disp.blend_nothing",
        "Nothing changed: the sets are farther apart than the width or already match.",
        "Rien n'a changé : les ensembles sont plus éloignés que la largeur ou déjà raccordés.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_round_trip_and_reject_bad_values() {
        let mut o = BlendOptions::default();
        assert!(o.set("blend_width", "256"));
        assert!(o.set("blend_falloff", "ease_out"));
        assert!(o.set("blend_mode", "a_over_b"));
        assert!(o.set("blend_match_edges", "true"));
        assert!(!o.set("blend_width", "0"));
        assert!(!o.set("blend_mode", "nope"));
        let back = BlendOptions::from_values(&o.values());
        assert_eq!(back, o);
        assert_eq!(back.mode, BlendMode::AOverB);
    }
}
