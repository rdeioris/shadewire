//! Checking generated HLSL with dxc (found by `shadewire::shader::fixtures::tool`): the library alone as a DXIL
//! library (its host functions external), and completed with stub host functions and a pixel shader entry.

use std::process::Command;

use shadewire::shader::Module;
use shadewire::shader::fixtures::{host_stubs, tool};
use shadewire::shader::print::Printer;
use shadewire::{Reflection, model_struct_name};

use crate::Hlsl;

/// `code` completed with stub host functions and a pixel shader `main`.
pub fn whole_shader(code: &str, reflection: &Reflection) -> String {
    let empty = Module::default();
    let printer = Printer::new(&Hlsl);
    let mut text = code.to_string();
    for stub in host_stubs(reflection) {
        text.push_str(&printer.item(&stub, &empty));
    }
    let model = model_struct_name(&reflection.model);
    let mut calls = format!("    {model} s = shadewire_eval_surface();\n");
    if code.contains("shadewire_eval_bsdf(") {
        calls.push_str(
            "    float3 n = float3(0.0, 0.0, 1.0);\n    ShadewireBsdf b = shadewire_eval_bsdf(s, n, n, n);\n    \
             float3 a = shadewire_eval_albedo(s, n, n);\n    float3 e = shadewire_eval_emission(s, n, n);\n",
        );
    }
    text.push_str(&format!("\nfloat4 main() : SV_Target {{\n{calls}    return float4(0.0, 0.0, 0.0, 0.0);\n}}\n"));
    text
}

/// Compiles `text` with dxc and `args`: `Ok(false)` when dxc is not installed.
pub fn dxc(name: &str, text: &str, args: &[&str]) -> Result<bool, String> {
    let Some(dxc) = tool("dxc") else {
        return Ok(false);
    };
    let dir = std::env::temp_dir().join("shadewire-hlsl-check");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{name}.hlsl"));
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    let out = Command::new(dxc)
        .args(args)
        .arg(&path)
        .arg("-Fo")
        .arg(dir.join(format!("{name}.dxil")))
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(true)
    } else {
        Err(format!("{args:?}: {}\n\n{}", String::from_utf8_lossy(&out.stderr), numbered(text)))
    }
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

fn numbered(text: &str) -> String {
    text.lines().enumerate().map(|(i, l)| format!("{:4} {l}", i + 1)).collect::<Vec<_>>().join("\n")
}
