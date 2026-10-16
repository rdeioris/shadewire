fn main() {
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).unwrap();
        let t = std::time::Instant::now();
        let r = shadewire_mdl::parse::parse_module(&text, Some(&path));
        match r {
            Ok(_) => println!("{} {path} ok", t.elapsed().as_micros()),
            Err(e) => println!("{} {path} {e}", t.elapsed().as_micros()),
        }
    }
}
