use hp_sniffer::procmem::{MemSource, Region};
use hp_sniffer::recover::FileAccess;
use hp_sniffer::scan::ScanOptions;
use hp_sniffer::session::{CaptureInput, capture_from, produce};
use std::sync::atomic::AtomicBool;

pub struct Snap {
    map: memmap2::Mmap,
    idx: Vec<(u64, u64, u64)>,
}

impl Snap {
    pub fn open(base: &str) -> Snap {
        let f = std::fs::File::open(format!("{base}.bin")).expect("bin");
        let map = unsafe { memmap2::Mmap::map(&f).expect("map") };
        let idx = std::fs::read_to_string(format!("{base}.idx"))
            .expect("idx")
            .lines()
            .map(|l| {
                let mut it = l.split(' ');
                (
                    u64::from_str_radix(it.next().expect("a"), 16).expect("hex"),
                    it.next().expect("n").parse().expect("n"),
                    it.next().expect("p").parse().expect("p"),
                )
            })
            .collect();
        Snap { map, idx }
    }
}

impl MemSource for Snap {
    fn regions(&self) -> Vec<Region> {
        self.idx.iter().map(|(a, n, _)| Region { base: *a, size: *n, writable: true, image: false }).collect()
    }

    fn read(&self, addr: u64, buf: &mut [u8]) -> usize {
        let mut done = 0usize;
        while done < buf.len() {
            let at = addr + done as u64;
            let i = self.idx.partition_point(|x| x.0 <= at);
            if i == 0 {
                break;
            }
            let (a, n, p) = self.idx[i - 1];
            if at >= a + n {
                break;
            }
            let off = (p + at - a) as usize;
            let take = (buf.len() - done).min((a + n - at) as usize);
            buf[done..done + take].copy_from_slice(&self.map[off..off + take]);
            done += take;
        }
        done
    }
}




fn main() {
    let args: Vec<String> = std::env::args().collect();
    let snap = Snap::open(&args[1]);
    let out = std::path::PathBuf::from(&args[2]);
    let map = args.get(3).cloned().unwrap_or_else(|| "gm_construct".to_owned());
    let game = std::path::PathBuf::from(args.get(4).cloned().unwrap_or_else(|| "D:/SteamLibrary/steamapps/common/GarrysMod/garrysmod".to_owned()));
    let use_file = args.get(5).is_some_and(|v| v == "file");
    let opts = ScanOptions { writable_only: true, min_region: 1 << 16, ..ScanOptions::default() };
    let hits = hp_sniffer::scan::find_entity_lumps(&snap, &opts);
    let hit = hits.iter().max_by_key(|h| h.entities).expect("entity lump").clone();
    let mut settings = hp_sniffer::SnifferSettings { save_models: false, save_materials: false, save_sounds: false, save_other: false, ..hp_sniffer::SnifferSettings::default() };
    if use_file {
        settings.set_internal("use_map_file", &hp_sniffer::SettingValue::Bool(true)).expect("setting");
    }
    let access = FileAccess::new(settings.file_allowed());
    let input = CaptureInput { game_dir: &game, map_hint: Some(map.clone()), settings: &settings, access: &access, rescan: None, rescan_limit: std::time::Duration::ZERO };
    let cancel = AtomicBool::new(false);
    let scratch = out.join("_work");
    std::fs::create_dir_all(&scratch).expect("scratch");
    let t = std::time::Instant::now();
    let cap = capture_from(&snap, &hit, &input, &scratch, None, &cancel).ok().expect("capture");
    println!("captured in {:?}, map file opens {}", t.elapsed(), cap.report.map_file_opens);
    let mut timings = Vec::new();
    let (report, produced) = produce(cap, &map, None, &settings, &out, "snapshot", 0, None, &cancel, &mut timings, None).ok().expect("produce");
    println!("{}", report.render_text());
    println!("bsp {:?}", produced.bsp_path);
}
