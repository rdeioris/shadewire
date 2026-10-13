//! The GLSL back end over the shared fixtures: each library, completed with stub host functions and a `main`,
//! compiles in glslangValidator at every version.

use shadewire::shader::fixtures::{self, reflection};
use shadewire::shader::material::{MaterialOptions, material};
use shadewire::shader::{Item, Ty};

use super::*;

const VERSIONS: [Version; 5] =
    [Version::Desktop(330), Version::Desktop(420), Version::Desktop(450), Version::Es(300), Version::Es(310)];

#[test]
fn every_fixture_compiles_at_every_version() {
    for (name, dag) in fixtures::all() {
        let r = reflection(&dag);
        for implicit_derivatives in [true, false] {
            let options = MaterialOptions { implicit_derivatives, ..MaterialOptions::default() };
            let module = material(&dag, &r, &options);
            for version in VERSIONS {
                let tag = format!("{name}_{version:?}_{implicit_derivatives}").replace(['(', ')'], "");
                let compiled = crate::validate::check(&tag, &print(&module, version), &r, version);
                if !compiled.unwrap_or_else(|e| panic!("{tag}:\n{e}")) {
                    eprintln!("glslangValidator not found: {tag} not compiled");
                }
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

#[test]
fn compile_refuses_versions_glsl_lacks() {
    let dag = fixtures::everything();
    let source = SourceInfo { source: "test".into(), mtlx_version: "1.39".into() };
    for version in [Version::Desktop(300), Version::Desktop(470), Version::Es(330), Version::Es(100)] {
        let err = compile(&dag, &source, &CompileOptions::default(), version).unwrap_err();
        assert!(matches!(err, Error::UnsupportedTarget(_)), "{version:?}: {err}");
    }
    assert_eq!(Version::new(false, None), Version::Desktop(420));
    assert_eq!(Version::new(true, None), Version::Es(300));
    assert_eq!(Version::new(true, Some(310)).check(), Ok(Version::Es(310)));
}
