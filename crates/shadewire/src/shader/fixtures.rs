//! Materials that exercise every part of the generated code, for the back ends' tests: each back end compiles
//! these and checks its output with its own compiler.

use std::collections::BTreeMap;

use crate::*;

/// Adds a node.
pub fn add(dag: &mut Dag, kind: NodeKind, ty: Type) -> NodeId {
    dag.add(Node { kind, ty, origin: String::new() })
}

/// Reflection at group 0, first binding 0.
pub fn reflection(dag: &Dag) -> Reflection {
    let info = SourceInfo { source: "t".into(), mtlx_version: "1.39".into() };
    reflect(dag, &BindingOptions { group: 0, first_binding: 0, reserve_uniform_binding: false }, &info).unwrap()
}

/// Uniforms of every kind, two textures (one with constant addressing), host reads (a geomprop, a transform,
/// time), logic, a switch, conversions and most math.
pub fn everything() -> Dag {
    let mut dag = Dag::new("standard_surface");
    let tex = |u| TextureDesc {
        file: "a.png".into(),
        colorspace: None,
        address_u: u,
        address_v: AddressMode::Constant,
        filter: FilterMode::Linear,
    };
    let t0 = dag.add_texture(tex(AddressMode::Periodic));
    let t1 = dag.add_texture(tex(AddressMode::Constant));
    let uv = add(&mut dag, NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), Type::Vector2);
    let c =
        add(&mut dag, NodeKind::Sample { texture: t0, uv, default: Value::Color3([0.0; 3]), grad: None }, Type::Color3);
    let a = add(&mut dag, NodeKind::Sample { texture: t1, uv, default: Value::Float(1.0), grad: None }, Type::Float);
    let flag = add(&mut dag, NodeKind::Uniform { name: "flag".into(), value: Value::Boolean(true) }, Type::Boolean);
    let n = add(&mut dag, NodeKind::Uniform { name: "n".into(), value: Value::Integer(2) }, Type::Integer);
    let prop = add(
        &mut dag,
        NodeKind::Context(ContextRead::GeomProp { name: "my-prop".into(), ty: Type::Color3 }),
        Type::Color3,
    );
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
    let rounded = add(&mut dag, NodeKind::Op { op: Op::Round, args: vec![sp] }, Type::Color3);
    let fr = add(&mut dag, NodeKind::Op { op: Op::Fract, args: vec![rounded] }, Type::Color3);
    let gt = add(&mut dag, NodeKind::Op { op: Op::IfGreater, args: vec![e, rm, fr, m] }, Type::Color3);
    let eq = add(&mut dag, NodeKind::Op { op: Op::Equal, args: vec![gt, m] }, Type::Boolean);
    let or = add(&mut dag, NodeKind::Op { op: Op::Or, args: vec![eq, and] }, Type::Boolean);
    dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: gt });
    dag.surface.fields.push(SurfaceField { name: "thin_walled".into(), ty: Type::Boolean, node: or });
    dag.surface.fields.push(SurfaceField { name: "normal".into(), ty: Type::Vector3, node: wn });
    dag.surface.fields.push(SurfaceField { name: "sample".into(), ty: Type::Float, node: n2 });
    dag
}

/// Derivatives feeding an explicit-gradient lookup with constant addressing.
pub fn gradient_sample() -> Dag {
    let mut dag = Dag::new("standard_surface");
    let t = dag.add_texture(TextureDesc {
        file: "a.png".into(),
        colorspace: None,
        address_u: AddressMode::Periodic,
        address_v: AddressMode::Constant,
        filter: FilterMode::Linear,
    });
    let uv = add(&mut dag, NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), Type::Vector2);
    let dx = add(&mut dag, NodeKind::Op { op: Op::Dpdx, args: vec![uv] }, Type::Vector2);
    let dy = add(&mut dag, NodeKind::Op { op: Op::Dpdy, args: vec![uv] }, Type::Vector2);
    let kind = NodeKind::Sample { texture: t, uv, default: Value::Color3([0.0; 3]), grad: Some([dx, dy]) };
    let c = add(&mut dag, kind, Type::Color3);
    dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: c });
    dag
}

/// Every color, noise, normal, rotate and hextile op at every result width, so every support library file.
pub fn library_ops() -> Dag {
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
    dag
}

const F: Type = Type::Float;
const C: Type = Type::Color3;
const V2: Type = Type::Vector2;
const V3: Type = Type::Vector3;
const B: Type = Type::Boolean;

/// A lobe kind, its closure type, and its inputs with their types.
type LobeInputs = (&'static str, ClosureType, &'static [(&'static str, Type)]);

/// Every lobe kind with its inputs' types, as the pbrlib nodedefs declare them (some left out, so they take
/// their defaults).
const LOBES: &[LobeInputs] = &[
    (
        "oren_nayar_diffuse_bsdf",
        ClosureType::Bsdf,
        &[("weight", F), ("color", C), ("roughness", F), ("normal", V3), ("energy_compensation", B)],
    ),
    ("burley_diffuse_bsdf", ClosureType::Bsdf, &[("weight", F), ("color", C), ("roughness", F), ("normal", V3)]),
    ("translucent_bsdf", ClosureType::Bsdf, &[("weight", F), ("color", C), ("normal", V3)]),
    (
        "subsurface_bsdf",
        ClosureType::Bsdf,
        &[("weight", F), ("color", C), ("radius", C), ("anisotropy", F), ("normal", V3)],
    ),
    (
        "dielectric_bsdf",
        ClosureType::Bsdf,
        &[
            ("weight", F),
            ("tint", C),
            ("ior", F),
            ("roughness", V2),
            ("retroreflective", B),
            ("thinfilm_thickness", F),
            ("thinfilm_ior", F),
            ("normal", V3),
            ("tangent", V3),
        ],
    ),
    (
        "conductor_bsdf",
        ClosureType::Bsdf,
        &[("weight", F), ("ior", C), ("extinction", C), ("roughness", V2), ("normal", V3)],
    ),
    (
        "generalized_schlick_bsdf",
        ClosureType::Bsdf,
        &[
            ("weight", F),
            ("color0", C),
            ("color82", C),
            ("color90", C),
            ("exponent", F),
            ("roughness", V2),
            ("tangent", V3),
        ],
    ),
    ("sheen_bsdf", ClosureType::Bsdf, &[("weight", F), ("color", C), ("roughness", F), ("normal", V3)]),
    ("chiang_hair_bsdf", ClosureType::Bsdf, &[("tint_R", C)]),
    ("uniform_edf", ClosureType::Edf, &[("color", C)]),
    ("conical_edf", ClosureType::Edf, &[("color", C), ("normal", V3), ("inner_angle", F)]),
    ("measured_edf", ClosureType::Edf, &[("color", C)]),
];

fn value(ty: Type) -> Value {
    match ty {
        Type::Float => Value::Float(0.5),
        Type::Color3 => Value::Color3([0.5; 3]),
        Type::Vector2 => Value::Vector2([0.1; 2]),
        Type::Vector3 => Value::Vector3([0.0, 0.0, 1.0]),
        _ => Value::Boolean(true),
    }
}

/// A constant surface field; returns its name.
pub fn surface_field(dag: &mut Dag, name: &str, ty: Type) -> String {
    let node = add(dag, NodeKind::Const(value(ty)), ty);
    dag.surface.fields.push(SurfaceField { name: name.into(), ty, node });
    name.into()
}

/// A closure tree with every lobe kind and every combination: the BSDFs layered, mixed, added and multiplied;
/// the EDFs added, mixed and shaped by a generalized Schlick EDF. Node `c4` is the dielectric (scatter mode RT),
/// `c5` the conductor (some inputs left out), `c7` the sheen (Zeltner), `c8` hair (not evaluated).
pub fn every_lobe() -> Dag {
    let mut dag = Dag::new("surface");
    let mut g = ClosureGraph::default();
    let mut bsdfs = Vec::new();
    let mut edfs = Vec::new();
    for (kind, ty, inputs) in LOBES {
        let id = g.nodes.len();
        let inputs =
            inputs.iter().map(|(n, t)| (n.to_string(), surface_field(&mut dag, &format!("c{id}_{n}"), *t))).collect();
        let settings = match *kind {
            "dielectric_bsdf" => BTreeMap::from([("scatter_mode".to_string(), "RT".to_string())]),
            "sheen_bsdf" => BTreeMap::from([("mode".to_string(), "zeltner".to_string())]),
            _ => BTreeMap::new(),
        };
        let node =
            g.add(ClosureNode::Lobe { kind: kind.to_string(), ty: *ty, inputs, settings, closures: BTreeMap::new() });
        if *ty == ClosureType::Bsdf { bsdfs.push(node) } else { edfs.push(node) }
    }
    let mut top = bsdfs[0];
    for (k, b) in bsdfs[1..].iter().enumerate() {
        top = match k % 4 {
            0 => g.add(ClosureNode::Layer { top: *b, base: top }),
            1 => g.add(ClosureNode::Mix { fg: *b, bg: top, mix: surface_field(&mut dag, &format!("m{k}"), F) }),
            2 => g.add(ClosureNode::Add { in1: *b, in2: top }),
            _ => {
                let scaled =
                    g.add(ClosureNode::Multiply { in1: *b, in2: surface_field(&mut dag, &format!("w{k}"), C) });
                g.add(ClosureNode::Add { in1: scaled, in2: top })
            }
        };
    }
    let sum = g.add(ClosureNode::Add { in1: edfs[0], in2: edfs[1] });
    let sum = g.add(ClosureNode::Mix { fg: sum, bg: edfs[2], mix: surface_field(&mut dag, "edf_mix", F) });
    let schlick = g.add(ClosureNode::Lobe {
        kind: "generalized_schlick_edf".into(),
        ty: ClosureType::Edf,
        inputs: BTreeMap::from([("exponent".to_string(), surface_field(&mut dag, "edf_exponent", F))]),
        settings: BTreeMap::new(),
        closures: BTreeMap::from([("base".to_string(), sum)]),
    });
    g.bsdf = Some(top);
    g.edf = Some(schlick);
    dag.surface.closure = Some(g);
    dag
}

/// A surface with an empty closure tree: it scatters and emits nothing.
pub fn empty_closure() -> Dag {
    let mut dag = Dag::new("surface");
    surface_field(&mut dag, "opacity", F);
    dag.surface.closure = Some(ClosureGraph::default());
    dag
}

/// Every fixture with a name, for back ends that compile them all.
pub fn all() -> Vec<(&'static str, Dag)> {
    vec![
        ("everything", everything()),
        ("gradient_sample", gradient_sample()),
        ("library_ops", library_ops()),
        ("every_lobe", every_lobe()),
        ("empty_closure", empty_closure()),
    ]
}
