//! The closure tree as code: `shadewire_eval_bsdf`, `shadewire_eval_albedo` and `shadewire_eval_emission`, calling
//! the lobes of the closure library files (ports of MaterialX's reference GLSL) and combining them as MaterialX
//! does.
//!
//! ```text
//! ShadewireBsdf shadewire_eval_bsdf(Model surface, vec3 to_light, vec3 to_viewer, vec3 shading_normal)
//! vec3 shadewire_eval_albedo(Model surface, vec3 to_viewer, vec3 shading_normal)
//! vec3 shadewire_eval_emission(Model surface, vec3 to_viewer, vec3 shading_normal)
//! ```
//!
//! The directions are unit vectors from the shaded point; the shading normal is what a lobe without a normal
//! input uses, and what `generalized_schlick_edf` reads. A lobe the library lacks scatters and emits nothing.

use std::collections::BTreeMap;

use super::values::cast;
use crate::shader::ast::*;
use crate::shader::names::{ident, struct_name};
use crate::{ClosureGraph, ClosureId, ClosureNode, ClosureType, Dag};

pub const BSDF_STRUCT: &str = "ShadewireBsdf";
const SURFACE: &str = "surface";
const TO_LIGHT: &str = "to_light";
const TO_VIEWER: &str = "to_viewer";
const SHADING_NORMAL: &str = "shading_normal";

/// A lobe argument's value when the material has none: the nodedef default.
#[derive(Clone, Copy)]
enum Fallback {
    F(f32),
    Bool(bool),
    Gray,
    White,
    Splat2(f32),
    Rgb(f32, f32, f32),
    ShadingNormal,
    /// A zero tangent makes the lobe pick any tangent: right for isotropic roughness.
    NoTangent,
}

impl Fallback {
    fn expr(self) -> Expr {
        let splat = |n: u8, x: f32| Expr::construct(Ty::vec(n), vec![Expr::f32(x)]);
        match self {
            Fallback::F(x) => Expr::f32(x),
            Fallback::Bool(b) => Expr::bool(b),
            Fallback::Gray => splat(3, 0.18),
            Fallback::White => splat(3, 1.0),
            Fallback::Splat2(x) => splat(2, x),
            Fallback::Rgb(r, g, b) => Expr::construct(Ty::vec(3), vec![Expr::f32(r), Expr::f32(g), Expr::f32(b)]),
            Fallback::ShadingNormal => Expr::name(SHADING_NORMAL, Ty::vec(3)),
            Fallback::NoTangent => splat(3, 0.0),
        }
    }
}

type Arg = (&'static str, Fallback);

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
const ROUGHNESS: Fallback = Fallback::Splat2(0.05);

fn bsdf_lobe(kind: &str) -> Option<Lobe> {
    use Fallback::*;
    let lobe = |file, args, settings| Some(Lobe { file, args, settings });
    match kind {
        "oren_nayar_diffuse_bsdf" => lobe(
            "closure_diffuse",
            &[
                ("weight", F(1.0)),
                ("color", Gray),
                ("roughness", F(0.0)),
                ("normal", ShadingNormal),
                ("energy_compensation", Bool(false)),
            ],
            &[],
        ),
        "burley_diffuse_bsdf" => lobe(
            "closure_diffuse",
            &[("weight", F(1.0)), ("color", Gray), ("roughness", F(0.0)), ("normal", ShadingNormal)],
            &[],
        ),
        "translucent_bsdf" => {
            lobe("closure_diffuse", &[("weight", F(1.0)), ("color", White), ("normal", ShadingNormal)], &[])
        }
        "subsurface_bsdf" => lobe(
            "closure_diffuse",
            &[
                ("weight", F(1.0)),
                ("color", Gray),
                ("radius", White),
                ("anisotropy", F(0.0)),
                ("normal", ShadingNormal),
            ],
            &[],
        ),
        "dielectric_bsdf" => lobe(
            "closure_specular",
            &[
                ("weight", F(1.0)),
                ("tint", White),
                ("ior", F(1.5)),
                ("roughness", ROUGHNESS),
                ("retroreflective", Bool(false)),
                ("thinfilm_thickness", F(0.0)),
                ("thinfilm_ior", F(1.5)),
                ("normal", ShadingNormal),
                ("tangent", NoTangent),
            ],
            &[SCATTER_MODE],
        ),
        "conductor_bsdf" => lobe(
            "closure_specular",
            &[
                ("weight", F(1.0)),
                ("ior", Rgb(0.183, 0.421, 1.373)),
                ("extinction", Rgb(3.424, 2.346, 1.770)),
                ("roughness", ROUGHNESS),
                ("retroreflective", Bool(false)),
                ("thinfilm_thickness", F(0.0)),
                ("thinfilm_ior", F(1.5)),
                ("normal", ShadingNormal),
                ("tangent", NoTangent),
            ],
            &[],
        ),
        "generalized_schlick_bsdf" => lobe(
            "closure_specular",
            &[
                ("weight", F(1.0)),
                ("color0", White),
                ("color82", White),
                ("color90", White),
                ("exponent", F(5.0)),
                ("roughness", ROUGHNESS),
                ("retroreflective", Bool(false)),
                ("thinfilm_thickness", F(0.0)),
                ("thinfilm_ior", F(1.5)),
                ("normal", ShadingNormal),
                ("tangent", NoTangent),
            ],
            &[SCATTER_MODE],
        ),
        "sheen_bsdf" => lobe(
            "closure_sheen",
            &[("weight", F(1.0)), ("color", White), ("roughness", F(0.3)), ("normal", ShadingNormal)],
            &[SHEEN_MODE],
        ),
        _ => None,
    }
}

/// The library file implementing a BSDF lobe, if any.
pub fn lobe_file(kind: &str) -> Option<&'static str> {
    bsdf_lobe(kind).map(|l| l.file)
}

/// The nodes `root` reads, itself included.
fn reachable(graph: &ClosureGraph, root: ClosureId) -> Vec<bool> {
    let mut seen = vec![false; graph.nodes.len()];
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if !std::mem::replace(&mut seen[id as usize], true) {
            stack.extend(graph.nodes[id as usize].children());
        }
    }
    seen
}

struct Walk<'a> {
    fields: BTreeMap<&'a str, Ty>,
    model: Ty,
}

impl Walk<'_> {
    /// A surface field: `surface.{field}`.
    fn field(&self, name: &str) -> Expr {
        let ty = self.fields.get(name).cloned().unwrap_or_else(|| panic!("closure reads unknown field {name}"));
        Expr::name(SURFACE, self.model.clone()).member(ident(name), ty)
    }

    fn input(&self, inputs: &BTreeMap<String, String>, name: &str, default: Fallback) -> Expr {
        inputs.get(name).map_or_else(|| default.expr(), |f| self.field(f))
    }

    fn lobe_args(
        &self,
        lobe: &Lobe,
        inputs: &BTreeMap<String, String>,
        settings: &BTreeMap<String, String>,
    ) -> Vec<Expr> {
        let mut args: Vec<Expr> =
            lobe.args.iter().map(|(input, default)| self.input(inputs, input, *default)).collect();
        for (name, values, default) in lobe.settings {
            let value =
                settings.get(*name).and_then(|v| values.iter().find(|(k, _)| k == v)).map_or(*default, |(_, n)| *n);
            args.push(Expr::i32(value));
        }
        args
    }

    /// A field as a color: a float weight splats.
    fn color_field(&self, name: &str) -> Expr {
        cast(self.field(name), &Ty::vec(3))
    }
}

/// Which context a BSDF walk evaluates: the response to a light, or to a uniform white environment.
#[derive(Clone, Copy)]
enum Context {
    Light,
    Albedo,
}

fn bsdf() -> Ty {
    Ty::Struct(BSDF_STRUCT.into())
}

fn var(id: usize, ty: Ty) -> Expr {
    Expr::name(format!("c{id}"), ty)
}

fn let_(id: usize, value: Expr, comment: Option<String>) -> Vec<Stmt> {
    let mut out = vec![Stmt::Let { name: format!("c{id}"), ty: value.ty.clone(), value: Some(value), mutable: false }];
    if let Some(text) = comment {
        out.push(Stmt::Comment { text, trailing: true });
    }
    out
}

fn none() -> Expr {
    Expr::call("shadewire_bsdf_none", Vec::new(), bsdf())
}

/// One `let` per node the BSDF root reads, children first, then the root's value.
fn bsdf_body(walk: &Walk, graph: &ClosureGraph, context: Context) -> Vec<Stmt> {
    let Some(root) = graph.bsdf else {
        let none = none();
        return vec![Stmt::Return(Some(match context {
            Context::Light => none,
            Context::Albedo => none.member("response", Ty::vec(3)),
        }))];
    };
    let live = reachable(graph, root);
    let mut body = Vec::new();
    for (id, node) in graph.nodes.iter().enumerate().filter(|(id, _)| live[*id]) {
        let c = |i: ClosureId| var(i as usize, bsdf());
        let (value, comment) = match node {
            ClosureNode::Lobe { kind, ty: ClosureType::Bsdf, inputs, settings, .. } => match bsdf_lobe(kind) {
                Some(lobe) => {
                    let mut args = walk.lobe_args(&lobe, inputs, settings);
                    let (suffix, directions) = match context {
                        Context::Light => ("light", vec![TO_LIGHT, TO_VIEWER]),
                        Context::Albedo => ("albedo", vec![TO_VIEWER]),
                    };
                    let mut call_args: Vec<Expr> = directions.into_iter().map(|d| Expr::name(d, Ty::vec(3))).collect();
                    call_args.append(&mut args);
                    (Expr::call(format!("shadewire_{kind}_{suffix}"), call_args, bsdf()), None)
                }
                None => (none(), Some(format!("{kind}: not evaluated"))),
            },
            ClosureNode::Lobe { kind, .. } => (none(), Some(format!("{kind}: not a BSDF"))),
            ClosureNode::Layer { top, base } => {
                (Expr::call("shadewire_layer_bsdf", vec![c(*top), c(*base)], bsdf()), None)
            }
            ClosureNode::Mix { fg, bg, mix } => {
                (Expr::call("shadewire_mix_bsdf", vec![c(*fg), c(*bg), walk.field(mix)], bsdf()), None)
            }
            ClosureNode::Add { in1, in2 } => (Expr::call("shadewire_add_bsdf", vec![c(*in1), c(*in2)], bsdf()), None),
            ClosureNode::Multiply { in1, in2 } => {
                (Expr::call("shadewire_multiply_bsdf", vec![c(*in1), walk.color_field(in2)], bsdf()), None)
            }
        };
        body.extend(let_(id, value, comment));
    }
    let root = var(root as usize, bsdf());
    body.push(Stmt::Return(Some(match context {
        Context::Light => root,
        Context::Albedo => root.member("response", Ty::vec(3)),
    })));
    body
}

/// One `let` per node the EDF root reads, children first, then the root's value.
fn emission_body(walk: &Walk, graph: &ClosureGraph) -> Vec<Stmt> {
    let zero = || Expr::construct(Ty::vec(3), vec![Expr::f32(0.0)]);
    let Some(root) = graph.edf else {
        return vec![Stmt::Return(Some(zero()))];
    };
    let live = reachable(graph, root);
    let mut body = Vec::new();
    let v3 = Ty::vec(3);
    for (id, node) in graph.nodes.iter().enumerate().filter(|(id, _)| live[*id]) {
        let c = |i: ClosureId| var(i as usize, v3.clone());
        let viewer = || Expr::name(TO_VIEWER, v3.clone());
        let (value, comment) = match node {
            ClosureNode::Lobe { kind, ty: ClosureType::Edf, inputs, closures, .. } => {
                let input = |name: &str, default: Fallback| walk.input(inputs, name, default);
                let call = |args: Vec<Expr>| Expr::call(format!("shadewire_{kind}"), args, v3.clone());
                match kind.as_str() {
                    "uniform_edf" | "measured_edf" => (call(vec![input("color", Fallback::White)]), None),
                    "conical_edf" => (
                        call(vec![
                            viewer(),
                            input("color", Fallback::White),
                            input("normal", Fallback::ShadingNormal),
                            input("inner_angle", Fallback::F(60.0)),
                            input("outer_angle", Fallback::F(0.0)),
                        ]),
                        None,
                    ),
                    "generalized_schlick_edf" => (
                        call(vec![
                            viewer(),
                            Expr::name(SHADING_NORMAL, v3.clone()),
                            input("color0", Fallback::White),
                            input("color90", Fallback::White),
                            input("exponent", Fallback::F(5.0)),
                            closures.get("base").map_or_else(zero, |b| c(*b)),
                        ]),
                        None,
                    ),
                    _ => (zero(), Some(format!("{kind}: not evaluated"))),
                }
            }
            ClosureNode::Lobe { kind, .. } => (zero(), Some(format!("{kind}: not an EDF"))),
            ClosureNode::Layer { top, base } => (Expr::binary(BinaryOp::Add, c(*top), c(*base), v3.clone()), None),
            ClosureNode::Mix { fg, bg, mix } => {
                let m = cast(walk.field(mix), &v3);
                (Expr::builtin(Builtin::Mix, vec![c(*bg), c(*fg), m], v3.clone()), None)
            }
            ClosureNode::Add { in1, in2 } => (Expr::binary(BinaryOp::Add, c(*in1), c(*in2), v3.clone()), None),
            ClosureNode::Multiply { in1, in2 } => {
                (Expr::binary(BinaryOp::Mul, c(*in1), walk.color_field(in2), v3.clone()), None)
            }
        };
        body.extend(let_(id, value, comment));
    }
    body.push(Stmt::Return(Some(var(root as usize, v3))));
    body
}

/// The three evaluation functions over the material's closure tree, after a comment; empty without one.
pub fn closure_items(dag: &Dag) -> Vec<Item> {
    let Some(graph) = &dag.surface.closure else {
        return Vec::new();
    };
    let model = Ty::Struct(struct_name(&dag.surface.model));
    let walk = Walk {
        fields: dag.surface.fields.iter().map(|f| (f.name.as_str(), Ty::of(f.ty))).collect(),
        model: model.clone(),
    };
    let v3 = || Ty::vec(3);
    let param = |name: &str, ty: Ty| (name.to_string(), ty);
    vec![
        Item::Comment(
            "The material's closure tree, evaluated as MaterialX's reference GLSL does. The response includes the"
                .into(),
        ),
        Item::Comment(
            "cosine; throughput is what the closure lets through to a closure layered under it; the albedo is the"
                .into(),
        ),
        Item::Comment("response to a uniform white environment.".into()),
        Item::Function(Function {
            name: "shadewire_eval_bsdf".into(),
            params: vec![
                param(SURFACE, model.clone()),
                param(TO_LIGHT, v3()),
                param(TO_VIEWER, v3()),
                param(SHADING_NORMAL, v3()),
            ],
            ret: bsdf(),
            body: bsdf_body(&walk, graph, Context::Light),
        }),
        Item::Function(Function {
            name: "shadewire_eval_albedo".into(),
            params: vec![param(SURFACE, model.clone()), param(TO_VIEWER, v3()), param(SHADING_NORMAL, v3())],
            ret: v3(),
            body: bsdf_body(&walk, graph, Context::Albedo),
        }),
        Item::Function(Function {
            name: "shadewire_eval_emission".into(),
            params: vec![param(SURFACE, model), param(TO_VIEWER, v3()), param(SHADING_NORMAL, v3())],
            ret: v3(),
            body: emission_body(&walk, graph),
        }),
    ]
}
