//! Checking generated MSL with Apple's Metal compiler (`xcrun metal`, macOS with Xcode's Metal toolchain): the
//! library alone (its host functions external), and completed with stub host functions and a fragment entry that
//! binds the material's resources and passes them to `shadewire_eval_surface`.

use shadewire::shader::fixtures::{host_stub_text, run_tool};
use shadewire::{Reflection, model_struct_name};

use crate::Msl;

/// `code` completed with stub host functions and a fragment function `shadewire_test_main`.
pub fn whole_shader(code: &str, reflection: &Reflection) -> String {
    let model = model_struct_name(&reflection.model);
    // The entry binds the resources in the order `shadewire_eval_surface` takes them.
    let (mut params, mut args) = (Vec::new(), Vec::new());
    if reflection.uniform_binding.is_some() {
        params.push("constant MaterialUniforms& shadewire_u [[buffer(0)]]".to_string());
        args.push("shadewire_u".to_string());
    }
    for (k, t) in reflection.textures.iter().enumerate() {
        params.push(format!("texture2d<float> shadewire_t{} [[texture({k})]]", t.index));
        params.push(format!("sampler shadewire_s{} [[sampler({k})]]", t.index));
        args.push(format!("shadewire_t{}", t.index));
        args.push(format!("shadewire_s{}", t.index));
    }
    let mut calls = format!("    {model} s = shadewire_eval_surface({});\n", args.join(", "));
    if code.contains("shadewire_eval_bsdf(") {
        calls.push_str(
            "    float3 n = float3(0.0f, 0.0f, 1.0f);\n    ShadewireBsdf b = shadewire_eval_bsdf(s, n, n, n);\n    \
             float3 a = shadewire_eval_albedo(s, n, n);\n    float3 e = shadewire_eval_emission(s, n, n);\n    \
             ShadewireAmbient m = shadewire_eval_ambient(s, n, n);\n",
        );
    }
    format!(
        "{code}{}\nfragment float4 shadewire_test_main({}) {{\n{calls}    return float4(0.0f);\n}}\n",
        host_stub_text(&Msl, reflection),
        params.join(", ")
    )
}

/// Compiles `text` to AIR with `xcrun metal`: `Ok(false)` when xcrun is not installed.
fn metal(name: &str, text: &str) -> Result<bool, String> {
    let output = format!("{{dir}}/{name}.air");
    run_tool("xcrun", &format!("{name}.metal"), text, &["-sdk", "macosx", "metal", "-c"], &["-o", &output])
}

/// Checks `code` as a library and, completed by [`whole_shader`], with a fragment entry; `Ok(false)` without the
/// Metal compiler (anywhere but macOS).
pub fn check(name: &str, code: &str, reflection: &Reflection) -> Result<bool, String> {
    if !metal(&format!("{name}_lib"), code)? {
        return Ok(false);
    }
    metal(&format!("{name}_frag"), &whole_shader(code, reflection))?;
    Ok(true)
}
