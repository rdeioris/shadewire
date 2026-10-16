//! Writes one MaterialX material as MDL, opened into its closure tree: `mtlx_mdl FILE MATERIAL`.
use shadewire::LOBE_MODELS;
use shadewire_mdl::{IrOptions, MdlOptions, compile, write::mdl_ident};

fn main() {
    let mut args = std::env::args().skip(1);
    let file = args.next().expect("a .mtlx file");
    let material = args.next().expect("a material");
    let text = std::fs::read_to_string(&file).unwrap();
    let options = IrOptions::with_models(&LOBE_MODELS);
    let ir = shadewire_mtlx::to_ir(&text, Some(&file), Some(&material), &options, &Default::default()).unwrap();
    let options = MdlOptions { name: mdl_ident(&material), ..Default::default() };
    print!("{}", compile(&ir.dag, &ir.source, &options).unwrap().code);
}
