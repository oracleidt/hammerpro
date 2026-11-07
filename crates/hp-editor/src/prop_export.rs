use std::collections::HashMap;

use glam::DVec3;
use hp_geom::{Aabb, DISP_FLAG_NO_PHYSICS, DispMesh};

use crate::{Document, Id, ObjRef};

mod collision;
pub use collision::EditableCollision;

pub const DISP_THICKNESS: f64 = 8.0;
const DISP_FLAT_TOLERANCE: f64 = 0.5;
const MAX_DISP_PIECES: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DispCorners {
    pub face: usize,
    pub grid: [[f64; 2]; 3],
    pub alpha: [f64; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropTri {
    pub material: String,
    pub pos: [DVec3; 3],
    pub normal: [DVec3; 3],
    pub uv: [[f64; 2]; 3],
    pub disp: Option<DispCorners>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DispFace {
    pub material: String,
    pub row_len: usize,
    pub base: [DVec3; 4],
    pub base_uv: [[f64; 2]; 4],
    pub normal: DVec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropGeometry {
    pub tris: Vec<PropTri>,
    pub disps: Vec<DispFace>,
    pub pieces: Vec<Vec<[DVec3; 3]>>,
    pub origin: DVec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropLod {
    pub switch: f64,
    pub tris: Vec<PropTri>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropExport {
    pub smd: String,
    pub phys_smd: String,
    pub lod_smds: Vec<String>,
    pub qc: String,
    pub materials: Vec<String>,
    pub origin: DVec3,
    pub triangles: usize,
    pub pieces: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropOptions {
    pub physics: bool,
    pub static_prop: bool,
    pub collision: bool,
    pub mass: f64,
}

impl Default for PropOptions {
    fn default() -> Self {
        Self { physics: false, static_prop: true, collision: true, mass: 10.0 }
    }
}

impl PropOptions {
    pub fn clamped(self) -> Self {
        Self {
            physics: self.physics,
            static_prop: self.static_prop && !self.physics,
            collision: self.collision || self.physics,
            mass: if self.mass.is_finite() { self.mass.clamp(0.1, 50_000.0) } else { 10.0 },
        }
    }
}

/// QC settings in model-local Source coordinates, shared by the preview and export.
#[derive(Clone, Debug, PartialEq)]
pub struct PropDirectives {
    pub illumposition: Option<DVec3>,
    pub bbox: Option<Aabb>,
    pub cdmaterials: String,
}

impl Default for PropDirectives {
    fn default() -> Self {
        Self { illumposition: None, bbox: None, cdmaterials: "/".to_owned() }
    }
}

impl PropDirectives {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.illumposition.is_some_and(|p| !p.is_finite()) {
            return Err("The illumination position must contain finite coordinates.");
        }
        if self.bbox.is_some_and(|b| !b.min.is_finite() || !b.max.is_finite() || !b.min.cmplt(b.max).all()) {
            return Err("Each minimum bound must be smaller than its maximum bound.");
        }
        let path = self.cdmaterials.trim_matches('/');
        if !path.is_empty()
            && path.split('/').any(|part| {
                part.is_empty()
                    || part == "."
                    || part == ".."
                    || !part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
            })
        {
            return Err("The material path must be relative to the materials folder.");
        }
        Ok(())
    }
}

pub fn material_key(name: &str) -> String {
    name.trim().replace('\\', "/").trim_end_matches(".vmt").to_ascii_lowercase()
}

fn uv(p: DVec3, u: &hp_vmf::TexAxis, v: &hp_vmf::TexAxis, size: (u32, u32)) -> [f64; 2] {
    let (w, h) = (size.0.max(1) as f64, size.1.max(1) as f64);
    let us = if u.scale.abs() > 1e-9 { u.scale } else { 0.25 };
    let vs = if v.scale.abs() > 1e-9 { v.scale } else { 0.25 };
    [(u.axis.dot(p) / us + u.shift) / w, (v.axis.dot(p) / vs + v.shift) / h]
}

fn solid_tris(
    doc: &Document,
    id: Id,
    center: DVec3,
    size: &dyn Fn(&str) -> (u32, u32),
    out: &mut Vec<PropTri>,
    disps: &mut Vec<DispFace>,
) {
    let Some(node) = doc.scene().solid(id) else { return };
    let has_disp = node.disp.iter().any(Option::is_some);
    for face in node.geom.faces.iter().filter(|_| !has_disp) {
        let Some(side) = node.solid.sides.get(face.side_index) else { continue };
        let material = material_key(&side.material);
        if material.starts_with("tools/") {
            continue;
        }
        let msize = size(&material);
        let n = face.plane.normal;
        let verts: Vec<(DVec3, [f64; 2])> =
            face.vertices.iter().map(|&p| (p - center, uv(p, &side.uaxis, &side.vaxis, msize))).collect();
        for t in face.triangles() {
            out.push(PropTri {
                material: material.clone(),
                pos: [verts[t[0]].0, verts[t[1]].0, verts[t[2]].0],
                normal: [n; 3],
                uv: [verts[t[0]].1, verts[t[1]].1, verts[t[2]].1],
                disp: None,
            });
        }
    }
    for (side_index, mesh) in node.disp.iter().enumerate() {
        let Some(mesh) = mesh else { continue };
        let Some(side) = node.solid.sides.get(side_index) else { continue };
        let material = material_key(&side.material);
        if material.starts_with("tools/") || mesh.row_len < 2 {
            continue;
        }
        let msize = size(&material);
        let n = mesh.row_len - 1;
        let grid = |i: usize| [(i % mesh.row_len) as f64 / n as f64, (i / mesh.row_len) as f64 / n as f64];
        let corner_index = [0, n, n * mesh.row_len + n, n * mesh.row_len];
        let Some(base) = corner_index.iter().map(|&i| mesh.base_positions.get(i).copied()).collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let base: [DVec3; 4] = [base[0], base[1], base[2], base[3]];
        let face = disps.len();
        disps.push(DispFace {
            material: material.clone(),
            row_len: mesh.row_len,
            base,
            base_uv: base.map(|p| uv(p, &side.uaxis, &side.vaxis, msize)),
            normal: mesh.face_normal,
        });
        for t in mesh.indices.chunks_exact(3) {
            let idx = [t[0] as usize, t[1] as usize, t[2] as usize];
            if idx.iter().any(|&i| i >= mesh.positions.len() || i >= mesh.base_positions.len()) {
                continue;
            }
            let pos = idx.map(|i| mesh.positions[i] - center);
            let flat = (pos[1] - pos[0]).cross(pos[2] - pos[0]).normalize_or_zero();
            let flat = if flat == DVec3::ZERO { mesh.face_normal } else { flat };
            let normal = idx.map(|i| {
                let v = mesh.normals.get(i).copied().unwrap_or(flat).normalize_or_zero();
                if v == DVec3::ZERO { flat } else { v }
            });
            out.push(PropTri {
                material: material.clone(),
                pos,
                normal,
                uv: idx.map(|i| uv(mesh.base_positions[i], &side.uaxis, &side.vaxis, msize)),
                disp: Some(DispCorners {
                    face,
                    grid: idx.map(grid),
                    alpha: idx.map(|i| mesh.alphas.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0)),
                }),
            });
        }
    }
}

const NON_SOLID_TOOLS: &[&str] = &[
    "tools/toolstrigger",
    "tools/toolshint",
    "tools/toolsskip",
    "tools/toolsareaportal",
    "tools/toolsoccluder",
    "tools/toolsfog",
    "tools/toolsorigin",
    "tools/toolsskybox",
    "tools/toolsskybox2d",
];

fn collision_pieces(doc: &Document, solids: &[Id], center: DVec3) -> Vec<Vec<[DVec3; 3]>> {
    let mut pieces = Vec::new();
    let mut disps: Vec<&DispMesh> = Vec::new();
    for id in solids {
        let Some(node) = doc.scene().solid(*id) else { continue };
        if node.disp.iter().any(Option::is_some) {
            for (side, mesh) in node.solid.sides.iter().zip(&node.disp) {
                let Some(mesh) = mesh else { continue };
                let no_physics = side.disp.as_ref().is_some_and(|d| d.flags & DISP_FLAG_NO_PHYSICS != 0);
                if !no_physics && !NON_SOLID_TOOLS.contains(&material_key(&side.material).as_str()) {
                    disps.push(mesh);
                }
            }
            continue;
        }
        let solid = node.solid.sides.iter().any(|s| !NON_SOLID_TOOLS.contains(&material_key(&s.material).as_str()));
        if !solid {
            continue;
        }
        let mut tris = Vec::new();
        for face in &node.geom.faces {
            for t in face.triangles() {
                tris.push([face.vertices[t[0]] - center, face.vertices[t[1]] - center, face.vertices[t[2]] - center]);
            }
        }
        if tris.len() >= 4 {
            pieces.push(tris);
        }
    }
    let mut tolerance = DISP_FLAT_TOLERANCE;
    loop {
        let mut slabs = Vec::new();
        for mesh in &disps {
            disp_slabs(mesh, center, tolerance, &mut slabs);
        }
        if slabs.len() <= MAX_DISP_PIECES || tolerance > 64.0 {
            pieces.extend(slabs);
            break;
        }
        tolerance *= 2.0;
    }
    pieces
}

pub fn disp_slabs(mesh: &DispMesh, center: DVec3, tolerance: f64, out: &mut Vec<Vec<[DVec3; 3]>>) {
    if mesh.row_len < 2 {
        return;
    }
    let n = mesh.row_len - 1;
    let mut cells: HashMap<(usize, usize), Vec<[usize; 3]>> = HashMap::new();
    for t in mesh.indices.chunks_exact(3) {
        let idx = [t[0] as usize, t[1] as usize, t[2] as usize];
        if idx.iter().any(|&i| i >= mesh.positions.len()) {
            continue;
        }
        let col = idx.iter().map(|i| i % mesh.row_len).min().unwrap_or(0).min(n - 1);
        let row = idx.iter().map(|i| i / mesh.row_len).min().unwrap_or(0).min(n - 1);
        cells.entry((col, row)).or_default().push(idx);
    }
    let blocks = Blocks { mesh, cells: &cells, n, center, tolerance };
    blocks.slabs((0, 0), n.next_power_of_two(), out);
}

struct Blocks<'a> {
    mesh: &'a DispMesh,
    cells: &'a HashMap<(usize, usize), Vec<[usize; 3]>>,
    n: usize,
    center: DVec3,
    tolerance: f64,
}

impl Blocks<'_> {
    fn slabs(&self, at: (usize, usize), size: usize, out: &mut Vec<Vec<[DVec3; 3]>>) {
        if at.0 >= self.n || at.1 >= self.n {
            return;
        }
        let mut tris = Vec::new();
        for c in at.0..(at.0 + size).min(self.n) {
            for r in at.1..(at.1 + size).min(self.n) {
                if let Some(t) = self.cells.get(&(c, r)) {
                    tris.extend_from_slice(t);
                }
            }
        }
        if tris.is_empty() {
            return;
        }
        if self.flatness(&tris).is_some_and(|d| d <= self.tolerance) {
            self.slab(&tris, out);
            return;
        }
        if size == 1 {
            for t in tris {
                self.slab(&[t], out);
            }
            return;
        }
        let h = size / 2;
        for (dc, dr) in [(0, 0), (h, 0), (0, h), (h, h)] {
            self.slabs((at.0 + dc, at.1 + dr), h, out);
        }
    }

    fn mean_normal(&self, tris: &[[usize; 3]]) -> Option<DVec3> {
        let mut sum = DVec3::ZERO;
        for t in tris {
            let p = t.map(|i| self.mesh.positions[i]);
            sum += (p[1] - p[0]).cross(p[2] - p[0]);
        }
        let n = sum.normalize_or_zero();
        (n != DVec3::ZERO).then_some(n)
    }

    fn flatness(&self, tris: &[[usize; 3]]) -> Option<f64> {
        let n = self.mean_normal(tris)?;
        let pts: Vec<DVec3> = tris.iter().flatten().map(|&i| self.mesh.positions[i]).collect();
        let mid = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
        Some(pts.iter().map(|p| n.dot(*p - mid).abs()).fold(0.0, f64::max))
    }

    fn slab(&self, tris: &[[usize; 3]], out: &mut Vec<Vec<[DVec3; 3]>>) {
        let Some(n) = self.mean_normal(tris) else { return };
        let down = -n * DISP_THICKNESS;
        let top = |i: usize| self.mesh.positions[i] - self.center;
        let mut piece = Vec::with_capacity(tris.len() * 2 + 8);
        let mut edges: HashMap<(usize, usize), usize> = HashMap::new();
        for t in tris {
            piece.push(t.map(top));
            piece.push([top(t[0]) + down, top(t[2]) + down, top(t[1]) + down]);
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        for t in tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                if edges.get(&(a.min(b), a.max(b))) == Some(&1) {
                    let (pa, pb) = (top(a), top(b));
                    piece.push([pb, pa, pa + down]);
                    piece.push([pb, pa + down, pb + down]);
                }
            }
        }
        out.push(piece);
    }
}

fn piece_normal(i: usize, count: usize) -> DVec3 {
    let n = count.max(1) as f64;
    let z = 1.0 - (i as f64 + 0.5) * 2.0 / n;
    let r = (1.0 - z * z).max(0.0).sqrt();
    let a = i as f64 * 2.399_963_229_728_653;
    DVec3::new(r * a.cos(), r * a.sin(), z)
}

const SMD_HEADER: &str =
    "version 1\nnodes\n0 \"static_prop\" -1\nend\nskeleton\ntime 0\n0 0 0 0 0 0 0\nend\ntriangles\n";

fn conv(v: DVec3) -> (f64, f64, f64) {
    (v.y, -v.x, v.z)
}

fn phys_smd_text(pieces: &[Vec<[DVec3; 3]>]) -> String {
    let mut s = String::from(SMD_HEADER);
    for (i, piece) in pieces.iter().enumerate() {
        let (nx, ny, nz) = conv(piece_normal(i, pieces.len()));
        for t in piece {
            s.push_str("phys\n");
            for p in t {
                let (px, py, pz) = conv(*p);
                s.push_str(&format!("0 {px:.4} {py:.4} {pz:.4} {nx:.6} {ny:.6} {nz:.6} 0 0\n"));
            }
        }
    }
    s.push_str("end\n");
    s
}

pub fn smd_text(tris: &[PropTri]) -> String {
    let mut s = String::from(SMD_HEADER);
    for t in tris {
        s.push_str(&t.material);
        s.push('\n');
        for k in 0..3 {
            let (px, py, pz) = conv(t.pos[k]);
            let (nx, ny, nz) = conv(t.normal[k]);
            let (u, v) = (t.uv[k][0], 1.0 - t.uv[k][1]);
            s.push_str(&format!("0 {px:.4} {py:.4} {pz:.4} {nx:.5} {ny:.5} {nz:.5} {u:.5} {v:.5}\n"));
        }
    }
    s.push_str("end\n");
    s
}

pub fn collect_prop(doc: &Document, solids: &[Id], size: &dyn Fn(&str) -> (u32, u32)) -> Option<PropGeometry> {
    let mut bounds: Option<Aabb> = None;
    for id in solids {
        if let Some(n) = doc.scene().solid(*id) {
            bounds = Some(match bounds {
                Some(b) => b.union(&n.bounds),
                None => n.bounds,
            });
        }
    }
    let center = bounds?.center().round();
    let mut tris = Vec::new();
    let mut disps = Vec::new();
    for id in solids {
        solid_tris(doc, *id, center, size, &mut tris, &mut disps);
    }
    if tris.is_empty() {
        return None;
    }
    Some(PropGeometry { tris, disps, pieces: collision_pieces(doc, solids, center), origin: center })
}

impl PropGeometry {
    pub fn files(&self, model_path: &str, surface_prop: &str, lods: &[PropLod]) -> PropExport {
        self.files_with_options(model_path, surface_prop, lods, PropOptions::default())
    }

    pub fn files_with_options(
        &self,
        model_path: &str,
        surface_prop: &str,
        lods: &[PropLod],
        options: PropOptions,
    ) -> PropExport {
        self.files_with_directives(model_path, surface_prop, lods, options, &PropDirectives::default())
    }

    pub fn files_with_directives(
        &self,
        model_path: &str,
        surface_prop: &str,
        lods: &[PropLod],
        options: PropOptions,
        directives: &PropDirectives,
    ) -> PropExport {
        let options = options.clamped();
        let mut materials: Vec<String> = self.tris.iter().map(|t| t.material.clone()).collect();
        materials.sort();
        materials.dedup();
        let model = model_path.trim_matches('/').replace('\\', "/");
        let surface = if surface_prop.trim().is_empty() { "default" } else { surface_prop.trim() };
        let stem = model.rsplit('/').next().unwrap_or("prop");
        let (phys_smd, collision) = if self.pieces.is_empty() || !options.collision {
            (String::new(), String::new())
        } else {
            let mass = if options.physics { format!("\t$mass {}\n", options.mass) } else { String::new() };
            let qc = format!(
                "$collisionmodel \"{stem}_phys.smd\" {{\n\t$concave\n\t$maxconvexpieces {}\n{mass}}}\n",
                self.pieces.len()
            );
            (phys_smd_text(&self.pieces), qc)
        };
        let mut lod_qc = String::new();
        let mut lod_smds = Vec::new();
        for (i, lod) in lods.iter().filter(|l| !l.tris.is_empty()).enumerate() {
            lod_qc.push_str(&format!(
                "$lod {}\n{{\n\treplacemodel \"{stem}_ref.smd\" \"{stem}_lod{}.smd\"\n}}\n",
                lod.switch.max(1.0).round(),
                i + 1
            ));
            lod_smds.push(smd_text(&lod.tris));
        }
        let static_prop = if options.static_prop { "$staticprop\n" } else { "" };
        let mut extra = String::new();
        if let Some(p) = directives.illumposition.filter(|p| p.is_finite()) {
            // studiomdl rotates $illumposition like SMD coordinates. $bbox, in
            // contrast, is already in the final model coordinate system.
            let (x, y, z) = conv(p);
            extra.push_str(&format!("$illumposition {x} {y} {z}\n"));
        }
        if let Some(b) = directives.bbox.filter(|b| b.min.is_finite() && b.max.is_finite() && b.min.cmplt(b.max).all())
        {
            extra.push_str(&format!("$bbox {} {} {} {} {} {}\n", b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z));
        }
        let material_path = directives.cdmaterials.trim_matches('/');
        let material_path = if material_path.is_empty() { "/" } else { material_path };
        let qc = format!(
            "$modelname \"{model}.mdl\"\n\
             {static_prop}\
             $surfaceprop \"{surface}\"\n\
             $cdmaterials \"{material_path}\"\n\
             {extra}\
             $body \"body\" \"{stem}_ref.smd\"\n\
             {lod_qc}\
             $sequence \"idle\" \"{stem}_ref.smd\" fps 1\n\
             {collision}",
        );
        PropExport {
            smd: smd_text(&self.tris),
            phys_smd,
            lod_smds,
            qc,
            materials,
            origin: self.origin,
            triangles: self.tris.len(),
            pieces: if options.collision { self.pieces.len() } else { 0 },
        }
    }
}

pub fn export_prop(
    doc: &Document,
    solids: &[Id],
    model_path: &str,
    surface_prop: &str,
    size: &dyn Fn(&str) -> (u32, u32),
) -> Option<PropExport> {
    Some(collect_prop(doc, solids, size)?.files(model_path, surface_prop, &[]))
}

impl Document {
    pub fn replace_with_prop(&mut self, ids: &[Id], model: &str, origin: DVec3) -> crate::EditResult<Id> {
        self.replace_with_prop_options(ids, model, origin, PropOptions::default())
    }

    pub fn replace_with_prop_options(
        &mut self,
        ids: &[Id],
        model: &str,
        origin: DVec3,
        options: PropOptions,
    ) -> crate::EditResult<Id> {
        let options = options.clamped();
        let model = model.trim();
        if model.is_empty() {
            return Err(crate::EditError::Invalid("empty model path".to_owned()));
        }
        let existing: Vec<Id> = ids.iter().copied().filter(|id| self.scene.solids.contains_key(id)).collect();
        if existing.is_empty() {
            return Err(crate::EditError::Invalid("no solid to convert".to_owned()));
        }
        let label = if options.physics {
            "Convert to physics prop"
        } else if options.static_prop {
            "Convert to static prop"
        } else {
            "Convert to dynamic prop"
        };
        self.tx(label, |doc| {
            doc.delete(&existing.iter().map(|id| ObjRef::Solid(*id)).collect::<Vec<_>>())?;
            let origin = format!("{} {} {}", origin.x.round(), origin.y.round(), origin.z.round());
            let model = format!("models/{}.mdl", model.trim_start_matches('/').trim_end_matches(".mdl"));
            doc.create_point_entity(
                if options.physics {
                    "prop_physics"
                } else if options.static_prop {
                    "prop_static"
                } else {
                    "prop_dynamic"
                },
                hp_vmf::parse_vec3(&origin).unwrap_or(DVec3::ZERO),
                &[
                    ("model".to_owned(), model),
                    ("solid".to_owned(), if options.collision { "6" } else { "0" }.to_owned()),
                ],
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Owner;
    use crate::tests::box_sides;

    #[test]
    fn a_box_exports_a_valid_smd_and_qc() {
        let mut doc = Document::new_empty();
        let b = doc
            .create_solid(
                box_sides(DVec3::new(-32.0, -32.0, 0.0), DVec3::new(32.0, 32.0, 64.0), "brick/wall"),
                Owner::World,
            )
            .expect("box");
        let export = export_prop(&doc, &[b], "hammerpro/test_0", "metal", &|_| (256, 256)).expect("export");
        assert_eq!(export.triangles, 12);
        assert_eq!(export.materials, vec!["brick/wall".to_string()]);
        assert_eq!(export.origin, DVec3::new(0.0, 0.0, 32.0));
        assert!(export.smd.starts_with("version 1\n"));
        assert!(export.smd.contains("brick/wall\n"), "the SMD names the material");
        assert_eq!(export.smd.matches("brick/wall\n").count(), 12);
        assert!(export.qc.contains("$staticprop"));
        assert!(export.qc.contains("$modelname \"hammerpro/test_0.mdl\""));
        assert!(export.qc.contains("$surfaceprop \"metal\""));
        assert!(export.qc.contains("test_0_ref.smd"));
        assert!(export.qc.contains("$collisionmodel \"test_0_phys.smd\""), "{}", export.qc);
        assert!(export.qc.contains("$concave") && export.qc.contains("$maxconvexpieces 1"));
        assert_eq!(export.phys_smd.matches("phys\n").count(), 12, "the box is one convex piece");
        let after = export.smd.split("triangles\n").nth(1).unwrap();
        let vline = after.lines().find(|l| l.starts_with("0 ")).unwrap();
        assert_eq!(vline.split_whitespace().count(), 9, "{vline}");
        assert!(
            after
                .lines()
                .any(|l| l.starts_with("0 32.0000 -32.0000 32.0000 ") && l.ends_with(" 1.00000 0.50000 1.50000")),
            "{after}"
        );
    }

    #[test]
    fn tool_faces_are_left_out_and_empty_is_none() {
        let mut doc = Document::new_empty();
        let b = doc
            .create_solid(box_sides(DVec3::ZERO, DVec3::splat(64.0), "tools/toolsnodraw"), Owner::World)
            .expect("box");
        assert!(export_prop(&doc, &[b], "m/x", "", &|_| (128, 128)).is_none(), "all nodraw: nothing to export");
        let drawn = doc
            .create_solid(box_sides(DVec3::new(64.0, 0.0, 0.0), DVec3::splat(128.0), "brick/wall"), Owner::World)
            .expect("drawn");
        let trigger = doc
            .create_solid(
                box_sides(DVec3::new(0.0, 0.0, 128.0), DVec3::splat(192.0), "tools/toolstrigger"),
                Owner::World,
            )
            .expect("trigger");
        let export = export_prop(&doc, &[b, drawn, trigger], "m/x", "", &|_| (128, 128)).expect("export");
        assert_eq!(export.triangles, 12, "only the drawn brush renders");
        assert_eq!(export.phys_smd.matches("phys\n").count(), 24, "two collision pieces");
        assert!(export.qc.contains("$maxconvexpieces 2"));
        let after = export.phys_smd.split("triangles\n").nth(1).expect("triangles");
        let normals: std::collections::HashSet<String> = after
            .lines()
            .filter(|l| l.starts_with("0 "))
            .map(|l| l.split_whitespace().skip(4).take(3).collect::<Vec<_>>().join(" "))
            .collect();
        assert_eq!(normals.len(), 2);
        assert!(export_prop(&doc, &[], "m/x", "", &|_| (128, 128)).is_none());
    }

    #[test]
    fn replace_with_prop_swaps_geometry() {
        let mut doc = Document::new_empty();
        let b = doc
            .create_solid(
                box_sides(DVec3::new(-16.0, -16.0, 0.0), DVec3::new(16.0, 16.0, 32.0), "brick/wall"),
                Owner::World,
            )
            .expect("box");
        let prop = doc.replace_with_prop(&[b], "hammerpro/test_0", DVec3::new(0.0, 0.0, 16.0)).expect("replace");
        assert_eq!(doc.undo_label(), Some("Convert to static prop"));
        assert!(doc.scene().solid(b).is_none(), "the box is gone");
        let e = &doc.scene().entity(prop).expect("prop").entity;
        assert_eq!(e.classname, "prop_static");
        assert_eq!(e.get("model"), Some("models/hammerpro/test_0.mdl"));
        assert_eq!(e.origin(), Some(DVec3::new(0.0, 0.0, 16.0)));
        assert!(doc.undo(), "one step back restores the box");
        assert!(doc.scene().solid(b).is_some());
    }

    #[test]
    fn custom_qc_positions_bounds_material_path_and_static_flag_are_exported() {
        let mut doc = Document::new_empty();
        let id = doc.create_solid(box_sides(DVec3::ZERO, DVec3::splat(64.0), "brick/wall"), Owner::World).unwrap();
        let geometry = collect_prop(&doc, &[id], &|_| (256, 256)).unwrap();
        let directives = PropDirectives {
            illumposition: Some(DVec3::new(1.25, -2.5, 30.0)),
            bbox: Some(Aabb::new(DVec3::new(-12.0, -24.0, -8.0), DVec3::new(16.0, 32.0, 48.0))),
            cdmaterials: "models/custom/map_props".to_owned(),
        };
        assert!(directives.validate().is_ok());
        let options = PropOptions { static_prop: false, ..Default::default() };
        let export = geometry.files_with_directives("custom/map_props/arch", "wood", &[], options, &directives);
        assert!(export.qc.contains("$modelname \"custom/map_props/arch.mdl\""));
        assert!(export.qc.contains("$illumposition -2.5 -1.25 30\n"));
        assert!(export.qc.contains("$bbox -12 -24 -8 16 32 48\n"));
        assert!(export.qc.contains("$cdmaterials \"models/custom/map_props\""));
        assert!(export.qc.contains("$surfaceprop \"wood\""));
        assert!(!export.qc.contains("$staticprop"));
        let entity = doc.replace_with_prop_options(&[id], "custom/map_props/arch", geometry.origin, options).unwrap();
        assert_eq!(doc.scene().entity(entity).unwrap().entity.classname, "prop_dynamic");
        assert!(doc.undo());
        assert!(doc.scene().solid(id).is_some());
        assert!(doc.scene().entity(entity).is_none());
        let physics = PropOptions { physics: true, collision: false, ..Default::default() }.clamped();
        assert!(physics.collision && !physics.static_prop);
    }

    #[test]
    fn invalid_manual_prop_directives_are_rejected() {
        for path in ["../elsewhere", "models/../outside", "a//b", "a\"\n$staticprop", "C:/absolute"] {
            assert!(PropDirectives { cdmaterials: path.into(), ..Default::default() }.validate().is_err(), "{path}");
        }
        assert!(
            PropDirectives { illumposition: Some(DVec3::splat(f64::NAN)), ..Default::default() }.validate().is_err()
        );
        assert!(
            PropDirectives { bbox: Some(Aabb { min: DVec3::ONE, max: DVec3::ZERO }), ..Default::default() }
                .validate()
                .is_err()
        );
    }

    fn hill(doc: &mut Document) -> Id {
        let b = doc
            .create_solid(box_sides(DVec3::ZERO, DVec3::new(256.0, 256.0, 64.0), "nature/blend"), Owner::World)
            .expect("box");
        let face = crate::FaceRef { solid: b, side: 0 };
        assert_eq!(doc.create_displacements(&[face], 3).expect("disp"), 1);
        let mut info = (**doc.scene().solid(b).unwrap().solid.sides[0].disp.as_ref().unwrap()).clone();
        let row = 9;
        for i in 0..row * row {
            let (c, r) = ((i % row) as f64, (i / row) as f64);
            info.distances[i] = if c < 4.0 && r < 4.0 { 8.0 * (4.0 - c) * (4.0 - r) / 4.0 } else { 0.0 };
            info.normals[i] = DVec3::Z;
            info.alphas[i] = c / 8.0 * 255.0;
        }
        doc.set_disp_data("hill", vec![(face, info)]).expect("set");
        b
    }

    #[test]
    fn a_displacement_renders_smooth_and_collides() {
        let mut doc = Document::new_empty();
        let b = hill(&mut doc);
        let g = collect_prop(&doc, &[b], &|_| (512, 512)).expect("geometry");
        assert_eq!(g.tris.len(), 128);
        assert_eq!(g.disps.len(), 1);
        assert_eq!(g.disps[0].row_len, 9);
        assert!(g.tris.iter().all(|t| t.disp.is_some_and(|d| d.face == 0)));
        assert!(g.tris.iter().any(|t| t.normal[0] != t.normal[1]));
        for t in &g.tris {
            let d = t.disp.unwrap();
            for k in 0..3 {
                assert!((d.alpha[k] - d.grid[k][0]).abs() < 1e-6, "{:?}", d);
            }
        }
        assert!(g.pieces.len() > 4 && g.pieces.len() < 64, "{} pieces", g.pieces.len());
        let mesh = doc.scene().solid(b).unwrap().disp[0].clone().unwrap();
        for p in &mesh.positions {
            let local = *p - g.origin;
            assert!(
                g.pieces.iter().any(|piece| piece.iter().flatten().any(|q| (*q - local).length() < 1e-6)),
                "{p} is on no slab"
            );
        }
        for piece in &g.pieces {
            let zs: Vec<f64> = piece.iter().flatten().map(|q| q.z).collect();
            let (lo, hi) = zs.iter().fold((f64::MAX, f64::MIN), |(a, b), z| (a.min(*z), b.max(*z)));
            assert!(hi - lo >= DISP_THICKNESS - 1e-6, "slabs are thick");
        }
        let export = g.files("hammerpro/hill", "grass", &[]);
        assert!(export.qc.contains(&format!("$maxconvexpieces {}", g.pieces.len())));
        assert_eq!(export.pieces, g.pieces.len());
        let mut info = (**doc.scene().solid(b).unwrap().solid.sides[0].disp.as_ref().unwrap()).clone();
        info.flags |= DISP_FLAG_NO_PHYSICS;
        doc.set_disp_data("flags", vec![(crate::FaceRef { solid: b, side: 0 }, info)]).expect("set");
        assert!(collect_prop(&doc, &[b], &|_| (512, 512)).unwrap().pieces.is_empty());
    }

    #[test]
    fn levels_of_detail_become_lod_blocks() {
        let mut doc = Document::new_empty();
        let b = hill(&mut doc);
        let g = collect_prop(&doc, &[b], &|_| (512, 512)).expect("geometry");
        let lods = [
            PropLod { switch: 40.0, tris: g.tris[..64].to_vec() },
            PropLod { switch: 80.0, tris: g.tris[..32].to_vec() },
            PropLod { switch: 160.0, tris: g.tris[..16].to_vec() },
        ];
        let export = g.files("hammerpro/hill", "grass", &lods);
        assert_eq!(export.lod_smds.len(), 3);
        assert_eq!(export.lod_smds[1].matches("nature/blend\n").count(), 32);
        assert!(
            export.qc.contains("$lod 40\n{\n\treplacemodel \"hill_ref.smd\" \"hill_lod1.smd\"\n}\n"),
            "{}",
            export.qc
        );
        assert!(export.qc.contains("\"hill_lod3.smd\""));
        assert!(export.qc.find("$lod").unwrap() < export.qc.find("$sequence").unwrap());
    }
}
