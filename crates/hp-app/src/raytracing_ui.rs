use egui::{RichText, Ui};

use crate::i18n::TrTable;
use crate::runtime::RayTracingStatus;
use crate::{t, tf, theme};

fn decimal(v: f64, digits: usize) -> String {
    let s = format!("{v:.digits$}");
    if crate::i18n::lang() == crate::i18n::Lang::Fr { s.replace('.', ",") } else { s }
}

pub fn status_line(rt: &RayTracingStatus) -> Option<String> {
    if !rt.active && rt.device.is_empty() {
        return None;
    }
    let device = if rt.device.is_empty() {
        t!("raytracing.device_pending").to_owned()
    } else if rt.gpu {
        tf!("raytracing.device_gpu", rt.device)
    } else {
        tf!("raytracing.device_cpu", rt.device)
    };
    if rt.converged {
        return Some(tf!("raytracing.converged", device, rt.samples, decimal(f64::from(rt.seconds), 1)));
    }
    if rt.pass == 0 {
        return Some(device);
    }
    Some(tf!("raytracing.pass", device, rt.pass, rt.samples, rt.target_samples))
}

pub fn status(ui: &mut Ui, rt: &RayTracingStatus) {
    if let Some(line) = status_line(rt) {
        if rt.active && !rt.converged && rt.pass > 0 {
            ui.add(crate::widgets::progress_bar(rt.fraction()).text(line));
        } else {
            ui.label(RichText::new(line).weak());
        }
    }
    if !rt.device.is_empty() && !rt.gpu {
        ui.label(RichText::new(format!("⚠ {}", t!("raytracing.no_gpu"))).color(theme::warning()).small());
    }
}

pub const TR: TrTable = &[
    ("raytracing.device_pending", "Looking for a graphics card", "Recherche d'une carte graphique"),
    ("raytracing.device_gpu", "GPU: {0}", "GPU : {0}"),
    ("raytracing.device_cpu", "CPU: {0}", "CPU : {0}"),
    (
        "raytracing.pass",
        "{0}, pass {1}, {2} of {3} samples per luxel",
        "{0}, passe {1}, {2} sur {3} échantillons par luxel",
    ),
    (
        "raytracing.converged",
        "{0}, converged at {1} samples per luxel in {2} s",
        "{0}, convergé à {1} échantillons par luxel en {2} s",
    ),
    (
        "raytracing.no_gpu",
        "No GPU detected, compiling with Ray Tracing will take longer",
        "Aucun GPU détecté, la compilation avec Ray Tracing prendra plus de temps",
    ),
    ("raytracing.settings_heading", "Ray traced lighting preview", "Aperçu de l'éclairage en ray tracing"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_clean() {
        assert!(crate::i18n::lint_table(TR).is_empty());
    }

    #[test]
    fn status_tells_the_device_and_progress() {
        let _lang = crate::testing::lang_lock();
        let mut rt = RayTracingStatus::default();
        assert!(status_line(&rt).is_none());
        rt.active = true;
        assert_eq!(status_line(&rt).unwrap(), "Looking for a graphics card");
        rt.device = "8 threads".into();
        rt.pass = 2;
        rt.samples = 32;
        rt.target_samples = 256;
        assert_eq!(status_line(&rt).unwrap(), "CPU: 8 threads, pass 2, 32 of 256 samples per luxel");
        rt.gpu = true;
        rt.device = "RTX".into();
        assert!(status_line(&rt).unwrap().starts_with("GPU: RTX"));
        rt.converged = true;
        rt.seconds = 4.25;
        assert!(status_line(&rt).unwrap().contains("converged at 32"));
    }
}
