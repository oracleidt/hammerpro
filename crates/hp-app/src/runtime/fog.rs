use glam::DVec3;
use hp_editor::{Document, Id};
use hp_render::Fog;
use hp_vmf::Entity;

use crate::i18n::TrTable;
use crate::tf;

pub const FOG_VOLUME: &str = "fog_volume";
pub const FOG_CONTROLLER: &str = "env_fog_controller";
pub const POST_PROCESS: &str = "postprocess_controller";
pub const COLOR_CORRECTION: &str = "color_correction";

pub const DEFAULT_PREVIEW_FOG: Fog = Fog {
    color: [0.55, 0.7, 0.95],
    color2: [0.55, 0.7, 0.95],
    dir: [1.0, 0.0, 0.0],
    blend: false,
    start: 0.0,
    end: 384.0,
    max_density: 0.85,
    far_z: 0.0,
};

pub fn is_fog_volume(class: &str) -> bool {
    class.trim().eq_ignore_ascii_case(FOG_VOLUME)
}

pub fn is_fog_controller(class: &str) -> bool {
    class.trim().eq_ignore_ascii_case(FOG_CONTROLLER)
}

fn floats3(s: &str) -> Option<[f64; 3]> {
    let mut it = s.split_ascii_whitespace().map(|t| t.parse::<f64>().ok().filter(|v| v.is_finite()));
    let out = [it.next()??, it.next()??, it.next()??];
    Some(out)
}

fn color_key(e: &Entity, key: &str, fallback: [f32; 3]) -> [f32; 3] {
    e.get(key).and_then(floats3).map_or(fallback, |c| c.map(|v| (v / 255.0).clamp(0.0, 1.0) as f32))
}

fn num(e: &Entity, key: &str, fallback: f32) -> f32 {
    e.get(key).and_then(|s| s.trim().parse::<f32>().ok()).filter(|v| v.is_finite()).unwrap_or(fallback)
}

fn flag(e: &Entity, key: &str) -> bool {
    num(e, key, 0.0) != 0.0
}

fn angles_forward(a: DVec3) -> [f32; 3] {
    let (p, y) = (a.x.to_radians(), a.y.to_radians());
    [(p.cos() * y.cos()) as f32, (p.cos() * y.sin()) as f32, (-p.sin()) as f32]
}

pub fn fog_enabled(e: &Entity) -> bool {
    flag(e, "fogenable")
}

pub fn controller_fog(e: &Entity) -> Fog {
    let color = color_key(e, "fogcolor", [1.0; 3]);
    let dir = if flag(e, "use_angles") {
        angles_forward(e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO))
    } else {
        e.get("fogdir").and_then(floats3).map_or([1.0, 0.0, 0.0], |d| d.map(|v| v as f32))
    };
    let start = num(e, "fogstart", 0.0).max(0.0);
    let far_z = num(e, "farz", -1.0);
    Fog {
        color,
        color2: color_key(e, "fogcolor2", color),
        dir,
        blend: flag(e, "fogblend"),
        start,
        end: num(e, "fogend", 4096.0).max(start + 1.0),
        max_density: num(e, "fogmaxdensity", 1.0).clamp(0.0, 1.0),
        far_z: if far_z > 0.0 { far_z } else { 0.0 },
    }
}

pub fn name_matches(pattern: &str, name: &str) -> bool {
    let (pattern, name) = (pattern.trim(), name.trim());
    if pattern.is_empty() || name.is_empty() {
        return false;
    }
    match pattern.strip_suffix('*') {
        Some(prefix) => {
            name.len() >= prefix.len()
                && name.is_char_boundary(prefix.len())
                && name[..prefix.len()].eq_ignore_ascii_case(prefix)
        }
        None => name.eq_ignore_ascii_case(pattern),
    }
}

fn named<'a>(doc: &'a Document, name: &str, class: Option<&str>) -> Option<(Id, &'a Entity)> {
    let scene = doc.scene();
    scene
        .entities()
        .iter()
        .map(|(id, n)| (*id, &n.entity))
        .filter(|(_, e)| class.is_none_or(|c| e.classname.trim().eq_ignore_ascii_case(c)))
        .filter(|(_, e)| e.get("targetname").is_some_and(|t| name_matches(name, t)))
        .min_by_key(|(id, _)| *id)
}

pub fn find_controller<'a>(doc: &'a Document, fog_name: &str) -> Option<(Id, &'a Entity)> {
    named(doc, fog_name, Some(FOG_CONTROLLER))
}

pub fn global_controller(doc: &Document, chosen: Option<Id>) -> Option<(Id, &Entity)> {
    let scene = doc.scene();
    chosen
        .and_then(|id| scene.entity(id).filter(|n| is_fog_controller(&n.entity.classname)).map(|n| (id, &n.entity)))
        .or_else(|| {
            let mut all: Vec<(u64, Id, &Entity)> = scene
                .entities()
                .iter()
                .filter(|(_, n)| is_fog_controller(&n.entity.classname))
                .map(|(id, n)| (n.order(), *id, &n.entity))
                .collect();
            all.sort_unstable_by_key(|(order, id, _)| (*order, *id));
            let pick = all.iter().rev().find(|(_, _, e)| is_master(e)).or(all.first());
            pick.map(|(_, id, e)| (*id, *e))
        })
}

pub fn is_master(e: &Entity) -> bool {
    e.get("spawnflags").and_then(|s| s.trim().parse::<i64>().ok()).is_some_and(|f| f & 1 != 0)
}

pub fn global_fog(doc: &Document, chosen: Option<Id>) -> Option<Fog> {
    let (_, e) = global_controller(doc, chosen)?;
    fog_enabled(e).then(|| controller_fog(e))
}

#[derive(Clone, Debug, PartialEq)]
pub enum FogSource {
    NoFogName,
    Missing(String),
    Disabled { id: Id, name: String },
    Controller { id: Id, name: String },
}

impl FogSource {
    pub fn kind(&self) -> &'static str {
        match self {
            FogSource::NoFogName => "no_fog_name",
            FogSource::Missing(_) => "missing_controller",
            FogSource::Disabled { .. } => "controller_disabled",
            FogSource::Controller { .. } => "controller",
        }
    }

    pub fn controller(&self) -> Option<Id> {
        match self {
            FogSource::Disabled { id, .. } | FogSource::Controller { id, .. } => Some(*id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    Unset,
    Missing(String),
    WrongClass { id: Id, name: String, class: String },
    Found { id: Id, name: String },
}

impl Link {
    pub fn kind(&self) -> &'static str {
        match self {
            Link::Unset => "unset",
            Link::Missing(_) => "missing",
            Link::WrongClass { .. } => "wrong_class",
            Link::Found { .. } => "found",
        }
    }

    pub fn target(&self) -> Option<Id> {
        match self {
            Link::WrongClass { id, .. } | Link::Found { id, .. } => Some(*id),
            _ => None,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            Link::Unset => None,
            Link::Missing(n) | Link::WrongClass { name: n, .. } | Link::Found { name: n, .. } => Some(n),
        }
    }
}

fn link(doc: &Document, e: &Entity, key: &str, class: &str) -> Link {
    let Some(name) = e.get(key).map(str::trim).filter(|n| !n.is_empty()) else { return Link::Unset };
    let name = name.to_owned();
    if let Some((id, _)) = named(doc, &name, Some(class)) {
        return Link::Found { id, name };
    }
    match named(doc, &name, None) {
        Some((id, other)) => Link::WrongClass { id, name, class: other.classname.trim().to_owned() },
        None => Link::Missing(name),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VolumeFog {
    pub id: Id,
    pub bounds: Option<(DVec3, DVec3)>,
    pub source: FogSource,
    pub preview: Fog,
    pub start_disabled: bool,
    pub post_process: Link,
    pub color_correction: Link,
}

impl VolumeFog {
    pub fn game_fog(&self) -> Option<Fog> {
        matches!(self.source, FogSource::Controller { .. }).then_some(self.preview)
    }

    pub fn changes_game_fog(&self) -> bool {
        self.source.controller().is_some()
    }

    pub fn uses_default_look(&self) -> bool {
        !matches!(self.source, FogSource::Controller { .. })
    }
}

pub fn resolve_volume(doc: &Document, id: Id) -> Option<VolumeFog> {
    let scene = doc.scene();
    let e = &scene.entity(id)?.entity;
    if !is_fog_volume(&e.classname) {
        return None;
    }
    let source = match e.get("FogName").map(str::trim).filter(|n| !n.is_empty()) {
        None => FogSource::NoFogName,
        Some(name) => match find_controller(doc, name) {
            None => FogSource::Missing(name.to_owned()),
            Some((cid, c)) => {
                let name = c.get("targetname").unwrap_or(name).trim().to_owned();
                if fog_enabled(c) {
                    FogSource::Controller { id: cid, name }
                } else {
                    FogSource::Disabled { id: cid, name }
                }
            }
        },
    };
    let preview = match &source {
        FogSource::Controller { id, .. } => {
            scene.entity(*id).map_or(DEFAULT_PREVIEW_FOG, |n| controller_fog(&n.entity))
        }
        _ => DEFAULT_PREVIEW_FOG,
    };
    let bounds = scene.entity_solid_bounds(id).map(|b| (b.min, b.max));
    Some(VolumeFog {
        id,
        bounds,
        source,
        preview,
        start_disabled: flag(e, "StartDisabled"),
        post_process: link(doc, e, "PostProcessName", POST_PROCESS),
        color_correction: link(doc, e, "ColorCorrectionName", COLOR_CORRECTION),
    })
}

pub fn fog_volume_ids(doc: &Document) -> Vec<Id> {
    let mut ids: Vec<Id> =
        doc.scene().entities().iter().filter(|(_, n)| is_fog_volume(&n.entity.classname)).map(|(id, _)| *id).collect();
    ids.sort_unstable();
    ids
}

pub fn volumes_using(doc: &Document, controller: Id) -> Vec<Id> {
    fog_volume_ids(doc)
        .into_iter()
        .filter(|id| resolve_volume(doc, *id).is_some_and(|v| v.source.controller() == Some(controller)))
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub warning: bool,
    pub text: String,
}

fn note(warning: bool, text: String) -> Note {
    Note { warning, text }
}

fn link_notes(l: &Link, found: &str, missing: &str, wrong: &str, out: &mut Vec<Note>) {
    match l {
        Link::Unset => {}
        Link::Found { name, .. } => out.push(note(false, tf!(found, name))),
        Link::Missing(name) => out.push(note(true, tf!(missing, name))),
        Link::WrongClass { name, class, .. } => out.push(note(true, tf!(wrong, name, class))),
    }
}

pub fn volume_notes(v: &VolumeFog) -> Vec<Note> {
    let mut out = Vec::new();
    match &v.source {
        FogSource::NoFogName => out.push(note(true, tf!("fog.no_name"))),
        FogSource::Missing(name) => out.push(note(true, tf!("fog.missing", name))),
        FogSource::Disabled { name, .. } => out.push(note(true, tf!("fog.disabled", name))),
        FogSource::Controller { name, .. } => out.push(note(false, tf!("fog.linked", name))),
    }
    if v.uses_default_look() {
        out.push(note(false, tf!("fog.default_look")));
    }
    if v.start_disabled {
        out.push(note(true, tf!("fog.start_disabled")));
    }
    link_notes(&v.post_process, "fog.pp_found", "fog.pp_missing", "fog.pp_wrong", &mut out);
    link_notes(&v.color_correction, "fog.cc_found", "fog.cc_missing", "fog.cc_wrong", &mut out);
    out
}

pub fn volume_label_notes(v: &VolumeFog) -> Vec<Note> {
    let mut out = Vec::new();
    match &v.source {
        FogSource::NoFogName => out.push(note(true, tf!("fog.short_no_name"))),
        FogSource::Missing(name) => out.push(note(true, tf!("fog.short_missing", name))),
        FogSource::Disabled { name, .. } => out.push(note(true, tf!("fog.short_disabled", name))),
        FogSource::Controller { .. } => {}
    }
    if v.start_disabled {
        out.push(note(true, tf!("fog.short_start_disabled")));
    }
    for (l, missing, class) in [
        (&v.post_process, "fog.short_pp_missing", POST_PROCESS),
        (&v.color_correction, "fog.short_cc_missing", COLOR_CORRECTION),
    ] {
        match l {
            Link::Missing(name) => out.push(note(true, tf!(missing, name))),
            Link::WrongClass { name, .. } => out.push(note(true, tf!("fog.short_wrong", name, class))),
            Link::Found { name, .. } => out.push(note(false, format!("{class}: \"{name}\""))),
            Link::Unset => {}
        }
    }
    out
}

pub fn controller_notes(doc: &Document, id: Id, previewed: Option<Id>, preview_on: bool) -> Vec<Note> {
    let Some(e) = doc.scene().entity(id).map(|n| &n.entity).filter(|e| is_fog_controller(&e.classname)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if !fog_enabled(e) {
        out.push(note(true, tf!("fog.controller_off")));
    }
    let shown = global_controller(doc, previewed).map(|(c, _)| c) == Some(id);
    out.push(match (preview_on, shown) {
        (true, true) => note(false, tf!("fog.controller_previewed")),
        (true, false) => note(false, tf!("fog.controller_other")),
        (false, _) => note(false, tf!("fog.controller_preview_off")),
    });
    let users = volumes_using(doc, id).len();
    if users > 0 {
        out.push(note(false, tf!("fog.controller_users", users)));
    }
    out
}

pub const TR: TrTable = &[
    (
        "fog.no_name",
        "No Fog Name: set it to the name of an env_fog_controller",
        "Pas de Fog Name : indiquez le nom d'un env_fog_controller",
    ),
    ("fog.missing", "No env_fog_controller named \"{0}\"", "Aucun env_fog_controller nommé \"{0}\""),
    (
        "fog.disabled",
        "The fog of \"{0}\" is disabled (Fog Enable: No): in game this volume clears the fog",
        "Le brouillard de \"{0}\" est désactivé (Fog Enable : No) : en jeu ce volume enlève le brouillard",
    ),
    ("fog.linked", "Fog of \"{0}\"", "Brouillard de \"{0}\""),
    ("fog.default_look", "Preview: default fog look", "Aperçu : brouillard par défaut"),
    (
        "fog.start_disabled",
        "Start Disabled: no effect in game until it gets Enable",
        "Start Disabled : aucun effet en jeu avant l'entrée Enable",
    ),
    ("fog.pp_found", "Post process: \"{0}\"", "Post-traitement : \"{0}\""),
    ("fog.pp_missing", "No postprocess_controller named \"{0}\"", "Aucun postprocess_controller nommé \"{0}\""),
    (
        "fog.pp_wrong",
        "\"{0}\" is a {1}, not a postprocess_controller",
        "\"{0}\" est un {1}, pas un postprocess_controller",
    ),
    ("fog.cc_found", "Color correction: \"{0}\"", "Correction colorimétrique : \"{0}\""),
    ("fog.cc_missing", "No color_correction named \"{0}\"", "Aucun color_correction nommé \"{0}\""),
    ("fog.cc_wrong", "\"{0}\" is a {1}, not a color_correction", "\"{0}\" est un {1}, pas un color_correction"),
    (
        "fog.controller_off",
        "Fog Enable is No: this controller gives no fog in game and the fog preview shows none",
        "Fog Enable vaut No : ce contrôleur ne donne aucun brouillard en jeu et l'aperçu n'en montre pas",
    ),
    (
        "fog.controller_previewed",
        "The 3D view previews this fog (View > Fog preview)",
        "La vue 3D affiche ce brouillard (Affichage > Aperçu du brouillard)",
    ),
    (
        "fog.controller_other",
        "The 3D view previews another env_fog_controller (View > Fog preview)",
        "La vue 3D affiche un autre env_fog_controller (Affichage > Aperçu du brouillard)",
    ),
    (
        "fog.controller_preview_off",
        "Fog preview is off (View > Fog preview)",
        "L'aperçu du brouillard est désactivé (Affichage > Aperçu du brouillard)",
    ),
    ("fog.controller_users", "Used by {0} fog_volume", "Utilisé par {0} fog_volume"),
    (
        "fish.missing_model",
        "{0} is not in the game content: no fish appear in game, the preview shows {1} instead. Set World model to {1}.",
        "{0} n'est pas dans le contenu du jeu : aucun poisson n'apparaît en jeu, l'aperçu montre {1} à la place. Mettez {1} comme World model.",
    ),
    ("fog.short_no_name", "No Fog Name", "Pas de Fog Name"),
    ("fog.short_missing", "No controller \"{0}\"", "Aucun contrôleur \"{0}\""),
    (
        "fog.short_disabled",
        "\"{0}\" has its fog disabled: clears the fog in game",
        "\"{0}\" a son brouillard désactivé : l'enlève en jeu",
    ),
    ("fog.short_start_disabled", "Start Disabled", "Start Disabled"),
    ("fog.short_pp_missing", "No post process \"{0}\"", "Aucun post-traitement \"{0}\""),
    ("fog.short_cc_missing", "No color correction \"{0}\"", "Aucune correction colorimétrique \"{0}\""),
    ("fog.short_wrong", "\"{0}\" is not a {1}", "\"{0}\" n'est pas un {1}"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use hp_vmf::{Map, Side, Solid, TexAxis};

    fn ent(id: Id, class: &str, props: &[(&str, &str)]) -> Entity {
        let mut e = Entity { id, classname: class.to_owned(), ..Default::default() };
        for (k, v) in props {
            e.set(k, *v);
        }
        e
    }

    fn block(id: Id, lo: DVec3, hi: DVec3) -> Solid {
        let p = DVec3::new;
        let planes = [
            [p(lo.x, hi.y, hi.z), p(hi.x, hi.y, hi.z), p(hi.x, lo.y, hi.z)],
            [p(lo.x, lo.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, hi.y, lo.z)],
            [p(lo.x, hi.y, hi.z), p(lo.x, lo.y, hi.z), p(lo.x, lo.y, lo.z)],
            [p(hi.x, hi.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, lo.y, hi.z)],
            [p(hi.x, hi.y, hi.z), p(lo.x, hi.y, hi.z), p(lo.x, hi.y, lo.z)],
            [p(hi.x, lo.y, lo.z), p(lo.x, lo.y, lo.z), p(lo.x, lo.y, hi.z)],
        ];
        let sides = planes
            .iter()
            .enumerate()
            .map(|(k, plane)| Side {
                id: id * 10 + k as Id,
                plane: *plane,
                material: "tools/toolsfog".to_owned(),
                uaxis: TexAxis { axis: DVec3::X, shift: 0.0, scale: 0.25 },
                vaxis: TexAxis { axis: DVec3::NEG_Y, shift: 0.0, scale: 0.25 },
                ..Default::default()
            })
            .collect();
        Solid { id, sides, ..Default::default() }
    }

    const CONTROLLER: &[(&str, &str)] = &[
        ("targetname", "Cave_Fog"),
        ("fogenable", "1"),
        ("fogcolor", "51 102 255"),
        ("fogstart", "64"),
        ("fogend", "512"),
        ("fogmaxdensity", "0.75"),
        ("farz", "2048"),
    ];

    fn doc(volume: &[(&str, &str)], controller: &[(&str, &str)], extra: &[Entity]) -> Document {
        crate::register_translations();
        let mut m = Map::new_empty();
        let mut v = ent(10, "fog_volume", volume);
        v.solids.push(block(11, DVec3::new(-64.0, -32.0, 0.0), DVec3::new(64.0, 32.0, 128.0)));
        m.entities.push(v);
        m.entities.push(ent(20, "env_fog_controller", controller));
        m.entities.extend(extra.iter().cloned());
        Document::from_map(m)
    }

    #[test]
    fn a_fresh_volume_has_the_default_look_and_a_hint() {
        let d = doc(&[], CONTROLLER, &[]);
        let v = resolve_volume(&d, 10).expect("volume");
        assert_eq!(v.source, FogSource::NoFogName);
        assert_eq!(v.preview, DEFAULT_PREVIEW_FOG);
        assert_eq!(v.bounds, Some((DVec3::new(-64.0, -32.0, 0.0), DVec3::new(64.0, 32.0, 128.0))));
        assert!(v.game_fog().is_none() && !v.changes_game_fog() && v.uses_default_look());
        let notes = volume_notes(&v);
        assert!(notes[0].warning && notes[0].text.contains("Fog Name"), "{notes:?}");
        assert!(resolve_volume(&d, 20).is_none(), "not a volume");
    }

    #[test]
    fn linked_disabled_and_missing_controllers() {
        let d = doc(&[("FogName", "cave*")], CONTROLLER, &[]);
        let v = resolve_volume(&d, 10).expect("volume");
        assert_eq!(v.source, FogSource::Controller { id: 20, name: "Cave_Fog".to_owned() });
        let f = v.game_fog().expect("fog in game");
        assert_eq!(f.color, [0.2, 0.4, 1.0]);
        assert_eq!((f.start, f.end, f.max_density, f.far_z), (64.0, 512.0, 0.75, 2048.0));
        assert!(!volume_notes(&v)[0].warning);
        let mut off = CONTROLLER.to_vec();
        off[1] = ("fogenable", "0");
        let v = resolve_volume(&doc(&[("FogName", "cave_fog")], &off, &[]), 10).expect("volume");
        assert_eq!(v.source, FogSource::Disabled { id: 20, name: "Cave_Fog".to_owned() });
        assert_eq!(v.preview, DEFAULT_PREVIEW_FOG, "a visible default look");
        assert!(v.changes_game_fog() && v.game_fog().is_none(), "in game it clears the fog");
        assert!(volume_notes(&v)[0].text.contains("clears"));
        let v = resolve_volume(&doc(&[("FogName", "nothing")], CONTROLLER, &[]), 10).expect("volume");
        assert_eq!(v.source, FogSource::Missing("nothing".to_owned()));
        assert!(!v.changes_game_fog());
        assert!(volume_notes(&v)[0].text.contains("\"nothing\""));
    }

    #[test]
    fn post_process_and_color_correction_links() {
        let extra = [
            ent(30, "postprocess_controller", &[("targetname", "pp")]),
            ent(31, "info_target", &[("targetname", "cc")]),
        ];
        let d = doc(
            &[
                ("FogName", "cave_fog"),
                ("PostProcessName", "pp"),
                ("ColorCorrectionName", "cc"),
                ("StartDisabled", "1"),
            ],
            CONTROLLER,
            &extra,
        );
        let v = resolve_volume(&d, 10).expect("volume");
        assert_eq!(v.post_process, Link::Found { id: 30, name: "pp".to_owned() });
        assert_eq!(
            v.color_correction,
            Link::WrongClass { id: 31, name: "cc".to_owned(), class: "info_target".to_owned() }
        );
        assert!(v.start_disabled);
        let notes = volume_notes(&v);
        assert!(notes.iter().any(|n| n.warning && n.text.contains("info_target")), "{notes:?}");
        assert!(notes.iter().any(|n| !n.warning && n.text.contains("\"pp\"")), "{notes:?}");
        let d = doc(&[("ColorCorrectionName", "nope")], CONTROLLER, &[]);
        let v = resolve_volume(&d, 10).expect("volume");
        assert_eq!((v.post_process.kind(), v.color_correction.kind()), ("unset", "missing"));
    }

    #[test]
    fn the_global_fog_follows_fog_enable_and_the_chosen_controller() {
        let mut off = CONTROLLER.to_vec();
        off[1] = ("fogenable", "0");
        let extra = [ent(40, "env_fog_controller", &[("fogenable", "1"), ("fogcolor", "255 0 0")])];
        let d = doc(&[("FogName", "cave_fog")], &off, &extra);
        assert!(global_fog(&d, None).is_none(), "the first controller has its fog off");
        assert_eq!(global_fog(&d, Some(40)).map(|f| f.color), Some([1.0, 0.0, 0.0]));
        assert!(global_fog(&d, Some(10)).is_none(), "not a controller: the first one");
        assert_eq!(global_controller(&d, Some(999)).map(|(id, _)| id), Some(20));
        assert!(controller_notes(&d, 20, None, true)[0].warning);
        assert_eq!(volumes_using(&d, 20), vec![10]);
        assert!(controller_notes(&d, 40, None, true).iter().all(|n| !n.warning));
        assert!(controller_notes(&d, 10, None, true).is_empty());
        let master = [ent(50, "env_fog_controller", &[("fogenable", "1"), ("spawnflags", "1")])];
        let d = doc(&[], &off, &master);
        assert_eq!(global_controller(&d, None).map(|(id, _)| id), Some(50), "the Master flag wins, like the game");
        let masters = [
            ent(50, "env_fog_controller", &[("fogenable", "1"), ("spawnflags", "1")]),
            ent(60, "env_fog_controller", &[("fogenable", "0"), ("spawnflags", "1")]),
        ];
        let d = doc(&[], &off, &masters);
        assert_eq!(global_controller(&d, None).map(|(id, _)| id), Some(60), "several masters: the last one of the map");
    }

    #[test]
    fn controller_keys_are_sanitized() {
        let f = controller_fog(&ent(
            1,
            "env_fog_controller",
            &[("fogstart", "-50"), ("fogend", "nan"), ("fogmaxdensity", "4"), ("fogcolor", "a b c"), ("farz", "-1")],
        ));
        assert_eq!((f.start, f.end, f.max_density, f.far_z), (0.0, 4096.0, 1.0, 0.0));
        assert_eq!(f.color, [1.0; 3]);
        assert_eq!(f.color2, f.color);
        let angled = controller_fog(&ent(1, "env_fog_controller", &[("use_angles", "1"), ("angles", "-90 0 0")]));
        assert!((angled.dir[2] - 1.0).abs() < 1e-6, "pitch -90 looks up: {:?}", angled.dir);
        assert!(name_matches("cave*", "CAVE_fog") && name_matches("Cave_Fog", "cave_fog"));
        assert!(!name_matches("cave", "cave_fog") && !name_matches("", "a") && !name_matches("*x", "a"));
        assert!(!name_matches("é*", "e"), "byte boundaries are safe");
    }

    #[test]
    fn table_is_clean() {
        assert!(crate::i18n::lint_table(TR).is_empty());
    }
}
