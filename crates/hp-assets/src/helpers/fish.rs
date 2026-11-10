use std::time::{Duration, Instant};

use glam::DVec3;
use hp_app::runtime::helpers::fish::{self as rt, Fish, FishPool, WaterBox};
use hp_app::runtime::render::render_display;
use hp_editor::{Document, Id};
use hp_render::{ModelInstance, ObjectFlags};

use crate::entities::{BOX_ALPHA, class_of, entity_object, point_color};
use crate::meshes::box_mesh;
use crate::models::{ModelEntry, entity_matrix};
use crate::producer::{DerivedBuild, DerivedObject, DerivedProducer, ProducerChanges, ProducerCx};
use crate::util::model_key;

const FRAME: Duration = Duration::from_millis(33);
const PLACEHOLDER: DVec3 = DVec3::new(6.0, 6.0, 3.0);

struct School {
    id: Id,
    model: String,
    fish: Vec<Fish>,
    color: [u8; 3],
    placeholders: Vec<u64>,
}

pub(crate) fn placeholder_key(pool: Id, fish: usize) -> u64 {
    (u64::from(pool) << 16) | (fish as u64 & 0xffff)
}

#[derive(Default)]
pub(crate) struct FishProducer {
    schools: Vec<School>,
    water: Option<Vec<WaterBox>>,
    dirty: bool,
    waiting: bool,
    shown: Option<(bool, bool)>,
    start: Option<Instant>,
    last: Option<Instant>,
}

pub(crate) fn pool_ids(doc: &Document) -> Vec<Id> {
    let mut ids: Vec<Id> = doc
        .scene()
        .entities()
        .iter()
        .filter(|(_, n)| rt::is_fish_pool(&n.entity.classname))
        .map(|(id, _)| *id)
        .collect();
    ids.sort_unstable();
    ids
}

impl FishProducer {
    fn swims(&self) -> bool {
        self.shown.is_some_and(|s| s.0 && s.1)
            && self.schools.iter().any(|s| s.fish.iter().any(|f| f.swim_radius > 0.0))
    }

    fn water_boxes(cx: &mut ProducerCx<'_>, pools: &[(Id, FishPool)]) -> Option<Vec<WaterBox>> {
        let scene = cx.doc.scene();
        let mut boxes = Vec::new();
        let mut pending = false;
        let near = |b: &hp_geom::Aabb| {
            pools.iter().any(|(_, p)| {
                let r = DVec3::splat(p.range + rt::SURFACE_MARGIN);
                b.min.cmple(p.origin + r).all() && b.max.cmpge(p.origin - r).all()
            })
        };
        let mut ids: Vec<Id> =
            scene.solids().keys().copied().filter(|id| scene.solid_bounds(*id).is_some_and(|b| near(&b))).collect();
        ids.sort_unstable();
        for id in ids {
            let Some(node) = scene.solid(id) else { continue };
            let mut water = false;
            for side in &node.solid.sides {
                match cx.assets.material_info_impl(&side.material) {
                    Some(info) if info.is_water => {
                        water = true;
                        break;
                    }
                    Some(_) => {}
                    None => pending = true,
                }
            }
            if water && let Some(b) = scene.solid_bounds(id) {
                boxes.push(WaterBox { min: b.min, max: b.max });
            }
        }
        (!pending).then_some(boxes)
    }
}

impl DerivedProducer for FishProducer {
    fn name(&self) -> &'static str {
        "fish"
    }

    fn changed(&mut self, changes: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool {
        let d = render_display();
        let shown = (d.fish && cx.opts.show_models, d.fish_swim);
        if self.shown != Some(shown) || changes.full || changes.entities {
            self.shown = Some(shown);
            self.dirty = true;
            self.water = None;
            return true;
        }
        let scene = cx.doc.scene();
        let touches = |id: &Id| {
            self.schools.iter().any(|s| s.id == *id)
                || scene.entity(*id).is_some_and(|n| rt::is_fish_pool(&n.entity.classname))
        };
        if changes.doc.entities_changed.iter().chain(&changes.doc.entities_removed).any(touches) {
            self.dirty = true;
            self.water = None;
        }
        if !changes.doc.solids_changed.is_empty() || !changes.doc.solids_removed.is_empty() {
            self.water = None;
            self.dirty |= !self.schools.is_empty();
        }
        if self.dirty || self.waiting {
            return true;
        }
        if !self.swims() {
            return false;
        }
        let since = self.last.map_or(FRAME, |t| t.elapsed());
        if since >= FRAME {
            return true;
        }
        if hp_app::runtime::render::animations_running()
            && let Some(ctx) = cx.assets.shared.ctx()
        {
            ctx.request_repaint_after(FRAME - since);
        }
        false
    }

    fn build(&mut self, cx: &mut ProducerCx<'_>) -> DerivedBuild {
        let (on, swim) = self.shown.unwrap_or((false, false));
        self.waiting = false;
        let mut out = DerivedBuild::default();
        if !on {
            self.schools.clear();
            self.dirty = false;
            self.start = None;
            out.clear = true;
            out.models = Some(Vec::new());
            return out;
        }
        if self.dirty {
            self.dirty = false;
            out.remove.extend(self.schools.iter().flat_map(|s| s.placeholders.iter().copied()));
            let pools: Vec<(Id, FishPool)> = pool_ids(cx.doc)
                .into_iter()
                .filter_map(|id| Some((id, rt::fish_pool(&cx.doc.scene().entity(id)?.entity)?)))
                .collect();
            if !pools.is_empty() && self.water.is_none() {
                self.water = Self::water_boxes(cx, &pools);
                if self.water.is_none() {
                    self.waiting = true;
                    self.dirty = true;
                }
            }
            let water = self.water.as_deref().unwrap_or(&[]);
            let fgd = cx.assets.fgd.clone();
            self.schools = pools
                .into_iter()
                .map(|(id, pool)| {
                    let color = cx
                        .doc
                        .scene()
                        .entity(id)
                        .map_or([255; 3], |n| point_color(&n.entity, class_of(fgd.as_deref(), &n.entity)));
                    School {
                        id,
                        model: model_key(&pool.model),
                        fish: rt::place_fish(u64::from(id), &pool, water),
                        color,
                        placeholders: Vec::new(),
                    }
                })
                .collect();
        }
        let now = Instant::now();
        let t = if swim { self.start.get_or_insert(now).elapsed().as_secs_f64() } else { 0.0 };
        self.last = Some(now);
        let mut models = Vec::new();
        let fallback = model_key(rt::FALLBACK_MODEL);
        for school in &mut self.schools {
            let Some(model) = cx.assets.render_model(cx.renderer, &school.model, 0) else {
                if matches!(cx.assets.models.map.get(&school.model), None | Some(ModelEntry::Loading)) {
                    self.waiting = true;
                } else if school.model != fallback {
                    school.model = fallback.clone();
                    self.waiting = true;
                } else if school.placeholders.is_empty() {
                    let [r, g, b] = school.color;
                    let mat = cx.assets.flat_material([r, g, b, BOX_ALPHA]);
                    for (i, f) in school.fish.iter().enumerate() {
                        let key = placeholder_key(school.id, i);
                        let mesh = box_mesh(f.pos - PLACEHOLDER, f.pos + PLACEHOLDER, mat, true, school.color);
                        out.upsert.push(DerivedObject {
                            key,
                            mesh,
                            owner: Some(school.id),
                            flags: ObjectFlags::NO_PROBE,
                        });
                        school.placeholders.push(key);
                    }
                }
                continue;
            };
            if !school.placeholders.is_empty() {
                out.remove.append(&mut school.placeholders);
            }
            for f in &school.fish {
                let (pos, yaw) = rt::fish_at(f, t);
                let m = entity_matrix(pos, DVec3::new(0.0, yaw, 0.0), 1.0);
                models.push(ModelInstance {
                    model,
                    transform: m.as_mat4().to_cols_array_2d(),
                    object: entity_object(school.id),
                    tint: [1.0; 4],
                    fade: [0.0; 2],
                });
            }
        }
        if self.swims() && hp_app::runtime::render::animations_running() {
            cx.assets.shared.request_repaint();
        }
        out.models = Some(models);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_keys_are_distinct() {
        assert_ne!(placeholder_key(1, 0), placeholder_key(0, 1));
        assert_ne!(placeholder_key(7, 255), placeholder_key(8, 0));
        assert_eq!(placeholder_key(7, 3) >> 16, 7);
    }

    #[test]
    fn pools_are_found_in_id_order() {
        let mut state = hp_app::testing::test_state();
        let b = state.doc.create_point_entity("func_fish_pool", DVec3::new(0.0, 0.0, 0.0), &[]).expect("pool");
        let a = state.doc.create_point_entity("func_FISH_pool", DVec3::new(64.0, 0.0, 0.0), &[]).expect("pool");
        state.doc.create_point_entity("info_target", DVec3::ZERO, &[]).expect("target");
        let mut want = vec![a, b];
        want.sort_unstable();
        assert_eq!(pool_ids(&state.doc), want);
    }
}
