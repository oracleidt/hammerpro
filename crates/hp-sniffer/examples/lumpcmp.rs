fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ours = hp_bsp::Bsp::open(&args[1]).expect("ours");
    let truth = hp_bsp::Bsp::open(&args[2]).expect("truth");
    let mut exact = 0usize;
    let mut total = 0usize;
    for i in 0..64usize {
        let t = truth.lump(i).map(|c| c.into_owned()).unwrap_or_default();
        let o = ours.lump(i).map(|c| c.into_owned()).unwrap_or_default();
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
        println!("{:>2} {:<28} truth {:>10} ours {:>10}  {}", i, hp_bsp::lump_name(i), t.len(), o.len(), status);
    }
    println!("exact bytes {exact} of {total} ({:.1}%)", 100.0 * exact as f64 / total.max(1) as f64);
}
