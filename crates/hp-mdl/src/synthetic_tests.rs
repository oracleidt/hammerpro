use crate::mdl::tests::Buf;
use crate::{BodyGroupSelect, FLAG_STATIC_PROP, LoadOptions, MdlError, Model, Pose, load, read_header};

const CHECKSUM: i32 = 0x1234_5678;

#[derive(Clone, Copy)]
struct Spec {
    version: i32,
    extended_vtx: bool,
    tristrip: bool,
    static_prop: bool,
    vtx_body_parts: i32,
    vvd_checksum: i32,
    vtx_checksum: i32,
}

impl Default for Spec {
    fn default() -> Self {
        Spec {
            version: 48,
            extended_vtx: false,
            tristrip: false,
            static_prop: true,
            vtx_body_parts: 1,
            vvd_checksum: CHECKSUM,
            vtx_checksum: CHECKSUM,
        }
    }
}

fn build(spec: Spec) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    (build_mdl(spec), build_vvd(spec), build_vtx(spec))
}

fn build_mdl(spec: Spec) -> Vec<u8> {
    let mut b = Buf::default();
    b.at(0, b"IDST");
    b.i32(4, spec.version);
    b.i32(8, CHECKSUM);
    b.str(12, "test/quad.mdl");
    b.f32s(104, &[0.0, 0.0, -1.0, 1.0, 1.0, 1.0]);
    b.i32(152, if spec.static_prop { FLAG_STATIC_PROP as i32 } else { 0 });
    b.i32(156, 1);
    b.i32(160, 408);
    b.i32(408, 2000 - 408);
    b.i32(412, -1);
    b.f32s(408 + 44, &[0.0, 0.0, 0.0, 1.0]);
    b.f32s(408 + 96, &[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    b.str(2000, "root");
    b.i32(204, 2);
    b.i32(208, 624);
    b.i32(624, 2010 - 624);
    b.i32(688, 2020 - 688);
    b.str(2010, "Wood");
    b.str(2020, "Metal");
    b.i32(212, 2);
    b.i32(216, 752);
    b.i32(752, 2030);
    b.i32(756, 2050);
    b.str(2030, "models\\Test\\");
    b.str(2050, "/Models/Other");
    b.i32(220, 2);
    b.i32(224, 2);
    b.i32(228, 760);
    for (i, v) in [0u16, 1, 1, 0].iter().enumerate() {
        b.u16(760 + i * 2, *v);
    }
    b.i32(232, 1);
    b.i32(236, 800);
    b.i32(800, 2070 - 800);
    b.i32(804, 2);
    b.i32(808, 1);
    b.i32(812, 816 - 800);
    b.str(2070, "body");
    b.str(816, "quad");
    b.i32(816 + 72, 1);
    b.i32(816 + 76, 1120 - 816);
    b.i32(816 + 80, 4);
    b.i32(816 + 84, 0);
    b.str(964, "blank");
    b.i32(964 + 84, 4 * 48);
    b.i32(1120, 0);
    b.i32(1128, 4);
    b.i32(1132, 0);
    for l in 0..8 {
        b.i32(1120 + 52 + l * 4, 4);
    }
    b.str(2080, "metal");
    b.i32(308, 2080);
    b.at(2100, &[0]);
    b.0
}

fn build_vvd(spec: Spec) -> Vec<u8> {
    let mut b = Buf::default();
    b.at(0, b"IDSV");
    b.i32(4, 4);
    b.i32(8, spec.vvd_checksum);
    b.i32(12, 1);
    for l in 0..8 {
        b.i32(16 + l * 4, 4);
    }
    b.i32(48, 0);
    b.i32(52, 64);
    b.i32(56, 64);
    b.i32(60, 64 + 4 * 48);
    let corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    for (i, c) in corners.iter().enumerate() {
        let v = 64 + i * 48;
        b.f32s(v, &[1.0, 0.0, 0.0]);
        b.at(v + 12, &[0, 0, 0, 1]);
        b.f32s(v + 16, &[c[0], c[1], 0.0, 0.0, 0.0, 1.0, c[0], 1.0 - c[1]]);
        b.f32s(64 + 4 * 48 + i * 16, &[1.0, 0.0, 0.0, 1.0]);
    }
    b.0
}

fn build_vtx(spec: Spec) -> Vec<u8> {
    let (group_size, strip_size) = if spec.extended_vtx { (33, 35) } else { (25, 27) };
    let mut b = Buf::default();
    b.i32(0, 7);
    b.i32(16, spec.vtx_checksum);
    b.i32(20, 1);
    b.i32(24, 600);
    b.i32(28, spec.vtx_body_parts);
    b.i32(32, 36);
    b.i32(36, 2);
    b.i32(40, 8);
    b.i32(44, 1);
    b.i32(48, 16);
    b.i32(52, 1);
    b.i32(56, 20);
    b.i32(60, 1);
    b.i32(64, 24);
    b.i32(84, 1);
    b.i32(88, 16);
    let g = 100;
    let verts = 200;
    let indices = 300;
    let strips = 400;
    b.i32(g, 4);
    b.i32(g + 4, (verts - g) as i32);
    let index_list: &[u16] = if spec.tristrip { &[1, 0, 2, 3] } else { &[0, 2, 1, 0, 3, 2] };
    b.i32(g + 8, index_list.len() as i32);
    b.i32(g + 12, (indices - g) as i32);
    b.i32(g + 16, 1);
    b.i32(g + 20, (strips - g) as i32);
    b.at(g + 24, &[0]);
    if group_size == 33 {
        b.i32(g + 25, 0);
        b.i32(g + 29, 0);
    }
    for i in 0..4u16 {
        let v = verts + usize::from(i) * 9;
        b.at(v, &[0, 1, 2, 1]);
        b.u16(v + 4, i);
        b.at(v + 6, &[0, 0, 0]);
    }
    for (i, ix) in index_list.iter().enumerate() {
        b.u16(indices + i * 2, *ix);
    }
    b.i32(strips, index_list.len() as i32);
    b.i32(strips + 4, 0);
    b.i32(strips + 8, 4);
    b.i32(strips + 12, 0);
    b.u16(strips + 16, 1);
    b.at(strips + 18, &[if spec.tristrip { 0x02 } else { 0x01 }]);
    if strip_size == 35 {
        b.i32(strips + 27, 0);
        b.i32(strips + 31, 0);
    }
    b.i32(600, 0);
    b.i32(604, 0);
    b.0
}

fn load_spec(spec: Spec, opts: &LoadOptions) -> Result<Model, MdlError> {
    let (mdl, vvd, vtx) = build(spec);
    load(&mdl, &vvd, &vtx, opts)
}

fn triangles(model: &Model) -> Vec<[[i32; 3]; 3]> {
    let mut out = Vec::new();
    for mesh in &model.meshes {
        for t in mesh.indices.chunks_exact(3) {
            let p = |i: u32| mesh.vertices[i as usize].pos.map(|c| c.round() as i32);
            let tri = [p(t[0]), p(t[1]), p(t[2])];
            let rotations = [tri, [tri[1], tri[2], tri[0]], [tri[2], tri[0], tri[1]]];
            out.push(rotations.into_iter().min().unwrap_or(tri));
        }
    }
    out.sort();
    out
}

#[test]
fn loads_synthetic_quad() {
    let model = load_spec(Spec::default(), &LoadOptions::default()).expect("load");
    assert_eq!(model.name, "test/quad.mdl");
    assert_eq!(model.version, 48);
    assert!(model.is_static_prop);
    assert_eq!(model.surface_prop, "metal");
    assert_eq!(model.bone_names, vec!["root".to_string()]);
    assert_eq!(model.meshes.len(), 1);
    assert_eq!(model.triangle_count(), 2);
    assert_eq!(model.vertex_count(), 4);
    assert_eq!(model.bounds, ([0.0, 0.0, 0.0], [1.0, 1.0, 0.0]));
    assert_eq!(model.body_parts.len(), 1);
    assert_eq!(model.body_parts[0].models.len(), 2);
    let mesh = &model.meshes[0];
    for t in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| mesh.vertices[i as usize].pos);
        let e1 = [b[0] - a[0], b[1] - a[1]];
        let e2 = [c[0] - a[0], c[1] - a[1]];
        assert!(e1[0] * e2[1] - e1[1] * e2[0] > 0.0, "triangle must be counter clockwise");
    }
    assert_eq!(mesh.material_index, 0);
    assert_eq!(model.material_candidates(mesh.skin_ref, 0), vec!["models/test/wood", "models/other/wood"]);
    assert_eq!(model.material_candidates(mesh.skin_ref, 1), vec!["models/test/metal", "models/other/metal"]);
    assert_eq!(model.material_candidates(mesh.skin_ref, 99)[0], "models/test/wood");
    assert!(model.material_candidates(42, 0).is_empty());
    assert_eq!(model.skin_count(), 2);
    assert_eq!(mesh.color_index, [0, 1, 2, 3]);

    let header = read_header(&build_mdl(Spec::default())).expect("header");
    assert_eq!((header.num_textures, header.num_body_parts, header.num_skin_families), (2, 1, 2));
}

#[test]
fn layouts_strips_and_versions_agree() {
    let reference = triangles(&load_spec(Spec::default(), &LoadOptions::default()).expect("standard"));
    for spec in [
        Spec { extended_vtx: true, version: 49, ..Spec::default() },
        Spec { extended_vtx: true, ..Spec::default() },
        Spec { version: 49, ..Spec::default() },
        Spec { version: 44, ..Spec::default() },
        Spec { tristrip: true, ..Spec::default() },
        Spec { static_prop: false, ..Spec::default() },
    ] {
        let model = load_spec(spec, &LoadOptions::default()).expect("variant loads");
        assert_eq!(triangles(&model), reference);
    }
}

#[test]
fn body_groups_lods_and_options() {
    let spec = Spec::default();
    let blank = load_spec(spec, &LoadOptions { body_groups: BodyGroupSelect::Body(1), ..LoadOptions::default() })
        .expect("blank choice");
    assert!(blank.meshes.is_empty());
    assert_eq!(blank.bounds, ([0.0, 0.0, -1.0], [1.0, 1.0, 1.0]), "falls back to the hull");
    let all =
        load_spec(spec, &LoadOptions { body_groups: BodyGroupSelect::All, ..LoadOptions::default() }).expect("all");
    assert_eq!(all.triangle_count(), 2);
    let chosen =
        load_spec(spec, &LoadOptions { body_groups: BodyGroupSelect::Choices(vec![7]), ..LoadOptions::default() })
            .expect("out of range choice");
    assert_eq!(chosen.triangle_count(), 2);
    let opts = LoadOptions { lod: 5, tangents: true, bone_weights: true, pose: Pose::Rest, ..LoadOptions::default() };
    let model = load_spec(spec, &opts).expect("lod clamps");
    assert_eq!((model.lod, model.num_lods), (0, 1));
    let mesh = &model.meshes[0];
    assert_eq!(mesh.tangents.len(), 4);
    assert_eq!(mesh.bone_weights.len(), 4);
    assert_eq!(mesh.bone_weights[0].count, 1);
}

#[test]
fn reports_mismatches() {
    let err = load_spec(Spec { vvd_checksum: 1, ..Spec::default() }, &LoadOptions::default());
    assert!(matches!(err, Err(MdlError::ChecksumMismatch { file: crate::FileKind::Vvd, .. })));
    let err = load_spec(Spec { vtx_checksum: 1, ..Spec::default() }, &LoadOptions::default());
    assert!(matches!(err, Err(MdlError::ChecksumMismatch { file: crate::FileKind::Vtx, .. })));
    let err = load_spec(Spec { vtx_body_parts: 0, ..Spec::default() }, &LoadOptions::default());
    assert!(matches!(err, Err(MdlError::StructureMismatch { .. })));
    let (mdl, vvd, vtx) = build(Spec::default());
    assert!(matches!(load(&vvd, &mdl, &vtx, &LoadOptions::default()), Err(MdlError::BadMagic { .. })));
}

#[test]
fn load_from_resolves_companions() {
    let (mdl, vvd, vtx) = build(Spec::default());
    let read = |path: &str| match path {
        "models/quad.mdl" => Some(mdl.clone()),
        "models/quad.vvd" => Some(vvd.clone()),
        "models/quad.sw.vtx" => Some(vtx.clone()),
        _ => None,
    };
    let model = crate::load_from("models/quad.mdl", read, &LoadOptions::default()).expect("load_from");
    assert_eq!(model.triangle_count(), 2);
    let err = crate::load_from("models/none.mdl", read, &LoadOptions::default());
    assert!(matches!(err, Err(MdlError::MissingFile { file: crate::FileKind::Mdl, .. })));
    let no_vtx = |path: &str| {
        if path.ends_with(".vtx") { None } else { read(path) }
    };
    let err = crate::load_from("models/quad.mdl", no_vtx, &LoadOptions::default());
    assert!(matches!(err, Err(MdlError::MissingFile { file: crate::FileKind::Vtx, .. })));
    let mut reads = 0;
    let files = crate::read_files("models/quad.mdl", |p| {
        reads += 1;
        read(p)
    })
    .expect("read once");
    assert_eq!(reads, 5, "the MDL, the VVD, then the VTX names up to the first one that exists");
    assert_eq!((&files.mdl, &files.vvd, &files.vtx), (&mdl, &vvd, &vtx));
    let all = LoadOptions { body_groups: BodyGroupSelect::All, ..LoadOptions::default() };
    assert_eq!(files.load(&all).unwrap(), load(&mdl, &vvd, &vtx, &all).unwrap(), "loads again without reading");
}

pub(crate) struct Rng(pub u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    pub(crate) fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

pub(crate) fn torture(files: [&[u8]; 3], seed: u64, rounds: usize) -> (usize, usize) {
    let mut rng = Rng(seed | 1);
    let (mut ok, mut err) = (0, 0);
    let opts = [
        LoadOptions::default(),
        LoadOptions {
            body_groups: BodyGroupSelect::All,
            lod: 3,
            tangents: true,
            bone_weights: true,
            ..LoadOptions::default()
        },
    ];
    let mut record = |r: Result<Model, MdlError>| if r.is_ok() { ok += 1 } else { err += 1 };
    for (which, file) in files.iter().enumerate() {
        let step = (file.len() / 64).max(1);
        for len in (0..file.len()).step_by(step) {
            let mut set = files.map(<[u8]>::to_vec);
            set[which].truncate(len);
            record(load(&set[0], &set[1], &set[2], &opts[len % 2]));
            let _ = read_header(&set[0]);
        }
    }
    for round in 0..rounds {
        let mut set = files.map(<[u8]>::to_vec);
        for _ in 0..1 + rng.below(8) {
            let which = rng.below(3);
            let file = &mut set[which];
            if file.is_empty() {
                continue;
            }
            let at = rng.below(file.len());
            match rng.below(4) {
                0 => file[at] ^= 1 << rng.below(8),
                1 => file[at] = rng.next() as u8,
                2 => {
                    let v = [0i32, -1, i32::MAX, i32::MIN, 1 << 20, -(at as i32)][rng.below(6)];
                    let end = (at + 4).min(file.len());
                    let bytes = v.to_le_bytes();
                    file[at..end].copy_from_slice(&bytes[..end - at]);
                }
                _ => {
                    let end = (at + 4).min(file.len());
                    let bytes = (rng.next() as u32 % 4096).to_le_bytes();
                    file[at..end].copy_from_slice(&bytes[..end - at]);
                }
            }
        }
        record(load(&set[0], &set[1], &set[2], &opts[round % 2]));
        let _ = read_header(&set[0]);
    }
    (ok, err)
}

#[test]
fn corrupted_synthetic_models_never_panic() {
    for spec in [Spec::default(), Spec { extended_vtx: true, version: 49, static_prop: false, ..Spec::default() }] {
        let (mdl, vvd, vtx) = build(spec);
        let (ok, err) = torture([&mdl, &vvd, &vtx], 0x9e37_79b9_7f4a_7c15, 20_000);
        assert!(ok > 0 && err > 0, "ok {ok} err {err}");
    }
}

#[test]
fn self_authored_native_studio_model_preserves_all_named_hitbox_sets() {
    let model = load(
        include_bytes!("../tests/data/hitboxes.mdl"),
        include_bytes!("../tests/data/hitboxes.vvd"),
        include_bytes!("../tests/data/hitboxes.dx90.vtx"),
        &LoadOptions::default(),
    )
    .unwrap();
    assert_eq!(model.bones.len(), 2);
    assert_eq!(model.hitbox_sets.len(), 2);
    assert_eq!(model.hitbox_sets[0].name, "default");
    assert_eq!(model.hitbox_sets[0].hitboxes, model.hitboxes);
    let effects = &model.hitbox_sets[1];
    assert_eq!(effects.name, "effects");
    assert_eq!(effects.hitboxes.len(), 2);
    assert_eq!(effects.hitboxes[0].bone, 0);
    assert_eq!(effects.hitboxes[0].min, [4.0, 14.0, 24.0]);
    assert_eq!(effects.hitboxes[0].max, [16.0, 26.0, 36.0]);
    assert_eq!(effects.hitboxes[1].bone, 1);
    assert_eq!(effects.hitboxes[1].min, [96.0, -4.0, -4.0]);
    assert_eq!(effects.hitboxes[1].max, [104.0, 4.0, 4.0]);
}

#[test]
fn named_hitbox_bone_matrices_match_the_native_studio_pose() {
    let model = load(
        include_bytes!("../tests/data/hitboxes.mdl"),
        include_bytes!("../tests/data/hitboxes.vvd"),
        include_bytes!("../tests/data/hitboxes.dx90.vtx"),
        &LoadOptions::default(),
    )
    .unwrap();
    let native: Vec<crate::SkinMatrix> =
        serde_json::from_str(include_str!("../tests/data/hitbox_native_bones.json")).unwrap();
    assert_eq!(model.bone_matrices.len(), native.len());
    for (bone, (actual, expected)) in model.bone_matrices.iter().zip(native).enumerate() {
        for row in 0..3 {
            for column in 0..4 {
                assert!(
                    (actual[row][column] - expected[row][column]).abs() < 0.0001,
                    "bone{bone} [{row}][{column}]: {} vs {}",
                    actual[row][column],
                    expected[row][column]
                );
            }
        }
    }
}
