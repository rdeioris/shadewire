//! Constant tables read at a run-time index (hand-written noises' permutation tables) lower to `NodeKind::Lookup`:
//! every back end's code validates, and the MDL writer writes the tables back as module constants the reader reads
//! again.

use shadewire::shader::CompileOptions;
use shadewire::{LOBE_MODELS, NodeKind, Type};
use shadewire_mdl::{IrOptions, MdlOptions, Resolver, to_ir};

const TABLES: &str = include_str!("fixtures/tables.mdl");

fn lookups(dag: &shadewire::Dag) -> usize {
    dag.nodes.iter().filter(|n| matches!(n.kind, NodeKind::Lookup { .. })).count()
}

#[test]
fn a_table_indexed_at_run_time_is_a_lookup() {
    let ir = to_ir(TABLES, None, Some("hashed"), &IrOptions::with_models(&LOBE_MODELS), &Resolver::default())
        .unwrap_or_else(|e| panic!("{e}"));
    let dag = shadewire::optimize::optimize(&ir.dag);
    // One table of 256 ints, read twice; the 80 computed tints are a table of vectors.
    let shapes: Vec<(Type, usize)> = dag.tables.iter().map(|t| (t.ty, t.values.len())).collect();
    assert_eq!(shapes, [(Type::Integer, 256), (Type::Vector3, 80)], "{}", dag.dump());
    assert_eq!(lookups(&dag), 3, "{}", dag.dump());
}

#[test]
fn every_back_end_validates_a_lookup() {
    let ir = to_ir(TABLES, None, Some("hashed"), &IrOptions::with_models(&LOBE_MODELS), &Resolver::default())
        .unwrap_or_else(|e| panic!("{e}"));
    let options = CompileOptions::default();
    let wgsl = shadewire_wgsl::compile(&ir.dag, &ir.source, &options).unwrap();
    assert!(wgsl.code.contains("var<private> shadewire_l0: array<i32, 256>"), "{}", wgsl.code);
    shadewire_wgsl::validate::check(&wgsl.code, &wgsl.reflection).unwrap_or_else(|e| panic!("WGSL:\n{e}"));
    for version in [shadewire_glsl::Version::Desktop(420), shadewire_glsl::Version::Es(300)] {
        let glsl = shadewire_glsl::compile(&ir.dag, &ir.source, &options, version).unwrap();
        shadewire_glsl::validate::check("tables", &glsl.code, &glsl.reflection, version)
            .unwrap_or_else(|e| panic!("GLSL {version:?}:\n{e}"));
    }
    let hlsl = shadewire_hlsl::compile(&ir.dag, &ir.source, &options).unwrap();
    shadewire_hlsl::validate::check("tables", &hlsl.code, &hlsl.reflection).unwrap_or_else(|e| panic!("HLSL:\n{e}"));
    let msl = shadewire_msl::compile(&ir.dag, &ir.source, &options).unwrap();
    shadewire_msl::validate::check("tables", &msl.code, &msl.reflection).unwrap_or_else(|e| panic!("MSL:\n{e}"));
}

#[test]
fn the_mdl_writer_writes_tables_the_reader_reads_again() {
    let ir = to_ir(TABLES, None, Some("hashed"), &IrOptions::with_models(&LOBE_MODELS), &Resolver::default())
        .unwrap_or_else(|e| panic!("{e}"));
    let written = shadewire_mdl::compile(&ir.dag, &ir.source, &MdlOptions { name: "hashed".into(), optimize: true })
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(written.code.contains("const int[256] shadewire_l0("), "{}", written.code);
    let again = to_ir(&written.code, None, None, &IrOptions::with_models(&LOBE_MODELS), &Resolver::default())
        .unwrap_or_else(|e| panic!("{e}\n{}", written.code));
    assert_eq!(lookups(&shadewire::optimize::optimize(&again.dag)), 3);
}
