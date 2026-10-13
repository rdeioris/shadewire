//! Helpers shared by the test modules.

use shadewire::Reflection;
use shadewire::shader::Ty;
use shadewire::shader::material::host_functions;
use shadewire::shader::print::Syntax;

use crate::Wgsl;

/// Parses and validates a whole module (all flags, all capabilities).
pub fn naga_validates(text: &str) {
    let module = naga::front::wgsl::parse_str(text).unwrap_or_else(|e| panic!("{text}\n{}", e.emit_to_string(text)));
    let mut validator =
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
    validator.validate(&module).unwrap_or_else(|e| panic!("{text}\n{e:?}"));
}

/// Validates an emitted module: stub definitions of every host function the reflection lists, then the module,
/// then a fragment entry calling `shadewire_eval_surface` (and the closure functions when there are), so naga
/// also checks the stage rules of `textureSample` and the derivatives.
pub fn validates(reflection: &Reflection, wgsl: &str) {
    let mut stubs = String::new();
    for f in host_functions(reflection) {
        let ret = Wgsl.ty(&f.ret);
        let body = if f.params.is_empty() { format!("{ret}()") } else { "v".to_string() };
        let params: Vec<String> = f.params.iter().map(|(n, t)| format!("{n}: {}", Wgsl.ty(t))).collect();
        stubs.push_str(&format!("fn {}({}) -> {ret} {{ return {body}; }}\n", f.name, params.join(", ")));
    }
    let z = Wgsl.ty(&Ty::vec(3));
    let calls = if wgsl.contains("fn shadewire_eval_bsdf(") {
        format!(
            "let s = shadewire_eval_surface(); let z = {z}(0.0); _ = shadewire_eval_bsdf(s, z, z, z); \
             _ = shadewire_eval_albedo(s, z, z); _ = shadewire_eval_emission(s, z, z);"
        )
    } else {
        "_ = shadewire_eval_surface();".into()
    };
    let entry = format!(
        "@fragment\nfn shadewire_test_fragment() -> @location(0) vec4<f32> {{\n    {calls}\n    return vec4<f32>(0.0);\n}}\n"
    );
    naga_validates(&format!("{stubs}\n{wgsl}\n{entry}"));
}
