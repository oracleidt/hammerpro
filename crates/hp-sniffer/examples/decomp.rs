fn main() {
    let a: Vec<String> = std::env::args().collect();
    let d = hp_decompile::decompile_file(
        std::path::Path::new(&a[1]),
        &hp_decompile::Options {
            map_name: String::new(),
            known_outputs: None,
            toggles: hp_sniffer::SnifferSettings::default().decompile.toggles(),
        },
    )
    .expect("decompile");
    std::fs::write(&a[2], hp_vmf::write(&d.map)).unwrap();
}
