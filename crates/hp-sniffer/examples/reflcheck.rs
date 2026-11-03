fn main() {
    let args: Vec<String> = std::env::args().collect();
    let fs = hp_sniffer::assets::mount_for(std::path::Path::new(&args[1])).expect("mount");
    let bsp = hp_bsp::Bsp::open(&args[2]).expect("bsp");
    let r = hp_sniffer::reflfix::apply(&bsp, &fs);
    println!("overridden {}", r.overridden);
    if let Some(b) = r.bsp {
        std::fs::write(&args[3], b.bytes()).expect("write");
    }
}
