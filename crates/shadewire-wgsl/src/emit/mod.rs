mod bindings;
mod coerce;
pub mod expressions;
mod module;
pub(crate) mod names;
pub mod ops;
pub mod probe;
mod reserved;
pub mod sample;
mod structs;
#[cfg(test)]
pub(crate) mod test_support;
pub(crate) mod types;
mod uniforms;

use shadewire_ir::{Dag, Reflection};

pub use probe::probe_function;

#[derive(Clone, Debug, PartialEq)]
pub struct EmitOptions {
    /// Textures are uploaded top row first, MaterialX texcoords have V up.
    pub flip_v: bool,
}

impl Default for EmitOptions {
    fn default() -> Self {
        EmitOptions { flip_v: true }
    }
}

/// Emits the WGSL module for `dag`. Reflection says which uniforms and textures are live.
pub fn emit(dag: &Dag, reflection: &Reflection, options: &EmitOptions) -> String {
    module::assemble(dag, reflection, options)
}

#[cfg(test)]
mod tests {
    use super::test_support::{add, naga_validates, reflection, validates};
    use super::*;
    use shadewire_ir::*;

    #[test]
    fn mix_of_uniform_constant_and_texcoord() {
        let mut dag = Dag::new("open_pbr_surface");
        let fg = add(&mut dag, NodeKind::Uniform { name: "tint".into(), value: Value::Color3([1.0, 0.5, 0.25]) }, Type::Color3);
        let bg = add(&mut dag, NodeKind::Const(Value::Color3([0.0, 0.0, 1.0])), Type::Color3);
        add(&mut dag, NodeKind::Const(Value::Float(9.0)), Type::Float);
        let uv = add(&mut dag, NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), Type::Vector2);
        let x = add(&mut dag, NodeKind::Op { op: Op::Extract(0), args: vec![uv] }, Type::Float);
        let mix = add(&mut dag, NodeKind::Op { op: Op::Mix, args: vec![fg, bg, x] }, Type::Color3);
        dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: mix });
        dag.surface.fields.push(SurfaceField { name: "base_weight".into(), ty: Type::Float, node: x });
        let r = reflection(&dag);
        let text = emit(&dag, &r, &EmitOptions::default());
        let expected = "\
struct OpenPbrSurface {
    base_color: vec3<f32>,
    base_weight: f32,
}

struct MaterialUniforms {
    tint: vec3<f32>,
}

@group(0) @binding(0) var<uniform> sw_u: MaterialUniforms;

fn sw_eval_surface() -> OpenPbrSurface {
    let n0: vec3<f32> = sw_u.tint;
    let n1: vec3<f32> = vec3<f32>(0.0, 0.0, 1.0);
    let n3: vec2<f32> = mx_texcoord_0();
    let n4: f32 = n3[0];
    let n5: vec3<f32> = (n1 + (n0 - n1) * n4);
    return OpenPbrSurface(n5, n4);
}
";
        assert_eq!(text, expected);
        validates(&dag, &r, &text);
    }

    #[test]
    fn everything_phase_one_emits_validates() {
        let mut dag = Dag::new("standard_surface");
        let tex = |u| TextureDesc { file: "a.png".into(), colorspace: None, address_u: u, address_v: AddressMode::Constant, filter: FilterMode::Linear };
        let t0 = dag.add_texture(tex(AddressMode::Periodic));
        let t1 = dag.add_texture(tex(AddressMode::Constant));
        let uv = add(&mut dag, NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), Type::Vector2);
        let c = add(&mut dag, NodeKind::Sample { texture: t0, uv, default: Value::Color3([0.0; 3]), grad: None }, Type::Color3);
        let a = add(&mut dag, NodeKind::Sample { texture: t1, uv, default: Value::Float(1.0), grad: None }, Type::Float);
        let flag = add(&mut dag, NodeKind::Uniform { name: "flag".into(), value: Value::Boolean(true) }, Type::Boolean);
        let n = add(&mut dag, NodeKind::Uniform { name: "n".into(), value: Value::Integer(2) }, Type::Integer);
        let prop = add(&mut dag, NodeKind::Context(ContextRead::GeomProp { name: "my-prop".into(), ty: Type::Color3 }), Type::Color3);
        let normal = add(&mut dag, NodeKind::Context(ContextRead::Normal(Space::Object)), Type::Vector3);
        let time = add(&mut dag, NodeKind::Context(ContextRead::Time), Type::Float);
        let op = Op::Transform { kind: TransformKind::Normal, from: Space::Object, to: Space::World };
        let wn = add(&mut dag, NodeKind::Op { op, args: vec![normal] }, Type::Vector3);
        let not = add(&mut dag, NodeKind::Op { op: Op::Not, args: vec![flag] }, Type::Boolean);
        let and = add(&mut dag, NodeKind::Op { op: Op::And, args: vec![not, flag] }, Type::Boolean);
        let sel = add(&mut dag, NodeKind::Op { op: Op::IfEqual, args: vec![and, flag, c, prop] }, Type::Color3);
        let mut sw_args: Vec<NodeId> = vec![sel; 10];
        sw_args.push(n);
        let sw = add(&mut dag, NodeKind::Op { op: Op::Switch, args: sw_args }, Type::Color3);
        let n2 = add(&mut dag, NodeKind::Op { op: Op::Multiply, args: vec![n, n] }, Type::Integer);
        let nf = add(&mut dag, NodeKind::Op { op: Op::Convert, args: vec![n2] }, Type::Float);
        let st = add(&mut dag, NodeKind::Op { op: Op::Smoothstep, args: vec![sw, a, nf] }, Type::Color3);
        let m = add(&mut dag, NodeKind::Op { op: Op::Modulo, args: vec![st, time] }, Type::Color3);
        let c4 = add(&mut dag, NodeKind::Op { op: Op::Convert, args: vec![m] }, Type::Color4);
        let e = add(&mut dag, NodeKind::Op { op: Op::Extract(3), args: vec![c4] }, Type::Float);
        let cross = add(&mut dag, NodeKind::Op { op: Op::CrossProduct, args: vec![wn, m] }, Type::Vector3);
        let dot = add(&mut dag, NodeKind::Op { op: Op::DotProduct, args: vec![cross, wn] }, Type::Float);
        let at = add(&mut dag, NodeKind::Op { op: Op::Atan2, args: vec![dot, e] }, Type::Float);
        let rm = add(&mut dag, NodeKind::Op { op: Op::Remap, args: vec![at, a, e, dot, nf] }, Type::Float);
        let comb = add(&mut dag, NodeKind::Op { op: Op::Combine, args: vec![rm, at, e] }, Type::Color3);
        let mixed = add(&mut dag, NodeKind::Op { op: Op::Mix, args: vec![comb, m, rm] }, Type::Color3);
        let sp = add(&mut dag, NodeKind::Op { op: Op::SafePower, args: vec![mixed, e] }, Type::Color3);
        dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: sp });
        dag.surface.fields.push(SurfaceField { name: "thin_walled".into(), ty: Type::Boolean, node: and });
        dag.surface.fields.push(SurfaceField { name: "normal".into(), ty: Type::Vector3, node: wn });
        let r = reflection(&dag);
        let text = emit(&dag, &r, &EmitOptions::default());
        assert!(text.contains("mx_geomprop_my_prop()"), "{text}");
        assert!(text.contains("return StandardSurface(n25, n10, n8);"), "{text}");
        validates(&dag, &r, &text);
    }

    /// Derivatives and the explicit-gradient lookup they feed are fragment-stage builtins.
    #[test]
    fn gradient_samples_validate() {
        let mut dag = Dag::new("standard_surface");
        let t = dag.add_texture(TextureDesc { file: "a.png".into(), colorspace: None, address_u: AddressMode::Periodic, address_v: AddressMode::Constant, filter: FilterMode::Linear });
        let uv = add(&mut dag, NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), Type::Vector2);
        let dx = add(&mut dag, NodeKind::Op { op: Op::Dpdx, args: vec![uv] }, Type::Vector2);
        let dy = add(&mut dag, NodeKind::Op { op: Op::Dpdy, args: vec![uv] }, Type::Vector2);
        let kind = NodeKind::Sample { texture: t, uv, default: Value::Color3([0.0; 3]), grad: Some([dx, dy]) };
        let c = add(&mut dag, kind, Type::Color3);
        dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: c });
        let r = reflection(&dag);
        let text = emit(&dag, &r, &EmitOptions::default());
        assert!(text.contains("textureSampleGrad(sw_t0, sw_s0, vec2<f32>(n0.x, 1.0 - n0.y), vec2<f32>(n1.x, -n1.y), vec2<f32>(n2.x, -n2.y))"), "{text}");
        validates(&dag, &r, &text);
    }

    /// Every color, noise, normal, rotate and hextile op at every result width, with the library they call.
    #[test]
    fn leaf_library_ops_emit_and_validate() {
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
        let amp = add(&mut dag, NodeKind::Uniform { name: "amp".into(), value: Value::Float(1.0) }, Type::Float);
        let oct = add(&mut dag, NodeKind::Uniform { name: "oct".into(), value: Value::Integer(3) }, Type::Integer);
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
        let r = reflection(&dag);
        let text = emit(&dag, &r, &EmitOptions::default());
        for name in crate::leaf_library::names_for(&dag) {
            assert_eq!(text.matches(crate::leaf_library::source(name).trim_end()).count(), 1, "{name}");
        }
        let entry = text.find("fn sw_eval_surface").expect("entry");
        assert!(text.find("fn mx_worley_noise_vec3_3d").expect("worley") < entry, "{text}");
        validates(&dag, &r, &text);
    }

    /// The fragment entry `validates` appends makes naga check stage rules on everything
    /// `sw_eval_surface` reaches: a compute-only builtin is refused.
    #[test]
    #[should_panic(expected = "Stage")]
    fn validates_checks_the_fragment_stage() {
        let mut dag = Dag::new("m");
        let c = add(&mut dag, NodeKind::Const(Value::Float(1.0)), Type::Float);
        dag.surface.fields.push(SurfaceField { name: "x".into(), ty: Type::Float, node: c });
        let text = "struct M { x: f32 }
fn sw_eval_surface() -> M {
    workgroupBarrier();
    return M(1.0);
}
";
        validates(&dag, &reflection(&dag), text);
    }

    #[test]
    fn naga_validates_accepts_a_plain_function() {
        naga_validates("fn f() -> f32 { return 1.0; }");
    }
}
