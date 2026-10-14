//! The closure tree as code: `shadewire_eval_bsdf`, `shadewire_eval_albedo` and `shadewire_eval_emission`, calling
//! the lobes of the closure library files (ports of MaterialX's reference GLSL) and combining them as MaterialX
//! does.
//!
//! ```text
//! ShadewireBsdf shadewire_eval_bsdf(Model surface, vec3 to_light, vec3 to_viewer, vec3 shading_normal)
//! vec3 shadewire_eval_albedo(Model surface, vec3 to_viewer, vec3 shading_normal)
//! vec3 shadewire_eval_emission(Model surface, vec3 to_viewer, vec3 shading_normal)
//! ShadewireAmbient shadewire_eval_ambient(Model surface, vec3 to_viewer, vec3 shading_normal)
//! ```
//!
//! [`ambient_items`] writes `shadewire_eval_ambient`, the albedo split into diffuse and specular, for a host that
//! lights the environment with a BRDF of its own (Godot's, Bevy's).
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

pub const AMBIENT_STRUCT: &str = "ShadewireAmbient";

/// Rec. 709 luminance, the weight of a color in the effective roughness.
fn luminance(color: Expr) -> Expr {
    let weights = Expr::construct(Ty::vec(3), vec![Expr::f32(0.2126), Expr::f32(0.7152), Expr::f32(0.0722)]);
    Expr::builtin(Builtin::Dot, vec![color, weights], Ty::F32)
}

/// The albedo split into what diffuse lobes and what specular lobes reflect: one `let` per node for its albedo
/// (`c`), its diffuse part (`d`; the rest of the response is specular) and its specular lobes' perceptual
/// roughness weighted by their albedo's luminance, with the weight (`r`, a sum and its weight).
fn ambient_body(walk: &Walk, graph: &ClosureGraph) -> Vec<Stmt> {
    let v2 = || Ty::vec(2);
    let v3 = || Ty::vec(3);
    let ambient = Ty::Struct(AMBIENT_STRUCT.into());
    let zero = |ty: Ty| Expr::construct(ty, vec![Expr::f32(0.0)]);
    let Some(root) = graph.bsdf else {
        return vec![Stmt::Return(Some(Expr::construct(ambient, vec![zero(v3()), zero(v3()), Expr::f32(1.0)])))];
    };
    let live = reachable(graph, root);
    let mut body = Vec::new();
    let named = |prefix: &str, id: usize, ty: Ty| Expr::name(format!("{prefix}{id}"), ty);
    let c = |i: ClosureId| named("c", i as usize, bsdf());
    let d = |i: ClosureId| named("d", i as usize, v3());
    let r = |i: ClosureId| named("r", i as usize, v2());
    let mul = |a: Expr, b: Expr, ty: Ty| Expr::binary(BinaryOp::Mul, a, b, ty);
    let add = |a: Expr, b: Expr, ty: Ty| Expr::binary(BinaryOp::Add, a, b, ty);
    let response = |e: Expr| e.member("response", v3());
    for (id, node) in graph.nodes.iter().enumerate().filter(|(id, _)| live[*id]) {
        let id_c = id as ClosureId;
        let (albedo, diffuse, rough, comment) = match node {
            ClosureNode::Lobe { kind, ty: ClosureType::Bsdf, inputs, settings, .. } => match bsdf_lobe(kind) {
                Some(lobe) => {
                    let mut args = vec![Expr::name(TO_VIEWER, v3())];
                    args.extend(walk.lobe_args(&lobe, inputs, settings));
                    let albedo = Expr::call(format!("shadewire_{kind}_albedo"), args, bsdf());
                    if lobe.file == "closure_specular" {
                        let alpha = walk.input(inputs, "roughness", ROUGHNESS);
                        let alpha = Expr::builtin(
                            Builtin::Clamp,
                            vec![alpha, zero(v2()), Expr::construct(v2(), vec![Expr::f32(1.0)])],
                            v2(),
                        );
                        let perceptual = Expr::builtin(
                            Builtin::Sqrt,
                            vec![Expr::call("shadewire_mx_average_alpha", vec![alpha], Ty::F32)],
                            Ty::F32,
                        );
                        let weight = luminance(response(c(id_c)));
                        let rough = Expr::construct(v2(), vec![mul(weight.clone(), perceptual, Ty::F32), weight]);
                        (albedo, zero(v3()), rough, None)
                    } else {
                        (albedo, response(c(id_c)), zero(v2()), None)
                    }
                }
                None => (none(), zero(v3()), zero(v2()), Some(format!("{kind}: not evaluated"))),
            },
            ClosureNode::Lobe { kind, .. } => (none(), zero(v3()), zero(v2()), Some(format!("{kind}: not a BSDF"))),
            ClosureNode::Layer { top, base } => {
                let through = c(*top).member("throughput", v3());
                (
                    Expr::call("shadewire_layer_bsdf", vec![c(*top), c(*base)], bsdf()),
                    add(d(*top), mul(d(*base), through.clone(), v3()), v3()),
                    add(r(*top), mul(r(*base), luminance(through), v2()), v2()),
                    None,
                )
            }
            ClosureNode::Mix { fg, bg, mix } => {
                let m = walk.field(mix);
                (
                    Expr::call("shadewire_mix_bsdf", vec![c(*fg), c(*bg), m.clone()], bsdf()),
                    Expr::builtin(Builtin::Mix, vec![d(*bg), d(*fg), cast(m.clone(), &v3())], v3()),
                    Expr::builtin(Builtin::Mix, vec![r(*bg), r(*fg), cast(m, &v2())], v2()),
                    None,
                )
            }
            ClosureNode::Add { in1, in2 } => (
                Expr::call("shadewire_add_bsdf", vec![c(*in1), c(*in2)], bsdf()),
                add(d(*in1), d(*in2), v3()),
                add(r(*in1), r(*in2), v2()),
                None,
            ),
            ClosureNode::Multiply { in1, in2 } => {
                let w = Expr::builtin(
                    Builtin::Clamp,
                    vec![walk.color_field(in2), zero(v3()), Expr::construct(v3(), vec![Expr::f32(1.0)])],
                    v3(),
                );
                (
                    Expr::call("shadewire_multiply_bsdf", vec![c(*in1), walk.color_field(in2)], bsdf()),
                    mul(d(*in1), w.clone(), v3()),
                    mul(r(*in1), luminance(w), v2()),
                    None,
                )
            }
        };
        body.extend(let_(id, albedo, comment));
        for (prefix, value) in [("d", diffuse), ("r", rough)] {
            body.push(Stmt::Let {
                name: format!("{prefix}{id}"),
                ty: value.ty.clone(),
                value: Some(value),
                mutable: false,
            });
        }
    }
    let specular = Expr::binary(BinaryOp::Sub, response(c(root)), d(root), v3());
    let weight = r(root).swizzle("y");
    let roughness = Expr::builtin(
        Builtin::Select,
        vec![
            Expr::f32(1.0),
            Expr::binary(
                BinaryOp::Div,
                r(root).swizzle("x"),
                Expr::builtin(Builtin::Max, vec![weight.clone(), Expr::f32(1e-8)], Ty::F32),
                Ty::F32,
            ),
            Expr::binary(BinaryOp::Gt, weight, Expr::f32(0.0), Ty::BOOL),
        ],
        Ty::F32,
    );
    body.push(Stmt::Return(Some(Expr::construct(ambient, vec![d(root), specular, roughness]))));
    body
}

/// `shadewire_eval_ambient`, for a back end that lights the environment with a BRDF of its own: the closure
/// tree's albedo split into its diffuse lobes' and its specular lobes' part, and the specular lobes' perceptual
/// roughness (√α) weighted by their albedo. Sheen counts as diffuse. Empty without a closure tree.
///
/// ```text
/// struct ShadewireAmbient { diffuse: vec3, specular: vec3, roughness: float }
/// ShadewireAmbient shadewire_eval_ambient(Model surface, vec3 to_viewer, vec3 shading_normal)
/// ```
pub fn ambient_items(dag: &Dag) -> Vec<Item> {
    let Some(graph) = &dag.surface.closure else {
        return Vec::new();
    };
    let model = Ty::Struct(struct_name(&dag.surface.model));
    let walk = Walk {
        fields: dag.surface.fields.iter().map(|f| (f.name.as_str(), Ty::of(f.ty))).collect(),
        model: model.clone(),
    };
    let param = |name: &str, ty: Ty| (name.to_string(), ty);
    vec![
        Item::Struct(Struct {
            name: AMBIENT_STRUCT.into(),
            members: vec![
                ("diffuse".into(), Ty::vec(3)),
                ("specular".into(), Ty::vec(3)),
                ("roughness".into(), Ty::F32),
            ],
            layout: None,
        }),
        Item::Function(Function {
            name: "shadewire_eval_ambient".into(),
            params: vec![param(SURFACE, model), param(TO_VIEWER, Ty::vec(3)), param(SHADING_NORMAL, Ty::vec(3))],
            ret: Ty::Struct(AMBIENT_STRUCT.into()),
            body: ambient_body(&walk, graph),
        }),
    ]
}
