//! The shared shader tree made an MDL module's functions: the material's struct, the support library it calls,
//! host functions defined over `::state`, and `shadewire_eval_surface` taking the uniforms and textures it reads as
//! parameters (MDL has no uniform blocks and no samplers).

use std::collections::HashSet;

use shadewire::shader::material::{MaterialOptions, eval_surface, host_functions, model_struct};
use shadewire::shader::names::UniformNames;
use shadewire::shader::*;
use shadewire::{AddressMode, Dag, HOST_PREFIX, Reflection, Type, Value};

use super::names::mdl_ident;

const UNIFORMS: &str = "shadewire_u";

/// A parameter of the material and of `shadewire_eval_surface`.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    /// The type `shadewire_eval_surface` takes.
    pub ty: Ty,
    /// The material parameter's MDL type (`color` for a color, else the same).
    pub mdl_ty: String,
    /// The default, as MDL source.
    pub default: String,
    /// `anno::` annotations, as MDL source.
    pub annotations: Vec<String>,
    /// The argument `shadewire_eval_surface` gets from the parameter.
    pub argument: String,
}

/// The module's functions and the parameters `shadewire_eval_surface` takes, approximations noted in
/// `reflection.approximations`.
pub fn build(dag: &Dag, reflection: &mut Reflection) -> (Module, Vec<Param>) {
    let mut library_dag = dag.clone();
    // The closure tree is ::df calls, not the lobe library.
    library_dag.surface.closure = None;
    let options = MaterialOptions { flip_v: false, implicit_derivatives: false };
    let mut items = vec![Item::Struct(model_struct(dag))];
    items.extend(shadewire::shader::library::items_for(&library_dag));
    let mut notes = Vec::new();
    for f in host_functions(reflection) {
        items.push(Item::Function(host_function(f, reflection, &mut notes)));
    }
    let mut surface = eval_surface(dag, reflection, &options);
    let declared: HashSet<&str> = items
        .iter()
        .filter_map(|i| match i {
            Item::Function(f) => Some(f.name.as_str()),
            Item::Struct(s) => Some(s.name.as_str()),
            Item::Const { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    let params = parameters(reflection, &declared, &mut notes);
    rewrite(&mut surface.body, reflection, &params);
    surface.params = params.iter().map(|p| (p.name.clone(), p.ty.clone())).collect();
    items.push(Item::Function(surface));
    for note in notes {
        if !reflection.approximations.contains(&note) {
            reflection.approximations.push(note);
        }
    }
    (Module { items }, params)
}

/// An MDL literal of `v`; colors as `color(...)` when `color`.
pub fn literal(v: &Value, color: bool) -> String {
    let f = |x: &f32| shadewire::shader::print::float(*x);
    let list = |xs: &[f32]| xs.iter().map(f).collect::<Vec<_>>().join(", ");
    match v {
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Float(x) => f(x),
        Value::Color3(c) if color => format!("color({})", list(c)),
        Value::Vector2(c) => format!("float2({})", list(c)),
        Value::Vector3(c) | Value::Color3(c) => format!("float3({})", list(c)),
        Value::Vector4(c) | Value::Color4(c) => format!("float4({})", list(c)),
        Value::Matrix33(c) => format!("float3x3({})", list(c)),
        Value::Matrix44(c) => format!("float4x4({})", list(c)),
    }
}

/// Whether the writer itself may name something `name` in the scope parameters live in: the module's `declared`
/// items, its own functions and textures (`shadewire_*`), the material's `let` (`s`, `c0`...) and the locals of
/// `shadewire_eval_surface` (`n0`...).
fn writer_owned(name: &str, declared: &HashSet<&str>) -> bool {
    let numbered = |prefix: char| {
        name.strip_prefix(prefix).is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
    };
    name == "s" || numbered('c') || numbered('n') || declared.contains(name) || name.starts_with("shadewire_")
}

/// The uniforms then the textures, each a material parameter; a uniform's name is clear of the writer's own (an `x`
/// put before it).
fn parameters(reflection: &Reflection, declared: &HashSet<&str>, notes: &mut Vec<String>) -> Vec<Param> {
    let mut taken: Vec<String> = Vec::new();
    let mut unique = |base: String| {
        let mut name = base.clone();
        let mut n = 2;
        while taken.contains(&name) {
            name = format!("{base}_{n}");
            n += 1;
        }
        taken.push(name.clone());
        name
    };
    let mut out = Vec::new();
    for u in &reflection.uniforms {
        let mut base = mdl_ident(&u.name);
        if writer_owned(&base, declared) {
            base = format!("x{base}");
        }
        let name = unique(base);
        let color = u.ty == Type::Color3;
        let ty = Ty::of(u.ty);
        // Booleans are uniform: `thin_walled` must be, and MDL's toggles are.
        let mdl_ty = match u.ty {
            Type::Color3 => "color".to_string(),
            Type::Boolean => "uniform bool".to_string(),
            _ => super::syntax::type_name(&ty),
        };
        let argument = if color { format!("float3({name})") } else { name.clone() };
        let ui = &u.ui;
        let mut annotations = Vec::new();
        if let (Some(min), Some(max)) = (&ui.min, &ui.max) {
            annotations.push(format!("anno::hard_range({}, {})", literal(min, color), literal(max, color)));
        }
        // A soft limit not authored is the hard one, as MaterialX reads it.
        if let (Some(min), Some(max)) =
            (ui.soft_min.as_ref().or(ui.min.as_ref()), ui.soft_max.as_ref().or(ui.max.as_ref()))
            && (ui.soft_min.is_some() || ui.soft_max.is_some())
        {
            annotations.push(format!("anno::soft_range({}, {})", literal(min, color), literal(max, color)));
        }
        if let Some(folder) = &ui.folder {
            let parts: Vec<&str> = folder.split('/').filter(|p| !p.is_empty()).collect();
            let mut groups: Vec<String> = parts.iter().take(2).map(|p| super::names::mdl_string(p)).collect();
            if parts.len() > 2 {
                groups.push(super::names::mdl_string(&parts[2..].join("/")));
            }
            if !groups.is_empty() {
                annotations.push(format!("anno::in_group({})", groups.join(", ")));
            }
        }
        out.push(Param { name, ty, mdl_ty, default: literal(&u.default, color), annotations, argument });
    }
    for t in &reflection.textures {
        let name = format!("shadewire_t{}", t.index);
        let (file, dropped) = super::names::mdl_resource(&t.file);
        if dropped {
            notes.push(format!("mdl: texture {} written without its drive letter, as {file}", t.file));
        }
        let default = format!("texture_2d({}, tex::gamma_linear)", super::names::mdl_string(&file));
        out.push(Param {
            name: name.clone(),
            ty: Ty::Texture2d,
            mdl_ty: "uniform texture_2d".to_string(),
            default,
            annotations: Vec::new(),
            argument: name,
        });
    }
    out
}

/// `tex::wrap_mode` of an address mode.
fn wrap(a: AddressMode) -> &'static str {
    match a {
        AddressMode::Periodic => "tex::wrap_repeat",
        AddressMode::Clamp => "tex::wrap_clamp",
        AddressMode::Mirror => "tex::wrap_mirrored_repeat",
        AddressMode::Constant => "tex::wrap_clip",
    }
}

/// Uniform block reads become the parameters, samples `tex::lookup_float4`.
fn rewrite(body: &mut [Stmt], reflection: &Reflection, params: &[Param]) {
    let block = UniformNames::new(reflection);
    let members: Vec<(String, String, bool)> = reflection
        .uniforms
        .iter()
        .enumerate()
        .map(|(i, u)| (block.member(i).to_string(), params[i].name.clone(), u.ty == Type::Boolean))
        .collect();
    let param_of = |member: &str| members.iter().find(|(m, _, _)| m == member);
    map_stmts(body, &mut |e| {
        // A boolean uniform is stored as u32 and read as `shadewire_u.m != 0u`.
        if let ExprKind::Binary { op: BinaryOp::Ne, lhs, rhs } = &e.kind
            && matches!(rhs.kind, ExprKind::Literal(Literal::U32(0)))
            && let Some(m) = block_member(lhs)
            && let Some((_, name, true)) = param_of(m)
        {
            *e = Expr::name(name.clone(), Ty::BOOL);
            return;
        }
        if let Some(m) = block_member(e)
            && let Some((_, name, _)) = param_of(m)
        {
            *e = Expr::name(name.clone(), e.ty.clone());
            return;
        }
        if let ExprKind::Builtin {
            builtin: Builtin::TextureSample | Builtin::TextureSampleLevel | Builtin::TextureSampleGrad,
            args,
        } = &e.kind
        {
            let ExprKind::Name(texture) = &args[0].kind else { return };
            let index: usize = texture.trim_start_matches("shadewire_t").parse().unwrap_or(0);
            let t = reflection.textures.iter().find(|t| t.index == index).expect("a reflected texture");
            let call = Expr::call(
                "tex::lookup_float4",
                vec![
                    args[0].clone(),
                    args[2].clone(),
                    Expr::name(wrap(t.address[0]), Ty::I32),
                    Expr::name(wrap(t.address[1]), Ty::I32),
                ],
                Ty::vec(4),
            );
            *e = call;
        }
    });
}

/// `shadewire_u.member`'s member name, when `e` is one.
fn block_member(e: &Expr) -> Option<&str> {
    match &e.kind {
        ExprKind::Member { base, member } if matches!(&base.kind, ExprKind::Name(n) if n == UNIFORMS) => Some(member),
        _ => None,
    }
}

/// `f` on every expression of `body`, innermost first.
fn map_stmts(body: &mut [Stmt], f: &mut impl FnMut(&mut Expr)) {
    fn expr(e: &mut Expr, f: &mut impl FnMut(&mut Expr)) {
        match &mut e.kind {
            ExprKind::Call { args, .. } | ExprKind::Builtin { args, .. } | ExprKind::Construct(args) => {
                args.iter_mut().for_each(|a| expr(a, f))
            }
            ExprKind::Convert(a)
            | ExprKind::Bitcast(a)
            | ExprKind::Deref(a)
            | ExprKind::AddressOf(a)
            | ExprKind::Unary { arg: a, .. }
            | ExprKind::Member { base: a, .. }
            | ExprKind::Swizzle { base: a, .. } => expr(a, f),
            ExprKind::Binary { lhs, rhs, .. } => {
                expr(lhs, f);
                expr(rhs, f);
            }
            ExprKind::Index { base, index } => {
                expr(base, f);
                expr(index, f);
            }
            ExprKind::Literal(_) | ExprKind::Name(_) => {}
        }
        f(e);
    }
    for s in body {
        match s {
            Stmt::Let { value: Some(v), .. } | Stmt::Return(Some(v)) | Stmt::Expr(v) => expr(v, f),
            Stmt::Assign { target, value, .. } => {
                expr(target, f);
                expr(value, f);
            }
            Stmt::If { cond, then, otherwise } => {
                expr(cond, f);
                map_stmts(then, f);
                map_stmts(otherwise, f);
            }
            Stmt::For { init, cond, update, body } => {
                map_stmts(std::slice::from_mut(init), f);
                expr(cond, f);
                map_stmts(std::slice::from_mut(update), f);
                map_stmts(body, f);
            }
            Stmt::Switch { selector, cases, default } => {
                expr(selector, f);
                for (values, body) in cases {
                    values.iter_mut().for_each(|v| expr(v, f));
                    map_stmts(body, f);
                }
                map_stmts(default, f);
            }
            _ => {}
        }
    }
}

/// A `::state` space by Shadewire's space name: internal is world, model is object.
fn space(name: &str) -> &'static str {
    match name {
        "world" => "state::coordinate_world",
        _ => "state::coordinate_object",
    }
}

fn state(function: &str, ty: Ty) -> Expr {
    Expr::call(format!("state::{function}"), Vec::new(), ty)
}

/// `state::transform_<kind>(internal, <space>, v)` unless the space is world.
fn from_internal(kind: &str, to: &str, v: Expr) -> Expr {
    if to == "world" {
        return v;
    }
    let args = vec![Expr::name("state::coordinate_internal", Ty::I32), Expr::name(space(to), Ty::I32), v];
    Expr::call(format!("state::transform_{kind}"), args, Ty::vec(3))
}

/// A host function's body over `::state`, its approximations noted.
fn host_function(mut f: Function, reflection: &Reflection, notes: &mut Vec<String>) -> Function {
    let read = f.name.strip_prefix(HOST_PREFIX).unwrap_or(&f.name).to_string();
    let v3 = Ty::vec(3);
    let ret = f.ret.clone();
    let zero_of = |ty: &Ty| Expr::construct(ty.clone(), vec![Expr::f32(0.0)]);
    let one_of =
        |ty: &Ty| if ty.is_scalar() { Expr::f32(1.0) } else { Expr::construct(ty.clone(), vec![Expr::f32(1.0)]) };
    let value = if let Some(index) = read.strip_prefix("texcoord_") {
        let i: i32 = index.parse().unwrap_or(0);
        let uvw = Expr::call("state::texture_coordinate", vec![Expr::i32(i)], v3.clone());
        if ret.width() == Some(2) { uvw.swizzle("xy") } else { uvw }
    } else if let Some((kind, from, to)) = transform_parts(&read) {
        f.params = vec![("v".into(), v3.clone())];
        let v = Expr::name("v", v3.clone());
        fn model(s: &str) -> &str {
            if s == "model" { "object" } else { s }
        }
        let (from, to) = (model(from), model(to));
        if from == to {
            v
        } else {
            let args = vec![Expr::name(space(from), Ty::I32), Expr::name(space(to), Ty::I32), v];
            Expr::call(format!("state::transform_{kind}"), args, v3.clone())
        }
    } else if let Some((what, to)) = read.rsplit_once('_')
        && matches!(what, "position" | "normal" | "tangent" | "bitangent")
    {
        let (value, kind) = match what {
            "position" => (state("position", v3.clone()), "point"),
            "normal" => (state("normal", v3.clone()), "normal"),
            "tangent" => (Expr::call("state::texture_tangent_u", vec![Expr::i32(0)], v3.clone()), "vector"),
            _ => (Expr::call("state::texture_tangent_v", vec![Expr::i32(0)], v3.clone()), "vector"),
        };
        from_internal(kind, if to == "model" { "object" } else { to }, value)
    } else if read.starts_with("view_direction") {
        notes.push("mdl: the view direction read as zero (MDL's state has none for surfaces)".to_string());
        zero_of(&ret)
    } else if read == "time" {
        state("animation_time", Ty::F32)
    } else if read == "frame" {
        notes.push("mdl: the frame read as 0".to_string());
        Expr::f32(0.0)
    } else if read.starts_with("geomcolor_") {
        notes.push(format!("mdl: {read} read as white (MDL's state has no vertex colors)"));
        one_of(&ret)
    } else if let Some(g) = reflection.context.geomprops.iter().find(|g| g.function == f.name) {
        notes.push(format!("mdl: geomprop {} read as its default", g.name));
        match &g.default {
            Some(v) => Expr::name(literal(v, false), ret.clone()),
            None => zero_of(&ret),
        }
    } else {
        notes.push(format!("mdl: host read {read} as zero"));
        zero_of(&ret)
    };
    f.body = vec![Stmt::Return(Some(value))];
    f
}

/// `transform_<kind>_<from>_to_<to>` as its parts.
fn transform_parts(name: &str) -> Option<(&str, &str, &str)> {
    let rest = name.strip_prefix("transform_")?;
    let (kind, spaces) = rest.split_once('_')?;
    let (from, to) = spaces.split_once("_to_")?;
    Some((kind, from, to))
}
