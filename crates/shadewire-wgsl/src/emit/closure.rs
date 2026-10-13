//! The closure tree as WGSL: `shadewire_eval_bsdf`, `shadewire_eval_albedo` and `shadewire_eval_emission`, calling
//! the lobes of `lib/closure_*.wgsl` (ports of MaterialX's reference GLSL) and combining them as MaterialX does.
//!
//! ```text
//! fn shadewire_eval_bsdf(surface: Model, to_light: vec3<f32>, to_viewer: vec3<f32>, shading_normal: vec3<f32>) -> ShadewireBsdf
//! fn shadewire_eval_albedo(surface: Model, to_viewer: vec3<f32>, shading_normal: vec3<f32>) -> vec3<f32>
//! fn shadewire_eval_emission(surface: Model, to_viewer: vec3<f32>, shading_normal: vec3<f32>) -> vec3<f32>
//! ```
//!
//! The directions are unit vectors from the shaded point; the shading normal is what a lobe without a normal
//! input uses, and what `generalized_schlick_edf` reads. The parameter names appear nowhere in the library, so
//! a translation keeps them. A lobe the library lacks scatters and emits nothing.

use shadewire::{ClosureGraph, ClosureId, ClosureNode, ClosureType, Dag};

use super::names::{ident, struct_name};

/// The closure library files, in dependency order.
pub const FILES: [&str; 4] = ["closure_base", "closure_diffuse", "closure_specular", "closure_sheen"];

/// One argument of a lobe function: the input it reads, and its value when the material has none (the
/// nodedef default; `shading_normal` is the shading normal).
type Arg = (&'static str, &'static str);

/// A uniform string input, passed as an integer: the input, each value's number, the default's number.
type Setting = (&'static str, &'static [(&'static str, i32)], i32);

/// What the library implements for a BSDF lobe: its file, its value arguments in order, its settings after them.
struct Lobe {
    file: &'static str,
    args: &'static [Arg],
    settings: &'static [Setting],
}

const SCATTER_MODE: Setting = ("scatter_mode", &[("R", 0), ("T", 1), ("RT", 2)], 0);
const SHEEN_MODE: Setting = ("mode", &[("conty_kulla", 0), ("zeltner", 1)], 0);
const GRAY: &str = "vec3<f32>(0.18)";
const WHITE: &str = "vec3<f32>(1.0)";
const ROUGHNESS: &str = "vec2<f32>(0.05)";
// A zero tangent makes the lobe pick any tangent: right for isotropic roughness.
const NO_TANGENT: &str = "vec3<f32>(0.0)";

fn bsdf_lobe(kind: &str) -> Option<Lobe> {
    let lobe = |file, args, settings| Some(Lobe { file, args, settings });
    match kind {
        "oren_nayar_diffuse_bsdf" => lobe(
            "closure_diffuse",
            &[
                ("weight", "1.0"),
                ("color", GRAY),
                ("roughness", "0.0"),
                ("normal", "shading_normal"),
                ("energy_compensation", "false"),
            ],
            &[],
        ),
        "burley_diffuse_bsdf" => lobe(
            "closure_diffuse",
            &[("weight", "1.0"), ("color", GRAY), ("roughness", "0.0"), ("normal", "shading_normal")],
            &[],
        ),
        "translucent_bsdf" => {
            lobe("closure_diffuse", &[("weight", "1.0"), ("color", WHITE), ("normal", "shading_normal")], &[])
        }
        "subsurface_bsdf" => lobe(
            "closure_diffuse",
            &[
                ("weight", "1.0"),
                ("color", GRAY),
                ("radius", WHITE),
                ("anisotropy", "0.0"),
                ("normal", "shading_normal"),
            ],
            &[],
        ),
        "dielectric_bsdf" => lobe(
            "closure_specular",
            &[
                ("weight", "1.0"),
                ("tint", WHITE),
                ("ior", "1.5"),
                ("roughness", ROUGHNESS),
                ("retroreflective", "false"),
                ("thinfilm_thickness", "0.0"),
                ("thinfilm_ior", "1.5"),
                ("normal", "shading_normal"),
                ("tangent", NO_TANGENT),
            ],
            &[SCATTER_MODE],
        ),
        "conductor_bsdf" => lobe(
            "closure_specular",
            &[
                ("weight", "1.0"),
                ("ior", "vec3<f32>(0.183, 0.421, 1.373)"),
                ("extinction", "vec3<f32>(3.424, 2.346, 1.770)"),
                ("roughness", ROUGHNESS),
                ("retroreflective", "false"),
                ("thinfilm_thickness", "0.0"),
                ("thinfilm_ior", "1.5"),
                ("normal", "shading_normal"),
                ("tangent", NO_TANGENT),
            ],
            &[],
        ),
        "generalized_schlick_bsdf" => lobe(
            "closure_specular",
            &[
                ("weight", "1.0"),
                ("color0", WHITE),
                ("color82", WHITE),
                ("color90", WHITE),
                ("exponent", "5.0"),
                ("roughness", ROUGHNESS),
                ("retroreflective", "false"),
                ("thinfilm_thickness", "0.0"),
                ("thinfilm_ior", "1.5"),
                ("normal", "shading_normal"),
                ("tangent", NO_TANGENT),
            ],
            &[SCATTER_MODE],
        ),
        "sheen_bsdf" => lobe(
            "closure_sheen",
            &[("weight", "1.0"), ("color", WHITE), ("roughness", "0.3"), ("normal", "shading_normal")],
            &[SHEEN_MODE],
        ),
        _ => None,
    }
}

/// The library files the closure tree calls, each once, in dependency order. Empty without a closure tree.
pub fn files_for(dag: &Dag) -> Vec<&'static str> {
    let Some(graph) = &dag.surface.closure else {
        return Vec::new();
    };
    let mut wanted = vec!["closure_base"];
    for node in &graph.nodes {
        if let ClosureNode::Lobe { kind, .. } = node
            && let Some(lobe) = bsdf_lobe(kind)
        {
            wanted.push(lobe.file);
        }
    }
    FILES.into_iter().filter(|f| wanted.contains(f)).collect()
}

/// The WGSL text of one closure library file. Panics on an unknown name.
pub fn source(name: &str) -> &'static str {
    match name {
        "closure_base" => include_str!("../../lib/closure_base.wgsl"),
        "closure_diffuse" => include_str!("../../lib/closure_diffuse.wgsl"),
        "closure_specular" => include_str!("../../lib/closure_specular.wgsl"),
        "closure_sheen" => include_str!("../../lib/closure_sheen.wgsl"),
        _ => panic!("unknown closure library file {name}"),
    }
}

/// The nodes `root` reads, itself included.
fn reachable(graph: &ClosureGraph, root: Option<ClosureId>) -> Vec<bool> {
    let mut seen = vec![false; graph.nodes.len()];
    let mut stack: Vec<ClosureId> = root.into_iter().collect();
    while let Some(id) = stack.pop() {
        if !std::mem::replace(&mut seen[id as usize], true) {
            stack.extend(graph.nodes[id as usize].children());
        }
    }
    seen
}

fn field(name: &str) -> String {
    format!("surface.{}", ident(name))
}

/// The arguments of a lobe call after the directions: each input's field or default, then the settings.
fn lobe_args(
    lobe: &Lobe,
    inputs: &std::collections::BTreeMap<String, String>,
    settings: &std::collections::BTreeMap<String, String>,
) -> String {
    let mut args: Vec<String> =
        lobe.args.iter().map(|(input, default)| inputs.get(*input).map_or(default.to_string(), |f| field(f))).collect();
    for (name, values, default) in lobe.settings {
        let value = settings.get(*name).and_then(|v| values.iter().find(|(k, _)| k == v)).map_or(*default, |(_, n)| *n);
        args.push(value.to_string());
    }
    args.join(", ")
}

/// Which context a BSDF walk evaluates: the response to a light, or to a uniform white environment.
#[derive(Clone, Copy)]
enum Context {
    Light,
    Albedo,
}

/// One `let` per node the BSDF root reads, children first, then the root's value.
fn bsdf_body(graph: &ClosureGraph, context: Context) -> Vec<String> {
    let Some(root) = graph.bsdf else {
        return vec!["return shadewire_bsdf_none();".into()];
    };
    let live = reachable(graph, Some(root));
    let mut lines = Vec::new();
    for (id, node) in graph.nodes.iter().enumerate() {
        if !live[id] {
            continue;
        }
        let value = match node {
            ClosureNode::Lobe { kind, ty: ClosureType::Bsdf, inputs, settings, .. } => match bsdf_lobe(kind) {
                Some(lobe) => {
                    let args = lobe_args(&lobe, inputs, settings);
                    match context {
                        Context::Light => format!("shadewire_{kind}_light(to_light, to_viewer, {args})"),
                        Context::Albedo => format!("shadewire_{kind}_albedo(to_viewer, {args})"),
                    }
                }
                None => format!("shadewire_bsdf_none();  // {kind}: not evaluated"),
            },
            ClosureNode::Lobe { kind, .. } => format!("shadewire_bsdf_none();  // {kind}: not a BSDF"),
            ClosureNode::Layer { top, base } => format!("shadewire_layer_bsdf(c{top}, c{base})"),
            ClosureNode::Mix { fg, bg, mix } => format!("shadewire_mix_bsdf(c{fg}, c{bg}, {})", field(mix)),
            ClosureNode::Add { in1, in2 } => format!("shadewire_add_bsdf(c{in1}, c{in2})"),
            ClosureNode::Multiply { in1, in2 } => format!("shadewire_multiply_bsdf(c{in1}, vec3<f32>({}))", field(in2)),
        };
        lines.push(terminate(format!("let c{id} = {value}")));
    }
    lines.push(format!("return c{root};"));
    lines
}

/// One `let` per node the EDF root reads, children first, then the root's value.
fn emission_body(graph: &ClosureGraph) -> Vec<String> {
    let Some(root) = graph.edf else {
        return vec!["return vec3<f32>(0.0);".into()];
    };
    let live = reachable(graph, Some(root));
    let mut lines = Vec::new();
    for (id, node) in graph.nodes.iter().enumerate() {
        if !live[id] {
            continue;
        }
        let value = match node {
            ClosureNode::Lobe { kind, ty: ClosureType::Edf, inputs, closures, .. } => {
                let input = |name: &str, default: &str| inputs.get(name).map_or(default.to_string(), |f| field(f));
                match kind.as_str() {
                    "uniform_edf" => format!("shadewire_uniform_edf({})", input("color", WHITE)),
                    "measured_edf" => format!("shadewire_measured_edf({})", input("color", WHITE)),
                    "conical_edf" => format!(
                        "shadewire_conical_edf(to_viewer, {}, {}, {}, {})",
                        input("color", WHITE),
                        input("normal", "shading_normal"),
                        input("inner_angle", "60.0"),
                        input("outer_angle", "0.0")
                    ),
                    "generalized_schlick_edf" => format!(
                        "shadewire_generalized_schlick_edf(to_viewer, shading_normal, {}, {}, {}, {})",
                        input("color0", WHITE),
                        input("color90", WHITE),
                        input("exponent", "5.0"),
                        closures.get("base").map_or("vec3<f32>(0.0)".to_string(), |c| format!("c{c}"))
                    ),
                    _ => format!("vec3<f32>(0.0);  // {kind}: not evaluated"),
                }
            }
            ClosureNode::Lobe { kind, .. } => format!("vec3<f32>(0.0);  // {kind}: not an EDF"),
            ClosureNode::Layer { top, base } => format!("c{top} + c{base}"),
            ClosureNode::Mix { fg, bg, mix } => format!("mix(c{bg}, c{fg}, vec3<f32>({}))", field(mix)),
            ClosureNode::Add { in1, in2 } => format!("c{in1} + c{in2}"),
            ClosureNode::Multiply { in1, in2 } => format!("c{in1} * vec3<f32>({})", field(in2)),
        };
        lines.push(terminate(format!("let c{id} = {value}")));
    }
    lines.push(format!("return c{root};"));
    lines
}

/// A statement ends with `;`, before any trailing comment.
fn terminate(line: String) -> String {
    if line.contains(";  //") { line } else { format!("{line};") }
}

fn function(signature: &str, body: Vec<String>) -> String {
    let mut out = format!("{signature} {{\n");
    for line in body {
        out.push_str(&format!("    {line}\n"));
    }
    out.push_str("}\n");
    out
}

/// The three evaluation functions over the material's closure tree. Empty without one.
pub fn closure_functions(dag: &Dag) -> String {
    let Some(graph) = &dag.surface.closure else {
        return String::new();
    };
    let model = struct_name(&dag.surface.model);
    let mut albedo = bsdf_body(graph, Context::Albedo);
    if let Some(last) = albedo.last_mut()
        && let Some(root) = last.strip_prefix("return ").and_then(|r| r.strip_suffix(';'))
    {
        *last = format!("return {root}.response;");
    }
    [
        "// The material's closure tree, evaluated as MaterialX's reference GLSL does. l: toward the light, v: toward\n\
         // the viewer, n: the shading normal. The response includes the cosine; throughput is what the closure lets\n\
         // through to a closure layered under it; the albedo is the response to a uniform white environment."
            .to_string(),
        function(
            &format!("fn shadewire_eval_bsdf(surface: {model}, to_light: vec3<f32>, to_viewer: vec3<f32>, shading_normal: vec3<f32>) -> ShadewireBsdf"),
            bsdf_body(graph, Context::Light),
        ),
        function(&format!("fn shadewire_eval_albedo(surface: {model}, to_viewer: vec3<f32>, shading_normal: vec3<f32>) -> vec3<f32>"), albedo),
        function(&format!("fn shadewire_eval_emission(surface: {model}, to_viewer: vec3<f32>, shading_normal: vec3<f32>) -> vec3<f32>"), emission_body(graph)),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::emit::EmitOptions;
    use crate::emit::test_support::{add, reflection, validates};
    use shadewire::{NodeKind, SurfaceField, Type, Value};

    const F: Type = Type::Float;
    const C: Type = Type::Color3;
    const V2: Type = Type::Vector2;
    const V3: Type = Type::Vector3;
    const B: Type = Type::Boolean;

    /// A lobe kind, its closure type, and its inputs with their types.
    type LobeInputs = (&'static str, ClosureType, &'static [(&'static str, Type)]);

    /// Every lobe kind with its inputs' types, as the pbrlib nodedefs declare them.
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

    fn surface_field(dag: &mut Dag, name: &str, ty: Type) -> String {
        let node = add(dag, NodeKind::Const(value(ty)), ty);
        dag.surface.fields.push(SurfaceField { name: name.into(), ty, node });
        name.into()
    }

    /// A tree with every lobe kind and every combination: the BSDFs layered, mixed, added and multiplied;
    /// the EDFs added and shaped by a generalized Schlick EDF.
    fn every_lobe() -> Dag {
        let mut dag = Dag::new("surface");
        let mut g = ClosureGraph::default();
        let mut bsdfs = Vec::new();
        let mut edfs = Vec::new();
        for (kind, ty, inputs) in LOBES {
            let id = g.nodes.len();
            let inputs = inputs
                .iter()
                .map(|(n, t)| (n.to_string(), surface_field(&mut dag, &format!("c{id}_{n}"), *t)))
                .collect();
            let settings = match *kind {
                "dielectric_bsdf" => BTreeMap::from([("scatter_mode".to_string(), "RT".to_string())]),
                "sheen_bsdf" => BTreeMap::from([("mode".to_string(), "zeltner".to_string())]),
                _ => BTreeMap::new(),
            };
            let node = g.add(ClosureNode::Lobe {
                kind: kind.to_string(),
                ty: *ty,
                inputs,
                settings,
                closures: BTreeMap::new(),
            });
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

    #[test]
    fn every_lobe_and_combination_validates_in_naga() {
        let dag = every_lobe();
        let r = reflection(&dag);
        let text = crate::emit(&dag, &r, &EmitOptions::default());
        for f in ["closure_base", "closure_diffuse", "closure_specular", "closure_sheen"] {
            assert!(text.contains(source(f)), "{f} missing");
        }
        assert!(text.contains("let c4 = shadewire_dielectric_bsdf_light(to_light, to_viewer, surface.c4_weight, surface.c4_tint, surface.c4_ior"), "{text}");
        assert!(text.contains("surface.c4_tangent, 2);"), "scatter_mode RT is 2: {text}");
        assert!(text.contains("let c7 = shadewire_sheen_bsdf_albedo(to_viewer, surface.c7_weight, surface.c7_color, surface.c7_roughness, surface.c7_normal, 1);"), "{text}");
        assert!(text.contains("let c8 = shadewire_bsdf_none();  // chiang_hair_bsdf: not evaluated"), "{text}");
        // An input the material leaves out takes the nodedef default.
        assert!(text.contains("shadewire_conductor_bsdf_light(to_light, to_viewer, surface.c5_weight, surface.c5_ior, surface.c5_extinction, surface.c5_roughness, false, 0.0, 1.5, surface.c5_normal, vec3<f32>(0.0))"), "{text}");
        assert!(text.contains("shadewire_generalized_schlick_edf(to_viewer, shading_normal, vec3<f32>(1.0), vec3<f32>(1.0), surface.edf_exponent, c"), "{text}");
        validates(&dag, &r, &text);
    }

    #[test]
    fn the_albedo_is_the_response_of_the_albedo_walk() {
        let text = closure_functions(&every_lobe());
        let albedo = text.split("fn shadewire_eval_albedo").nth(1).unwrap();
        let root = every_lobe().surface.closure.unwrap().bsdf.unwrap();
        assert!(albedo.contains(&format!("return c{root}.response;")), "{albedo}");
    }

    #[test]
    fn no_closure_tree_no_closure_code() {
        let mut dag = Dag::new("gltf_pbr");
        surface_field(&mut dag, "base_color", C);
        assert!(files_for(&dag).is_empty());
        assert!(closure_functions(&dag).is_empty());
    }

    #[test]
    fn an_empty_tree_scatters_and_emits_nothing() {
        let mut dag = Dag::new("surface");
        surface_field(&mut dag, "opacity", F);
        dag.surface.closure = Some(ClosureGraph::default());
        let text = closure_functions(&dag);
        assert!(text.contains("return shadewire_bsdf_none();"), "{text}");
        assert!(text.contains("return shadewire_bsdf_none().response;"), "{text}");
        assert!(text.contains("return vec3<f32>(0.0);"), "{text}");
        let r = reflection(&dag);
        validates(&dag, &r, &crate::emit(&dag, &r, &EmitOptions::default()));
    }
}
