//! Lowers every material of an MDL module and reports what happened: `mdl_ir FILE [SEARCH_PATH...]`.
use shadewire::LOBE_MODELS;
use shadewire_mdl::{IrOptions, Resolver, material_names, to_ir};

fn main() {
    let mut args = std::env::args().skip(1);
    let file = args.next().expect("an .mdl file");
    let resolver = Resolver::new(args.map(Into::into).collect());
    let text = std::fs::read_to_string(&file).unwrap();
    let names = match material_names(&text, Some(&file), &resolver) {
        Ok(n) => n,
        Err(e) => return println!("{file}: {e}"),
    };
    let options = IrOptions::with_models(&LOBE_MODELS);
    let (mut ok, mut failed) = (0, 0);
    for name in &names {
        match to_ir(&text, Some(&file), Some(name), &options, &resolver) {
            Ok(ir) => {
                ok += 1;
                println!("ok   {name}: {} nodes, {} warnings", ir.dag.nodes.len(), ir.warnings.len());
                for w in &ir.warnings {
                    println!("       {w}");
                }
            }
            Err(e) => {
                failed += 1;
                println!("FAIL {name}: {e}");
            }
        }
    }
    println!("{file}: {ok} ok, {failed} failed");
}
