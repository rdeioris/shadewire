//! Helpers shared by the emitter's test modules.

use shadewire::reflection::live::live_nodes;
use shadewire::{
    BindingOptions, ContextRead, Dag, HOST_PREFIX, Node, NodeId, NodeKind, Reflection, SourceInfo, Type, reflect,
};

use super::ops::Arg;
use super::types::wgsl_type;

/// `n0`, `n1`... typed as given: op arguments for emission tests.
pub fn args(types: &[Type]) -> Vec<Arg> {
    types.iter().enumerate().map(|(i, t)| (format!("n{i}"), *t)).collect()
}

pub fn add(dag: &mut Dag, kind: NodeKind, ty: Type) -> NodeId {
    dag.add(Node { kind, ty, origin: String::new() })
}

/// Reflection at group 0, first binding 0.
pub fn reflection(dag: &Dag) -> Reflection {
    let info = SourceInfo { source: "t".into(), mtlx_version: "1.39".into() };
    reflect(dag, &BindingOptions { group: 0, first_binding: 0, reserve_uniform_binding: false }, &info).unwrap()
}

/// Parses and validates a whole module (all flags, all capabilities).
pub fn naga_validates(text: &str) {
    let module = naga::front::wgsl::parse_str(text).unwrap_or_else(|e| panic!("{text}\n{}", e.emit_to_string(text)));
    let mut validator =
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
    validator.validate(&module).unwrap_or_else(|e| panic!("{text}\n{e:?}"));
}

/// Validates an emitted module: stub definitions of every host function it calls
/// (one per live context read, geomprops under their reflected names, one per transform),
/// then the module, then a fragment entry calling `shadewire_eval_surface`, so naga also checks
/// the stage and uniformity rules of `textureSample`.
/// (naga 30 hard-disables the fragment uniformity requirement,
/// `DISABLE_UNIFORMITY_REQ_FOR_FRAGMENT_STAGE`, so only the stage rules bite.)
pub fn validates(dag: &Dag, reflection: &Reflection, wgsl: &str) {
    let mut stubs: Vec<String> = Vec::new();
    let live = live_nodes(dag);
    for (i, node) in dag.nodes.iter().enumerate() {
        let NodeKind::Context(read) = &node.kind else {
            continue;
        };
        if !live[i] {
            continue;
        }
        let name = match read {
            ContextRead::GeomProp { name, .. } => reflection
                .context
                .geomprops
                .iter()
                .find(|g| g.name == *name)
                .expect("reflected geomprop")
                .function
                .clone(),
            other => other.function_name(),
        };
        let ty = wgsl_type(read.ty());
        let stub = format!("fn {name}() -> {ty} {{ return {ty}(); }}\n");
        if !stubs.contains(&stub) {
            stubs.push(stub);
        }
    }
    for t in &reflection.context.transforms {
        stubs.push(format!("fn {HOST_PREFIX}{t}(v: vec3<f32>) -> vec3<f32> {{ return v; }}\n"));
    }
    let entry = "@fragment\nfn shadewire_test_fragment() -> @location(0) vec4<f32> {\n    _ = shadewire_eval_surface();\n    return vec4<f32>(0.0);\n}\n";
    naga_validates(&format!("{}\n{wgsl}\n{entry}", stubs.concat()));
}
