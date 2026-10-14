//! Checking generated GLSL with glslangValidator: the library, stub definitions of every host function the
//! reflection lists, and a `main` calling everything a host may.

use shadewire::shader::fixtures::{host_stub_text, run_tool};
use shadewire::{Reflection, model_struct_name};

use crate::{Glsl, Version};

/// `code` completed with stub host functions and a fragment `main`.
pub fn whole_shader(code: &str, reflection: &Reflection, version: Version) -> String {
    let model = model_struct_name(&reflection.model);
    let mut calls = format!("    {model} s = shadewire_eval_surface();\n");
    if code.contains("shadewire_eval_bsdf(") {
        calls.push_str(
            "    vec3 n = vec3(0.0, 0.0, 1.0);\n    ShadewireBsdf b = shadewire_eval_bsdf(s, n, n, n);\n    \
             vec3 a = shadewire_eval_albedo(s, n, n);\n    vec3 e = shadewire_eval_emission(s, n, n);\n    \
             ShadewireAmbient m = shadewire_eval_ambient(s, n, n);\n",
        );
    }
    format!(
        "{code}{}\nlayout(location = 0) out vec4 shadewire_test_color;\nvoid main() {{\n{calls}    shadewire_test_color = vec4(0.0);\n}}\n",
        host_stub_text(&Glsl { version }, reflection)
    )
}

/// Checks `code` completed by [`whole_shader`] as a fragment shader; `Ok(false)` when glslangValidator is not
/// installed.
pub fn check(name: &str, code: &str, reflection: &Reflection, version: Version) -> Result<bool, String> {
    run_tool(
        "glslangValidator",
        &format!("{name}.frag"),
        &whole_shader(code, reflection, version),
        &["-S", "frag"],
        &[],
    )
}
