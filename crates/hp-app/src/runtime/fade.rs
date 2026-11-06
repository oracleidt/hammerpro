use hp_vmf::Entity;

#[derive(Clone, Debug)]
struct Playback {
    entity: hp_vmf::Id,
    serial: u64,
    start: f64,
    duration: f64,
    hold: f64,
    color: [f32; 4],
    fade_in: bool,
    modulate: bool,
}

#[derive(Clone, Debug, Default)]
pub struct FadePreview {
    playback: Option<Playback>,
}

fn number(e: &Entity, key: &str, default: f64) -> f64 {
    e.get(key).and_then(|s| s.parse::<f64>().ok()).filter(|v| v.is_finite()).unwrap_or(default)
}

impl FadePreview {
    pub fn play(&mut self, entity: &Entity, serial: u64, time: f64) {
        if !entity.classname.eq_ignore_ascii_case("env_fade") || !time.is_finite() {
            return;
        }
        let flags = number(entity, "spawnflags", 0.0) as u32;
        let mut color = [0.0, 0.0, 0.0, (number(entity, "renderamt", 255.0) / 255.0).clamp(0.0, 1.0) as f32];
        for (dst, src) in color[..3].iter_mut().zip(entity.get("rendercolor").unwrap_or("0 0 0").split_whitespace()) {
            *dst = src.parse::<f32>().ok().filter(|x| x.is_finite()).unwrap_or(0.0).clamp(0.0, 255.0) / 255.0;
        }
        self.playback = Some(Playback {
            entity: entity.id,
            serial,
            start: time,
            duration: number(entity, "duration", 2.0).clamp(0.0, 3600.0),
            hold: number(entity, "holdtime", 0.0).clamp(0.0, 3600.0),
            color,
            fade_in: flags & 1 != 0,
            modulate: flags & 2 != 0,
        });
    }

    pub fn stop(&mut self) {
        self.playback = None;
    }

    pub fn sample(&self, state: &crate::AppState) -> Option<([f32; 4], bool)> {
        let p = self.playback.as_ref()?;
        let age = state.frame.time - p.start;
        if state.sidecar.serial() != p.serial
            || !age.is_finite()
            || age < 0.0
            || age >= (p.duration + p.hold).max(0.001)
            || !state.doc.is_entity_visible(p.entity)
            || !state.doc.scene().entity(p.entity).is_some_and(|n| n.entity.classname.eq_ignore_ascii_case("env_fade"))
        {
            return None;
        }
        let amount = if p.fade_in {
            1.0 - ((age - p.hold) / p.duration.max(0.001)).clamp(0.0, 1.0)
        } else {
            (age / p.duration.max(0.001)).clamp(0.0, 1.0)
        } as f32;
        let mut color = p.color;
        color[3] *= amount;
        Some((color, p.modulate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_respects_hold_order_and_stops_even_with_stay_out() {
        for flags in [0, 1, 2, 8, 9] {
            let mut state = crate::testing::test_state();
            let mut entity = Entity { id: 900, classname: "env_fade".into(), ..Default::default() };
            entity.set("duration", "2");
            entity.set("holdtime", "1");
            entity.set("rendercolor", "255 64 0");
            entity.set("renderamt", "128");
            entity.set("spawnflags", flags.to_string());
            let mut map = hp_vmf::Map::new_empty();
            map.entities.push(entity.clone());
            state.doc = hp_editor::Document::from_map(map);
            state.runtime.fade.play(&entity, state.sidecar.serial(), 10.0);
            for (time, out, fade_in) in [(10.0, 0.0, 1.0), (10.5, 0.25, 1.0), (12.0, 1.0, 0.5), (12.5, 1.0, 0.25)] {
                state.frame.time = time;
                let (color, modulate) = state.runtime.fade.sample(&state).unwrap();
                let expected = if flags & 1 != 0 { fade_in } else { out };
                assert!((color[3] - expected * 128.0 / 255.0).abs() < 1e-5);
                assert_eq!(modulate, flags & 2 != 0);
                assert_eq!(color[0], 1.0);
            }
            state.frame.time = 13.0;
            assert!(state.runtime.fade.sample(&state).is_none());
            state.frame.time = 11.0;
            state.runtime.fade.stop();
            assert!(state.runtime.fade.sample(&state).is_none());
        }
    }
}
