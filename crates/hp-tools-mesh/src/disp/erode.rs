use glam::DVec3;
use hp_app::state::AppState;
use hp_app::tool::ToolOption;
use hp_app::{Severity, t, tf};
use hp_geom::{HeightGraph, HydraulicParams, NoiseParams, ThermalParams};

use super::commands::{reshape, targets};
use super::seams::NEIGHBOUR_MIN_DOT;
use super::smooth::{SmoothPart, Surface, write_back};

pub const UNDO_THERMAL: &str = "Thermal erosion";
pub const UNDO_HYDRAULIC: &str = "Hydraulic erosion";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseKind {
    Random,
    Fractal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainOp {
    Noise,
    Thermal,
    Hydraulic,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerrainOptions {
    pub noise_kind: NoiseKind,
    pub noise_scale: f64,
    pub noise_octaves: u32,
    pub noise_persistence: f64,
    pub seed: u64,
    pub thermal_iterations: u32,
    pub thermal_talus: f64,
    pub thermal_strength: f64,
    pub hydraulic_iterations: u32,
    pub hydraulic_rain: f64,
    pub hydraulic_evaporation: f64,
    pub hydraulic_capacity: f64,
    pub hydraulic_deposition: f64,
    pub hydraulic_strength: f64,
}

impl Default for TerrainOptions {
    fn default() -> Self {
        let (n, th, hy) = (NoiseParams::default(), ThermalParams::default(), HydraulicParams::default());
        Self {
            noise_kind: NoiseKind::Random,
            noise_scale: n.scale,
            noise_octaves: n.octaves,
            noise_persistence: n.persistence,
            seed: n.seed,
            thermal_iterations: th.iterations,
            thermal_talus: th.talus_degrees,
            thermal_strength: th.strength,
            hydraulic_iterations: hy.iterations,
            hydraulic_rain: hy.rain,
            hydraulic_evaporation: hy.evaporation,
            hydraulic_capacity: hy.capacity,
            hydraulic_deposition: hy.deposition,
            hydraulic_strength: hy.strength,
        }
    }
}

fn num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_owned() } else { s.to_owned() }
}

impl TerrainOptions {
    pub fn noise_params(&self) -> NoiseParams {
        NoiseParams {
            scale: self.noise_scale,
            octaves: self.noise_octaves,
            persistence: self.noise_persistence,
            seed: self.seed,
        }
    }

    pub fn thermal_params(&self) -> ThermalParams {
        ThermalParams {
            iterations: self.thermal_iterations,
            talus_degrees: self.thermal_talus,
            strength: self.thermal_strength,
        }
    }

    pub fn hydraulic_params(&self) -> HydraulicParams {
        HydraulicParams {
            iterations: self.hydraulic_iterations,
            rain: self.hydraulic_rain,
            evaporation: self.hydraulic_evaporation,
            capacity: self.hydraulic_capacity,
            deposition: self.hydraulic_deposition,
            strength: self.hydraulic_strength,
            seed: self.seed,
        }
    }

    pub fn values(&self) -> Vec<ToolOption> {
        let kind = match self.noise_kind {
            NoiseKind::Random => "random",
            NoiseKind::Fractal => "fractal",
        };
        vec![
            ToolOption::new(
                "noise_type",
                kind,
                "Add noise kind: random (a random height per vertex) or fractal (Perlin noise with octaves, the \
                 same everywhere on the map, so neighbouring displacements match)",
            ),
            ToolOption::new(
                "noise_scale",
                num(self.noise_scale),
                "fractal noise: size of the biggest features in units (1 to 65536)",
            ),
            ToolOption::new(
                "noise_octaves",
                self.noise_octaves.to_string(),
                "fractal noise: layers of detail (1 to 8)",
            ),
            ToolOption::new(
                "noise_persistence",
                num(self.noise_persistence),
                "fractal noise: how much each finer layer counts compared to the previous (0.05 to 1)",
            ),
            ToolOption::new(
                "terrain_seed",
                self.seed.to_string(),
                "seed of the fractal noise and of the hydraulic rain (0 to 4294967295): the same seed gives the same result",
            ),
            ToolOption::new(
                "thermal_iterations",
                self.thermal_iterations.to_string(),
                "thermal erosion: passes (1 to 1000)",
            ),
            ToolOption::new(
                "thermal_talus",
                num(self.thermal_talus),
                "thermal erosion: steepest slope in degrees that material stays on (0 to 89)",
            ),
            ToolOption::new(
                "thermal_strength",
                num(self.thermal_strength),
                "thermal erosion: share of the excess that slides per pass (0 to 1)",
            ),
            ToolOption::new(
                "hydraulic_iterations",
                self.hydraulic_iterations.to_string(),
                "hydraulic erosion: passes (1 to 2000)",
            ),
            ToolOption::new(
                "hydraulic_rain",
                num(self.hydraulic_rain),
                "hydraulic erosion: water added to every vertex per pass (0 to 100)",
            ),
            ToolOption::new(
                "hydraulic_evaporation",
                num(self.hydraulic_evaporation),
                "hydraulic erosion: share of the water that evaporates per pass (0 to 1)",
            ),
            ToolOption::new(
                "hydraulic_capacity",
                num(self.hydraulic_capacity),
                "hydraulic erosion: sediment the water carries per unit of water and slope (0 to 10)",
            ),
            ToolOption::new(
                "hydraulic_deposition",
                num(self.hydraulic_deposition),
                "hydraulic erosion: share of the sediment above capacity that settles per pass (0 to 1)",
            ),
            ToolOption::new(
                "hydraulic_strength",
                num(self.hydraulic_strength),
                "hydraulic erosion: share of the missing capacity dissolved from the ground per pass (0 to 1)",
            ),
        ]
    }

    pub fn set(&mut self, key: &str, value: &str) -> bool {
        let v = value.trim().to_ascii_lowercase();
        let number = v.parse::<f64>().ok().filter(|n| n.is_finite());
        let within = |lo: f64, hi: f64| number.filter(|n| (lo..=hi).contains(n));
        let whole = |lo: u32, hi: u32| v.parse::<u32>().ok().filter(|n| (lo..=hi).contains(n));
        match key {
            "noise_type" => match v.as_str() {
                "random" => Some(NoiseKind::Random),
                "fractal" => Some(NoiseKind::Fractal),
                _ => None,
            }
            .map(|k| self.noise_kind = k)
            .is_some(),
            "noise_scale" => within(1.0, 65536.0).map(|n| self.noise_scale = n).is_some(),
            "noise_octaves" => whole(1, hp_geom::MAX_OCTAVES).map(|n| self.noise_octaves = n).is_some(),
            "noise_persistence" => within(0.05, 1.0).map(|n| self.noise_persistence = n).is_some(),
            "terrain_seed" => v.parse::<u32>().ok().map(|n| self.seed = u64::from(n)).is_some(),
            "thermal_iterations" => whole(1, 1000).map(|n| self.thermal_iterations = n).is_some(),
            "thermal_talus" => within(0.0, 89.0).map(|n| self.thermal_talus = n).is_some(),
            "thermal_strength" => within(0.0, 1.0).map(|n| self.thermal_strength = n).is_some(),
            "hydraulic_iterations" => {
                whole(1, hp_geom::MAX_ITERATIONS).map(|n| self.hydraulic_iterations = n).is_some()
            }
            "hydraulic_rain" => within(0.0, 100.0).map(|n| self.hydraulic_rain = n).is_some(),
            "hydraulic_evaporation" => within(0.0, 1.0).map(|n| self.hydraulic_evaporation = n).is_some(),
            "hydraulic_capacity" => within(0.0, 10.0).map(|n| self.hydraulic_capacity = n).is_some(),
            "hydraulic_deposition" => within(0.0, 1.0).map(|n| self.hydraulic_deposition = n).is_some(),
            "hydraulic_strength" => within(0.0, 1.0).map(|n| self.hydraulic_strength = n).is_some(),
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

fn height_graph(parts: &[SmoothPart<'_>]) -> (Surface, HeightGraph) {
    let surface = Surface::build(parts, |mesh, _| mesh.face_normal, |_, _| 1.0, None);
    let heights: Vec<f64> = surface.nodes.iter().map(|n| n.pos.dot(n.axis)).collect();
    let links = (0..surface.nodes.len())
        .map(|ni| {
            let Some(node) = surface.nodes.get(ni) else { return Vec::new() };
            surface
                .neighbours(parts, ni, false)
                .into_iter()
                .filter_map(|m| {
                    let other = surface.nodes.get(m)?;
                    if node.axis.dot(other.axis) < NEIGHBOUR_MIN_DOT {
                        return None;
                    }
                    let d = other.pos - node.pos;
                    Some((m, (d - node.axis * d.dot(node.axis)).length()))
                })
                .collect()
        })
        .collect();
    (surface, HeightGraph::new(heights, links))
}

pub(crate) fn erode_parts(parts: &mut [SmoothPart<'_>], run: impl FnOnce(&mut HeightGraph)) -> Vec<usize> {
    let mut changed = vec![0usize; parts.len()];
    let (surface, mut graph) = height_graph(parts);
    let before = graph.heights.clone();
    run(&mut graph);
    let new_pos: Vec<DVec3> = surface
        .nodes
        .iter()
        .zip(before.iter().zip(&graph.heights))
        .map(|(n, (old, new))| n.pos + n.axis * (new - old))
        .collect();
    write_back(parts, &surface, &new_pos, &mut changed);
    changed
}

pub fn apply(state: &mut AppState, op: TerrainOp, opts: &TerrainOptions) -> usize {
    match op {
        TerrainOp::Noise => noise(state, opts),
        TerrainOp::Thermal => {
            let p = opts.thermal_params();
            erosion(state, UNDO_THERMAL, move |g| g.thermal(&p))
        }
        TerrainOp::Hydraulic => {
            let p = opts.hydraulic_params();
            erosion(state, UNDO_HYDRAULIC, move |g| g.hydraulic(&p))
        }
    }
}

fn erosion(state: &mut AppState, label: &'static str, run: impl FnOnce(&mut HeightGraph)) -> usize {
    match reshape(state, label, |parts| erode_parts(parts, run)) {
        Some((faces, moved)) if moved > 0 => {
            state.notify(Severity::Success, tf!("disp.eroded", faces));
            moved
        }
        Some(_) => {
            state.notify(Severity::Info, t!("disp.erode_nothing"));
            0
        }
        None => 0,
    }
}

fn noise(state: &mut AppState, opts: &TerrainOptions) -> usize {
    if opts.noise_kind == NoiseKind::Random {
        return super::commands::add_noise(state);
    }
    let faces = targets(&state.doc);
    if faces.is_empty() {
        state.notify(Severity::Info, t!("disp.nothing_selected"));
        return 0;
    }
    let (min, max) = state.settings.mesh.noise_range();
    match state.doc.disp_fractal_noise(&faces, min, max, &opts.noise_params()) {
        Ok(n) => {
            state.notify(Severity::Success, tf!("disp.noised", faces.len()));
            n
        }
        Err(e) => {
            state.notify(Severity::Warning, tf!("disp.failed", crate::common::edit_error_text(&e)));
            0
        }
    }
}

impl super::DisplacementTool {
    pub(super) fn noise_kind_ui(&mut self, ui: &mut egui::Ui) {
        noise_ui(ui, &mut self.terrain);
    }

    pub(super) fn erosion_ui(&mut self, ui: &mut egui::Ui, any: bool, actions: &mut Vec<super::PanelAction>) {
        let o = &mut self.terrain;
        ui.add_space(4.0);
        ui.label(egui::RichText::new(t!("disp.erosion")).strong());
        ui.label(egui::RichText::new(t!("disp.erosion_hint")).weak());
        egui::CollapsingHeader::new(t!("disp.thermal")).id_salt("hp_disp_thermal").show(ui, |ui| {
            thermal_grid(ui, o);
        });
        if ui
            .add_enabled(any, egui::Button::new(t!("disp.thermal_run")))
            .on_hover_text(t!("disp.thermal_tip"))
            .clicked()
        {
            actions.push(super::PanelAction::Terrain(TerrainOp::Thermal));
        }
        egui::CollapsingHeader::new(t!("disp.hydraulic")).id_salt("hp_disp_hydraulic").show(ui, |ui| {
            hydraulic_grid(ui, o);
        });
        if ui
            .add_enabled(any, egui::Button::new(t!("disp.hydraulic_run")))
            .on_hover_text(t!("disp.hydraulic_tip"))
            .clicked()
        {
            actions.push(super::PanelAction::Terrain(TerrainOp::Hydraulic));
        }
    }
}

pub fn noise_ui(ui: &mut egui::Ui, o: &mut TerrainOptions) {
    ui.horizontal(|ui| {
        for (kind, label, tip) in [
            (NoiseKind::Random, "disp.noise_random", "disp.noise_random_tip"),
            (NoiseKind::Fractal, "disp.noise_fractal", "disp.noise_fractal_tip"),
        ] {
            if ui.add(egui::Button::selectable(o.noise_kind == kind, t!(label))).on_hover_text(t!(tip)).clicked() {
                o.noise_kind = kind;
            }
        }
    });
    if o.noise_kind != NoiseKind::Fractal {
        return;
    }
    egui::Grid::new("hp_disp_fractal").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label(t!("disp.noise_scale"));
        ui.add(egui::DragValue::new(&mut o.noise_scale).speed(2.0).range(1.0..=65536.0).max_decimals(1))
            .on_hover_text(t!("disp.noise_scale_tip"));
        ui.end_row();
        ui.label(t!("disp.noise_octaves"));
        ui.add(egui::DragValue::new(&mut o.noise_octaves).range(1..=hp_geom::MAX_OCTAVES))
            .on_hover_text(t!("disp.noise_octaves_tip"));
        ui.end_row();
        ui.label(t!("disp.noise_persistence"));
        ui.add(egui::DragValue::new(&mut o.noise_persistence).speed(0.01).range(0.05..=1.0).max_decimals(2))
            .on_hover_text(t!("disp.noise_persistence_tip"));
        ui.end_row();
        seed_row(ui, &mut o.seed);
    });
}

pub fn thermal_grid(ui: &mut egui::Ui, o: &mut TerrainOptions) {
    egui::Grid::new("hp_disp_thermal_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label(t!("disp.erosion_iterations"));
        ui.add(egui::DragValue::new(&mut o.thermal_iterations).range(1..=1000))
            .on_hover_text(t!("disp.erosion_iterations_tip"));
        ui.end_row();
        ui.label(t!("disp.thermal_talus"));
        ui.add(egui::DragValue::new(&mut o.thermal_talus).speed(0.5).range(0.0..=89.0).max_decimals(1))
            .on_hover_text(t!("disp.thermal_talus_tip"));
        ui.end_row();
        ui.label(t!("disp.strength"));
        ui.add(egui::DragValue::new(&mut o.thermal_strength).speed(0.01).range(0.0..=1.0).max_decimals(2))
            .on_hover_text(t!("disp.thermal_strength_tip"));
        ui.end_row();
    });
}

pub fn hydraulic_grid(ui: &mut egui::Ui, o: &mut TerrainOptions) {
    egui::Grid::new("hp_disp_hydraulic_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label(t!("disp.erosion_iterations"));
        ui.add(egui::DragValue::new(&mut o.hydraulic_iterations).range(1..=hp_geom::MAX_ITERATIONS))
            .on_hover_text(t!("disp.erosion_iterations_tip"));
        ui.end_row();
        ui.label(t!("disp.hydraulic_rain"));
        ui.add(egui::DragValue::new(&mut o.hydraulic_rain).speed(0.05).range(0.0..=100.0).max_decimals(2))
            .on_hover_text(t!("disp.hydraulic_rain_tip"));
        ui.end_row();
        ui.label(t!("disp.hydraulic_evaporation"));
        ui.add(egui::DragValue::new(&mut o.hydraulic_evaporation).speed(0.01).range(0.0..=1.0).max_decimals(2))
            .on_hover_text(t!("disp.hydraulic_evaporation_tip"));
        ui.end_row();
        ui.label(t!("disp.hydraulic_capacity"));
        ui.add(egui::DragValue::new(&mut o.hydraulic_capacity).speed(0.05).range(0.0..=10.0).max_decimals(2))
            .on_hover_text(t!("disp.hydraulic_capacity_tip"));
        ui.end_row();
        ui.label(t!("disp.hydraulic_deposition"));
        ui.add(egui::DragValue::new(&mut o.hydraulic_deposition).speed(0.01).range(0.0..=1.0).max_decimals(2))
            .on_hover_text(t!("disp.hydraulic_deposition_tip"));
        ui.end_row();
        ui.label(t!("disp.strength"));
        ui.add(egui::DragValue::new(&mut o.hydraulic_strength).speed(0.01).range(0.0..=1.0).max_decimals(2))
            .on_hover_text(t!("disp.hydraulic_strength_tip"));
        ui.end_row();
        seed_row(ui, &mut o.seed);
    });
}

fn seed_row(ui: &mut egui::Ui, seed: &mut u64) {
    ui.label(t!("disp.terrain_seed"));
    let mut value = (*seed).min(u64::from(u32::MAX)) as u32;
    if ui.add(egui::DragValue::new(&mut value)).on_hover_text(t!("disp.terrain_seed_tip")).changed() {
        *seed = u64::from(value);
    }
    ui.end_row();
}

pub const TR: hp_app::i18n::TrTable = &[
    ("disp.noise_random", "Random", "Aléatoire"),
    (
        "disp.noise_random_tip",
        "A random height between the minimum and the maximum for every vertex",
        "Une hauteur aléatoire entre le minimum et le maximum pour chaque sommet",
    ),
    ("disp.noise_fractal", "Fractal", "Fractal"),
    (
        "disp.noise_fractal_tip",
        "Smooth Perlin noise with several octaves between the minimum and the maximum; the same everywhere on the map, so neighbouring displacements match",
        "Bruit de Perlin doux à plusieurs octaves entre le minimum et le maximum ; identique partout sur la carte, donc les displacements voisins se raccordent",
    ),
    ("disp.noise_scale", "Scale", "Échelle"),
    ("disp.noise_scale_tip", "Size of the biggest features, in units", "Taille des plus grands reliefs, en unités"),
    ("disp.noise_octaves", "Octaves", "Octaves"),
    (
        "disp.noise_octaves_tip",
        "Layers of detail, each twice finer than the previous",
        "Couches de détail, chacune deux fois plus fine que la précédente",
    ),
    ("disp.noise_persistence", "Persistence", "Persistance"),
    (
        "disp.noise_persistence_tip",
        "How much each finer layer counts compared to the previous one",
        "Poids de chaque couche plus fine par rapport à la précédente",
    ),
    ("disp.terrain_seed", "Seed", "Graine"),
    (
        "disp.terrain_seed_tip",
        "The same seed always gives the same result (fractal noise and hydraulic rain)",
        "La même graine donne toujours le même résultat (bruit fractal et pluie de l'érosion hydraulique)",
    ),
    ("disp.erosion", "Erosion", "Érosion"),
    (
        "disp.erosion_hint",
        "Connected displacements erode as one terrain and stay sewn; heights follow the face normal",
        "Les displacements connectés s'érodent comme un seul terrain et restent cousus ; les hauteurs suivent la normale de la face",
    ),
    ("disp.thermal", "Thermal", "Thermique"),
    ("disp.hydraulic", "Hydraulic", "Hydraulique"),
    ("disp.erosion_iterations", "Iterations", "Itérations"),
    ("disp.erosion_iterations_tip", "Number of passes", "Nombre de passes"),
    ("disp.thermal_talus", "Talus angle", "Angle d'éboulis"),
    (
        "disp.thermal_talus_tip",
        "Steepest slope, in degrees, that material stays on; steeper slopes slide",
        "Pente la plus raide, en degrés, sur laquelle la matière tient ; au-delà elle glisse",
    ),
    (
        "disp.thermal_strength_tip",
        "Share of the excess that slides down each pass (0 to 1)",
        "Part de l'excédent qui glisse à chaque passe (0 à 1)",
    ),
    ("disp.thermal_run", "Thermal erosion", "Érosion thermique"),
    (
        "disp.thermal_tip",
        "Material slides down the slopes steeper than the talus angle; the total height is kept. One undo step",
        "La matière glisse sur les pentes plus raides que l'angle d'éboulis ; la hauteur totale est conservée. Une seule étape d'annulation",
    ),
    ("disp.hydraulic_rain", "Rain", "Pluie"),
    ("disp.hydraulic_rain_tip", "Water added to every vertex each pass", "Eau ajoutée à chaque sommet à chaque passe"),
    ("disp.hydraulic_evaporation", "Evaporation", "Évaporation"),
    (
        "disp.hydraulic_evaporation_tip",
        "Share of the water that evaporates each pass (0 to 1)",
        "Part de l'eau qui s'évapore à chaque passe (0 à 1)",
    ),
    ("disp.hydraulic_capacity", "Capacity", "Capacité"),
    (
        "disp.hydraulic_capacity_tip",
        "Sediment the water carries per unit of water and slope",
        "Sédiments que l'eau transporte par unité d'eau et de pente",
    ),
    ("disp.hydraulic_deposition", "Deposition", "Dépôt"),
    (
        "disp.hydraulic_deposition_tip",
        "Share of the sediment above the capacity that settles each pass (0 to 1)",
        "Part des sédiments au-dessus de la capacité qui se déposent à chaque passe (0 à 1)",
    ),
    (
        "disp.hydraulic_strength_tip",
        "Share of the missing capacity dissolved from the ground each pass (0 to 1)",
        "Part de la capacité manquante arrachée au sol à chaque passe (0 à 1)",
    ),
    ("disp.hydraulic_run", "Hydraulic erosion", "Érosion hydraulique"),
    (
        "disp.hydraulic_tip",
        "Rain washes the terrain: it digs where the water runs fast and drops sediment where it slows. One undo step",
        "La pluie lave le terrain : elle creuse où l'eau coule vite et dépose les sédiments où elle ralentit. Une seule étape d'annulation",
    ),
    ("disp.eroded", "{0} displacement(s) eroded.", "{0} displacement(s) érodé(s)."),
    ("disp.erode_nothing", "Nothing changed.", "Rien n'a changé."),
    ("disp.thermal_title", "Thermal erosion", "Érosion thermique"),
    ("disp.hydraulic_title", "Hydraulic erosion", "Érosion hydraulique"),
    ("disp.fractal_title", "Fractal noise", "Bruit fractal"),
    ("shell.undo.thermal_erosion", "Thermal erosion", "Érosion thermique"),
    ("shell.undo.hydraulic_erosion", "Hydraulic erosion", "Érosion hydraulique"),
    ("shell.undo.fractal_noise", "Fractal noise", "Bruit fractal"),
];
