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
    let truth = hp_bsp::Bsp::open(&args[2]).expect("bsp");
    let hdr = args.get(4).map(|s| s != "ldr").unwrap_or(true);
    let opts = ScanOptions::default();
    let hits = hp_sniffer::scan::find_entity_lumps(&snap, &opts);
    let text = hits.first().map(|h| h.text.clone()).expect("entities");
    let ents: Vec<(u64, usize)> = hits.iter().map(|h| (h.addr, h.text.len())).collect();
    for h in &hits {
        println!("entity hit {:x} len {} entities {}", h.addr, h.text.len(), h.entities);
        for p in hp_sniffer::hub::find_pointer_hits(&snap, h.addr, 8) {
            println!("  pointer at {p:x}");
        }
    }
    println!("entity lumps {}, structure by entities: {}", ents.len(), hp_sniffer::cmstruct::find_by_entities(&snap, &ents, &[8, 4]).map_or("no".to_owned(), |c| format!("yes at {:x} planes {}", c.base, c.planes.count)));
    let t = std::time::Instant::now();
    let world = hp_sniffer::memmap::find_all_mode(&snap, &opts, "x", hdr).expect("world");
    println!("world found in {:?}, hub: {:?}", t.elapsed(), world.hub.as_ref().map(|h| (&h.profile, h.pieces.len())));
    if let Some(h) = &world.hub {
        println!("missing {:?} problems {:?}", h.missing, h.problems);
    }
    if let Some(h) = &world.hub {
        let last = h.disp_extras.values().map(|x| x.sample_start).max();
        println!("disp extras {} last start {:?}", h.disp_extras.len(), last);
        println!("sample vector {:?}", None::<(u64, usize)>.map(|(a, n)| (format!("{a:x}"), n)));
    }
    if let Some(r) = &world.render {
        let tex = truth.lump(6).expect("texinfo").into_owned();
        let names: Vec<String> = world.cm.cm.surface_names.clone();
        let want: Vec<i32> = tex.chunks_exact(72).map(|c| i32::from_le_bytes([c[68], c[69], c[70], c[71]])).collect();
        let acc = |ranks: &[i32]| ranks.iter().zip(&want).filter(|(a, b)| a == b).count();
        let all = hp_sniffer::memmap::material_ranks(&world, r, &names);
        let by_id = hp_sniffer::memmap::texdata_ranks(&r.texinfo, &r.material_ids, &names);
        let by_ptr = hp_sniffer::memmap::texdata_ranks(&r.texinfo, &std::collections::HashMap::new(), &names);
        println!("texinfo {} names {} ids {}: final {} by id {} by pointer {}", want.len(), names.len(), r.material_ids.len(), acc(&all), acc(&by_id), acc(&by_ptr));
    }
    let bsp = hp_sniffer::memmap::build_bsp(&world, &text).expect("bsp");
    if let Some(out) = args.get(3)
        && out != "-"
    {
        std::fs::write(out, bsp.bytes()).expect("write");
    }
    let mut exact = 0usize;
    let mut total = 0usize;
    for i in 0..64usize {
        let t = truth.lump(i).map(|c| c.into_owned()).unwrap_or_default();
        let o = bsp.lump(i).map(|c| c.into_owned()).unwrap_or_default();
        if t.is_empty() && o.is_empty() {
            continue;
        }
        let status = if t == o {
            exact += t.len();
            "EXACT".to_owned()
        } else if o.is_empty() {
            "MISSING".to_owned()
        } else if t.is_empty() {
            "EXTRA".to_owned()
        } else {
            let same = t.iter().zip(&o).filter(|(a, b)| a == b).count();
            format!("differs {:.2}% of the bytes equal", 100.0 * same as f64 / t.len().max(o.len()) as f64)
        };
        total += t.len();
        println!(
            "{:>2} {:<28} truth {:>9} ours {:>9}  {}",
            i,
            hp_bsp::lump_name(i),
            t.len(),
            o.len(),
            status
        );
    }
    println!("exact bytes {exact} of {total}");
}
