use std::collections::HashSet;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::{DVec2, DVec3};
use hp_app::AppState;
use hp_editor::FaceRef;
use hp_geom::{DispMesh, SolidGeom};
use hp_vmf::TexAxis;
use serde::{Deserialize, Serialize};

mod model;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportKind {
    Vmf,
    Obj,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LastExport {
    pub kind: Option<ExportKind>,
    pub path: Option<PathBuf>,
}

impl hp_app::sidecar::SidecarSection for LastExport {
    const NAME: &'static str = "startup_export";
}

fn write_atomic(path: &Path, write: impl FnOnce(&mut BufWriter<std::fs::File>) -> io::Result<()>) -> io::Result<()> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut out = BufWriter::new(std::fs::File::create(&tmp)?);
        write(&mut out)?;
        out.into_inner().map_err(|e| e.into_error())?.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

pub fn write_vmf(map: &hp_vmf::Map, path: &Path) -> Result<(), String> {
    let opts = hp_vmf::SaveOptions { backup: false, sync: true };
    hp_vmf::save_with(map, path, &opts).map(|_| ()).map_err(|e| e.to_string())
}

#[derive(Clone, Copy, Debug)]
struct SideInfo {
    material: u32,
    u: TexAxis,
    v: TexAxis,
}

struct ObjSolid {
    id: u32,
    geom: Arc<SolidGeom>,
    sides: Vec<SideInfo>,
    disp: Vec<Option<Arc<DispMesh>>>,
}

pub struct ObjInput {
    solids: Vec<ObjSolid>,
    materials: Vec<(String, [f64; 2])>,
    layout: ObjLayout,
    disp_only: bool,
}

pub const METERS_PER_UNIT: f64 = 0.01905;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObjLayout {
    pub meters: bool,
    pub z_up: bool,
}

impl ObjLayout {
    pub fn from_settings(s: &hp_app::settings::MeshSettings) -> Self {
        Self { meters: s.mesh_export_meters, z_up: s.mesh_export_z_up }
    }

    fn position(self, p: DVec3) -> DVec3 {
        let q = self.direction(p);
        if self.meters { q * METERS_PER_UNIT } else { q }
    }

    fn direction(self, d: DVec3) -> DVec3 {
        if self.z_up { d } else { yup(d) }
    }

    fn header(self) -> String {
        format!(
            "# Exported by Hammer Pro ({}, {} up)",
            if self.meters { "meters" } else { "Source units" },
            if self.z_up { "Z" } else { "Y" }
        )
    }
}

const DEFAULT_TEXTURE_SIZE: f64 = 512.0;

fn is_tool(material: &str) -> bool {
    material.to_ascii_lowercase().replace('\\', "/").starts_with("tools/")
}

impl ObjInput {
    pub fn from_state(state: &mut AppState) -> Self {
        Self::build(state, None, ObjLayout::default())
    }

    pub fn from_displacements(state: &mut AppState, faces: &[FaceRef], layout: ObjLayout) -> Self {
        let wanted: HashSet<(u32, usize)> = faces.iter().map(|f| (f.solid, f.side)).collect();
        Self::build(state, Some(&wanted), layout)
    }

    pub(crate) fn build(state: &mut AppState, only: Option<&HashSet<(u32, usize)>>, layout: ObjLayout) -> Self {
        let mut names: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        let mut materials: Vec<(String, [f64; 2])> = Vec::new();
        let mut solids = Vec::with_capacity(state.doc.scene().solids().len());
        for node in state.doc.scene().solids().values() {
            let sides: Vec<SideInfo> = node
                .solid
                .sides
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    if only.is_some_and(|w| !w.contains(&(node.solid.id, i))) {
                        return SideInfo { material: u32::MAX, u: s.uaxis, v: s.vaxis };
                    }
                    let next = u32::try_from(materials.len()).unwrap_or(u32::MAX);
                    let material = *names.entry(s.material.clone()).or_insert_with(|| {
                        materials.push((s.material.clone(), [DEFAULT_TEXTURE_SIZE; 2]));
                        next
                    });
                    SideInfo { material, u: s.uaxis, v: s.vaxis }
                })
                .collect();
            if only.is_some() && sides.iter().all(|s| s.material == u32::MAX) {
                continue;
            }
            solids.push(ObjSolid { id: node.solid.id, geom: Arc::clone(&node.geom), sides, disp: node.disp.clone() });
        }
        solids.sort_by_key(|s| s.id);
        for (name, size) in &mut materials {
            if let Some(info) = state.assets.material_info(name)
                && info.width > 0
                && info.height > 0
            {
                *size = [f64::from(info.width), f64::from(info.height)];
            }
        }
        Self { solids, materials, layout, disp_only: only.is_some() }
    }
}

fn yup(p: DVec3) -> DVec3 {
    DVec3::new(p.x, p.z, -p.y)
}

fn axis_coord(a: &TexAxis, p: DVec3) -> f64 {
    let scale = if a.scale.abs() < 1e-6 { 1.0 } else { a.scale };
    p.dot(a.axis) / scale + a.shift
}

fn uv(side: &SideInfo, size: [f64; 2], p: DVec3) -> DVec2 {
    DVec2::new(axis_coord(&side.u, p) / size[0], -axis_coord(&side.v, p) / size[1])
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObjStats {
    pub faces: usize,
    pub triangles: usize,
}

pub fn write_obj(out: &mut impl Write, input: &ObjInput, mtl: &str) -> io::Result<ObjStats> {
    writeln!(out, "{}", input.layout.header())?;
    writeln!(out, "mtllib {mtl}")?;
    let mut stats = ObjStats::default();
    let (mut nv, mut nt, mut nn) = (1usize, 1usize, 1usize);
    let mut current: Option<u32> = None;
    for solid in &input.solids {
        let mut wrote_group = false;
        for face in &solid.geom.faces {
            let Some(side) = solid.sides.get(face.side_index) else { continue };
            let Some((name, size)) = input.materials.get(side.material as usize) else { continue };
            if is_tool(name) || face.vertices.len() < 3 {
                continue;
            }
            if !wrote_group {
                writeln!(out, "g solid_{}", solid.id)?;
                wrote_group = true;
            }
            if current != Some(side.material) {
                writeln!(out, "usemtl {}", obj_name(name))?;
                current = Some(side.material);
            }
            let disp = solid.disp.get(face.side_index).and_then(Option::as_ref);
            if input.disp_only && disp.is_none() {
                continue;
            }
            match disp {
                Some(mesh) => {
                    for (p, base) in mesh.positions.iter().zip(&mesh.base_positions) {
                        let q = input.layout.position(*p);
                        writeln!(out, "v {} {} {}", q.x, q.y, q.z)?;
                        let t = uv(side, *size, *base);
                        writeln!(out, "vt {} {}", t.x, t.y)?;
                    }
                    for n in &mesh.normals {
                        let n = input.layout.direction(*n);
                        writeln!(out, "vn {} {} {}", n.x, n.y, n.z)?;
                    }
                    let count = mesh.positions.len();
                    for tri in mesh.indices.chunks_exact(3) {
                        let idx: Vec<usize> = tri.iter().map(|&i| i as usize).collect();
                        if idx.iter().any(|&i| i >= count) {
                            continue;
                        }
                        write!(out, "f")?;
                        for i in idx {
                            write!(out, " {}/{}/{}", nv + i, nt + i, nn + i)?;
                        }
                        writeln!(out)?;
                        stats.triangles += 1;
                    }
                    nv += count;
                    nt += count;
                    nn += mesh.normals.len();
                }
                None => {
                    for p in &face.vertices {
                        let q = input.layout.position(*p);
                        writeln!(out, "v {} {} {}", q.x, q.y, q.z)?;
                        let t = uv(side, *size, *p);
                        writeln!(out, "vt {} {}", t.x, t.y)?;
                    }
                    let n = input.layout.direction(face.normal());
                    writeln!(out, "vn {} {} {}", n.x, n.y, n.z)?;
                    write!(out, "f")?;
                    for i in 0..face.vertices.len() {
                        write!(out, " {}/{}/{}", nv + i, nt + i, nn)?;
                    }
                    writeln!(out)?;
                    nv += face.vertices.len();
                    nt += face.vertices.len();
                    nn += 1;
                    stats.triangles += face.vertices.len() - 2;
                }
            }
            stats.faces += 1;
        }
    }
    Ok(stats)
}

fn obj_name(name: &str) -> String {
    name.replace('\\', "/").chars().map(|c| if c.is_whitespace() { '_' } else { c }).collect()
}

pub fn write_mtl(out: &mut impl Write, input: &ObjInput) -> io::Result<()> {
    writeln!(out, "# Exported by Hammer Pro")?;
    for (name, _) in &input.materials {
        if is_tool(name) {
            continue;
        }
        writeln!(out, "newmtl {}", obj_name(name))?;
        writeln!(out, "Kd 0.8 0.8 0.8")?;
    }
    Ok(())
}

pub fn export_obj(input: &ObjInput, path: &Path) -> Result<ObjStats, String> {
    let mtl_path = path.with_extension("mtl");
    let mtl_name = mtl_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut stats = ObjStats::default();
    write_atomic(path, |out| {
        stats = write_obj(out, input, &mtl_name)?;
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    write_atomic(&mtl_path, |out| write_mtl(out, input)).map_err(|e| e.to_string())?;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ObjInput {
        let mut state = hp_app::testing::test_state();
        ObjInput::from_state(&mut state)
    }

    #[test]
    fn the_test_map_exports_its_visible_faces() {
        let input = input();
        let mut text = Vec::new();
        let stats = write_obj(&mut text, &input, "room.mtl").expect("write");
        let text = String::from_utf8(text).expect("utf8");
        assert!(stats.faces > 0 && stats.triangles >= stats.faces, "{stats:?}");
        assert!(text.starts_with("# Exported by Hammer Pro"));
        assert!(text.contains("mtllib room.mtl"));
        let (v, vt, vn) = (
            text.lines().filter(|l| l.starts_with("v ")).count(),
            text.lines().filter(|l| l.starts_with("vt ")).count(),
            text.lines().filter(|l| l.starts_with("vn ")).count(),
        );
        for line in text.lines().filter(|l| l.starts_with("f ")) {
            for corner in line.split_whitespace().skip(1) {
                let idx: Vec<usize> = corner.split('/').map(|n| n.parse().expect("index")).collect();
                assert!(idx[0] >= 1 && idx[0] <= v && idx[1] <= vt && idx[2] <= vn, "{line}");
            }
        }
        assert!(!text.to_ascii_lowercase().contains("usemtl tools/"), "tool textures are left out");
        let mut mtl = Vec::new();
        write_mtl(&mut mtl, &input).expect("mtl");
        let mtl = String::from_utf8(mtl).expect("utf8");
        assert!(mtl.contains("newmtl"));
    }

    #[test]
    fn exports_are_written_atomically() {
        let dir = std::env::temp_dir().join(format!("hammerpro_export_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let input = input();
        let obj = dir.join("room.obj");
        let stats = export_obj(&input, &obj).expect("export");
        assert!(stats.faces > 0);
        assert!(obj.is_file() && dir.join("room.mtl").is_file());
        assert_eq!(std::fs::read_dir(&dir).expect("list").count(), 2, "no temporary file left");
        let mut state = hp_app::testing::test_state();
        let map = state.doc.to_map();
        write_vmf(&map, &dir.join("copy.vmf")).expect("vmf");
        assert!(!dir.join("copy.vmx").exists(), "no backup for a copy");
        assert!(write_vmf(&map, &dir.join("missing/copy.vmf")).is_err());
        assert!(export_obj(&ObjInput::from_state(&mut state), &dir.join("missing/a.obj")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn layouts_move_and_scale_points() {
        let p = DVec3::new(100.0, 200.0, 300.0);
        let source = ObjLayout::default();
        assert_eq!(source.position(p), DVec3::new(100.0, 300.0, -200.0));
        let z = ObjLayout { meters: true, z_up: true };
        assert_eq!(z.position(p), p * METERS_PER_UNIT);
        assert_eq!(z.direction(p), p);
        assert!(z.header().contains("meters") && z.header().contains("Z up"));
        assert!(source.header().contains("Source units") && source.header().contains("Y up"));
    }

    #[test]
    fn no_displacement_means_nothing_to_write() {
        let mut state = hp_app::testing::test_state();
        let input = ObjInput::from_displacements(&mut state, &[FaceRef::new(2, 0)], ObjLayout::default());
        let mut text = Vec::new();
        let stats = write_obj(&mut text, &input, "a.mtl").expect("write");
        assert_eq!(stats.faces, 0);
        assert!(!String::from_utf8(text).expect("utf8").contains(
            "
v "
        ));
    }

    #[test]
    fn coordinates_and_names() {
        assert_eq!(yup(DVec3::new(1.0, 2.0, 3.0)), DVec3::new(1.0, 3.0, -2.0));
        assert_eq!(obj_name("My Folder\\Brick 01"), "My_Folder/Brick_01");
        assert!(is_tool("TOOLS\\toolsnodraw") && !is_tool("concrete/a"));
    }
}
