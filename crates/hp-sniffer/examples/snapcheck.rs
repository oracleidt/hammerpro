use hp_sniffer::procmem::{MemSource, Region};
use hp_sniffer::scan::ScanOptions;
use std::time::Instant;

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
    let t = Instant::now();
    let hits = hp_sniffer::scan::find_entity_lumps(&snap, &ScanOptions::default());
    let text = hits.first().map(|h| h.text.clone()).expect("entities");
    let world = hp_sniffer::memmap::find_all(&snap, &ScanOptions::default(), "rp_rockford_v2b").expect("world");
    println!("memory world found in {:?}", t.elapsed());
    println!(
        "extras: disps {} overlays {} props {} cubemaps {}",
        world.extras.displacements.len(),
        world.extras.overlays.len(),
        world.extras.props.len(),
        world.extras.cubemaps.len()
    );
    if let Some(ap) = &world.extras.area_portals {
        println!("area portals {} verts {} at {:x} / {:x}", ap.count(), ap.verts.len(), ap.records_addr, ap.verts_addr);
    } else {
        println!("no area portals");
    }
    {
        let opts = ScanOptions::default();
        let t = Instant::now();
        let faces = hp_sniffer::world::find_faces(&snap, &opts, world.cm.cm.planes_addr, world.cm.cm.planes);
        println!("faces {:?} in {:?}", faces.as_ref().map(|f| (format!("{:x}", f.0), f.1.len())), t.elapsed());
        if let Some((fa, fl)) = &faces {
            let maxt = fl.iter().map(|f| usize::from(f.texinfo)).max().unwrap_or(0) + 1;
            let ti = hp_sniffer::world::find_texinfo(&snap, *fa, maxt);
            println!("texinfo {:?} wanted {}", ti.as_ref().map(|f| (format!("{:x}", f.0), f.1.len())), maxt);
        }
    }
    if let Some(r) = &world.render {
        println!(
            "addrs faces {:x} texinfo {:x} vertindices {:x} vertexes {:x}",
            r.faces_addr, r.texinfo_addr, r.vertindices_addr, r.vertexes_addr
        );
    }
    match &world.render {
        Some(r) => println!(
            "render: faces {} texinfo {} vertindices {} vertexes {}",
            r.faces.len(),
            r.texinfo.len(),
            r.vertindices.len(),
            r.vertexes.len()
        ),
        None => println!("render world not found"),
    }
    let bsp = hp_sniffer::memmap::build_bsp(&world, &text).expect("bsp");
    if let Some(out) = args.get(3) {
        std::fs::write(out, bsp.bytes()).expect("write");
    }
    let mem_vmf = hp_decompile::decompile(
        &bsp,
        &hp_decompile::Options { map_name: "rp_rockford_v2b".to_owned(), ..hp_decompile::Options::default() },
    )
    .expect("decompile");
    println!("{:?}", mem_vmf.report);
    if let Some(out) = args.get(4) {
        std::fs::write(out, hp_vmf::write(&mem_vmf.map)).expect("write vmf");
    }
    if let Some(file) = args.get(2) {
        let key = |sd: &hp_vmf::Side| {
            let p = sd.plane;
            format!(
                "{:.0},{:.0},{:.0}|{:.0},{:.0},{:.0}|{:.0},{:.0},{:.0}",
                p[0].x, p[0].y, p[0].z, p[1].x, p[1].y, p[1].z, p[2].x, p[2].y, p[2].z
            )
        };
        let solids = |m: &hp_vmf::Map| {
            let mut out: std::collections::HashMap<String, Vec<(String, String, String)>> =
                std::collections::HashMap::new();
            for s in m.world.solids.iter().chain(m.entities.iter().flat_map(|e| e.solids.iter())) {
                let mut sides: Vec<String> = s.sides.iter().map(&key).collect();
                sides.sort();
                let mut detail: Vec<(String, String, String)> = s
                    .sides
                    .iter()
                    .map(|sd| {
                        (
                            key(sd),
                            sd.material.to_ascii_lowercase(),
                            format!("{:?} {:?} {}", sd.uaxis, sd.vaxis, sd.lightmap_scale),
                        )
                    })
                    .collect();
                detail.sort();
                out.insert(sides.join(";"), detail);
            }
            out
        };
        let reference_map = {
            let r = hp_bsp::Bsp::open(file).expect("reference");
            hp_decompile::decompile(
                &r,
                &hp_decompile::Options { map_name: "rp_rockford_v2b".to_owned(), ..hp_decompile::Options::default() },
            )
            .expect("reference decompile")
            .map
        };
        let (a, b) = (solids(&mem_vmf.map), solids(&reference_map));
        let mut same_solid = 0usize;
        let mut sides_total = 0usize;
        let mut same_material = 0usize;
        let mut same_uv = 0usize;
        let mut visible = 0usize;
        let mut visible_uv = 0usize;
        for (k, da) in &a {
            if let Some(db) = b.get(k) {
                same_solid += 1;
                for (x, y) in da.iter().zip(db) {
                    sides_total += 1;
                    if x.1 == y.1 {
                        same_material += 1;
                    }
                    if x.2 == y.2 {
                        same_uv += 1;
                    }
                    if !x.1.starts_with("tools/") {
                        visible += 1;
                        if x.2 == y.2 {
                            visible_uv += 1;
                        }
                    }
                }
            }
        }
        println!(
            "solids {} vs {}: same planes {}; sides {}: same material {}, same uv {}; visible sides {} same uv {}",
            a.len(),
            b.len(),
            same_solid,
            sides_total,
            same_material,
            same_uv,
            visible,
            visible_uv
        );
        let reference = hp_bsp::Bsp::open(file).expect("reference");
        let r = hp_decompile::decompile(
            &reference,
            &hp_decompile::Options { map_name: "rp_rockford_v2b".to_owned(), ..hp_decompile::Options::default() },
        )
        .expect("reference decompile");
        println!("{:?}", r.report);
        if let Some(out) = args.get(5) {
            std::fs::write(out, hp_vmf::write(&r.map)).expect("write ref vmf");
        }
    }
}
