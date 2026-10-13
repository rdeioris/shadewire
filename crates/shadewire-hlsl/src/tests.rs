//! The HLSL back end over the shared fixtures: each library compiles in dxc as a library (its host functions
//! external), and completed with stub host functions and an entry point as a pixel shader, as HLSL 2018 and 2021.

use std::process::Command;

use shadewire::shader::Module;
use shadewire::shader::fixtures::{self, host_stubs, reflection, tool};
use shadewire::shader::material::{MaterialOptions, material};
use shadewire::shader::print::Printer;

use super::*;

/// The library, then stub host functions and a pixel shader calling everything the host may.
fn whole_shader(module: &Module, reflection: &shadewire::Reflection) -> String {
    let printer = Printer::new(&Hlsl);
    let mut text = print(module);
    for stub in host_stubs(reflection) {
        text.push_str(&printer.item(&stub, module));
    }
    let model = module.functions().find(|f| f.name == "shadewire_eval_surface").map(|f| Hlsl.ty(&f.ret)).unwrap();
    let mut calls = format!("    {model} s = shadewire_eval_surface();\n");
    if module.functions().any(|f| f.name == "shadewire_eval_bsdf") {
        calls.push_str(
            "    float3 n = float3(0.0, 0.0, 1.0);\n    ShadewireBsdf b = shadewire_eval_bsdf(s, n, n, n);\n    \
             float3 a = shadewire_eval_albedo(s, n, n);\n    float3 e = shadewire_eval_emission(s, n, n);\n",
        );
    }
    text.push_str(&format!("\nfloat4 main() : SV_Target {{\n{calls}    return float4(0.0, 0.0, 0.0, 0.0);\n}}\n"));
    text
}

fn dxc(name: &str, text: &str, args: &[&str]) {
    let Some(dxc) = tool("dxc") else {
        eprintln!("dxc not found: {name} not compiled");
        return;
    };
    let dir = std::env::temp_dir().join("shadewire-hlsl-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.hlsl"));
    std::fs::write(&path, text).unwrap();
    let out =
        Command::new(dxc).args(args).arg(&path).arg("-Fo").arg(dir.join(format!("{name}.dxil"))).output().unwrap();
    let log = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{name} {args:?}:\n{log}\n\n{}", numbered(text));
}

fn numbered(text: &str) -> String {
    text.lines().enumerate().map(|(i, l)| format!("{:4} {l}", i + 1)).collect::<Vec<_>>().join("\n")
}

#[test]
fn every_fixture_compiles() {
    for (name, dag) in fixtures::all() {
        let r = reflection(&dag);
        for implicit_derivatives in [true, false] {
            let options = MaterialOptions { implicit_derivatives, ..MaterialOptions::default() };
            let module = material(&dag, &r, &options);
            let tag = format!("{name}_{implicit_derivatives}");
            dxc(&format!("{tag}_lib"), &print(&module), &["-T", "lib_6_3", "-HV", "2021"]);
            for hv in ["2018", "2021"] {
                dxc(&format!("{tag}_ps{hv}"), &whole_shader(&module, &r), &["-T", "ps_6_0", "-E", "main", "-HV", hv]);
            }
        }
    }
}

#[test]
fn resources_and_padding() {
    let dag = fixtures::library_ops();
    let r = reflection(&dag);
    let text = print(&material(&dag, &r, &MaterialOptions::default()));
    assert!(text.contains("ConstantBuffer<MaterialUniforms> shadewire_u : register(b0, space0);"), "{text}");
    // Padded to the reflected block size.
    assert!(
        text.contains("struct MaterialUniforms {\n    float amp;\n    int oct;\n    uint shadewire_pad0;\n    uint shadewire_pad1;\n};"),
        "{text}"
    );
    let dag = fixtures::everything();
    let r = reflection(&dag);
    let text = print(&material(&dag, &r, &MaterialOptions::default()));
    assert!(text.contains("Texture2D<float4> shadewire_t0 : register(t1, space0);"), "{text}");
    assert!(text.contains("SamplerState shadewire_s0 : register(s2, space0);"), "{text}");
    assert!(text.contains(".Sample(shadewire_s0, float2(n0.x, 1.0 - n0.y)).xyz"), "{text}");
    assert_eq!(hlsl_type(Type::Color3), "float3");
}
