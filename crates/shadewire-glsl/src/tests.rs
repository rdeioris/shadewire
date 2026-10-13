//! The GLSL back end over the shared fixtures: each library, completed with stub host functions and a `main`,
//! compiles in glslangValidator at every version.

use std::process::Command;

use shadewire::shader::fixtures::{self, host_stubs, reflection, tool};
use shadewire::shader::material::{MaterialOptions, material};
use shadewire::shader::print::Printer;
use shadewire::shader::{Item, Module, Ty};

use super::*;

const VERSIONS: [Version; 5] =
    [Version::Desktop(330), Version::Desktop(420), Version::Desktop(450), Version::Es(300), Version::Es(310)];

/// The library, then stub host functions and a `main` calling everything the host may.
fn whole_shader(module: &Module, reflection: &shadewire::Reflection, version: Version) -> String {
    let syntax = Glsl { version };
    let printer = Printer::new(&syntax);
    let mut text = print(module, version);
    for stub in host_stubs(reflection) {
        text.push_str(&printer.item(&stub, module));
    }
    let model =
        module.functions().find(|f| f.name == "shadewire_eval_surface").map(|f| syntax.ty(&f.ret)).expect("the entry");
    let closures = module.functions().any(|f| f.name == "shadewire_eval_bsdf");
    let mut calls = format!("    {model} s = shadewire_eval_surface();\n");
    if closures {
        calls.push_str(
            "    ShadewireBsdf b = shadewire_eval_bsdf(s, vec3(0.0), vec3(0.0, 0.0, 1.0), vec3(0.0, 0.0, 1.0));\n    \
             vec3 a = shadewire_eval_albedo(s, vec3(0.0, 0.0, 1.0), vec3(0.0, 0.0, 1.0));\n    \
             vec3 e = shadewire_eval_emission(s, vec3(0.0, 0.0, 1.0), vec3(0.0, 0.0, 1.0));\n",
        );
    }
    text.push_str(&format!(
        "\nlayout(location = 0) out vec4 shadewire_test_color;\nvoid main() {{\n{calls}    shadewire_test_color = vec4(0.0);\n}}\n"
    ));
    text
}

fn glslang(name: &str, text: &str) {
    let Some(validator) = tool("glslangValidator") else {
        eprintln!("glslangValidator not found: {name} not compiled");
        return;
    };
    let dir = std::env::temp_dir().join("shadewire-glsl-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.frag"));
    std::fs::write(&path, text).unwrap();
    let out = Command::new(validator).arg("-S").arg("frag").arg(&path).output().unwrap();
    let log = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{name}:\n{log}\n\n{}", numbered(text));
}

fn numbered(text: &str) -> String {
    text.lines().enumerate().map(|(i, l)| format!("{:4} {l}", i + 1)).collect::<Vec<_>>().join("\n")
}

#[test]
fn every_fixture_compiles_at_every_version() {
    for (name, dag) in fixtures::all() {
        let r = reflection(&dag);
        for implicit_derivatives in [true, false] {
            let options = MaterialOptions { implicit_derivatives, ..MaterialOptions::default() };
            let module = material(&dag, &r, &options);
            for version in VERSIONS {
                let tag = format!("{name}_{version:?}_{implicit_derivatives}").replace(['(', ')'], "");
                glslang(&tag, &whole_shader(&module, &r, version));
            }
        }
    }
}

#[test]
fn bindings_follow_the_version() {
    let dag = fixtures::everything();
    let r = reflection(&dag);
    let module = material(&dag, &r, &MaterialOptions::default());
    let new = print(&module, Version::Desktop(420));
    assert!(new.starts_with("#version 420 core\n"), "{new}");
    assert!(new.contains("layout(std140, binding = 0) uniform MaterialUniforms {"), "{new}");
    assert!(new.contains("layout(binding = 1) uniform sampler2D shadewire_t0;"), "{new}");
    assert!(!new.contains("shadewire_s0"), "samplers are combined: {new}");
    let old = print(&module, Version::Es(300));
    assert!(old.starts_with("#version 300 es\nprecision highp float;"), "{old}");
    assert!(old.contains("layout(std140) uniform MaterialUniforms {"), "{old}");
    assert!(old.contains("\nuniform sampler2D shadewire_t0;"), "{old}");
}

#[test]
fn host_functions_are_declared() {
    let dag = fixtures::everything();
    let r = reflection(&dag);
    let text = print(&material(&dag, &r, &MaterialOptions::default()), Version::default());
    assert!(text.contains("vec2 shadewire_host_texcoord_0();\n"), "{text}");
    assert!(text.contains("vec3 shadewire_host_transform_normal_object_to_world(vec3 v);\n"), "{text}");
    assert!(text.contains("StandardSurface shadewire_eval_surface() {"), "{text}");
    assert!(text.contains("float sample_;"), "{text}");
}

#[test]
fn closures_and_types() {
    let dag = fixtures::every_lobe();
    let r = reflection(&dag);
    let text = print(&material(&dag, &r, &MaterialOptions::default()), Version::default());
    assert!(
        text.contains(
            "ShadewireBsdf shadewire_eval_bsdf(Surface surface, vec3 to_light, vec3 to_viewer, vec3 shading_normal) {"
        ),
        "{text}"
    );
    assert!(text.contains("ShadewireBsdf c4 = shadewire_dielectric_bsdf_light(to_light, to_viewer,"), "{text}");
    assert_eq!(glsl_type(Type::Color4), "vec4");
    assert_eq!(Glsl { version: Version::default() }.ty(&Ty::Vector(3, Scalar::U32)), "uvec3");
    let _ = Item::Comment(String::new());
}
