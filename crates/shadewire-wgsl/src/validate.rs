//! Checking generated WGSL with naga: the code, stub definitions of every host function the reflection lists, and
//! a fragment entry calling everything a host may, so naga also checks the stage rules of implicit-derivative
//! sampling. For tests (feature `validate`).

use shadewire::Reflection;
use shadewire::shader::Ty;
use shadewire::shader::fixtures::host_stub_text;
use shadewire::shader::print::Syntax;

use crate::Wgsl;

/// Parses and validates a whole module (all flags, all capabilities).
pub fn naga(text: &str) -> Result<(), String> {
    let module = naga::front::wgsl::parse_str(text).map_err(|e| e.emit_to_string(text))?;
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(&module)
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

/// `code` completed with stub host functions and a fragment entry, as one module.
pub fn whole_shader(code: &str, reflection: &Reflection) -> String {
    let stubs = host_stub_text(&Wgsl, reflection);
    let z = Wgsl.ty(&Ty::vec(3));
    let calls = if code.contains("fn shadewire_eval_bsdf(") {
        format!(
            "let s = shadewire_eval_surface(); let z = {z}(0.0, 0.0, 1.0); _ = shadewire_eval_bsdf(s, z, z, z); \
             _ = shadewire_eval_albedo(s, z, z); _ = shadewire_eval_emission(s, z, z); _ = shadewire_eval_ambient(s, z, z);"
        )
    } else {
        "_ = shadewire_eval_surface();".into()
    };
    format!(
        "{stubs}\n{code}\n@fragment\nfn shadewire_test_fragment() -> @location(0) vec4<f32> {{\n    {calls}\n    return vec4<f32>(0.0);\n}}\n"
    )
}

/// Validates `code` completed by [`whole_shader`].
pub fn check(code: &str, reflection: &Reflection) -> Result<(), String> {
    naga(&whole_shader(code, reflection))
}
