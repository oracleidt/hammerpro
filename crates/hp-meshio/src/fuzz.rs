#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use glam::{DVec2, DVec3};

use crate::export::{ExportFormat, export};
use crate::{Material, MeshObject, Model, Triangle, UpAxis};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn sample_model() -> Model {
    let mut triangles = Vec::new();
    for i in 0..12 {
        let o = DVec3::new(f64::from(i) * 8.0, 0.0, 0.0);
        triangles.push(Triangle {
            pos: [o, o + DVec3::X * 8.0, o + DVec3::Y * 8.0],
            normal: [DVec3::Z; 3],
            uv: [DVec2::ZERO, DVec2::X, DVec2::Y],
            material: (i % 2) as usize,
        });
    }
    Model {
        triangles,
        materials: vec![
            Material { name: "a".into(), color: [1.0, 0.5, 0.25, 1.0], ..Material::default() },
            Material { name: "b".into(), ..Material::default() },
        ],
        up: UpAxis::Z,
        meters_per_unit: 1.0 / crate::UNITS_PER_METER,
        objects: vec![MeshObject { name: "o".into(), extras: None, first: 0, count: 12 }],
    }
}

fn mutate(rng: &mut Rng, data: &[u8], text: bool) -> Vec<u8> {
    let mut out = data.to_vec();
    match rng.below(6) {
        0 => out.truncate(rng.below(out.len() + 1)),
        1 => {
            for _ in 0..1 + rng.below(8) {
                if !out.is_empty() {
                    let i = rng.below(out.len());
                    out[i] = (rng.next() & 0xff) as u8;
                }
            }
        }
        2 if text => {
            let tokens: &[&[u8]] = &[
                b"-1",
                b"0",
                b"99999999",
                b"-99999999",
                b"nan",
                b"inf",
                b"1e308",
                b"f 1 2",
                b"f -5 -6 -7",
                b"//",
                b"\n",
                b" ",
                b"v",
                b"vt",
                b"vn",
                b"usemtl",
                b"mtllib ../../../../x.mtl",
            ];
            let i = rng.below(out.len() + 1);
            let t = tokens[rng.below(tokens.len())];
            out.splice(i..i, t.iter().copied());
        }
        3 => {
            let i = rng.below(out.len() + 1);
            let n = rng.below(16);
            let j = (i + n).min(out.len());
            out.drain(i..j);
        }
        4 => {
            if out.len() >= 4 {
                let i = rng.below(out.len() - 3);
                let v = [0xffu8, 0xff, 0xff, 0x7f];
                out[i..i + 4].copy_from_slice(&v);
            }
        }
        _ => {
            let i = rng.below(out.len() + 1);
            let tail = out[i..].to_vec();
            out.extend_from_slice(&tail);
        }
    }
    out
}

fn check(model: &Model) {
    for t in &model.triangles {
        assert!(t.material < model.materials.len().max(1), "material index in range");
    }
    for o in &model.objects {
        let _ = model.object_triangles(o);
    }
}

fn read_all(format: ExportFormat, bytes: &[u8]) {
    let dir = Path::new("E:/hp-meshio-fuzz-missing-dir");
    let result = match format {
        ExportFormat::Obj => crate::obj::read(bytes, dir),
        ExportFormat::Gltf => crate::gltf::read(bytes, dir),
        ExportFormat::Fbx => crate::fbx::read(bytes, dir),
    };
    if let Ok(mut model) = result {
        check(&model);
        model.merge_materials();
        check(&model);
    }
}

#[test]
fn malformed_files_never_panic() {
    let model = sample_model();
    for format in [ExportFormat::Obj, ExportFormat::Gltf, ExportFormat::Fbx] {
        let files = export(&model, format, "fuzz");
        let main = files.iter().find(|f| f.name.ends_with(format.ext())).expect("main file");
        read_all(format, &main.data);
        let mut rng = Rng(0x5eed_u64 ^ format.ext().len() as u64);
        for round in 0..1500 {
            let bytes = mutate(&mut rng, &main.data, format == ExportFormat::Obj);
            let r = std::panic::catch_unwind(|| read_all(format, &bytes));
            assert!(r.is_ok(), "{format:?} round {round} panicked on {} bytes", bytes.len());
        }
    }
}

#[test]
fn hostile_obj_lines_are_refused_or_skipped() {
    for text in [
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 4\n",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf -4 -2 -1\n",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1/9/9 2/9/9 3/9/9\n",
        "v nan 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        "v 1e308 1e308 1e308\nv -1e308 0 0\nv 0 1 0\nf 1 2 3\n",
        "f 1 2 3\n",
        "v 0 0 0\nf 1 1 1\n",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2\n",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 0 1 2\n",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 18446744073709551615 2 3\n",
    ] {
        let r = std::panic::catch_unwind(|| read_all(ExportFormat::Obj, text.as_bytes()));
        assert!(r.is_ok(), "panicked on {text:?}");
    }
}
