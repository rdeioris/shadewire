//! The HLSL back end over the shared fixtures: each library compiles in dxc as a library (its host functions
//! external), and completed with stub host functions and an entry point as a pixel shader, as HLSL 2018 and 2021.

use shadewire::shader::fixtures::{self, reflection};
use shadewire::shader::material::{MaterialOptions, material};

use super::*;

#[test]
fn every_fixture_compiles() {
    for (name, dag) in fixtures::all() {
        let r = reflection(&dag);
        for implicit_derivatives in [true, false] {
            let options = MaterialOptions { implicit_derivatives, ..MaterialOptions::default() };
            let module = material(&dag, &r, &options);
            let tag = format!("{name}_{implicit_derivatives}");
            if !crate::validate::check(&tag, &print(&module), &r).unwrap_or_else(|e| panic!("{tag}:\n{e}")) {
                eprintln!("dxc not found: {tag} not compiled");
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
