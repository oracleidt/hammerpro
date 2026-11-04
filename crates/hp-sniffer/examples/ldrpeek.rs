use hp_sniffer::procmem::GameProcess;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pid: u32 = args[1].parse().expect("pid");
    let proc0 = GameProcess::open(pid).expect("open");
    let base = if args[2] == "auto" {
        let opts = hp_sniffer::scan::ScanOptions {
            writable_only: true,
            min_region: 1 << 16,
            ..hp_sniffer::scan::ScanOptions::default()
        };
        let ents: Vec<(u64, usize)> =
            hp_sniffer::scan::find_entity_lumps(&proc0, &opts).into_iter().map(|h| (h.addr, h.text.len())).collect();
        let cs = hp_sniffer::cmstruct::find_by_entities(&proc0, &ents, &[8]).expect("collision structure");
        let hub = hp_sniffer::hub::find_hub(&proc0, cs.planes.addr, cs.planes.count as usize, &[8]).expect("hub");
        println!("hub base {:x}", hub.base);
        hub.base
    } else {
        u64::from_str_radix(&args[2], 16).expect("base")
    };
    let hdr = args.get(3).is_none_or(|v| v != "ldr");
    let proc = GameProcess::open(pid).expect("open");
    let sum = |b: &[u8]| b.iter().fold(0u64, |a, v| a.wrapping_mul(1099511628211).wrapping_add(u64::from(*v) + 1));
    match hp_sniffer::hubdata::capture_lighting(&proc, base, 8, hdr) {
        Some(d) => {
            println!("lightdata {:x} faces {}", d.lightdata, d.faces.len());
            for p in &d.pieces {
                println!("lump {} {} bytes hash {:x}", p.index, p.bytes.len(), sum(&p.bytes));
                if let Ok(dir) = std::env::var("PEEK_SAVE") {
                    std::fs::write(format!("{dir}/peek_{}.bin", p.index), &p.bytes).ok();
                }
            }
        }
        None => println!("no hub"),
    }
}
