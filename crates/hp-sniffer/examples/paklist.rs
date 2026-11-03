fn main() {
    let a: Vec<String> = std::env::args().collect();
    let b = hp_bsp::Bsp::read(&a[1]).unwrap();
    let p = b.pakfile().unwrap();
    for e in p.entries() {
        println!("{}", e.name);
    }
}
