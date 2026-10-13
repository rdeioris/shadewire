//! The material's code as a tree: the model struct, the uniform block and textures, the host functions it calls,
//! the support library items it needs, `shadewire_eval_surface()` and, with a closure tree, the functions
//! evaluating it.

pub mod closure;
pub mod nodes;
pub mod ops;
pub mod values;

use crate::shader::ast::*;
use crate::shader::library;
use crate::shader::names::{UniformNames, ident, struct_name};
use crate::{Dag, HOST_PREFIX, NodeId, NodeKind, Reflection};
use nodes::{UNIFORM_STRUCT, UNIFORMS, node_lets, pure, sampler_name, texture_name};
use values::{cast, node};

/// How the material's code samples and differentiates.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialOptions {
    /// Textures are uploaded top row first, MaterialX texcoords have V up: flip V when sampling.
    pub flip_v: bool,
    /// Sample with implicit derivatives and differentiate (`dpdx`), which only a fragment stage may. Off, plain
    /// samples read mip level 0 and derivatives are zero, so the code also runs in a compute stage.
    pub implicit_derivatives: bool,
}

impl Default for MaterialOptions {
    fn default() -> Self {
        MaterialOptions { flip_v: true, implicit_derivatives: true }
    }
}

/// The struct `shadewire_eval_surface` returns: one member per surface field.
pub fn model_struct(dag: &Dag) -> Struct {
    Struct {
        name: struct_name(&dag.surface.model),
        members: dag.surface.fields.iter().map(|f| (ident(&f.name), Ty::of(f.ty))).collect(),
        layout: None,
    }
}

/// The uniform struct and its resource, empty when no uniform is live. Booleans are stored as u32.
pub fn uniform_items(reflection: &Reflection) -> Vec<Item> {
    let Some(binding) = reflection.uniform_binding else {
        return Vec::new();
    };
    let names = UniformNames::new(reflection);
    let members = reflection
        .uniforms
        .iter()
        .enumerate()
        .map(|(i, u)| (names.member(i).to_string(), if u.ty == crate::Type::Boolean { Ty::U32 } else { Ty::of(u.ty) }))
        .collect();
    let offsets = reflection.uniforms.iter().map(|u| u.offset).collect();
    vec![
        Item::Struct(Struct {
            name: UNIFORM_STRUCT.into(),
            members,
            layout: Some((offsets, reflection.uniform_block_size)),
        }),
        Item::Resource(Resource {
            name: UNIFORMS.into(),
            kind: ResourceKind::Uniform(UNIFORM_STRUCT.into()),
            group: reflection.group,
            binding,
        }),
    ]
}

/// One texture and one sampler per live texture, named by the Dag texture index.
pub fn texture_items(reflection: &Reflection) -> Vec<Item> {
    let mut out = Vec::new();
    for t in &reflection.textures {
        let resource = |name, kind, binding| Item::Resource(Resource { name, kind, group: reflection.group, binding });
        out.push(resource(texture_name(t.index), ResourceKind::Texture2d, t.binding));
        out.push(resource(sampler_name(t.index), ResourceKind::Sampler, t.sampler_binding));
    }
    out
}

/// The host functions the code calls, as reflection lists them: the context reads, the geomprops and the
/// transforms.
pub fn host_functions(reflection: &Reflection) -> Vec<Function> {
    let context = &reflection.context;
    let mut out = Vec::new();
    let read = |name: String, ty| Function { name, params: Vec::new(), ret: Ty::of(ty), body: Vec::new() };
    for entry in context.varyings.iter().chain(&context.derived).chain(&context.globals) {
        out.push(read(format!("{HOST_PREFIX}{}", entry.name), entry.ty));
    }
    for g in &context.geomprops {
        out.push(read(g.function.clone(), g.ty));
    }
    for t in &context.transforms {
        out.push(Function {
            name: format!("{HOST_PREFIX}{t}"),
            params: vec![("v".into(), Ty::vec(3))],
            ret: Ty::vec(3),
            body: Vec::new(),
        });
    }
    out
}

/// `shadewire_eval_surface()`: the live nodes' lets, then the model struct in field order.
pub fn eval_surface(dag: &Dag, reflection: &Reflection, options: &MaterialOptions) -> Function {
    let model = Ty::Struct(struct_name(&dag.surface.model));
    let mut body = node_lets(dag, reflection, options);
    let fields =
        dag.surface.fields.iter().map(|f| cast(node(f.node, Ty::of(dag.node(f.node).ty)), &Ty::of(f.ty))).collect();
    body.push(Stmt::Return(Some(Expr::construct(model.clone(), fields))));
    Function { name: "shadewire_eval_surface".into(), params: Vec::new(), ret: model, body }
}

/// The whole material: structs, resources, host function declarations, library items, `shadewire_eval_surface`
/// and the closure functions, in an order every back end can print as is (declarations before use).
pub fn material(dag: &Dag, reflection: &Reflection, options: &MaterialOptions) -> Module {
    let mut items = vec![Item::Struct(model_struct(dag))];
    items.extend(uniform_items(reflection));
    items.extend(texture_items(reflection));
    items.extend(host_functions(reflection).into_iter().map(Item::Extern));
    items.extend(library::items_for(dag));
    items.push(Item::Function(eval_surface(dag, reflection, options)));
    items.extend(closure::closure_items(dag));
    Module { items }
}

/// `fn {name}() -> vec4<f32>` computing one node from constants, widened to vec4 with 0 fill: for the GPU parity
/// test.
///
/// # Panics
/// On uniforms, context reads and samples.
pub fn probe(dag: &Dag, target: NodeId, name: &str) -> Function {
    let mut needed = vec![false; dag.nodes.len()];
    let mut stack = vec![target];
    while let Some(id) = stack.pop() {
        if !needed[id.index()] {
            needed[id.index()] = true;
            stack.extend(dag.args(id));
        }
    }
    let mut body = Vec::new();
    for (i, n) in dag.nodes.iter().enumerate().filter(|(i, _)| needed[*i]) {
        if !matches!(n.kind, NodeKind::Const(_) | NodeKind::Op { .. }) {
            panic!("probe {name}: node {i} reads host data ({:?})", n.kind);
        }
        let id = NodeId(i as u32);
        body.push(Stmt::Let { name: format!("n{i}"), ty: Ty::of(n.ty), value: Some(pure(dag, id)), mutable: false });
    }
    let value = node(target, Ty::of(dag.node(target).ty));
    let zero = || Expr::f32(0.0);
    let widened = match value.ty.width() {
        Some(1) => Expr::construct(Ty::vec(4), vec![cast(value, &Ty::F32), zero(), zero(), zero()]),
        Some(2) => Expr::construct(Ty::vec(4), vec![value, zero(), zero()]),
        Some(3) => Expr::construct(Ty::vec(4), vec![value, zero()]),
        _ => value,
    };
    body.push(Stmt::Return(Some(widened)));
    Function { name: name.into(), params: Vec::new(), ret: Ty::vec(4), body }
}
