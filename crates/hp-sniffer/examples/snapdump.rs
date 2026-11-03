use hp_sniffer::procmem::{GameProcess, MemSource};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pid: u32 = args[1].parse().expect("pid");
    let out = &args[2];
    let p = GameProcess::open(pid).expect("open");
    let mut f = std::io::BufWriter::new(std::fs::File::create(format!("{out}.bin")).expect("bin"));
    let mut idx = std::fs::File::create(format!("{out}.idx")).expect("idx");
    let mut buf = vec![0u8; 16 << 20];
    let mut pos = 0u64;
    for r in p.regions() {
        if r.image {
            continue;
        }
        let mut off = 0u64;
        while off < r.size {
            let n = ((r.size - off) as usize).min(buf.len());
            let got = p.read(r.base + off, &mut buf[..n]);
            if got > 0 {
                f.write_all(&buf[..got]).expect("write");
                writeln!(idx, "{:x} {} {}", r.base + off, got, pos).expect("idx");
                pos += got as u64;
            }
            off += n as u64;
        }
    }
    println!("dumped {pos} bytes");
}
