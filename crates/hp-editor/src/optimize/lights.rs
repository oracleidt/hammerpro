use glam::DVec3;
use hp_geom::Aabb;

use super::{Finding, Impact, SolidIndex, Topic, fmt_point};
use crate::scene::Owner;
use crate::selection::ObjRef;
use crate::{Document, EditError, EditResult, Id};

const NEAR_PROP: f64 = 96.0;
const NEAR_TEXTURE: f64 = 64.0;
const MOVE_REACH: f64 = 512.0;
const REDUNDANT: f64 = 24.0;
const SPOT_WALL: f64 = 20.0;

const LAMP_WORDS: [&str; 14] = [
    "light",
    "lamp",
    "lantern",
    "bulb",
    "fluor",
    "chandelier",
    "candle",
    "torch",
    "sconce",
    "spotlight",
    "neon",
    "flare",
    "beacon",
    "fixture",
];
const SPRITE_CLASSES: [&str; 5] =
    ["env_sprite", "env_sprite_oriented", "env_lightglow", "point_spotlight", "beam_spotlight"];
const LIGHT_CLASSES: [&str; 3] = ["light", "light_spot", "light_dynamic"];

pub struct LightKnowledge<'a> {
    pub model_bounds: &'a dyn Fn(&hp_vmf::Entity) -> Option<Aabb>,
    pub light_material: &'a dyn Fn(&str) -> bool,
    pub texlight: &'a dyn Fn(&str) -> bool,
    pub max_switchable: usize,
}

pub fn light_material_by_name(material: &str) -> bool {
    let m = material.to_ascii_lowercase().replace('\\', "/");
    if m.starts_with("tools/") {
        return false;
    }
    let last = m.rsplit('/').next().unwrap_or(&m);
    m.starts_with("lights/")
        || m.contains("/lights/")
        || ["light", "lamp", "fluor", "neon"].iter().any(|w| last.contains(w)) && !last.contains("lightmap")
}

pub fn lamp_model(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    let last = m.rsplit('/').next().unwrap_or(&m);
    !last.contains("lighthouse") && !last.contains("lightswitch") && LAMP_WORDS.iter().any(|w| last.contains(w))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FixtureKind {
    Prop,
    Texture,
    Sprite,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fixture {
    pub kind: FixtureKind,
    pub key: String,
    pub bounds: Aabb,
    pub emit: DVec3,
    pub object: ObjRef,
    pub texlight: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightRec {
    pub id: Id,
    pub class: String,
    pub origin: DVec3,
    pub rgb: [f64; 3],
    pub brightness: f64,
    pub named: bool,
    pub styled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LightFix {
    Move { id: Id, to: DVec3 },
    Add { at: DVec3, value: String },
    SetKey { id: Id, key: String, value: String },
    Merge { keep: Id, remove: Id, value: String },
    Delete(Id),
    AddSun { at: DVec3 },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightAudit {
    pub lights: usize,
    pub fixtures: usize,
    pub findings: Vec<Finding>,
    pub fixes: Vec<(usize, LightFix)>,
}

impl LightAudit {
    pub fn fixes_for(&self, kinds: &[&str]) -> Vec<LightFix> {
        self.fixes
            .iter()
            .filter(|(i, _)| {
                self.findings.get(*i).and_then(|f| f.fix).is_some_and(|k| kinds.is_empty() || kinds.contains(&k))
            })
            .map(|(_, f)| f.clone())
            .collect()
    }
}

fn parse_light(s: &str) -> Option<([f64; 3], f64)> {
    let v: Vec<f64> = s.split_ascii_whitespace().take(4).map_while(|p| p.parse::<f64>().ok()).collect();
    let (rgb, b) = match v.len() {
        4 => ([v[0], v[1], v[2]], v[3]),
        3 => ([v[0], v[1], v[2]], 200.0),
        _ => return None,
    };
    (rgb.iter().chain([&b]).all(|x| x.is_finite() && *x >= 0.0)).then_some((rgb, b))
}

fn chroma(rgb: [f64; 3]) -> [f64; 3] {
    let s = rgb[0] + rgb[1] + rgb[2];
    if s <= 0.0 { [1.0 / 3.0; 3] } else { [rgb[0] / s, rgb[1] / s, rgb[2] / s] }
}

fn chroma_distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (a, b) = (chroma(a), chroma(b));
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn light_text(rgb: [f64; 3], b: f64) -> String {
    format!("{:.0} {:.0} {:.0} {:.0}", rgb[0], rgb[1], rgb[2], b)
}

fn key_num(e: &hp_vmf::Entity, k: &str) -> Option<f64> {
    e.get(k)
        .and_then(|v| v.split_ascii_whitespace().next())
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

pub fn light_direction(e: &hp_vmf::Entity) -> DVec3 {
    let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
    let angle = key_num(e, "angle").unwrap_or(0.0);
    if angle == -1.0 {
        return DVec3::Z;
    }
    if angle == -2.0 {
        return DVec3::NEG_Z;
    }
    let yaw = if angle != 0.0 { angle } else { angles.y };
    let pitch = key_num(e, "pitch").filter(|p| *p != 0.0).unwrap_or(angles.x);
    let (sp, cp) = pitch.to_radians().sin_cos();
    let (sy, cy) = yaw.to_radians().sin_cos();
    DVec3::new(cp * cy, cp * sy, sp)
}

fn distance_to(b: &Aabb, p: DVec3) -> f64 {
    (p.clamp(b.min, b.max) - p).length()
}

fn hanging(idx: &SolidIndex, b: &Aabb) -> bool {
    let c = b.center();
    let up = (1..=8).any(|i| idx.contains(DVec3::new(c.x, c.y, b.max.z + f64::from(i) * 8.0)).is_some());
    let down = (1..=8).any(|i| idx.contains(DVec3::new(c.x, c.y, b.min.z - f64::from(i) * 8.0)).is_some());
    up && (!down || b.size().z < 48.0)
}

pub fn fixtures(doc: &Document, k: &LightKnowledge<'_>, idx: &SolidIndex) -> Vec<Fixture> {
    let mut out = Vec::new();
    for n in doc.scene.entities.values() {
        let e = &n.entity;
        if !doc.is_visible(ObjRef::Entity(e.id)) {
            continue;
        }
        let class = e.classname.to_ascii_lowercase();
        let Some(origin) = e.origin() else { continue };
        if class.starts_with("prop_") {
            let model = e.get("model").unwrap_or_default();
            if !lamp_model(model) {
                continue;
            }
            let bounds = (k.model_bounds)(e)
                .filter(|b| !b.is_empty())
                .unwrap_or_else(|| Aabb::new(origin - DVec3::splat(8.0), origin + DVec3::splat(8.0)));
            let c = bounds.center();
            let emit = if hanging(idx, &bounds) {
                DVec3::new(c.x, c.y, bounds.min.z - 6.0)
            } else {
                DVec3::new(c.x, c.y, bounds.max.z + 6.0)
            };
            out.push(Fixture {
                kind: FixtureKind::Prop,
                key: model.to_ascii_lowercase(),
                bounds,
                emit,
                object: ObjRef::Entity(e.id),
                texlight: false,
            });
        } else if SPRITE_CLASSES.contains(&class.as_str()) {
            out.push(Fixture {
                kind: FixtureKind::Sprite,
                key: e.get("model").unwrap_or(&class).to_ascii_lowercase(),
                bounds: Aabb::new(origin - DVec3::splat(4.0), origin + DVec3::splat(4.0)),
                emit: origin,
                object: ObjRef::Entity(e.id),
                texlight: false,
            });
        }
    }
    for n in doc.scene.solids.values() {
        if !n.geom.valid || !doc.is_visible(ObjRef::Solid(n.solid.id)) {
            continue;
        }
        if let Owner::Entity(owner) = n.owner
            && doc.scene.entity(owner).is_some_and(|e| {
                let c = e.entity.classname.to_ascii_lowercase();
                c.starts_with("trigger_") || c.starts_with("func_areaportal") || c == "func_occluder"
            })
        {
            continue;
        }
        let mut best: Option<(&hp_geom::FaceGeom, &str)> = None;
        let mut b = Aabb::EMPTY;
        let mut texlight = false;
        for f in &n.geom.faces {
            let Some(side) = n.solid.sides.get(f.side_index) else { continue };
            if !(k.light_material)(&side.material) {
                continue;
            }
            texlight |= (k.texlight)(&side.material);
            for v in &f.vertices {
                b.extend(*v);
            }
            if best.is_none_or(|(bf, _)| f.area() > bf.area()) {
                best = Some((f, side.material.as_str()));
            }
        }
        let Some((face, material)) = best else { continue };
        out.push(Fixture {
            kind: FixtureKind::Texture,
            key: material.to_ascii_lowercase(),
            bounds: b,
            emit: face.center() + face.normal() * 6.0,
            object: ObjRef::Solid(n.solid.id),
            texlight,
        });
    }
    out
}

fn lights(doc: &Document) -> Vec<LightRec> {
    let mut out: Vec<LightRec> = doc
        .scene
        .entities
        .values()
        .filter(|n| LIGHT_CLASSES.iter().any(|c| n.entity.classname.eq_ignore_ascii_case(c)))
        .filter(|n| doc.is_visible(ObjRef::Entity(n.entity.id)))
        .filter_map(|n| {
            let e = &n.entity;
            let origin = e.origin()?;
            let (rgb, brightness) = e.get("_light").and_then(parse_light).unwrap_or(([255.0; 3], 200.0));
            Some(LightRec {
                id: e.id,
                class: e.classname.to_ascii_lowercase(),
                origin,
                rgb,
                brightness,
                named: e.get("targetname").is_some_and(|t| !t.trim().is_empty()),
                styled: key_num(e, "style").is_some_and(|s| s != 0.0),
            })
        })
        .collect();
    out.sort_by_key(|l| l.id);
    out
}

fn near_fixture(f: &Fixture, p: DVec3) -> bool {
    let reach = if f.kind == FixtureKind::Texture { NEAR_TEXTURE } else { NEAR_PROP };
    distance_to(&f.bounds, p) <= reach
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

pub fn audit(doc: &Document, k: &LightKnowledge<'_>) -> LightAudit {
    let idx = SolidIndex::new(doc, false);
    let fx = fixtures(doc, k, &idx);
    let ls = lights(doc);
    let mut a = LightAudit { lights: ls.len(), fixtures: fx.len(), ..LightAudit::default() };
    let push = |a: &mut LightAudit, f: Finding, fix: Option<LightFix>| {
        a.findings.push(f);
        if let Some(x) = fix {
            a.fixes.push((a.findings.len() - 1, x));
        }
    };
    let nearest = |p: DVec3| {
        fx.iter().enumerate().map(|(i, f)| (i, distance_to(&f.bounds, p))).min_by(|x, y| x.1.total_cmp(&y.1))
    };
    let fixture_of: Vec<Option<usize>> =
        ls.iter().map(|l| nearest(l.origin).filter(|(i, _)| near_fixture(&fx[*i], l.origin)).map(|(i, _)| i)).collect();
    let props: Vec<Aabb> = doc
        .scene
        .entities
        .values()
        .filter(|n| n.entity.classname.eq_ignore_ascii_case("prop_static"))
        .filter(|n| n.entity.get("disableshadows").is_none_or(|v| v.trim() == "0"))
        .filter_map(|n| (k.model_bounds)(&n.entity))
        .filter(|b| !b.is_empty() && b.size().min_element() > 8.0)
        .collect();
    let groups: std::collections::HashMap<String, Vec<usize>> = {
        let mut g: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
        for (li, f) in fixture_of.iter().enumerate() {
            if let Some(fi) = f {
                g.entry(fx[*fi].key.clone()).or_default().push(li);
            }
        }
        g
    };
    let group_value = |key: &str| -> Option<([f64; 3], f64)> {
        let members = groups.get(key)?;
        if members.is_empty() {
            return None;
        }
        let rgb = [0, 1, 2].map(|c| median(members.iter().map(|m| ls[*m].rgb[c]).collect()));
        Some((rgb, median(members.iter().map(|m| ls[*m].brightness).collect())))
    };
    for (li, l) in ls.iter().enumerate() {
        let obj = vec![ObjRef::Entity(l.id)];
        let blocked_world = idx.contains(l.origin).is_some();
        let blocked_prop = props.iter().any(|b| {
            let inner = Aabb::new(b.min + b.size() * 0.1, b.max - b.size() * 0.1);
            inner.contains_point(l.origin)
        });
        let near = nearest(l.origin);
        let target = near.filter(|(_, d)| *d <= MOVE_REACH).map(|(i, _)| fx[i].emit);
        if blocked_world || blocked_prop {
            let to = target.filter(|t| idx.contains(*t).is_none()).or_else(|| free_spot(&idx, l.origin));
            push(
                &mut a,
                Finding::new(
                    Topic::Lights,
                    "light_blocked",
                    Impact::High,
                    format!(
                        "Light {} at {} is inside {}: its light never leaves it. Move it out, just under or in front of \
                         its lamp.",
                        l.id,
                        fmt_point(l.origin),
                        if blocked_world { "a brush" } else { "a prop" }
                    ),
                )
                .score(500.0)
                .objects(obj.clone())
                .points(vec![l.origin])
                .args(vec![l.id.to_string(), fmt_point(l.origin)])
                .fix("unblock"),
                to.map(|to| LightFix::Move { id: l.id, to }),
            );
            continue;
        }
        if fixture_of[li].is_none() && l.class != "light_dynamic" {
            let mut f = Finding::new(
                Topic::Lights,
                "light_no_fixture",
                Impact::Medium,
                match target {
                    Some(t) => format!(
                        "Light {} at {} has no lamp near it (floating light): players see light coming from nowhere. Move \
                         it to the lamp at {}, or add a fixture (a lamp prop or a light texture) where it is.",
                        l.id,
                        fmt_point(l.origin),
                        fmt_point(t)
                    ),
                    None => format!(
                        "Light {} at {} has no lamp near it (floating light): add a fixture there (a lamp prop, a light \
                         texture or a sprite) or move the light to a lamp.",
                        l.id,
                        fmt_point(l.origin)
                    ),
                },
            )
            .score(100.0)
            .objects(obj.clone())
            .points(vec![l.origin])
            .args(vec![l.id.to_string(), fmt_point(l.origin), target.map(fmt_point).unwrap_or_default()]);
            if target.is_some() {
                f = f.fix("move_to_fixture");
            }
            push(&mut a, f, target.map(|to| LightFix::Move { id: l.id, to }));
        }
        if l.class == "light_spot" {
            let e = doc.scene.entity(l.id).map(|n| &n.entity);
            if let Some(e) = e {
                let dir = light_direction(e);
                let hit = (1..=5).any(|i| idx.contains(l.origin + dir * (SPOT_WALL * f64::from(i) / 5.0)).is_some());
                if hit {
                    push(
                        &mut a,
                        Finding::new(
                            Topic::Lights,
                            "spot_into_wall",
                            Impact::Medium,
                            format!(
                                "Spot light {} at {} points straight into a wall or floor a few units away: most of its cone \
                                 is wasted. Aim it at the space it lights (pitch -90 points down).",
                                l.id,
                                fmt_point(l.origin)
                            ),
                        )
                        .score(80.0)
                        .objects(obj.clone())
                        .points(vec![l.origin])
                        .args(vec![l.id.to_string(), fmt_point(l.origin)]),
                        None,
                    );
                }
            }
        }
        if l.brightness <= 0.0 && !l.named {
            push(
                &mut a,
                Finding::new(
                    Topic::Lights,
                    "light_dark",
                    Impact::Low,
                    format!(
                        "Light {} at {} has brightness 0 and no name: it lights nothing. Delete it.",
                        l.id,
                        fmt_point(l.origin)
                    ),
                )
                .score(10.0)
                .objects(obj.clone())
                .points(vec![l.origin])
                .args(vec![l.id.to_string(), fmt_point(l.origin)])
                .fix("delete"),
                Some(LightFix::Delete(l.id)),
            );
        }
        if let Some(e) = doc.scene.entity(l.id).map(|n| &n.entity) {
            let fifty = key_num(e, "_fifty_percent_distance").unwrap_or(0.0);
            let zero = key_num(e, "_zero_percent_distance").unwrap_or(0.0);
            let dist = key_num(e, "_distance").unwrap_or(0.0);
            if fifty > 0.0 && zero > 0.0 && zero <= fifty {
                push(
                    &mut a,
                    Finding::new(
                        Topic::Lights,
                        "light_falloff",
                        Impact::Low,
                        format!(
                            "Light {} has _zero_percent_distance {zero:.0} not larger than _fifty_percent_distance {fifty:.0}: \
                             VRAD ignores the falloff. Make the zero distance about twice the fifty distance.",
                            l.id
                        ),
                    )
                    .score(20.0)
                    .objects(obj.clone())
                    .points(vec![l.origin])
                    .args(vec![l.id.to_string(), format!("{zero:.0}"), format!("{fifty:.0}")])
                    .fix("falloff"),
                    Some(LightFix::SetKey {
                        id: l.id,
                        key: "_zero_percent_distance".to_owned(),
                        value: format!("{:.0}", fifty * 2.0),
                    }),
                );
            } else if dist > 0.0 && dist < 64.0 {
                push(
                    &mut a,
                    Finding::new(
                        Topic::Lights,
                        "light_short_distance",
                        Impact::Low,
                        format!(
                            "Light {} stops at _distance {dist:.0}: the light is cut off with a hard edge. Use 0 (no limit) or \
                             a larger distance.",
                            l.id
                        ),
                    )
                    .score(15.0)
                    .objects(obj.clone())
                    .points(vec![l.origin])
                    .args(vec![l.id.to_string(), format!("{dist:.0}")])
                    .fix("falloff"),
                    Some(LightFix::SetKey { id: l.id, key: "_distance".to_owned(), value: "0".to_owned() }),
                );
            }
        }
    }
    for (key, members) in &groups {
        if members.len() < 3 {
            continue;
        }
        let Some((rgb, b)) = group_value(key) else { continue };
        let value = light_text(rgb, b);
        for &m in members {
            let l = &ls[m];
            if l.named {
                continue;
            }
            let ratio = if b > 0.0 { l.brightness / b } else { 1.0 };
            let off_color = chroma_distance(l.rgb, rgb) > 0.08;
            if !(0.5..=2.0).contains(&ratio) || off_color {
                push(
                    &mut a,
                    Finding::new(
                        Topic::Lights,
                        "light_inconsistent",
                        Impact::Medium,
                        format!(
                            "Light {} at {} does not match the other {} lights of the same lamp ({}): it uses {} where they \
                             use {value}. Identical lamps should give the same light.",
                            l.id,
                            fmt_point(l.origin),
                            members.len() - 1,
                            short_key(key),
                            light_text(l.rgb, l.brightness)
                        ),
                    )
                    .score(60.0 + (ratio.ln().abs() * 40.0))
                    .objects(vec![ObjRef::Entity(l.id)])
                    .points(vec![l.origin])
                    .args(vec![
                        l.id.to_string(),
                        fmt_point(l.origin),
                        (members.len() - 1).to_string(),
                        short_key(key),
                        light_text(l.rgb, l.brightness),
                        value.clone(),
                    ])
                    .fix("harmonize"),
                    Some(LightFix::SetKey { id: l.id, key: "_light".to_owned(), value: value.clone() }),
                );
            }
        }
    }
    for f in &fx {
        if f.texlight {
            continue;
        }
        if ls.iter().any(|l| near_fixture(f, l.origin)) {
            continue;
        }
        let value = group_value(&f.key).map_or_else(|| "255 214 170 150".to_owned(), |(rgb, b)| light_text(rgb, b));
        let free = idx.contains(f.emit).is_none();
        let mut finding = Finding::new(
            Topic::Lights,
            "fixture_no_light",
            if f.kind == FixtureKind::Texture { Impact::Low } else { Impact::Medium },
            format!(
                "The {} {} at {} looks like a light source but no light is near it: the lamp glows but lights nothing. Add \
                 a light at {} ({value}), or use a model or texture that is not a lamp.",
                match f.kind {
                    FixtureKind::Prop => "lamp",
                    FixtureKind::Texture => "light texture",
                    FixtureKind::Sprite => "sprite",
                },
                short_key(&f.key),
                fmt_point(f.bounds.center()),
                fmt_point(f.emit)
            ),
        )
        .score(70.0)
        .objects(vec![f.object])
        .points(vec![f.bounds.center()])
        .boxes(vec![f.bounds])
        .args(vec![short_key(&f.key), fmt_point(f.bounds.center()), fmt_point(f.emit), value.clone()]);
        if free {
            finding = finding.fix("add_light");
        }
        push(&mut a, finding, free.then_some(LightFix::Add { at: f.emit, value }));
    }
    let mut merged: Vec<Id> = Vec::new();
    for (i, x) in ls.iter().enumerate() {
        for y in ls.iter().skip(i + 1) {
            if x.class != "light" || y.class != "light" || x.named || y.named || x.styled || y.styled {
                continue;
            }
            if merged.contains(&x.id) || merged.contains(&y.id) {
                continue;
            }
            if (x.origin - y.origin).length() > REDUNDANT || chroma_distance(x.rgb, y.rgb) > 0.06 {
                continue;
            }
            let (keep, remove) = if x.brightness >= y.brightness { (x, y) } else { (y, x) };
            merged.push(remove.id);
            let value = light_text(keep.rgb, keep.brightness + remove.brightness);
            push(
                &mut a,
                Finding::new(
                    Topic::Lights,
                    "light_redundant",
                    Impact::Low,
                    format!(
                        "Lights {} and {} are {:.0} units apart with the same colour: one light with the summed brightness \
                         ({value}) looks the same and compiles faster.",
                        keep.id,
                        remove.id,
                        (x.origin - y.origin).length()
                    ),
                )
                .score(30.0)
                .objects(vec![ObjRef::Entity(keep.id), ObjRef::Entity(remove.id)])
                .points(vec![keep.origin])
                .args(vec![keep.id.to_string(), remove.id.to_string(), value.clone()])
                .fix("merge"),
                Some(LightFix::Merge { keep: keep.id, remove: remove.id, value }),
            );
        }
    }
    let switchable: std::collections::BTreeSet<String> = doc
        .scene
        .entities
        .values()
        .filter(|n| LIGHT_CLASSES[..2].iter().any(|c| n.entity.classname.eq_ignore_ascii_case(c)))
        .filter_map(|n| n.entity.get("targetname").map(|t| t.trim().to_ascii_lowercase()).filter(|t| !t.is_empty()))
        .collect();
    let max = k.max_switchable.max(1);
    if switchable.len() * 4 > max * 3 {
        let objs: Vec<ObjRef> = ls.iter().filter(|l| l.named).map(|l| ObjRef::Entity(l.id)).collect();
        push(
            &mut a,
            Finding::new(
                Topic::Lights,
                "lights_switchable",
                if switchable.len() > max { Impact::High } else { Impact::Medium },
                format!(
                    "{} switchable (named) lights, the engine allows {max}: every named light adds a lightmap page on each \
                     face it touches (4 at most per face). Only name the lights that really switch; give lights that \
                     switch together the same name.",
                    switchable.len()
                ),
            )
            .score(switchable.len() as f64 * 10.0)
            .objects(objs)
            .args(vec![switchable.len().to_string(), max.to_string()]),
            None,
        );
    }
    let has_sky = doc.scene.solids.values().any(|n| {
        n.owner == Owner::World
            && n.solid.sides.iter().any(|s| s.material.to_ascii_lowercase().starts_with("tools/toolsskybox"))
    });
    let has_sun = doc.scene.entities.values().any(|n| n.entity.classname.eq_ignore_ascii_case("light_environment"));
    if has_sky && !has_sun {
        let at = world_inside(doc);
        push(
            &mut a,
            Finding::new(
                Topic::Lights,
                "no_light_environment",
                Impact::High,
                "The map has sky but no light_environment: outdoor areas get no sun and no sky light. Add one light_environment \
                 (sun angle, colour and ambient)."
                    .to_owned(),
            )
            .score(800.0)
            .points(at.into_iter().collect())
            .fix("add_sun"),
            at.map(|at| LightFix::AddSun { at }),
        );
    }
    let fixes = std::mem::take(&mut a.fixes);
    let mut pairs: Vec<(Finding, Option<LightFix>)> =
        std::mem::take(&mut a.findings).into_iter().map(|f| (f, None)).collect();
    for (i, fix) in fixes {
        if let Some(p) = pairs.get_mut(i) {
            p.1 = Some(fix);
        }
    }
    pairs.sort_by(|x, y| y.0.rank().total_cmp(&x.0.rank()).then_with(|| x.0.code.cmp(y.0.code)));
    for (i, (f, fix)) in pairs.into_iter().enumerate() {
        a.findings.push(f);
        if let Some(x) = fix {
            a.fixes.push((i, x));
        }
    }
    a
}

fn short_key(key: &str) -> String {
    let k = key.trim_end_matches(".mdl");
    k.rsplit('/').next().unwrap_or(k).to_owned()
}

fn free_spot(idx: &SolidIndex, p: DVec3) -> Option<DVec3> {
    let dirs = [DVec3::NEG_Z, DVec3::Z, DVec3::X, DVec3::NEG_X, DVec3::Y, DVec3::NEG_Y];
    (1..=16).find_map(|i| {
        let d = f64::from(i) * 4.0;
        dirs.iter().map(|dir| p + *dir * d).find(|q| idx.contains(*q).is_none())
    })
}

fn world_inside(doc: &Document) -> Option<DVec3> {
    let spawn = doc.scene.entities.values().find_map(|n| {
        let c = n.entity.classname.to_ascii_lowercase();
        (c.starts_with("info_player") || c == "gmod_player_start").then(|| n.entity.origin()).flatten()
    });
    if let Some(s) = spawn {
        return Some(s + DVec3::new(0.0, 0.0, 64.0));
    }
    let b = doc
        .scene
        .solids
        .values()
        .filter(|n| n.owner == Owner::World && n.geom.valid)
        .fold(Aabb::EMPTY, |acc, n| acc.union(&n.geom.aabb));
    (!b.is_empty()).then(|| b.center())
}

impl Document {
    pub fn apply_light_fixes(&mut self, fixes: &[LightFix]) -> EditResult<usize> {
        if fixes.is_empty() {
            return Ok(0);
        }
        self.tx("Fix lights", |doc| {
            let mut n = 0;
            let mut deleted: Vec<ObjRef> = Vec::new();
            for f in fixes {
                match f {
                    LightFix::Move { id, to } => {
                        if !to.is_finite() {
                            return Err(EditError::Invalid("light position is not finite".to_owned()));
                        }
                        let v = hp_vmf::fmt_vec3(*to);
                        n += usize::from(doc.update_entity(*id, |e| e.set("origin", v)));
                    }
                    LightFix::SetKey { id, key, value } => {
                        n += usize::from(doc.update_entity(*id, |e| e.set(key, value.clone())));
                    }
                    LightFix::Merge { keep, remove, value } => {
                        n += usize::from(doc.update_entity(*keep, |e| e.set("_light", value.clone())));
                        deleted.push(ObjRef::Entity(*remove));
                    }
                    LightFix::Delete(id) => deleted.push(ObjRef::Entity(*id)),
                    LightFix::Add { at, value } => {
                        doc.create_point_entity("light", *at, &[("_light".to_owned(), value.clone())])?;
                        n += 1;
                    }
                    LightFix::AddSun { at } => {
                        let kv = [
                            ("angles", "-50 45 0"),
                            ("pitch", "-50"),
                            ("_light", "255 236 200 300"),
                            ("_ambient", "140 160 190 40"),
                        ]
                        .map(|(k, v)| (k.to_owned(), v.to_owned()));
                        doc.create_point_entity("light_environment", *at, &kv)?;
                        n += 1;
                    }
                }
            }
            deleted.sort_unstable();
            deleted.dedup();
            if !deleted.is_empty() {
                n += doc.delete(&deleted)?;
            }
            Ok(n)
        })
    }
}
