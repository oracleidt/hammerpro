use hp_bsp::{Bsp, LayoutOptions, Lock, ProtectOptions, ReorderOptions, lock_hash, protect};

fn base(seed: &str) -> ProtectOptions {
    ProtectOptions {
        watermark: "PROTTEST".into(),
        strip_original_faces: true,
        randomize_materials: true,
        salt: "abc".into(),
        lock: Some(Lock { hash: lock_hash("secret"), block: false }),
        layout_seed: seed.into(),
        ..Default::default()
    }
}

#[test]
#[ignore]
fn write_shuffle_samples() {
    let src = std::env::var("HP_SAMPLE_SRC").unwrap_or_else(|_| r"E:\hp-scratch\agent-protect\work\hpt_src.bsp".into());
    let out = std::env::var("HP_SAMPLE_OUT").unwrap_or_else(|_| r"E:\hp-scratch\agent-protect\work".into());
    let full = Bsp::read(&src).expect("source map");
    let on = LayoutOptions::strongest();
    let none = LayoutOptions::default();
    let variants: Vec<(&str, LayoutOptions, ReorderOptions)> = vec![
        ("hpt_shuffle", on.clone(), ReorderOptions { tables: true, world: true }),
        ("hpt_shuffle_lumps", LayoutOptions { shuffle_lumps: true, ..none.clone() }, ReorderOptions::default()),
        (
            "hpt_shuffle_gaps",
            LayoutOptions { shuffle_lumps: true, junk_gaps: true, ..none.clone() },
            ReorderOptions::default(),
        ),
        ("hpt_shuffle_decoys", LayoutOptions { decoys: true, ..none.clone() }, ReorderOptions::default()),
        ("hpt_shuffle_header", LayoutOptions { scramble_header: true, ..none.clone() }, ReorderOptions::default()),
        (
            "hpt_shuffle_gamelumps",
            LayoutOptions { shuffle_game_lumps: true, ..none.clone() },
            ReorderOptions::default(),
        ),
        ("hpt_shuffle_pak", LayoutOptions { random_pak: true, ..none.clone() }, ReorderOptions::default()),
        ("hpt_shuffle_tables", none.clone(), ReorderOptions { tables: true, world: false }),
        ("hpt_shuffle_world", none.clone(), ReorderOptions { tables: false, world: true }),
        ("hpt_shuffle_layout", on.clone(), ReorderOptions::default()),
    ];
    for (name, layout, reorder) in variants {
        let mut opts = base(&format!("seed-{name}"));
        opts.layout = layout;
        opts.reorder = reorder;
        let (bytes, report) = protect(&full, &opts).expect("protect");
        std::fs::write(format!(r"{out}\{name}.bsp"), &bytes).expect("write");
        eprintln!("{name}: {} bytes, {report:?}", bytes.len());
    }
}
