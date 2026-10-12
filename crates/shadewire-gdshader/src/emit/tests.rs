use super::test_support::{add, godot_rules, reflection};
use super::*;
use crate::options::Transparency;
use shadewire::*;

fn shader(dag: &Dag, options: &GdshaderOptions) -> Assembled {
    let r = reflection(dag);
    let assembled = emit(dag, &r, &UniformNames::new(&r), options);
    godot_rules(&assembled.code);
    assembled
}

fn code(dag: &Dag) -> String {
    shader(dag, &GdshaderOptions::default()).code
}

#[test]
fn gltf_mix_of_uniform_constant_and_texcoord() {
    let mut dag = Dag::new("gltf_pbr");
    let fg =
        add(&mut dag, NodeKind::Uniform { name: "tint".into(), value: Value::Color3([1.0, 0.5, 0.25]) }, Type::Color3);
    let bg = add(&mut dag, NodeKind::Const(Value::Color3([0.0, 0.0, 1.0])), Type::Color3);
    add(&mut dag, NodeKind::Const(Value::Float(9.0)), Type::Float);
    let uv = add(&mut dag, NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), Type::Vector2);
    let x = add(&mut dag, NodeKind::Op { op: Op::Extract(0), args: vec![uv] }, Type::Float);
    let mix = add(&mut dag, NodeKind::Op { op: Op::Mix, args: vec![fg, bg, x] }, Type::Color3);
    let half = add(&mut dag, NodeKind::Const(Value::Float(0.5)), Type::Float);
    dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: mix });
    dag.surface.fields.push(SurfaceField { name: "roughness".into(), ty: Type::Float, node: x });
    dag.surface.fields.push(SurfaceField { name: "metallic".into(), ty: Type::Float, node: half });
    let expected = "\
shader_type spatial;
render_mode cull_back;

group_uniforms material;
uniform vec3 tint : source_color, color_conversion_disabled = vec3(1.0, 0.5, 0.25);
group_uniforms;

float shadewire_luminance(vec3 shadewire_c) {
    return dot(shadewire_c, vec3(0.2126, 0.7152, 0.0722));
}

float shadewire_specular(float shadewire_ior, float shadewire_weight, vec3 shadewire_tint) {
    float shadewire_r = (shadewire_ior - 1.0) / (shadewire_ior + 1.0);
    float shadewire_f0 = shadewire_r * shadewire_r * shadewire_weight * shadewire_luminance(shadewire_tint);
    return clamp(sqrt(shadewire_f0 / 0.16), 0.0, 1.0);
}

struct ShadewireContext {
    vec2 texcoord_0;
};

struct GltfPbr {
    vec3 base_color;
    float roughness;
    float metallic;
};

GltfPbr shadewire_eval_surface(ShadewireContext ctx) {
    vec3 n0 = tint;
    vec3 n1 = vec3(0.0, 0.0, 1.0);
    vec2 n3 = ctx.texcoord_0;
    float n4 = n3.x;
    vec3 n5 = (n1 + (n0 - n1) * n4);
    float n6 = 0.5;
    return GltfPbr(n5, n4, n6);
}

void fragment() {
    ShadewireContext ctx;
    ctx.texcoord_0 = vec2(UV.x, 1.0 - UV.y);
    GltfPbr s = shadewire_eval_surface(ctx);
    ALBEDO = s.base_color;
    METALLIC = s.metallic;
    ROUGHNESS = s.roughness;
    SPECULAR = shadewire_specular(1.5, 1.0, vec3(1.0, 1.0, 1.0));
}
";
    assert_eq!(code(&dag), expected);
}

/// A surface field with a constant value, by name.
fn constant(dag: &mut Dag, name: &str, value: Value) {
    let ty = value.ty();
    let node = add(dag, NodeKind::Const(value), ty);
    dag.surface.fields.push(SurfaceField { name: name.into(), ty, node });
}

/// A surface field read from a uniform of the same name.
fn uniform(dag: &mut Dag, name: &str, value: Value) {
    let ty = value.ty();
    let node = add(dag, NodeKind::Uniform { name: name.into(), value }, ty);
    dag.surface.fields.push(SurfaceField { name: name.into(), ty, node });
}

fn context_field(dag: &mut Dag, name: &str, read: ContextRead) {
    let ty = read.ty();
    let node = add(dag, NodeKind::Context(read), ty);
    dag.surface.fields.push(SurfaceField { name: name.into(), ty, node });
}

fn fragment_of(code: &str) -> &str {
    &code[code.find("void fragment()").expect("fragment")..]
}

#[test]
fn gltf_maps_every_godot_output_and_drops_the_rest() {
    let mut dag = Dag::new("gltf_pbr");
    uniform(&mut dag, "base_color", Value::Color3([0.8; 3]));
    uniform(&mut dag, "metallic", Value::Float(1.0));
    uniform(&mut dag, "roughness", Value::Float(0.5));
    uniform(&mut dag, "normal", Value::Vector3([0.0, 0.0, 1.0]));
    uniform(&mut dag, "occlusion", Value::Float(0.5));
    uniform(&mut dag, "specular", Value::Float(1.0));
    constant(&mut dag, "specular_color", Value::Color3([1.0, 0.5, 0.5]));
    constant(&mut dag, "ior", Value::Float(1.5));
    uniform(&mut dag, "alpha", Value::Float(0.5));
    constant(&mut dag, "alpha_mode", Value::Integer(1));
    uniform(&mut dag, "alpha_cutoff", Value::Float(0.25));
    uniform(&mut dag, "clearcoat", Value::Float(1.0));
    uniform(&mut dag, "clearcoat_roughness", Value::Float(0.1));
    uniform(&mut dag, "clearcoat_normal", Value::Vector3([1.0, 0.0, 0.0]));
    uniform(&mut dag, "anisotropy_strength", Value::Float(0.5));
    uniform(&mut dag, "anisotropy_rotation", Value::Float(1.0));
    uniform(&mut dag, "sheen_color", Value::Color3([0.1; 3]));
    uniform(&mut dag, "emissive", Value::Color3([1.0; 3]));
    uniform(&mut dag, "emissive_strength", Value::Float(2.0));
    constant(&mut dag, "transmission", Value::Float(0.5));
    constant(&mut dag, "iridescence", Value::Float(0.0));
    let options = GdshaderOptions { emission_scale: 0.5, ..GdshaderOptions::default() };
    let out = shader(&dag, &options);
    assert!(out.code.starts_with("shader_type spatial;\nrender_mode cull_back;\n"), "{}", out.code);
    assert_eq!(
        fragment_of(&out.code),
        "void fragment() {
    GltfPbr s = shadewire_eval_surface();
    ALBEDO = s.base_color;
    METALLIC = s.metallic;
    ROUGHNESS = s.roughness;
    SPECULAR = shadewire_specular(s.ior, s.specular, s.specular_color);
    NORMAL = normalize((VIEW_MATRIX * vec4(s.normal, 0.0)).xyz);
    AO = s.occlusion;
    CLEARCOAT = s.clearcoat;
    CLEARCOAT_ROUGHNESS = s.clearcoat_roughness;
    ANISOTROPY = s.anisotropy_strength;
    ANISOTROPY_FLOW = vec2(cos(s.anisotropy_rotation), sin(s.anisotropy_rotation));
    RIM = shadewire_luminance(s.sheen_color);
    EMISSION = s.emissive_strength * s.emissive * 0.5;
    ALPHA = s.alpha;
    ALPHA_SCISSOR_THRESHOLD = s.alpha_cutoff;
}
"
    );
    assert_eq!(
        out.approximations,
        [
            "godot: specular_color as its luminance",
            "godot: coat roughness remapped (Godot's coat stays glossy)",
            "godot: clearcoat_normal dropped",
            "godot: anisotropy_strength approximated (Godot's anisotropy model)",
            "godot: sheen_color approximated as rim light",
            "godot: transmission dropped",
        ]
    );
}

#[test]
fn neutral_fields_write_nothing() {
    let mut dag = Dag::new("gltf_pbr");
    constant(&mut dag, "base_color", Value::Color3([0.8; 3]));
    context_field(&mut dag, "normal", ContextRead::Normal(Space::World));
    constant(&mut dag, "occlusion", Value::Float(1.0));
    constant(&mut dag, "alpha", Value::Float(1.0));
    constant(&mut dag, "alpha_mode", Value::Integer(2));
    constant(&mut dag, "clearcoat", Value::Float(0.0));
    constant(&mut dag, "emissive", Value::Color3([0.0; 3]));
    let out = shader(&dag, &GdshaderOptions::default());
    let fragment = fragment_of(&out.code);
    for absent in ["NORMAL =", "AO", "CLEARCOAT", "EMISSION", "ALPHA", "RIM", "ANISOTROPY"] {
        assert!(!fragment.contains(absent), "{absent} in {fragment}");
    }
    assert!(out.approximations.is_empty(), "{:?}", out.approximations);
}

#[test]
fn transparency_follows_the_option_and_alpha_mode() {
    let base = |alpha: Value, mode: i32| {
        let mut dag = Dag::new("gltf_pbr");
        constant(&mut dag, "base_color", Value::Color3([0.8; 3]));
        uniform(&mut dag, "alpha", alpha);
        constant(&mut dag, "alpha_mode", Value::Integer(mode));
        dag
    };
    let with = |dag: &Dag, transparency| shader(dag, &GdshaderOptions { transparency, ..Default::default() }).code;
    let blend = with(&base(Value::Float(0.5), 2), Transparency::Auto);
    assert!(blend.contains("render_mode blend_mix, depth_draw_opaque, cull_back;"), "{blend}");
    assert!(blend.contains("    ALPHA = s.alpha;\n}"), "{blend}");
    let opaque = with(&base(Value::Float(0.5), 0), Transparency::Auto);
    assert!(!opaque.contains("ALPHA") && opaque.contains("render_mode cull_back;"), "{opaque}");
    let scissor = with(&base(Value::Float(0.5), 1), Transparency::Auto);
    assert!(scissor.contains("ALPHA_SCISSOR_THRESHOLD = 0.5;"), "{scissor}");
    let hash = with(&base(Value::Float(0.5), 2), Transparency::Hash);
    assert!(hash.contains("ALPHA = s.alpha;\n    ALPHA_HASH_SCALE = 1.0;"), "{hash}");
    let forced = with(&base(Value::Float(0.5), 2), Transparency::Opaque);
    assert!(!forced.contains("ALPHA"), "{forced}");
}

#[test]
fn standard_surface_mapping() {
    let mut dag = Dag::new("standard_surface");
    uniform(&mut dag, "base", Value::Float(1.0));
    uniform(&mut dag, "base_color", Value::Color3([0.8; 3]));
    constant(&mut dag, "diffuse_roughness", Value::Float(0.5));
    uniform(&mut dag, "specular_rotation", Value::Float(0.25));
    uniform(&mut dag, "specular_anisotropy", Value::Float(0.5));
    uniform(&mut dag, "subsurface", Value::Float(0.5));
    uniform(&mut dag, "sheen", Value::Float(0.5));
    uniform(&mut dag, "coat", Value::Float(1.0));
    constant(&mut dag, "coat_color", Value::Color3([1.0, 0.0, 0.0]));
    uniform(&mut dag, "emission", Value::Float(1.0));
    uniform(&mut dag, "emission_color", Value::Color3([1.0; 3]));
    uniform(&mut dag, "opacity", Value::Color3([0.5; 3]));
    uniform(&mut dag, "thin_walled", Value::Boolean(false));
    context_field(&mut dag, "normal", ContextRead::Normal(Space::World));
    let out = shader(&dag, &GdshaderOptions::default());
    assert!(out.code.contains("render_mode blend_mix, depth_draw_opaque, cull_disabled;"), "{}", out.code);
    // `base` is a hextile local in the WGSL library; here it stays the uniform's own name.
    assert!(out.code.contains("uniform float base = 1.0;"), "{}", out.code);
    let fragment = fragment_of(&out.code);
    for line in [
        "ALBEDO = s.base * s.base_color;",
        "METALLIC = 0.0;",
        "ROUGHNESS = 0.2;",
        "SPECULAR = shadewire_specular(1.5, 1.0, vec3(1.0, 1.0, 1.0));",
        "CLEARCOAT = s.coat;",
        "ANISOTROPY_FLOW = vec2(cos(s.specular_rotation * 6.28318530718), sin(s.specular_rotation * 6.28318530718));",
        "SSS_STRENGTH = s.subsurface;",
        "RIM = s.sheen;",
        "EMISSION = s.emission * s.emission_color;",
        "ALPHA = shadewire_luminance(s.opacity);",
    ] {
        assert!(fragment.contains(line), "{line} not in {fragment}");
    }
    assert!(!fragment.contains("NORMAL ="), "{fragment}");
    assert!(out.approximations.contains(&"godot: coat_color dropped".to_string()), "{:?}", out.approximations);
    assert!(out.approximations.contains(&"godot: diffuse_roughness dropped".to_string()), "{:?}", out.approximations);
}

#[test]
fn open_pbr_mapping() {
    let mut dag = Dag::new("open_pbr_surface");
    uniform(&mut dag, "base_weight", Value::Float(1.0));
    uniform(&mut dag, "base_color", Value::Color3([0.8; 3]));
    uniform(&mut dag, "base_metalness", Value::Float(0.0));
    uniform(&mut dag, "specular_weight", Value::Float(1.0));
    uniform(&mut dag, "specular_ior", Value::Float(1.5));
    uniform(&mut dag, "specular_roughness_anisotropy", Value::Float(0.5));
    uniform(&mut dag, "fuzz_weight", Value::Float(0.5));
    uniform(&mut dag, "emission_luminance", Value::Float(100.0));
    uniform(&mut dag, "emission_color", Value::Color3([1.0; 3]));
    constant(&mut dag, "geometry_opacity", Value::Float(1.0));
    constant(&mut dag, "geometry_thin_walled", Value::Boolean(true));
    constant(&mut dag, "transmission_weight", Value::Float(1.0));
    let out = shader(&dag, &GdshaderOptions::default());
    assert!(out.code.contains("render_mode cull_disabled;"), "{}", out.code);
    let fragment = fragment_of(&out.code);
    for line in [
        "ALBEDO = s.base_weight * s.base_color;",
        "METALLIC = s.base_metalness;",
        "SPECULAR = shadewire_specular(s.specular_ior, s.specular_weight, vec3(1.0, 1.0, 1.0));",
        "ANISOTROPY = s.specular_roughness_anisotropy;",
        "RIM = s.fuzz_weight;",
        "EMISSION = s.emission_luminance * s.emission_color;",
    ] {
        assert!(fragment.contains(line), "{line} not in {fragment}");
    }
    assert!(!fragment.contains("ALPHA") && !fragment.contains("ANISOTROPY_FLOW"), "{fragment}");
    assert!(out.approximations.contains(&"godot: transmission_weight dropped".to_string()));
}

#[test]
fn unlit_is_unshaded() {
    let mut dag = Dag::new("surface_unlit");
    uniform(&mut dag, "emission", Value::Float(1.0));
    uniform(&mut dag, "emission_color", Value::Color3([1.0, 0.0, 0.0]));
    uniform(&mut dag, "opacity", Value::Float(0.5));
    let code = code(&dag);
    assert!(code.contains("render_mode unshaded, blend_mix, depth_draw_opaque, cull_back;"), "{code}");
    assert!(fragment_of(&code).contains("ALBEDO = s.emission * s.emission_color;\n    ALPHA = s.opacity;\n"), "{code}");
    assert!(!code.contains("shadewire_specular"), "{code}");
}

#[test]
fn context_reads_fill_from_godot_built_ins() {
    let mut dag = Dag::new("m");
    let reads = [
        ContextRead::Position(Space::World),
        ContextRead::Position(Space::Object),
        ContextRead::Normal(Space::Model),
        ContextRead::Tangent(Space::World),
        ContextRead::Bitangent(Space::Object),
        ContextRead::TexCoord { index: 1, ty: Type::Vector3 },
        ContextRead::TexCoord { index: 2, ty: Type::Vector2 },
        ContextRead::GeomColor { index: 0, ty: Type::Color3 },
        ContextRead::GeomProp { name: "my-prop".into(), ty: Type::Float },
        ContextRead::ViewDirection(Space::World),
        ContextRead::ViewDirection(Space::Object),
        ContextRead::Time,
        ContextRead::Frame,
    ];
    for (i, read) in reads.into_iter().enumerate() {
        context_field(&mut dag, &format!("f{i}"), read);
    }
    let p = dag.surface.fields[0].node;
    let op = Op::Transform { kind: TransformKind::Point, from: Space::World, to: Space::Object };
    let t = add(&mut dag, NodeKind::Op { op, args: vec![p] }, Type::Vector3);
    dag.surface.fields.push(SurfaceField { name: "moved".into(), ty: Type::Vector3, node: t });
    dag.geomprop_defaults.insert("my-prop".into(), Value::Float(0.25));
    let out = shader(&dag, &GdshaderOptions::default());
    let code = &out.code;
    for line in [
        "varying vec3 shadewire_v_bitangent_object;\nvarying vec3 shadewire_v_normal_object;\nvarying vec3 shadewire_v_position_object;\n",
        "void vertex() {\n    shadewire_v_bitangent_object = BINORMAL;\n    shadewire_v_normal_object = NORMAL;\n    shadewire_v_position_object = VERTEX;\n}\n",
        "    ctx.bitangent_object = normalize(shadewire_v_bitangent_object);\n",
        "    ctx.frame = floor(TIME * 60.0);\n",
        "    ctx.geomcolor_0 = COLOR.rgb;\n",
        "    ctx.geomprop_my_prop = 0.25;\n",
        "    ctx.normal_model = normalize(shadewire_v_normal_object);\n",
        "    ctx.position_object = shadewire_v_position_object;\n",
        "    ctx.position_world = (INV_VIEW_MATRIX * vec4(VERTEX, 1.0)).xyz;\n",
        "    ctx.tangent_world = normalize((INV_VIEW_MATRIX * vec4(TANGENT, 0.0)).xyz);\n",
        "    ctx.texcoord_1 = vec3(vec2(UV2.x, 1.0 - UV2.y), 0.0);\n",
        "    ctx.texcoord_2 = vec2(0.0, 0.0);\n",
        "    ctx.time = TIME;\n",
        "    ctx.view_direction_world = normalize((INV_VIEW_MATRIX * vec4(VERTEX, 1.0)).xyz - CAMERA_POSITION_WORLD);\n",
        "    ctx.model_matrix = MODEL_MATRIX;\n    ctx.model_normal_matrix = MODEL_NORMAL_MATRIX;\n",
        "(inverse(ctx.model_matrix) * vec4(n0, 1.0)).xyz",
    ] {
        assert!(code.contains(line), "{line} not in\n{code}");
    }
    assert_eq!(
        out.approximations[..3],
        [
            "godot: frame is floor(TIME * 60.0)",
            "godot: geomprop my-prop is its default",
            "godot: texcoord_2 not available (zero)",
        ]
    );
}

#[test]
fn textures_are_named_uniforms_without_source_color() {
    let mut dag = Dag::new("gltf_pbr");
    let tex = |u, filter| TextureDesc {
        file: "a.png".into(),
        colorspace: Some("srgb_texture".into()),
        address_u: u,
        address_v: AddressMode::Constant,
        filter,
    };
    let t0 = dag.add_texture(tex(AddressMode::Mirror, FilterMode::Linear));
    let t1 = dag.add_texture(tex(AddressMode::Periodic, FilterMode::Closest));
    let uv = add(&mut dag, NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), Type::Vector2);
    let c =
        add(&mut dag, NodeKind::Sample { texture: t0, uv, default: Value::Color3([0.0; 3]), grad: None }, Type::Color3);
    let a = add(&mut dag, NodeKind::Sample { texture: t1, uv, default: Value::Float(1.0), grad: None }, Type::Float);
    dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: c });
    dag.surface.fields.push(SurfaceField { name: "roughness".into(), ty: Type::Float, node: a });
    let code = code(&dag);
    assert!(
        code.contains(
            "group_uniforms material;\nuniform sampler2D shadewire_t0 : filter_linear_mipmap_anisotropic, repeat_disable;\nuniform sampler2D shadewire_t1 : filter_nearest_mipmap, repeat_enable;\ngroup_uniforms;\n"
        ),
        "{code}"
    );
    assert!(code.contains("vec3 n1 = mix(textureGrad(shadewire_t0, vec2((1.0 - abs(mod(n0.x, 2.0) - 1.0)), 1.0 - n0.y), dFdx(vec2(n0.x, 1.0 - n0.y)), dFdy(vec2(n0.x, 1.0 - n0.y))).rgb, vec3(0.0, 0.0, 0.0), ((n0.y < 0.0 || n0.y > 1.0) ? 1.0 : 0.0));"), "{code}");
    assert!(
        code.contains("float n2 = mix(textureGrad(shadewire_t1, vec2(n0.x, 1.0 - clamp(n0.y, 0.0, 1.0))"),
        "{code}"
    );
}

/// Every op the WGSL back end's library test covers, at every result width, with the library it calls.
#[test]
fn leaf_library_ops_emit_and_keep_the_rules() {
    let mut dag = Dag::new("m");
    let ctx = |dag: &mut Dag, read: ContextRead| {
        let ty = read.ty();
        add(dag, NodeKind::Context(read), ty)
    };
    let uv = ctx(&mut dag, ContextRead::TexCoord { index: 0, ty: Type::Vector2 });
    let pos = ctx(&mut dag, ContextRead::Position(Space::Object));
    let n = ctx(&mut dag, ContextRead::Normal(Space::World));
    let t = ctx(&mut dag, ContextRead::Tangent(Space::World));
    let b = ctx(&mut dag, ContextRead::Bitangent(Space::World));
    let col = add(&mut dag, NodeKind::Const(Value::Color4([0.2, 0.4, 0.6, 1.0])), Type::Color4);
    let col3 = add(&mut dag, NodeKind::Const(Value::Color3([0.2, 0.4, 0.6])), Type::Color3);
    // Uniforms named like library parameters and locals.
    let amp = add(&mut dag, NodeKind::Uniform { name: "jit".into(), value: Value::Float(1.0) }, Type::Float);
    let oct = add(&mut dag, NodeKind::Uniform { name: "octaves".into(), value: Value::Integer(3) }, Type::Integer);
    let style = add(&mut dag, NodeKind::Const(Value::Integer(1)), Type::Integer);
    let mut fields: Vec<(Op, Vec<NodeId>, Type)> = vec![
        (Op::Luminance, vec![col3, col3], Type::Color3),
        (Op::Luminance, vec![col, col3], Type::Color4),
        (Op::RgbToHsv, vec![col], Type::Color4),
        (Op::HsvToRgb, vec![col3], Type::Color3),
        (Op::CellNoise2d, vec![uv], Type::Float),
        (Op::CellNoise3d, vec![pos], Type::Float),
        (Op::NormalMap, vec![col3, amp, n, t, b], Type::Vector3),
        (Op::Rotate2d, vec![uv, amp], Type::Vector2),
        (Op::HextileCoord(1), vec![uv, amp, uv, amp, uv, amp, uv], Type::Vector2),
        (Op::HextileGrad(2), vec![uv, uv, amp, uv, amp, uv], Type::Vector2),
        (Op::HextileWeights, vec![uv, col3, amp], Type::Vector3),
        (Op::Dpdx, vec![uv], Type::Vector2),
        (Op::Smoothstep, vec![col3, amp, amp], Type::Color3),
        (Op::Round, vec![amp], Type::Integer),
    ];
    for ty in [Type::Float, Type::Vector2, Type::Vector3, Type::Vector4] {
        fields.push((Op::Noise2d, vec![amp, amp, uv], ty));
        fields.push((Op::Noise3d, vec![amp, amp, pos], ty));
        fields.push((Op::Fractal3d, vec![amp, oct, amp, amp, pos], ty));
        if ty != Type::Vector4 {
            fields.push((Op::WorleyNoise2d, vec![uv, amp, style], ty));
            fields.push((Op::WorleyNoise3d, vec![pos, amp, style], ty));
        }
    }
    for (i, (op, args, ty)) in fields.into_iter().enumerate() {
        let node = add(&mut dag, NodeKind::Op { op, args }, ty);
        dag.surface.fields.push(SurfaceField { name: format!("f{i}"), ty, node });
    }
    let code = code(&dag);
    for name in crate::leaf_library::names_for(&dag) {
        assert_eq!(code.matches(crate::leaf_library::source(name).trim_end()).count(), 1, "{name}");
    }
    assert!(code.contains("uniform float jit_ = 1.0;\nuniform int octaves = 3;\n"), "{code}");
    assert!(code.contains("    float n7 = jit_;\n"), "{code}");
    assert!(code.contains("    vec3 n38 = shadewire_worley_noise_vec3_3d(n1, n7, n9, 0);\n"), "{code}");
}

#[test]
fn boolean_and_integer_uniforms_are_native() {
    let mut dag = Dag::new("m");
    let flag = add(&mut dag, NodeKind::Uniform { name: "flag".into(), value: Value::Boolean(true) }, Type::Boolean);
    let n = add(&mut dag, NodeKind::Uniform { name: "count".into(), value: Value::Integer(2) }, Type::Integer);
    let one = add(&mut dag, NodeKind::Const(Value::Float(1.0)), Type::Float);
    let sel = add(&mut dag, NodeKind::Op { op: Op::IfEqual, args: vec![flag, one, n, one] }, Type::Float);
    dag.surface.fields.push(SurfaceField { name: "x".into(), ty: Type::Float, node: sel });
    let code = code(&dag);
    assert!(code.contains("uniform bool flag = true;\nuniform int count = 2;\n"), "{code}");
    assert!(code.contains("float n3 = ((float(n0) == n2) ? float(n1) : n2);"), "{code}");
}

/// Every material of the vendored MaterialX examples compiles to a shader that keeps the rules,
/// with authored values as uniforms and baked in. With `SHADEWIRE_GDSHADER_OUT` set, the shaders
/// are also written there, for `godot --headless` to check.
#[test]
fn every_vendored_example_compiles() {
    use shadewire_mtlx::include::resolve_includes;
    use shadewire_mtlx::parse::parse_str;
    use shadewire_mtlx::{MtlxOptions, to_ir};
    use std::path::Path;

    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/materialx/resources/Materials/Examples");
    let out_dir = std::env::var_os("SHADEWIRE_GDSHADER_OUT").map(std::path::PathBuf::from);
    let mut count = 0;
    for folder in ["StandardSurface", "OpenPbr", "GltfPbr"] {
        let mut files: Vec<_> = std::fs::read_dir(examples.join(folder))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "mtlx"))
            .collect();
        files.sort();
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap();
            let name = path.display().to_string();
            let root = resolve_includes(parse_str(&text, Some(&name)).unwrap(), path.parent(), Some(&name)).unwrap();
            let materials: Vec<String> = root
                .children
                .iter()
                .filter(|c| c.category == "surfacematerial")
                .map(|c| c.name().to_string())
                .collect();
            for material in materials {
                for values_as_uniforms in [true, false] {
                    let options = MtlxOptions { values_as_uniforms, ..MtlxOptions::default() };
                    let ir = to_ir(&text, Some(&path), Some(&material), &options).unwrap();
                    let out = crate::compile(&ir.dag, &ir.source, &GdshaderOptions::default())
                        .unwrap_or_else(|e| panic!("{name} {material}: {e}"));
                    godot_rules(&out.code);
                    assert_eq!(out.uniform_names.len(), out.reflection.uniforms.len());
                    if let Some(dir) = &out_dir {
                        std::fs::create_dir_all(dir).unwrap();
                        let stem = path.file_stem().unwrap().to_string_lossy();
                        let suffix = if values_as_uniforms { "" } else { "_constants" };
                        std::fs::write(dir.join(format!("{stem}_{material}{suffix}.gdshader")), &out.code).unwrap();
                    }
                    count += 1;
                }
            }
        }
    }
    assert!(count > 60, "{count} shaders");
}
