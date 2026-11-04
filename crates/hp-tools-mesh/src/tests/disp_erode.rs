use super::*;
use crate::disp::erode::{NoiseKind, TerrainOp, TerrainOptions, apply};

fn spiked() -> (AppState, FaceRef) {
    let (mut state, _, face) = floor_with_disp(3);
    state.doc.select_faces(&[face], SelectOp::Replace);
    let mut d = state.doc.face_side(face).and_then(|s| s.disp.as_deref().cloned()).expect("disp");
    d.distances[40] = 120.0;
    state.doc.set_disp_data("Spike", vec![(face, d)]).expect("spike");
    (state, face)
}

fn total(state: &AppState, face: FaceRef) -> f64 {
    distances(state, face).iter().sum()
}

fn peak(state: &AppState, face: FaceRef) -> f64 {
    distances(state, face).iter().copied().fold(f64::MIN, f64::max)
}

#[test]
fn thermal_erosion_levels_a_spike_in_one_undo_step() {
    let (mut state, face) = spiked();
    let (steps, sum) = (undo_steps(&state), total(&state, face));
    let opts = TerrainOptions {
        thermal_iterations: 60,
        thermal_talus: 20.0,
        thermal_strength: 1.0,
        ..TerrainOptions::default()
    };
    assert!(apply(&mut state, TerrainOp::Thermal, &opts) > 0);
    assert_eq!(undo_steps(&state), steps + 1);
    assert_eq!(state.doc.undo_label(), Some("Thermal erosion"));
    assert!(peak(&state, face) < 60.0, "{}", peak(&state, face));
    assert!((total(&state, face) - sum).abs() < 1e-3 * sum.abs().max(1.0), "{} {sum}", total(&state, face));
    state.doc.undo();
    assert_eq!(peak(&state, face), 120.0);
}

#[test]
fn hydraulic_erosion_is_repeatable_with_the_same_seed() {
    let opts = TerrainOptions { hydraulic_iterations: 30, seed: 11, ..TerrainOptions::default() };
    let (mut a, fa) = spiked();
    let (mut b, fb) = spiked();
    assert!(apply(&mut a, TerrainOp::Hydraulic, &opts) > 0);
    apply(&mut b, TerrainOp::Hydraulic, &opts);
    assert_eq!(distances(&a, fa), distances(&b, fb));
    assert_eq!(a.doc.undo_label(), Some("Hydraulic erosion"));
    assert!(peak(&a, fa) < 120.0);
}

#[test]
fn fractal_noise_is_deterministic_and_options_round_trip() {
    let (mut state, _, face) = floor_with_disp(3);
    state.doc.select_faces(&[face], SelectOp::Replace);
    state.settings.mesh.noise_min = -10.0;
    state.settings.mesh.noise_max = 10.0;
    let mut opts = TerrainOptions::default();
    assert!(opts.set("noise_type", "fractal") && opts.set("terrain_seed", "5") && opts.set("noise_scale", "100"));
    assert_eq!(opts.noise_kind, NoiseKind::Fractal);
    assert!(apply(&mut state, TerrainOp::Noise, &opts) > 0);
    assert_eq!(state.doc.undo_label(), Some("Fractal noise"));
    let first = distances(&state, face);
    assert!(first.iter().all(|d| d.abs() <= 10.0 + 1e-9) && first.iter().any(|d| d.abs() > 0.5));
    state.doc.undo();
    apply(&mut state, TerrainOp::Noise, &opts);
    assert_eq!(distances(&state, face), first);
    assert_eq!(TerrainOptions::from_values(&opts.values()), opts);
    for (k, v) in [("noise_octaves", "9"), ("thermal_talus", "90"), ("noise_type", "x"), ("hydraulic_rain", "nan")] {
        assert!(!opts.set(k, v), "{k} {v}");
    }
}
