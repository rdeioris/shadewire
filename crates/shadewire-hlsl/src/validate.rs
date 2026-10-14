//! Checking generated HLSL with dxc: the library alone as a DXIL library (its host functions external), and
//! completed with stub host functions and a pixel shader entry.

use shadewire::shader::fixtures::{host_stub_text, run_tool};
use shadewire::{Reflection, model_struct_name};

use crate::Hlsl;

/// `code` completed with stub host functions and a pixel shader `main`.
pub fn whole_shader(code: &str, reflection: &Reflection) -> String {
    let model = model_struct_name(&reflection.model);
    let mut calls = format!("    {model} s = shadewire_eval_surface();\n");
    if code.contains("shadewire_eval_bsdf(") {
        calls.push_str(
            "    float3 n = float3(0.0, 0.0, 1.0);\n    ShadewireBsdf b = shadewire_eval_bsdf(s, n, n, n);\n    \
             float3 a = shadewire_eval_albedo(s, n, n);\n    float3 e = shadewire_eval_emission(s, n, n);\n    \
             ShadewireAmbient m = shadewire_eval_ambient(s, n, n);\n",
        );
    }
    format!(
        "{code}{}\nfloat4 main() : SV_Target {{\n{calls}    return float4(0.0, 0.0, 0.0, 0.0);\n}}\n",
        host_stub_text(&Hlsl, reflection)
    )
}

/// Compiles `text` with dxc and `args`: `Ok(false)` when dxc is not installed.
fn dxc(name: &str, text: &str, args: &[&str]) -> Result<bool, String> {
    let output = format!("{{dir}}/{name}.dxil");
    run_tool("dxc", &format!("{name}.hlsl"), text, args, &["-Fo", &output])
}

/// Checks `code` as a library (HLSL 2021) and, completed by [`whole_shader`], as a pixel shader in HLSL 2018 and
/// 2021; `Ok(false)` when dxc is not installed.
pub fn check(name: &str, code: &str, reflection: &Reflection) -> Result<bool, String> {
    if !dxc(&format!("{name}_lib"), code, &["-T", "lib_6_3", "-HV", "2021"])? {
        return Ok(false);
    }
    let whole = whole_shader(code, reflection);
    for hv in ["2018", "2021"] {
        dxc(&format!("{name}_ps{hv}"), &whole, &["-T", "ps_6_0", "-E", "main", "-HV", hv])?;
    }
    Ok(true)
}
