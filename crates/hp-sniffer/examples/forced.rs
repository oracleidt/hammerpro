use hp_sniffer::procmem::{MemSource, Region};
use hp_sniffer::scan::ScanOptions;

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
    let opts = ScanOptions::default();
    let hits = hp_sniffer::scan::find_entity_lumps(&snap, &opts);
    let _ = hits.first().expect("entities");
    let mut x64 = hp_sniffer::hubdec::X64;
    match args[2].as_str() {
        "leaf" => x64.leaf_stride += 8,
        "face" => x64.s2_stride += 8,
        "node" => x64.node_stride += 8,
        _ => {}
    }
    hp_sniffer::hubdec::override_profiles(Some(vec![x64]));
    let world = hp_sniffer::memmap::find_all_mode(&snap, &opts, "x", true).expect("world");
    match &world.hub {
        Some(h) => {
            println!("profile {} pieces {}", h.profile, h.pieces.len());
            println!("missing {:?}", h.missing);
            println!("problems {:?}", h.problems);
        }
        None => println!("no hub structure recognized"),
    }
}
