use crate::document::Document;
use crate::ids::{IdSet, id_set};
use crate::ops::{ENTITY_COLOR, solid_color};
use crate::scene::{Owner, Scene, SolidNode};
use crate::selection::ObjRef;
use crate::{EditResult, Id};

pub const UNDO_LABEL: &str = "Set display color";
const MERGE_TAG: &str = "hp-editor:display-color";

pub const PALETTE: [[u8; 3]; 16] = [
    [240, 140, 40],
    [215, 170, 20],
    [140, 195, 40],
    [50, 180, 80],
    [30, 170, 150],
    [40, 165, 225],
    [75, 120, 235],
    [125, 100, 230],
    [170, 90, 220],
    [220, 70, 180],
    [235, 115, 150],
    [175, 115, 60],
    [150, 150, 55],
    [110, 140, 175],
    [155, 155, 160],
    [205, 175, 125],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayColor {
    Set([u8; 3]),
    Random,
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayColorState {
    Empty,
    Default,
    Custom([u8; 3]),
    Mixed,
}

pub fn is_default_color(c: [u8; 3]) -> bool {
    (c[0] == 0 && c[1] >= 100 && c[2] >= 100) || c == ENTITY_COLOR
}

pub fn custom_color(c: [u8; 3]) -> [u8; 3] {
    if c == ENTITY_COLOR {
        return [c[0] + 1, c[1], c[2]];
    }
    if is_default_color(c) {
        return [1, c[1], c[2]];
    }
    c
}

pub fn chosen(c: [u8; 3]) -> Option<[u8; 3]> {
    (!is_default_color(c)).then_some(c)
}

fn hsv(h: f64, s: f64, v: f64) -> [u8; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i as i32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    let c = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    [c(r), c(g), c(b)]
}

fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

pub fn random_color(seed: u64, index: usize) -> [u8; 3] {
    const GOLDEN: f64 = 0.618_033_988_749_895;
    const RED_GAP: f64 = 0.06;
    let start = (mix(seed) >> 11) as f64 / (1u64 << 53) as f64;
    let raw = (start + index as f64 * GOLDEN).rem_euclid(1.0);
    let hue = RED_GAP + raw * (1.0 - 2.0 * RED_GAP);
    let jitter = (mix(seed ^ index as u64) >> 40) as f64 / (1u64 << 24) as f64;
    custom_color(hsv(hue, 0.62 + 0.18 * jitter, 0.92 - 0.12 * jitter))
}

pub fn solid_chosen_color(scene: &Scene, node: &SolidNode) -> Option<[u8; 3]> {
    let own = chosen(node.solid.editor.color);
    match node.owner {
        Owner::Entity(e) if own.is_none() => scene.entity(e).and_then(|en| chosen(en.entity.editor.color)),
        _ => own,
    }
}

fn unit_color(scene: &Scene, obj: ObjRef) -> Option<Option<[u8; 3]>> {
    match obj {
        ObjRef::Solid(id) => scene.solid(id).map(|n| solid_chosen_color(scene, n)),
        ObjRef::Entity(id) => scene.entity(id).map(|n| chosen(n.entity.editor.color)),
    }
}

impl Document {
    pub fn display_color_of(&self, obj: ObjRef) -> Option<Option<[u8; 3]>> {
        unit_color(&self.scene, obj)
    }

    pub fn display_color_state(&self, objs: &[ObjRef]) -> DisplayColorState {
        let mut out = DisplayColorState::Empty;
        for o in objs {
            let Some(c) = unit_color(&self.scene, *o) else { continue };
            let next = c.map_or(DisplayColorState::Default, DisplayColorState::Custom);
            out = match out {
                DisplayColorState::Empty => next,
                cur if cur == next => cur,
                _ => return DisplayColorState::Mixed,
            };
        }
        out
    }

    pub fn set_display_color(&mut self, objs: &[ObjRef], change: DisplayColor, merge: bool) -> EditResult<usize> {
        let units: Vec<ObjRef> = {
            let mut seen = indexmap::IndexSet::<ObjRef, crate::IdBuildHasher>::default();
            for o in objs {
                if self.scene.contains(*o) {
                    seen.insert(*o);
                }
            }
            seen.into_iter().collect()
        };
        if units.is_empty() {
            return Err(crate::EditError::Nothing("nothing to color"));
        }
        let seed = self.revision();
        let mut solids: Vec<(Id, [u8; 3])> = Vec::new();
        let mut entities: Vec<(Id, [u8; 3])> = Vec::new();
        for (i, u) in units.iter().enumerate() {
            let pick = |id: Id, entity: bool| match change {
                DisplayColor::Set(c) => custom_color(c),
                DisplayColor::Random => random_color(seed, i),
                DisplayColor::Reset if entity => ENTITY_COLOR,
                DisplayColor::Reset => solid_color(id),
            };
            match *u {
                ObjRef::Solid(id) => solids.push((id, pick(id, false))),
                ObjRef::Entity(id) => {
                    let c = pick(id, true);
                    entities.push((id, c));
                    for sid in self.scene.entity_solid_ids(id) {
                        let sc = if change == DisplayColor::Reset { solid_color(sid) } else { c };
                        solids.push((sid, sc));
                    }
                }
            }
        }
        let groups = self.fully_covered_groups(&units);
        let group_colors: Vec<(Id, [u8; 3])> = match change {
            DisplayColor::Set(c) => groups.iter().map(|g| (*g, custom_color(c))).collect(),
            DisplayColor::Reset => groups.iter().map(|g| (*g, solid_color(*g))).collect(),
            DisplayColor::Random => Vec::new(),
        };
        let solid_updates: Vec<(Id, hp_vmf::Solid)> = solids
            .iter()
            .filter_map(|(id, c)| {
                let n = self.scene.solid(*id)?;
                (n.solid.editor.color != *c).then(|| {
                    let mut s = n.solid.clone();
                    s.editor.color = *c;
                    (*id, s)
                })
            })
            .collect();
        let entity_updates: Vec<(Id, [u8; 3])> = entities
            .into_iter()
            .filter(|(id, c)| self.scene.entity(*id).is_some_and(|n| n.entity.editor.color != *c))
            .collect();
        let group_updates: Vec<hp_vmf::Group> = group_colors
            .into_iter()
            .filter_map(|(id, c)| {
                let n = self.scene.group(id)?;
                (n.group.editor.color != c).then(|| {
                    let mut g = n.group.clone();
                    g.editor.color = c;
                    g
                })
            })
            .collect();
        let changed = solid_updates.len() + entity_updates.len();
        if changed == 0 && group_updates.is_empty() {
            return Ok(0);
        }
        let nested = self.in_transaction();
        if !nested {
            if merge {
                self.begin_merge(UNDO_LABEL, MERGE_TAG);
            } else {
                self.begin(UNDO_LABEL);
            }
        }
        self.update_solids(solid_updates);
        for (id, c) in entity_updates {
            self.update_entity(id, |e| e.editor.color = c);
        }
        for g in group_updates {
            self.put_group(g);
        }
        if !nested {
            self.commit();
        }
        Ok(changed)
    }

    fn fully_covered_groups(&self, units: &[ObjRef]) -> IdSet {
        let set: std::collections::HashSet<ObjRef> = units.iter().copied().collect();
        let mut roots = id_set(0);
        for u in units {
            if let Some(g) = self.scene.object_group(*u) {
                roots.insert(self.scene.group_root(g));
            }
        }
        let mut out = id_set(0);
        for root in roots {
            for g in self.scene.group_subtree(root) {
                let members = self.scene.group_members(g);
                if !members.is_empty() && members.iter().all(|m| set.contains(m)) {
                    out.insert(g);
                }
            }
        }
        out
    }
}
