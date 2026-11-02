use glam::{DVec3, Vec3A};

use super::super::tests::{compile_vmf, luxel_at, room_with};
use super::super::{PropOptions, RadOptions, RadQuality, light_bsp_with};
use crate::emit::lump;

const CHECKSUM: i32 = 0x0123_4567;
const HALF: f32 = 16.0;
const HEIGHT: f32 = 32.0;

fn put_i32(b: &mut Vec<u8>, at: usize, v: i32) {
    if b.len() < at + 4 {
        b.resize(at + 4, 0);
    }
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn put_f32(b: &mut Vec<u8>, at: usize, v: f32) {
    put_i32(b, at, v.to_bits() as i32);
}

fn put_str(b: &mut Vec<u8>, at: usize, s: &str) {
    if b.len() < at + s.len() + 1 {
        b.resize(at + s.len() + 1, 0);
    }
    b[at..at + s.len()].copy_from_slice(s.as_bytes());
}

type Vert = ([f32; 3], [f32; 3]);

fn box_mesh() -> (Vec<Vert>, Vec<[u16; 3]>) {
    let (h, z) = (HALF, HEIGHT);
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([0.0, 0.0, 1.0], [[-h, -h, z], [h, -h, z], [h, h, z], [-h, h, z]]),
        ([0.0, 0.0, -1.0], [[-h, h, 0.0], [h, h, 0.0], [h, -h, 0.0], [-h, -h, 0.0]]),
        ([1.0, 0.0, 0.0], [[h, -h, 0.0], [h, h, 0.0], [h, h, z], [h, -h, z]]),
        ([-1.0, 0.0, 0.0], [[-h, h, 0.0], [-h, -h, 0.0], [-h, -h, z], [-h, h, z]]),
        ([0.0, 1.0, 0.0], [[h, h, 0.0], [-h, h, 0.0], [-h, h, z], [h, h, z]]),
        ([0.0, -1.0, 0.0], [[-h, -h, 0.0], [h, -h, 0.0], [h, -h, z], [-h, -h, z]]),
    ];
    let mut verts = Vec::new();
    let mut tris = Vec::new();
    for (n, quad) in faces {
        let base = verts.len() as u16;
        for p in quad {
            verts.push((p, n));
        }
        tris.push([base, base + 2, base + 1]);
        tris.push([base, base + 3, base + 2]);
    }
    (verts, tris)
}

fn box_files() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (verts, tris) = box_mesh();
    let mut mdl = vec![0u8; 408];
    mdl[0..4].copy_from_slice(b"IDST");
    put_i32(&mut mdl, 4, 48);
    put_i32(&mut mdl, 8, CHECKSUM);
    put_str(&mut mdl, 12, "test/box.mdl");
    for (k, v) in [-HALF, -HALF, 0.0, HALF, HALF, HEIGHT].into_iter().enumerate() {
        put_f32(&mut mdl, 104 + k * 4, v);
    }
    put_i32(&mut mdl, 152, 0x10);
    let (tex, cd, part, model, mesh, names) = (408, 472, 476, 492, 640, 756);
    put_i32(&mut mdl, 204, 1);
    put_i32(&mut mdl, 208, tex as i32);
    put_i32(&mut mdl, 212, 1);
    put_i32(&mut mdl, 216, cd as i32);
    put_i32(&mut mdl, 232, 1);
    put_i32(&mut mdl, 236, part as i32);
    put_i32(&mut mdl, tex, (names - tex) as i32);
    put_i32(&mut mdl, cd, (names + 4) as i32);
    put_i32(&mut mdl, part + 4, 1);
    put_i32(&mut mdl, part + 8, 1);
    put_i32(&mut mdl, part + 12, (model - part) as i32);
    put_i32(&mut mdl, model + 72, 1);
    put_i32(&mut mdl, model + 76, (mesh - model) as i32);
    put_i32(&mut mdl, model + 80, verts.len() as i32);
    put_i32(&mut mdl, mesh + 8, verts.len() as i32);
    put_str(&mut mdl, names, "box");
    put_str(&mut mdl, names + 4, "models/test/");
    let len = mdl.len() as i32;
    put_i32(&mut mdl, 76, len);
    let mut vvd = vec![0u8; 64];
    vvd[0..4].copy_from_slice(b"IDSV");
    put_i32(&mut vvd, 4, 4);
    put_i32(&mut vvd, 8, CHECKSUM);
    put_i32(&mut vvd, 12, 1);
    put_i32(&mut vvd, 16, verts.len() as i32);
    put_i32(&mut vvd, 52, 64);
    put_i32(&mut vvd, 56, 64);
    put_i32(&mut vvd, 60, 64 + 48 * verts.len() as i32);
    for (i, (p, n)) in verts.iter().enumerate() {
        let o = 64 + i * 48;
        for k in 0..3 {
            put_f32(&mut vvd, o + 16 + k * 4, p[k]);
            put_f32(&mut vvd, o + 28 + k * 4, n[k]);
        }
        put_f32(&mut vvd, o + 44, 0.0);
    }
    let mut vtx = vec![0u8; 36];
    put_i32(&mut vtx, 0, 7);
    put_i32(&mut vtx, 16, CHECKSUM);
    put_i32(&mut vtx, 20, 1);
    put_i32(&mut vtx, 28, 1);
    put_i32(&mut vtx, 32, 36);
    let (bp, md, lod, me, grp) = (36, 44, 52, 64, 73);
    put_i32(&mut vtx, bp, 1);
    put_i32(&mut vtx, bp + 4, (md - bp) as i32);
    put_i32(&mut vtx, md, 1);
    put_i32(&mut vtx, md + 4, (lod - md) as i32);
    put_i32(&mut vtx, lod, 1);
    put_i32(&mut vtx, lod + 4, (me - lod) as i32);
    put_i32(&mut vtx, me, 1);
    put_i32(&mut vtx, me + 4, (grp - me) as i32);
    let (vt, ix) = (grp + 25, grp + 25 + 9 * verts.len());
    let st = ix + 2 * 3 * tris.len();
    put_i32(&mut vtx, grp, verts.len() as i32);
    put_i32(&mut vtx, grp + 4, (vt - grp) as i32);
    put_i32(&mut vtx, grp + 8, (3 * tris.len()) as i32);
    put_i32(&mut vtx, grp + 12, (ix - grp) as i32);
    put_i32(&mut vtx, grp + 16, 1);
    put_i32(&mut vtx, grp + 20, (st - grp) as i32);
    vtx.resize(st + 27, 0);
    for i in 0..verts.len() {
        vtx[vt + i * 9 + 4..vt + i * 9 + 6].copy_from_slice(&(i as u16).to_le_bytes());
    }
    for (k, i) in tris.iter().flatten().enumerate() {
        vtx[ix + k * 2..ix + k * 2 + 2].copy_from_slice(&i.to_le_bytes());
    }
    put_i32(&mut vtx, st, (3 * tris.len()) as i32);
    vtx[st + 18] = 1;
    (mdl, vvd, vtx)
}

struct BoxModel {
    files: (Vec<u8>, Vec<u8>, Vec<u8>),
}

impl crate::props::ModelSource for BoxModel {
    fn bounds(&self, _path: &str) -> Option<(DVec3, DVec3)> {
        None
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        match path.to_ascii_lowercase().as_str() {
            "models/test/box.mdl" => Some(self.files.0.clone()),
            "models/test/box.vvd" => Some(self.files.1.clone()),
            "models/test/box.dx90.vtx" => Some(self.files.2.clone()),
            _ => None,
        }
    }
}

fn room_with_box(extra: &str) -> Vec<u8> {
    let ents = format!(
        "entity\n{{\n\t\"id\" \"500\"\n\t\"classname\" \"light\"\n\t\"origin\" \"64 128 128\"\n\t\"_light\" \"255 255 255 200\"\n\t\"_quadratic_attn\" \"1\"\n}}\n\
         entity\n{{\n\t\"id\" \"501\"\n\t\"classname\" \"prop_static\"\n\t\"model\" \"models/test/box.mdl\"\n\t\"origin\" \"128 128 0\"\n\t\"angles\" \"0 0 0\"\n\t\"solid\" \"6\"\n{extra}}}\n"
    );
    compile_vmf(&room_with("DEV/DEV_MEASUREGENERIC01B", "DEV/DEV_MEASUREGENERIC01B", &ents))
}

fn opts(lighting: bool) -> RadOptions {
    RadOptions {
        hdr: true,
        ldr: true,
        bounces: 1,
        quality: RadQuality::Draft,
        threads: Some(2),
        props: PropOptions { lighting, ..PropOptions::default() },
        ..RadOptions::default()
    }
}

fn pak_file(bsp: &[u8], name: &str) -> Option<Vec<u8>> {
    let src = hp_bsp::Bsp::from_vec(bsp.to_vec()).ok()?;
    let pak = src.pakfile().ok()?;
    pak.read_name(name).ok().flatten().map(|c| c.into_owned())
}

fn map_flags(bsp: &[u8]) -> u32 {
    let src = hp_bsp::Bsp::from_vec(bsp.to_vec()).unwrap();
    let raw = src.lump(lump::MAP_FLAGS).unwrap();
    u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])
}

#[test]
fn box_prop_shadow_and_vertex_lighting() {
    let models = BoxModel { files: box_files() };
    let bsp = room_with_box("");
    let lit = light_bsp_with(&bsp, &opts(true), &models, &mut |_| {}).expect("light");
    assert!(crate::validate::validate(&lit).is_empty());
    let bare = light_bsp_with(&bsp, &opts(false), &crate::props::NoModels, &mut |_| {}).expect("light");
    let floor = |b: &[u8], x: f64| luxel_at(b, DVec3::new(x, 128.0, 0.0), DVec3::Z, false).expect("floor").x;
    let (shadow, open) = (floor(&lit, 158.0), floor(&bare, 158.0));
    assert!(shadow < open * 0.5, "shadow {shadow} open {open}");
    let (a, b) = (floor(&lit, 40.0), floor(&bare, 40.0));
    assert!((a - b).abs() < b * 0.1, "{a} {b}");
    for name in ["sp_0.vhv", "sp_hdr_0.vhv"] {
        let f = pak_file(&lit, name).unwrap_or_else(|| panic!("{name}"));
        let u = |o: usize| u32::from_le_bytes(f[o..o + 4].try_into().unwrap());
        assert_eq!((u(0), u(4) as i32, u(8), u(12), u(16), u(20)), (2, CHECKSUM, 4, 4, 24, 1));
        assert_eq!((u(40), u(44), u(48)), (0, 24, 512));
        assert_eq!(f.len(), 1024);
        let color = |v: usize| {
            Vec3A::new(f32::from(f[512 + v * 4 + 2]), f32::from(f[512 + v * 4 + 1]), f32::from(f[512 + v * 4]))
        };
        assert!(color(0).x > 40.0 && color(0).x > color(4).x, "top {} bottom {}", color(0), color(4));
        assert!((0..24).all(|v| f[512 + v * 4 + 3] == 255));
    }
    assert_eq!(map_flags(&lit) & 3, 3);
    assert_eq!(map_flags(&bare) & 3, 0);
    assert!(pak_file(&bare, "sp_0.vhv").is_none());
}

#[test]
fn box_prop_flags() {
    let models = BoxModel { files: box_files() };
    let bsp = room_with_box("\t\"disableshadows\" \"1\"\n");
    let lit = light_bsp_with(&bsp, &opts(true), &models, &mut |_| {}).expect("light");
    let bare = light_bsp_with(&bsp, &opts(false), &crate::props::NoModels, &mut |_| {}).expect("light");
    let floor = |b: &[u8]| luxel_at(b, DVec3::new(158.0, 128.0, 0.0), DVec3::Z, false).expect("floor").x;
    assert!((floor(&lit) - floor(&bare)).abs() < floor(&bare) * 0.1);
    assert!(pak_file(&lit, "sp_0.vhv").is_some());
    let bsp = room_with_box("\t\"disablevertexlighting\" \"1\"\n");
    let lit = light_bsp_with(&bsp, &opts(true), &models, &mut |_| {}).expect("light");
    assert!(pak_file(&lit, "sp_0.vhv").is_none() && pak_file(&lit, "sp_hdr_0.vhv").is_none());
    assert!(floor(&lit) < floor(&bare) * 0.5);
    assert_eq!(map_flags(&lit) & 3, 3);
}

struct Counting {
    inner: BoxModel,
    reads: std::sync::Mutex<Vec<String>>,
}

impl crate::props::ModelSource for Counting {
    fn bounds(&self, path: &str) -> Option<(DVec3, DVec3)> {
        self.inner.bounds(path)
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.reads.lock().unwrap().push(path.to_ascii_lowercase());
        self.inner.read_file(path)
    }
}

#[test]
fn box_prop_shadow_only_with_shadows_on() {
    let bsp = room_with_box("");
    let models = || Counting { inner: BoxModel { files: box_files() }, reads: std::sync::Mutex::new(Vec::new()) };
    let floor = |b: &[u8], x: f64| luxel_at(b, DVec3::new(x, 128.0, 0.0), DVec3::Z, false).expect("floor").x;
    let with = |lighting: bool, shadows: bool| {
        let mut o = opts(lighting);
        o.props.shadows = shadows;
        o
    };
    let mut bare_lines = Vec::new();
    let bare = light_bsp_with(&bsp, &opts(false), &crate::props::NoModels, &mut |l| bare_lines.push(l.to_owned()))
        .expect("light");
    let open = floor(&bare, 158.0);
    let scene = |lines: &[String]| lines.iter().find(|l| l.contains(" lights, ")).cloned().unwrap_or_default();

    let m = models();
    let lit = light_bsp_with(&bsp, &with(false, true), &m, &mut |_| {}).expect("light");
    assert!(floor(&lit, 158.0) < open * 0.5, "shadow {} open {open}", floor(&lit, 158.0));
    assert!(m.reads.lock().unwrap().iter().any(|p| p.ends_with(".phy")), "the collision model is looked for");

    let m = models();
    let mut lines = Vec::new();
    let lit = light_bsp_with(&bsp, &with(false, false), &m, &mut |l| lines.push(l.to_owned())).expect("light");
    assert!(crate::validate::validate(&lit).is_empty());
    let (a, b) = (floor(&lit, 158.0), open);
    assert!((a - b).abs() <= b * 0.02, "no shadow: {a} open {b}");
    assert!(m.reads.lock().unwrap().is_empty(), "{:?}", m.reads.lock().unwrap());
    assert!(lines.iter().any(|l| l.contains("-nostaticpropshadows")), "{lines:?}");
    assert_eq!(scene(&lines), scene(&bare_lines));

    let m = models();
    let lit = light_bsp_with(&bsp, &with(true, false), &m, &mut |_| {}).expect("light");
    let (a, b) = (floor(&lit, 158.0), open);
    assert!((a - b).abs() <= b * 0.02, "no shadow: {a} open {b}");
    assert!(pak_file(&lit, "sp_0.vhv").is_some() && map_flags(&lit) & 3 == 3);
    assert!(!m.reads.lock().unwrap().iter().any(|p| p.ends_with(".phy")));
}

#[test]
fn box_prop_from_the_map_pakfile() {
    let (mdl, vvd, vtx) = box_files();
    let bsp = room_with_box("");
    let mut pak = hp_bsp::PakWriter::new();
    pak.add("models/test/box.mdl", mdl).unwrap();
    pak.add("models/test/box.vvd", vvd).unwrap();
    pak.add("models/test/box.dx90.vtx", vtx).unwrap();
    let mut edit = hp_bsp::Bsp::from_vec(bsp.clone()).unwrap().edit();
    edit.set_pakfile(pak.finish_vbsp().unwrap()).unwrap();
    let packed = edit.to_vec().unwrap();
    let mut lines = Vec::new();
    let lit = light_bsp_with(&packed, &opts(true), &crate::props::NoModels, &mut |l| lines.push(l.to_owned()))
        .expect("light");
    assert!(!lines.iter().any(|l| l.contains("Unable to load")), "{lines:?}");
    assert!(pak_file(&lit, "sp_0.vhv").is_some() && pak_file(&lit, "sp_hdr_0.vhv").is_some());
    assert!(pak_file(&lit, "models/test/box.mdl").is_some(), "the packed model stays");
    let bare = light_bsp_with(&bsp, &opts(false), &crate::props::NoModels, &mut |_| {}).expect("light");
    let floor = |b: &[u8]| luxel_at(b, DVec3::new(158.0, 128.0, 0.0), DVec3::Z, false).expect("floor").x;
    assert!(floor(&lit) < floor(&bare) * 0.5, "the packed prop casts its shadow");
}

struct Slow {
    inner: BoxModel,
}

impl crate::props::ModelSource for Slow {
    fn bounds(&self, path: &str) -> Option<(DVec3, DVec3)> {
        self.inner.bounds(path)
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        std::thread::sleep(std::time::Duration::from_millis(400));
        self.inner.read_file(path)
    }
}

#[test]
fn slow_stages_print_progress() {
    let bsp = room_with_box("");
    let models = Slow { inner: BoxModel { files: box_files() } };
    let mut lines = Vec::new();
    light_bsp_with(&bsp, &opts(true), &models, &mut |l| lines.push(l.to_owned())).expect("light");
    let progress: Vec<&String> = lines.iter().filter(|l| l.starts_with("Lighting ") && l.contains("(stage ")).collect();
    assert!(!progress.is_empty(), "{lines:?}");
    let first = progress[0];
    assert!(first.contains(": scene ") && first.contains("(stage 1 of ") && first.contains(" elapsed, "), "{first}");
    let plan = lines.iter().find(|l| l.starts_with("Lighting stages: ")).expect("plan");
    assert!(plan.ends_with("leaf ambient, static props, writing"), "{plan}");
}
