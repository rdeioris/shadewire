//! GPU parity: every op case run through the interpreter and as WGSL on the GPU must agree.

mod gpu;

use gpu::cases::{all, family, Case};
use shadewire_ir::interpret::{eval_nodes, ContextSource, NoTextures};
use shadewire_ir::{ContextRead, Space, TransformKind, Value};
use shadewire_wgsl::emit::probe_function;
use shadewire_wgsl::leaf_library;

/// Parity cases read no host data.
struct NoContext;

impl ContextSource for NoContext {
    fn read(&mut self, read: &ContextRead) -> Value {
        panic!("parity case read {read:?}")
    }
    fn transform(&mut self, kind: TransformKind, _: Space, _: Space, _: [f32; 3]) -> [f32; 3] {
        panic!("parity case transformed a {kind:?}")
    }
}

/// The interpreter's value of the case node, padded to four components with 0.
fn expected(case: &Case) -> [f32; 4] {
    let values = eval_nodes(&case.dag, &mut NoContext, &NoTextures);
    let floats = values[case.node.index()].floats();
    let mut out = [0.0; 4];
    out[..floats.len()].copy_from_slice(&floats);
    out
}

/// Relative-or-absolute error, the worst component: `|gpu - cpu| / max(1, |cpu|)`.
/// Values are compared, not bits, so -0.0 equals 0.0. A NaN on either side is infinite error.
fn error(gpu: [f32; 4], cpu: [f32; 4]) -> f32 {
    let mut worst: f32 = 0.0;
    for (g, c) in gpu.iter().zip(cpu) {
        let e = if g.is_nan() || c.is_nan() { f32::INFINITY } else if *g == c { 0.0 } else { (g - c).abs() / c.abs().max(1.0) };
        worst = worst.max(e);
    }
    worst
}

/// The library files any case needs (each once, in dependency order) and every probe function.
fn wgsl_functions(cases: &[Case]) -> String {
    let mut names: Vec<&str> = Vec::new();
    for case in cases {
        names.extend(leaf_library::names_for(&case.dag));
    }
    let mut text: Vec<String> = Vec::new();
    for name in leaf_library::FILES {
        if names.contains(&name) {
            text.push(leaf_library::source(name).to_string());
        }
    }
    for (i, case) in cases.iter().enumerate() {
        let probe = probe_function(&case.dag, case.node, &format!("case_{i}"));
        text.push(gpu::opaque_constants(&case.dag, case.node, &probe));
    }
    text.join("\n")
}

/// Checks the module before it reaches a device, for a readable error.
fn naga_validates(text: &str) {
    let module = naga::front::wgsl::parse_str(text).unwrap_or_else(|e| panic!("{}", e.emit_to_string(text)));
    let mut validator = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
    validator.validate(&module).unwrap_or_else(|e| panic!("{e:?}"));
}

/// Case count and worst error of one family.
struct FamilyStats {
    name: String,
    count: usize,
    worst: f32,
}

/// Opt-in: about three minutes of driver compile on a shared GPU.
/// `SHADEWIRE_REQUIRE_GPU=1` turns the no-adapter skip into a failure.
#[test]
#[ignore = "gpu parity: cargo test -p shadewire-wgsl --test gpu_parity -- --ignored"]
fn interpreter_and_wgsl_agree_on_every_op() {
    let cases = all();
    let functions = wgsl_functions(&cases);
    naga_validates(&gpu::shader(&functions, cases.len()));
    let Some(gpu) = gpu::run_probes(&functions, cases.len()) else { return };
    assert_eq!(gpu.len(), cases.len());
    let mut families: Vec<FamilyStats> = Vec::new();
    let mut failures = 0;
    for (case, gpu) in cases.iter().zip(gpu) {
        let cpu = expected(case);
        let e = error(gpu, cpu);
        let fam = family(&case.name);
        match families.iter_mut().find(|f| f.name == fam) {
            Some(f) => {
                f.count += 1;
                f.worst = f.worst.max(e);
            }
            None => families.push(FamilyStats { name: fam.to_string(), count: 1, worst: e }),
        }
        if e > case.tolerance {
            failures += 1;
            println!("MISMATCH {}\n    gpu {gpu:?}\n    cpu {cpu:?}\n    error {e:e} > {:e}", case.name, case.tolerance);
        }
    }
    for f in &families {
        println!("{:8} {:5} cases, max error {:e}", f.name, f.count, f.worst);
    }
    println!("{} cases, {failures} mismatches", cases.len());
    assert_eq!(failures, 0, "{failures} of {} cases disagree", cases.len());
}
