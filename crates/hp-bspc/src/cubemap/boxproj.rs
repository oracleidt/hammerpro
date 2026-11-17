use ahash::AHashMap;
use glam::DVec3;

pub const BOX_KEY: &str = "hp_pcbox";
pub const PIXEL_SHADER: &str = "hp_boxcube_ps20";
pub const VERTEX_SHADER: &str = "hp_boxcube_vs20";
pub const SHADER_FILES: [(&str, &[u8]); 2] = [
    ("shaders/fxc/hp_boxcube_ps20b.vcs", include_bytes!("../../assets/shaders/hp_boxcube_ps20b.vcs")),
    ("shaders/fxc/hp_boxcube_vs20.vcs", include_bytes!("../../assets/shaders/hp_boxcube_vs20.vcs")),
];
const INSIDE_MARGIN: f64 = 64.0;
const QUAD_MARGIN: f64 = 8.0;

pub fn parse_box(text: &str) -> Option<(DVec3, DVec3)> {
    let v: Vec<f64> = text.split_ascii_whitespace().filter_map(|x| x.parse().ok()).collect();
    if v.len() != 6 || v.iter().any(|x| !x.is_finite()) {
        return None;
    }
    let (a, b) = (DVec3::new(v[0], v[1], v[2]), DVec3::new(v[3], v[4], v[5]));
    let (lo, hi) = (a.min(b), a.max(b));
    ((hi - lo).min_element() > 1.0).then_some((lo, hi))
}

pub fn box_text(lo: DVec3, hi: DVec3) -> String {
    format!("{} {} {} {} {} {}", lo.x.round(), lo.y.round(), lo.z.round(), hi.x.round(), hi.y.round(), hi.z.round())
}

pub fn inside(b: (DVec3, DVec3), p: DVec3) -> bool {
    let m = DVec3::splat(INSIDE_MARGIN);
    p.cmpge(b.0 - m).all() && p.cmple(b.1 + m).all()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub mask_mode: u8,
    pub tint: [f32; 3],
    pub contrast: f32,
    pub fresnel: f32,
}

pub fn look_of(m: &hp_vmt::Material) -> (Look, Option<String>) {
    let tex = |k: &str| m.get(k).map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
    let bump = tex("$bumpmap").or_else(|| tex("$normalmap"));
    let (mask_mode, mask) = if m.flag("$normalmapalphaenvmapmask") && bump.is_some() {
        (2, bump)
    } else if m.flag("$basealphaenvmapmask") && tex("$basetexture").is_some() {
        (1, tex("$basetexture"))
    } else if let Some(mask) = tex("$envmapmask") {
        (3, Some(mask))
    } else {
        (0, None)
    };
    let tint = m.get_vec3("$envmaptint").or_else(|| m.get_f32("$envmaptint").map(|v| [v; 3])).unwrap_or([1.0; 3]);
    let look = Look {
        mask_mode,
        tint,
        contrast: m.get_f32("$envmapcontrast").unwrap_or(0.0).clamp(0.0, 1.0),
        fresnel: m.get_f32("$fresnelreflection").unwrap_or(1.0).clamp(0.0, 1.0),
    };
    (look, mask)
}

pub fn supported(m: &hp_vmt::Material) -> bool {
    let shader = m.shader.trim().to_ascii_lowercase();
    let flat = shader == "lightmappedgeneric" || shader == "sdk_lightmappedgeneric";
    flat && !m.flag("$translucent") && !m.flag("$alphatest") && !m.flag("$additive") && !m.has("$basetexture2")
}

fn num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r == r.trunc() { format!("{}", r as i64) } else { format!("{r}") }
}

pub fn overlay_vmt(look: &Look, mask: Option<&str>, cubemap: &str, b: (DVec3, DVec3), origin: [i32; 3]) -> String {
    let mut lines: Vec<(String, String)> = vec![
        ("$pixshader".into(), PIXEL_SHADER.into()),
        ("$vertexshader".into(), VERTEX_SHADER.into()),
        ("$texture1".into(), cubemap.into()),
        ("$additive".into(), "1".into()),
        ("$depthtest".into(), "1".into()),
        ("$vertexnormal".into(), "1".into()),
        ("$cull".into(), "1".into()),
    ];
    if let Some(m) = mask {
        lines.push(("$basetexture".into(), m.into()));
        lines.push(("$linearread_basetexture".into(), "1".into()));
    }
    let mode = if mask.is_some() { look.mask_mode } else { 0 };
    let c = [
        [b.0.x, b.0.y, b.0.z, f64::from(mode)],
        [b.1.x, b.1.y, b.1.z, f64::from(look.fresnel)],
        [f64::from(origin[0]), f64::from(origin[1]), f64::from(origin[2]), f64::from(look.contrast)],
        [f64::from(look.tint[0]), f64::from(look.tint[1]), f64::from(look.tint[2]), 1.0],
    ];
    for (i, row) in c.iter().enumerate() {
        for (k, axis) in ["x", "y", "z", "w"].iter().enumerate() {
            lines.push((format!("$c{i}_{axis}"), num(row[k])));
        }
    }
    let mut out = String::from("\"screenspace_general\"\r\n{\r\n");
    for (k, v) in lines {
        out.push_str(&format!("\t\"{k}\" \"{v}\"\r\n"));
    }
    out.push_str("}\r\n");
    out
}

#[derive(Clone, Debug)]
pub struct BoxFace {
    pub side_id: u32,
    pub patched: String,
    pub overlay: String,
    pub points: Vec<DVec3>,
    pub normal: DVec3,
    pub texture: [[f32; 4]; 2],
    pub size: [i32; 2],
}

pub struct Planned {
    pub entities: Vec<hp_vmf::Entity>,
    pub dropped: Vec<String>,
    pub faces: usize,
}

type GroupKey = (String, [i64; 4]);

fn key_of(f: &BoxFace) -> GroupKey {
    let n = f.normal;
    let d = f.points.first().map_or(0.0, |p| p.dot(n));
    let q = |v: f64| (v * 64.0).round() as i64;
    (f.patched.clone(), [q(n.x), q(n.y), q(n.z), (d * 2.0).round() as i64])
}

fn axis(row: [f32; 4]) -> DVec3 {
    DVec3::new(f64::from(row[0]), f64::from(row[1]), f64::from(row[2]))
}

fn overlay_entity(group: &[&BoxFace], id: u32) -> Option<hp_vmf::Entity> {
    let first = group.first()?;
    let n = first.normal.normalize_or_zero();
    if n == DVec3::ZERO {
        return None;
    }
    let (s_axis, t_axis) = (axis(first.texture[0]), axis(first.texture[1]));
    let flat = s_axis - n * s_axis.dot(n);
    let bu = if flat.length() > 1e-6 { flat.normalize() } else { n.any_orthonormal_vector() };
    let bv = n.cross(bu);
    let pts: Vec<DVec3> = group.iter().flat_map(|f| f.points.iter().copied()).collect();
    if pts.is_empty() {
        return None;
    }
    let center = pts.iter().fold(DVec3::ZERO, |a, p| a + *p) / pts.len() as f64;
    let (mut u0, mut u1, mut v0, mut v1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for p in &pts {
        let d = *p - center;
        u0 = u0.min(d.dot(bu));
        u1 = u1.max(d.dot(bu));
        v0 = v0.min(d.dot(bv));
        v1 = v1.max(d.dot(bv));
    }
    let (u0, u1, v0, v1) = (u0 - QUAD_MARGIN, u1 + QUAD_MARGIN, v0 - QUAD_MARGIN, v1 + QUAD_MARGIN);
    let w = f64::from(first.size[0].max(1));
    let h = f64::from(first.size[1].max(1));
    let s0 = (center.dot(s_axis) + f64::from(first.texture[0][3])) / w;
    let t0 = (center.dot(t_axis) + f64::from(first.texture[1][3])) / h;
    let ds = s_axis.dot(bu) / w;
    let dt = t_axis.dot(bv) / h;
    let mut e = hp_vmf::Entity { id, classname: "info_overlay".to_owned(), ..Default::default() };
    let v3 = |v: DVec3| format!("{} {} {}", num(v.x), num(v.y), num(v.z));
    e.set("origin", v3(center));
    e.set("material", first.overlay.clone());
    let mut sides: Vec<u32> = group.iter().map(|f| f.side_id).collect();
    sides.sort_unstable();
    sides.dedup();
    e.set("sides", sides.iter().map(u32::to_string).collect::<Vec<_>>().join(" "));
    e.set("BasisOrigin", v3(center));
    e.set("BasisU", v3(bu));
    e.set("BasisV", v3(bv));
    e.set("BasisNormal", v3(n));
    e.set("uv0", format!("{} {} 0", num(u0), num(v0)));
    e.set("uv1", format!("{} {} 0", num(u0), num(v1)));
    e.set("uv2", format!("{} {} 0", num(u1), num(v1)));
    e.set("uv3", format!("{} {} 0", num(u1), num(v0)));
    e.set("StartU", num(s0 + u0 * ds));
    e.set("EndU", num(s0 + u1 * ds));
    e.set("StartV", num(t0 + v0 * dt));
    e.set("EndV", num(t0 + v1 * dt));
    e.set("RenderOrder", "0");
    e.set("fademindist", "-1");
    Some(e)
}

pub fn plan(
    faces: &[BoxFace],
    side_faces: &AHashMap<u32, Vec<u32>>,
    budget: usize,
    max_faces: usize,
    first_id: u32,
) -> Planned {
    let mut groups: Vec<(GroupKey, Vec<&BoxFace>)> = Vec::new();
    let mut at: AHashMap<GroupKey, usize> = AHashMap::new();
    for f in faces {
        if side_faces.get(&f.side_id).is_none_or(|l| l.is_empty()) {
            continue;
        }
        let k = key_of(f);
        match at.get(&k) {
            Some(&i) => groups[i].1.push(f),
            None => {
                at.insert(k.clone(), groups.len());
                groups.push((k, vec![f]));
            }
        }
    }
    let mut chunks: Vec<(String, Vec<&BoxFace>)> = Vec::new();
    for (k, list) in groups {
        let mut cur: Vec<&BoxFace> = Vec::new();
        let mut count = 0usize;
        for f in list {
            let n = side_faces.get(&f.side_id).map_or(0, Vec::len);
            if n >= max_faces {
                continue;
            }
            if count + n >= max_faces && !cur.is_empty() {
                chunks.push((k.0.clone(), std::mem::take(&mut cur)));
                count = 0;
            }
            cur.push(f);
            count += n;
        }
        if !cur.is_empty() {
            chunks.push((k.0.clone(), cur));
        }
    }
    let mut per_patch: Vec<(String, usize)> = Vec::new();
    for (p, _) in &chunks {
        match per_patch.iter_mut().find(|(n, _)| n == p) {
            Some(e) => e.1 += 1,
            None => per_patch.push((p.clone(), 1)),
        }
    }
    let mut kept: Vec<String> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();
    let mut used = 0usize;
    for (p, n) in per_patch {
        if used + n <= budget {
            used += n;
            kept.push(p);
        } else {
            dropped.push(p);
        }
    }
    let covered: std::collections::HashSet<u32> =
        chunks.iter().filter(|(p, _)| kept.contains(p)).flat_map(|(_, g)| g.iter().map(|f| f.side_id)).collect();
    for f in faces {
        let lost = !covered.contains(&f.side_id) && side_faces.get(&f.side_id).is_some_and(|l| !l.is_empty());
        if (lost || !kept.contains(&f.patched)) && !dropped.contains(&f.patched) {
            dropped.push(f.patched.clone());
        }
    }
    kept.retain(|p| !dropped.contains(p));
    let mut entities = Vec::new();
    let mut count = 0;
    for (_, group) in chunks.iter().filter(|(p, _)| kept.contains(p)) {
        if let Some(e) = overlay_entity(group, first_id + entities.len() as u32) {
            count += group.len();
            entities.push(e);
        }
    }
    Planned { entities, dropped, faces: count }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(side: u32, patched: &str, z: f64, x0: f64) -> BoxFace {
        BoxFace {
            side_id: side,
            patched: patched.to_owned(),
            overlay: format!("{patched}_overlay"),
            points: vec![
                DVec3::new(x0, 0.0, z),
                DVec3::new(x0 + 128.0, 0.0, z),
                DVec3::new(x0 + 128.0, 128.0, z),
                DVec3::new(x0, 128.0, z),
            ],
            normal: DVec3::Z,
            texture: [[4.0, 0.0, 0.0, 0.0], [0.0, -4.0, 0.0, 0.0]],
            size: [512, 512],
        }
    }

    #[test]
    fn boxes_parse_and_print() {
        let (lo, hi) = parse_box("10 20 30 -10 -20 -30").expect("box");
        assert_eq!((lo, hi), (DVec3::new(-10.0, -20.0, -30.0), DVec3::new(10.0, 20.0, 30.0)));
        assert_eq!(box_text(lo, hi), "-10 -20 -30 10 20 30");
        assert!(parse_box("1 2 3").is_none() && parse_box("0 0 0 0 0 0").is_none());
        assert!(inside((lo, hi), DVec3::new(70.0, 0.0, 0.0)) && !inside((lo, hi), DVec3::new(80.0, 0.0, 0.0)));
    }

    #[test]
    fn the_overlay_material_follows_the_envmap_keys() {
        let m = hp_vmt::parse(
            "\"LightmappedGeneric\"\n{\n\"$basetexture\" \"tile/a\"\n\"$bumpmap\" \"tile/a_normal\"\n\"$envmap\" \"env_cubemap\"\n\"$normalmapalphaenvmapmask\" \"1\"\n\"$envmaptint\" \"[.5 .25 1]\"\n\"$envmapcontrast\" \"1\"\n}\n",
        )
        .expect("vmt");
        assert!(supported(&m));
        let (look, mask) = look_of(&m);
        assert_eq!(
            (look.mask_mode, mask.as_deref(), look.tint, look.contrast),
            (2, Some("tile/a_normal"), [0.5, 0.25, 1.0], 1.0)
        );
        let text = overlay_vmt(&look, mask.as_deref(), "maps/x/c1_2_3", (DVec3::ZERO, DVec3::splat(256.0)), [1, 2, 3]);
        for want in [
            "screenspace_general",
            "\"$pixshader\" \"hp_boxcube_ps20\"",
            "\"$texture1\" \"maps/x/c1_2_3\"",
            "\"$c1_x\" \"256\"",
            "\"$c2_z\" \"3\"",
            "\"$c3_y\" \"0.25\"",
            "\"$c0_w\" \"2\"",
        ] {
            assert!(text.contains(want), "{want} in {text}");
        }
        let glass = hp_vmt::parse("\"LightmappedGeneric\"\n{\n\"$translucent\" \"1\"\n}\n").expect("vmt");
        assert!(!supported(&glass));
    }

    #[test]
    fn coplanar_faces_share_an_overlay_and_the_budget_drops_whole_materials() {
        let faces =
            vec![face(1, "a", 0.0, 0.0), face(2, "a", 0.0, 128.0), face(3, "a", 64.0, 0.0), face(4, "b", 0.0, 512.0)];
        let mut side_faces = AHashMap::new();
        for s in 1..=4 {
            side_faces.insert(s, vec![s * 10]);
        }
        let p = plan(&faces, &side_faces, 8, 64, 100);
        assert_eq!((p.entities.len(), p.faces, p.dropped.len()), (3, 4, 0));
        let floor =
            p.entities.iter().find(|e| e.get("sides") == Some("1 2")).expect("one overlay for the two floor faces");
        assert_eq!(floor.get("BasisNormal"), Some("0 0 1"));
        assert_eq!(floor.get("material"), Some("a_overlay"));
        let su: f64 = floor.get("StartU").and_then(|v| v.parse().ok()).expect("u");
        let eu: f64 = floor.get("EndU").and_then(|v| v.parse().ok()).expect("u");
        assert!(((eu - su) - (256.0 + 16.0) * 4.0 / 512.0).abs() < 1e-2, "{su} {eu}");
        let tight = plan(&faces, &side_faces, 2, 64, 100);
        assert_eq!(tight.dropped, vec!["b".to_owned()]);
        assert_eq!(tight.entities.len(), 2);
        let mut many = AHashMap::new();
        many.insert(1, (0..40).collect::<Vec<u32>>());
        many.insert(2, (40..80).collect::<Vec<u32>>());
        let split = plan(&faces[..2], &many, 8, 64, 0);
        assert_eq!(split.entities.len(), 2, "64 faces per overlay at most");
    }
}
