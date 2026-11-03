use hp_sniffer::procmem::GameProcess;
use hp_sniffer::scan::ScanOptions;
use std::sync::atomic::AtomicBool;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pid: u32 = args[1].parse().expect("pid");
    let map = args.get(2).cloned();
    let proc = GameProcess::open(pid).expect("open");
    let install = hp_sniffer::launch::detect_install("").expect("install");
    let opts = ScanOptions::default();
    let world = hp_sniffer::memmap::find_all_mode(&proc, &opts, "x", true).expect("world");
    let hub = world.hub.as_ref().expect("hub");
    let sum = |b: &[u8]| b.iter().fold(0u64, |a, v| a.wrapping_mul(1099511628211).wrapping_add(u64::from(*v) + 1));
    println!("hub base {:x} ptr {} faces {}", hub.base, hub.ptr, hub.faces.len());
    for i in [53usize, 8, 55, 56, 54, 15] {
        if let Some(p) = hub.piece(i) {
            println!("hdr pass lump {i} {} bytes hash {:x}", p.bytes.len(), sum(&p.bytes));
        }
    }
    let mut tail = hp_sniffer::launch::LogTail::new(install.console_log());
    let cancel = AtomicBool::new(false);
    let say = |t: &str| println!("{t}");
    let req = hp_sniffer::ldrpass::Request {
        install: &install,
        tail: &mut tail,
        local_map: map.as_deref(),
        connect: None,
        hub_base: hub.base,
        ptr: hub.ptr,
        surfaces: hub.faces.len(),
        timeout: std::time::Duration::from_secs(300),
        settle: std::time::Duration::from_secs(6),
        cancel: &cancel,
        say: &say,
    };
    match hp_sniffer::ldrpass::read(&proc, req) {
        Ok(d) => {
            for i in [8usize, 7, 56, 52, 15] {
                if let Some(p) = d.piece(i) {
                    println!("ldr pass lump {i} {} bytes hash {:x}", p.bytes.len(), sum(&p.bytes));
                }
            }
            if std::env::var("LDR_SAVE").is_ok() {
                for p in &d.pieces {
                    std::fs::write(format!("E:/hp-scratch/agent-nofile/ldr_{}.bin", p.index), &p.bytes).ok();
                }
            }
        }
        Err(e) => println!("failed: {e}"),
    }
    hp_sniffer::ldrpass::restore(&install);
}
