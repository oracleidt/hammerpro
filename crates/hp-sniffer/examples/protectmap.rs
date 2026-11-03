use hp_bsp::{Bsp, Lock, ProtectOptions, lock_hash, lump, protect};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = &args[1];
    let output = &args[2];
    let mut opts = ProtectOptions {
        watermark: String::new(),
        strip_original_faces: true,
        randomize_materials: true,
        salt: "salt".into(),
        ..Default::default()
    };
    let mut full = Bsp::read(input).expect("read");
    let mut add: Vec<(String, String)> = Vec::new();
    let mut custom_mat: Option<String> = None;
    let mut custom_model: Option<String> = None;
    let mut off = false;
    for a in &args[3..] {
        let (k, v) = a.split_once('=').unwrap_or((a.as_str(), ""));
        match k {
            "watermark" => opts.watermark = v.to_owned(),
            "salt" => opts.salt = v.to_owned(),
            "lock" => opts.lock = Some(Lock { hash: lock_hash(v), block: false }),
            "randomize" => opts.randomize_materials = v == "1",
            "faces" => opts.strip_original_faces = v == "1",
            "logic" => opts.strip_logic = v == "1",
            "conns" => opts.strip_connections = v == "1",
            "add" => {
                let (n, p) = v.split_once('=').expect("add=name=path");
                add.push((n.to_owned(), p.to_owned()));
            }
            "custommat" => custom_mat = Some(v.to_owned()),
            "custommodel" => custom_model = Some(v.to_owned()),
            "noprotect" => off = true,
            _ => panic!("unknown {k}"),
        }
    }
    if !add.is_empty() || custom_mat.is_some() || custom_model.is_some() {
        let mut edit = Bsp::from_vec(full.bytes().to_vec()).unwrap().edit();
        let mut pak = full.pakfile().unwrap().to_writer().unwrap();
        for (n, p) in &add {
            pak.add(n, std::fs::read(p).unwrap()).unwrap();
        }
        if let Some(spec) = &custom_mat {
            let (old, new) = spec.split_once('>').expect("old>new");
            let data = full.lump(lump::TEXDATA_STRING_DATA).unwrap();
            let table = full.lump(lump::TEXDATA_STRING_TABLE).unwrap();
            let mut nd: Vec<u8> = Vec::new();
            let mut nt: Vec<u8> = Vec::new();
            let mut seen: std::collections::HashMap<String, u32> = Default::default();
            for c in table.chunks_exact(4) {
                let o = u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize;
                let end = data[o..].iter().position(|&b| b == 0).unwrap();
                let mut name = String::from_utf8_lossy(&data[o..o + end]).into_owned();
                if name.eq_ignore_ascii_case(old) {
                    name = new.to_owned();
                }
                let at = *seen.entry(name.clone()).or_insert_with(|| {
                    let at = nd.len() as u32;
                    nd.extend_from_slice(name.as_bytes());
                    nd.push(0);
                    at
                });
                nt.extend_from_slice(&at.to_le_bytes());
            }
            edit.replace_lump(lump::TEXDATA_STRING_DATA, nd).unwrap();
            edit.replace_lump(lump::TEXDATA_STRING_TABLE, nt).unwrap();
        }
        if let Some(spec) = &custom_model {
            let (old, new) = spec.split_once('>').expect("old>new");
            let mut gl = full.game_lumps().unwrap();
            let g = gl.get(hp_bsp::gamelump::id::STATIC_PROPS).unwrap().clone();
            let data = g.data().unwrap().into_owned();
            let (mut lumpv, fmt) = hp_bsp::staticprops::StaticPropLump::parse(&data, g.version, None).unwrap();
            for n in lumpv.names.iter_mut() {
                if n.eq_ignore_ascii_case(old) {
                    *n = new.to_owned();
                }
            }
            gl.set(g.id, g.version, lumpv.to_bytes(fmt.unwrap())).unwrap();
            edit.set_game_lumps(gl);
        }
        edit.set_pakfile(pak.finish(hp_bsp::PakOptions::default()).unwrap()).unwrap();
        full = Bsp::from_vec(edit.to_vec().unwrap()).unwrap();
    }
    if off {
        std::fs::write(output, full.bytes()).unwrap();
        return;
    }
    let (bytes, r) = protect(&full, &opts).expect("protect");
    std::fs::write(output, bytes).unwrap();
    println!("{r:?}");
}
