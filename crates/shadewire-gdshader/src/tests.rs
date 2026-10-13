use std::path::{Path, PathBuf};

use shadewire_mtlx::{MtlxOptions, Resolver, material_names, to_ir};

use crate::{GdshaderOptions, GdshaderOutput, LOBE_MODELS, MODELS, Transparency, compile};

fn options(models: &[&str], values_as_uniforms: bool) -> MtlxOptions {
    MtlxOptions { models: models.iter().map(|m| m.to_string()).collect(), values_as_uniforms, ..MtlxOptions::default() }
}

/// With `SHADEWIRE_GDSHADER_OUT` set, writes a generated shader there, for Godot to check
/// (`tools/godot_check/check_gdshader.py`).
fn write_out(name: &str, code: &str) {
    if let Some(dir) = std::env::var_os("SHADEWIRE_GDSHADER_OUT") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("test_{name}.gdshader")), code).unwrap();
    }
}

fn compile_text(name: &str, text: &str, models: &[&str], gd: &GdshaderOptions) -> GdshaderOutput {
    let ir = to_ir(text, None, None, &options(models, true)).unwrap_or_else(|e| panic!("{name}: {e}"));
    let out = compile(&ir.dag, &ir.source, gd).unwrap_or_else(|e| panic!("{name}: {e}"));
    write_out(name, &out.code);
    out
}

fn fragment_of(code: &str) -> &str {
    let start = code.find("void fragment()").expect("fragment");
    let end = code[start..].find("\n}\n").map_or(code.len(), |e| start + e + 3);
    &code[start..end]
}

const GLTF: &str = r#"<?xml version="1.0"?>
<materialx version="1.39">
  <gltf_pbr name="SR" type="surfaceshader">
    <input name="base_color" type="color3" value="0.8, 0.2, 0.1" />
    <input name="metallic" type="float" value="0" />
    <input name="roughness" type="float" value="0.4" />
    <input name="alpha" type="float" value="0.5" />
    <input name="alpha_mode" type="integer" value="1" />
  </gltf_pbr>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR" />
  </surfacematerial>
</materialx>"#;

#[test]
fn a_named_model_maps_onto_godots_outputs() {
    let out = compile_text("gltf", GLTF, &MODELS, &GdshaderOptions::default());
    let code = &out.code;
    // alpha_mode is a uniform here, so the transparency is a blend, not a cutout.
    assert!(code.starts_with("shader_type spatial;\nrender_mode blend_mix, depth_draw_opaque, cull_back;\n"), "{code}");
    assert!(
        code.contains("uniform vec3 base_color : source_color, color_conversion_disabled = vec3(0.8, 0.2, 0.1);"),
        "{code}"
    );
    // No uniform block, sampler or host declaration survives.
    for gone in ["MaterialUniforms", "shadewire_u", "shadewire_host_", "shadewire_s0"] {
        assert!(!code.contains(gone), "{gone} in\n{code}");
    }
    assert!(code.contains("GltfPbr shadewire_eval_surface(ShadewireContext ctx)"), "{code}");
    assert!(!code.contains("void light()"), "{code}");
    let fragment = fragment_of(code);
    for line in [
        "ALBEDO = s.base_color;",
        "ROUGHNESS = s.roughness;",
        "SPECULAR = shadewire_specular(s.ior, s.specular, s.specular_color);",
        "ALPHA = s.alpha;",
    ] {
        assert!(fragment.contains(line), "{line} not in\n{fragment}");
    }
    assert_eq!(out.uniform_names.len(), out.reflection.uniforms.len());
    // Baked in, the MASK alpha mode is a cutout.
    let ir = to_ir(GLTF, None, None, &options(&MODELS, false)).unwrap();
    let baked = compile(&ir.dag, &ir.source, &GdshaderOptions::default()).unwrap();
    assert!(fragment_of(&baked.code).contains("ALPHA_SCISSOR_THRESHOLD = s.alpha_cutoff;"), "{}", baked.code);
}

const SURFACE: &str = r#"<?xml version="1.0"?>
<materialx version="1.39">
  <oren_nayar_diffuse_bsdf name="diffuse" type="BSDF">
    <input name="color" type="color3" value="0.5, 0.5, 0.5" />
  </oren_nayar_diffuse_bsdf>
  <dielectric_bsdf name="coat" type="BSDF">
    <input name="ior" type="float" value="1.5" />
    <input name="roughness" type="vector2" value="0.3, 0.3" />
  </dielectric_bsdf>
  <layer name="layered" type="BSDF">
    <input name="top" type="BSDF" nodename="coat" />
    <input name="base" type="BSDF" nodename="diffuse" />
  </layer>
  <surface name="SR" type="surfaceshader">
    <input name="bsdf" type="BSDF" nodename="layered" />
  </surface>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR" />
  </surfacematerial>
</materialx>"#;

#[test]
fn a_closure_tree_is_lit_by_its_lobes_in_light() {
    let out = compile_text("surface", SURFACE, &MODELS, &GdshaderOptions::default());
    let code = &out.code;
    for f in ["shadewire_eval_bsdf(", "shadewire_eval_albedo(", "shadewire_eval_emission("] {
        assert!(code.contains(f), "{f} not in\n{code}");
    }
    let fragment = fragment_of(code);
    for line in [
        "ALBEDO = shadewire_eval_albedo(s, shadewire_v, shadewire_n);",
        "METALLIC = 0.0;",
        "SPECULAR = 0.0;",
        "EMISSION = shadewire_eval_emission(s, shadewire_v, shadewire_n);",
    ] {
        assert!(fragment.contains(line), "{line} not in\n{fragment}");
    }
    let light = &code[code.find("void light()").expect("light()")..];
    assert!(light.contains("Surface s = Surface("), "{light}");
    assert!(
        light.contains(
            "SPECULAR_LIGHT += shadewire_eval_bsdf(s, shadewire_l, shadewire_v, shadewire_n).response * LIGHT_COLOR * ATTENUATION;"
        ),
        "{light}"
    );
    // The colors and roughness are uniforms: light() reads them, only the normals travel.
    assert!(light.contains("color"), "{light}");
    assert!(out.reflection.approximations.contains(&crate::lobes::AMBIENT.to_string()));
}

/// Only what varies per fragment travels to light(), each DAG value once.
#[test]
fn light_reads_constants_and_uniforms_directly() {
    let ir = to_ir(SURFACE, None, None, &options(&MODELS, false)).unwrap();
    let out = compile(&ir.dag, &ir.source, &GdshaderOptions::default()).unwrap();
    let varyings = out.code.matches("varying vec4 shadewire_l").count();
    // The normals of both lobes (one node) and the dielectric's tangent: six floats, two vec4s.
    assert_eq!(varyings, 2, "{}", out.code);
    let ir = to_ir(SURFACE, None, None, &options(&MODELS, true)).unwrap();
    let uniforms = compile(&ir.dag, &ir.source, &GdshaderOptions::default()).unwrap();
    assert_eq!(uniforms.code.matches("varying vec4 shadewire_l").count(), 2, "{}", uniforms.code);
}

#[test]
fn lobe_models_open_named_models_into_closure_trees() {
    let mapped = compile_text("gltf_mapped", GLTF, &MODELS, &GdshaderOptions::default());
    let lobes = compile_text("gltf_lobes", GLTF, &LOBE_MODELS, &GdshaderOptions::default());
    assert_eq!(mapped.reflection.model, "gltf_pbr");
    assert_eq!(lobes.reflection.model, "surface");
    assert!(lobes.code.contains("void light()"), "{}", lobes.code);
    // The glTF cutout opacity still drives the alpha.
    assert!(fragment_of(&lobes.code).contains("ALPHA = s.opacity;"), "{}", lobes.code);
}

#[test]
fn transparency_option_and_emission_scale() {
    let gd = GdshaderOptions { transparency: Transparency::Hash, emission_scale: 2.0, ..GdshaderOptions::default() };
    let out = compile_text("gltf_hash", GLTF, &MODELS, &gd);
    assert!(fragment_of(&out.code).contains("ALPHA_HASH_SCALE = 1.0;"), "{}", out.code);
    let opaque = GdshaderOptions { transparency: Transparency::Opaque, ..GdshaderOptions::default() };
    let out = compile_text("gltf_opaque", GLTF, &MODELS, &opaque);
    assert!(!fragment_of(&out.code).contains("ALPHA"), "{}", out.code);
}

#[test]
fn boolean_uniforms_are_native() {
    let text = r#"<?xml version="1.0"?>
<materialx version="1.39">
  <standard_surface name="SR" type="surfaceshader">
    <input name="thin_walled" type="boolean" value="true" />
  </standard_surface>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR" />
  </surfacematerial>
</materialx>"#;
    let out = compile_text("bool", text, &MODELS, &GdshaderOptions::default());
    assert!(out.code.contains("uniform bool thin_walled = true;"), "{}", out.code);
    assert!(!out.code.contains("!= 0u"), "{}", out.code);
    assert!(out.code.contains("render_mode cull_disabled;"), "{}", out.code);
}

#[test]
fn transforms_read_the_context_matrices() {
    let text = r#"<?xml version="1.0"?>
<materialx version="1.39">
  <nodegraph name="NG">
    <position name="p" type="vector3"><input name="space" type="string" value="world" /></position>
    <transformpoint name="t" type="vector3">
      <input name="in" type="vector3" nodename="p" />
      <input name="fromspace" type="string" value="world" />
      <input name="tospace" type="string" value="object" />
    </transformpoint>
    <convert name="c" type="color3"><input name="in" type="vector3" nodename="t" /></convert>
    <output name="out" type="color3" nodename="c" />
  </nodegraph>
  <standard_surface name="SR" type="surfaceshader">
    <input name="base_color" type="color3" nodegraph="NG" output="out" />
  </standard_surface>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR" />
  </surfacematerial>
</materialx>"#;
    let out = compile_text("transform", text, &MODELS, &GdshaderOptions::default());
    assert!(out.code.contains("inverse(ctx.model_matrix)"), "{}", out.code);
    assert!(fragment_of(&out.code).contains("ctx.model_matrix = MODEL_MATRIX;"), "{}", out.code);
}

#[test]
fn mirror_is_emulated_and_textures_carry_their_hints() {
    let text = r#"<?xml version="1.0"?>
<materialx version="1.39">
  <nodegraph name="NG">
    <image name="i" type="color3">
      <input name="file" type="filename" value="a.png" />
      <input name="uaddressmode" type="string" value="mirror" />
      <input name="vaddressmode" type="string" value="clamp" />
    </image>
    <output name="out" type="color3" nodename="i" />
  </nodegraph>
  <standard_surface name="SR" type="surfaceshader">
    <input name="base_color" type="color3" nodegraph="NG" output="out" />
  </standard_surface>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR" />
  </surfacematerial>
</materialx>"#;
    let out = compile_text("mirror", text, &MODELS, &GdshaderOptions::default());
    assert!(
        out.code.contains("uniform sampler2D shadewire_t0 : filter_linear_mipmap_anisotropic, repeat_disable;"),
        "{}",
        out.code
    );
    assert!(out.code.contains("(1.0 - abs(mod("), "{}", out.code);
    assert!(out.code.contains("textureGrad(shadewire_t0, vec2("), "{}", out.code);
}

#[test]
fn uniforms_keep_clear_of_declared_names() {
    let text = r#"<?xml version="1.0"?>
<materialx version="1.39">
  <nodegraph name="NG">
    <input name="base" type="float" value="0.5" />
    <input name="uniform" type="float" value="0.5" />
    <input name="n3" type="float" value="0.5" />
    <multiply name="m" type="float">
      <input name="in1" type="float" interfacename="base" />
      <input name="in2" type="float" interfacename="uniform" />
    </multiply>
    <add name="a" type="float">
      <input name="in1" type="float" nodename="m" />
      <input name="in2" type="float" interfacename="n3" />
    </add>
    <output name="out" type="float" nodename="a" />
  </nodegraph>
  <standard_surface name="SR" type="surfaceshader">
    <input name="specular_roughness" type="float" nodegraph="NG" output="out" />
  </standard_surface>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR" />
  </surfacematerial>
</materialx>"#;
    let out = compile_text("names", text, &MODELS, &GdshaderOptions::default());
    for name in &out.uniform_names {
        assert!(out.code.contains(&format!(" {name} ")) || out.code.contains(&format!(" {name};")), "{name}");
    }
    assert!(out.uniform_names.iter().any(|n| n == "uniform_"), "{:?}", out.uniform_names);
    assert!(out.uniform_names.iter().any(|n| n == "n3_"), "{:?}", out.uniform_names);
}

#[test]
fn models_are_ones_the_front_end_knows() {
    for m in MODELS.iter().chain(&LOBE_MODELS) {
        let known = shadewire_mtlx::surface::KNOWN_MODELS.contains(m) || *m == shadewire_mtlx::surface::CLOSURE_MODEL;
        assert!(known, "{m}");
    }
}

/// Every material of the vendored examples compiles both ways, values as uniforms and baked in, within the limits
/// a runtime host sets for untrusted documents.
#[test]
fn every_vendored_example_compiles_mapped_and_with_lobes() {
    let limited =
        GdshaderOptions { max_textures: Some(16), max_loop_iterations: Some(16), ..GdshaderOptions::default() };
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/materialx/resources/Materials/Examples");
    let mut count = 0;
    let mut lobes = 0;
    for folder in ["StandardSurface", "OpenPbr", "GltfPbr", "UsdPreviewSurface", "DisneyPrincipled"] {
        let mut files: Vec<_> = std::fs::read_dir(examples.join(folder))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "mtlx"))
            .collect();
        files.sort();
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap();
            for material in material_names(&text, path.to_str(), &Resolver::default()).unwrap() {
                for models in [&MODELS[..], &LOBE_MODELS[..]] {
                    for values_as_uniforms in [true, false] {
                        let ir = to_ir(&text, path.to_str(), Some(&material), &options(models, values_as_uniforms))
                            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                        let out = compile(&ir.dag, &ir.source, &limited)
                            .unwrap_or_else(|e| panic!("{} {material}: {e}", path.display()));
                        assert_eq!(out.uniform_names.len(), out.reflection.uniforms.len());
                        // Godot allows 32 varyings.
                        assert!(out.code.matches("\nvarying ").count() <= 28, "{}", path.display());
                        if out.code.contains("void light()") {
                            lobes += 1;
                        }
                        count += 1;
                    }
                }
            }
        }
    }
    assert!(count > 200, "{count} shaders");
    assert!(lobes > 100, "{lobes} with lobes");
}

const FRACTAL: &str = r#"<?xml version="1.0"?>
<materialx version="1.39">
  <nodegraph name="NG">
    <input name="octaves" type="integer" value="3" />
    <fractal3d name="f" type="float">
      <input name="octaves" type="integer" interfacename="octaves" />
    </fractal3d>
    <output name="out" type="float" nodename="f" />
  </nodegraph>
  <standard_surface name="SR" type="surfaceshader">
    <input name="base" type="float" nodegraph="NG" output="out" />
  </standard_surface>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR" />
  </surfacematerial>
</materialx>"#;

#[test]
fn fractal_octaves_from_a_uniform_are_clamped() {
    let unlimited = compile_text("fractal", FRACTAL, &MODELS, &GdshaderOptions::default());
    assert!(unlimited.code.contains("i < octaves;"), "{}", unlimited.code);
    let options = GdshaderOptions { max_loop_iterations: Some(12), ..GdshaderOptions::default() };
    let limited = compile_text("fractal_limited", FRACTAL, &MODELS, &options);
    assert!(limited.code.contains("i < min(octaves, 12);"), "{}", limited.code);
    assert!(!limited.code.contains("i < octaves;"), "{}", limited.code);
}

#[test]
fn textures_over_the_limit_fail() {
    let mut text = String::from(r#"<materialx version="1.39"><nodegraph name="NG">"#);
    for i in 0..3 {
        text.push_str(&format!(
            r#"<image name="i{i}" type="float"><input name="file" type="filename" value="t{i}.png" /></image>"#
        ));
    }
    text.push_str(
        r#"<add name="a" type="float"><input name="in1" type="float" nodename="i0" /><input name="in2" type="float" nodename="i1" /></add>
      <add name="b" type="float"><input name="in1" type="float" nodename="a" /><input name="in2" type="float" nodename="i2" /></add>
      <output name="out" type="float" nodename="b" /></nodegraph>
      <standard_surface name="SR" type="surfaceshader"><input name="base" type="float" nodegraph="NG" output="out" /></standard_surface>
      <surfacematerial name="M" type="material"><input name="surfaceshader" type="surfaceshader" nodename="SR" /></surfacematerial>
    </materialx>"#,
    );
    let ir = to_ir(&text, None, None, &options(&MODELS, true)).unwrap();
    let at =
        |max| compile(&ir.dag, &ir.source, &GdshaderOptions { max_textures: Some(max), ..GdshaderOptions::default() });
    assert_eq!(at(3).unwrap().reflection.textures.len(), 3);
    assert_eq!(at(2).unwrap_err().to_string(), "the material samples 3 textures, more than 2 (max_textures)");
}
