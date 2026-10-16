//! Writes one MDL material as MaterialX: `mdl_mtlx FILE MATERIAL [SEARCH_PATH...]`.
use shadewire::LOBE_MODELS;
use shadewire_mdl::{IrOptions, Resolver, to_ir};

fn main() {
    let mut args = std::env::args().skip(1);
    let file = args.next().expect("an .mdl file");
    let material = args.next().expect("a material");
    let resolver = Resolver::new(args.map(Into::into).collect());
    let text = std::fs::read_to_string(&file).unwrap();
    let ir = to_ir(&text, Some(&file), Some(&material), &IrOptions::with_models(&LOBE_MODELS), &resolver).unwrap();
    let write = shadewire_mtlx::WriteOptions { name: material, ..Default::default() };
    print!("{}", shadewire_mtlx::from_ir(&ir.dag, &write).unwrap().text);
}
