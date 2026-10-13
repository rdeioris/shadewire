//! Shadewire HLSL back end: the material's shader tree (built by `shadewire::shader`) printed as an HLSL library,
//! with its reflection. The host appends its own HLSL (the host functions, its lights, its entry point) and
//! compiles the whole (shader model 5.1 and up: register spaces).
//!
//! Resources sit at reflection's numbers: the uniform block at `register(b{uniform_binding}, space{group})`,
//! texture `t` at `register(t{t.binding}, space{group})` and its sampler at `register(s{t.sampler_binding},
//! space{group})`. The uniform block is padded to reflection's offsets.

use shadewire::shader::print::{Field, Printer, Syntax, padded, visit_exprs};
use shadewire::shader::*;
use shadewire::{Dag, Error, SourceInfo, Type};

pub use shadewire::shader::{CompileOptions, Compiled};

/// The HLSL syntax.
pub struct Hlsl;

fn scalar(s: Scalar) -> &'static str {
    match s {
        Scalar::Bool => "bool",
        Scalar::I32 => "int",
        Scalar::U32 => "uint",
        Scalar::F32 => "float",
    }
}

/// The selects a module makes, as (value type, condition type) pairs: each becomes a `shadewire_select`
/// overload, which evaluates both values as WGSL's `select` does (HLSL 2021's `?:` evaluates one).
fn selects(module: &Module) -> Vec<(Ty, Ty)> {
    let mut out: Vec<(Ty, Ty)> = Vec::new();
    visit_exprs(module, &mut |e| {
        if let ExprKind::Builtin { builtin: Builtin::Select, args } = &e.kind {
            let pair = (args[0].ty.clone(), args[2].ty.clone());
            if !out.contains(&pair) {
                out.push(pair);
            }
        }
    });
    out
}

fn is_matrix(e: &Expr) -> bool {
    matches!(e.ty, Ty::Matrix(..))
}

impl Syntax for Hlsl {
    /// A matrix of `c` columns and `r` rows is stored transposed (`float{c}x{r}`, its rows the columns), so that
    /// constructing one from columns reads as written and `m[i]` is column `i`; products swap accordingly.
    fn ty(&self, ty: &Ty) -> String {
        match ty {
            Ty::Scalar(s) => scalar(*s).into(),
            Ty::Vector(n, s) => format!("{}{n}", scalar(*s)),
            Ty::Matrix(c, r) => format!("float{c}x{r}"),
            Ty::Struct(name) => name.clone(),
            Ty::Ptr(t) => self.ty(t),
            Ty::Texture2d => "Texture2D<float4>".into(),
            Ty::Sampler => "SamplerState".into(),
            Ty::Void => "void".into(),
            Ty::Unknown => panic!("an untyped expression reached the printer"),
        }
    }

    fn builtin_name(&self, b: Builtin) -> &'static str {
        match b {
            Builtin::Mix => "lerp",
            Builtin::Fract => "frac",
            Builtin::InverseSqrt => "rsqrt",
            Builtin::Dpdx => "ddx",
            Builtin::Dpdy => "ddy",
            Builtin::Fma => "mad",
            // HLSL's round rounds halfway cases to even, as WGSL's.
            other => other.wgsl_name(),
        }
    }

    fn builtin(&self, p: &Printer<Self>, b: Builtin, args: &[Expr], _ty: &Ty) -> String {
        let method = |name: &str, rest: &[Expr]| {
            let mut a = vec![p.expr(&args[1])];
            a.extend(rest.iter().map(|e| p.expr(e)));
            format!("{}.{name}({})", p.operand(&args[0], 12), a.join(", "))
        };
        match b {
            Builtin::Select => format!("shadewire_select({})", p.list(args)),
            Builtin::TextureSample => method("Sample", &args[2..]),
            Builtin::TextureSampleLevel => method("SampleLevel", &args[2..]),
            Builtin::TextureSampleGrad => method("SampleGrad", &args[2..]),
            other => format!("{}({})", self.builtin_name(other), p.list(args)),
        }
    }

    /// One argument of the same width (a splat or a relabel) is a cast: HLSL's constructors need every component.
    fn construct(&self, p: &Printer<Self>, ty: &Ty, args: &[Expr]) -> String {
        match ty {
            Ty::Struct(name) => format!("shadewire_make_{name}({})", p.list(args)),
            Ty::Vector(..) if args.len() == 1 => format!("(({})({}))", self.ty(ty), p.expr(&args[0])),
            _ => format!("{}({})", self.ty(ty), p.list(args)),
        }
    }

    fn convert(&self, p: &Printer<Self>, ty: &Ty, arg: &Expr) -> String {
        format!("(({})({}))", self.ty(ty), p.expr(arg))
    }

    fn bitcast(&self, p: &Printer<Self>, ty: &Ty, arg: &Expr) -> String {
        let function = match ty.scalar().expect("a numeric bitcast") {
            Scalar::F32 => "asfloat",
            Scalar::U32 => "asuint",
            Scalar::I32 => "asint",
            Scalar::Bool => panic!("bitcast to bool"),
        };
        format!("{function}({})", p.expr(arg))
    }

    /// Products with a matrix are `mul`, operands swapped for the transposed storage.
    fn binary(&self, p: &Printer<Self>, op: BinaryOp, lhs: &Expr, rhs: &Expr, _ty: &Ty) -> Option<String> {
        if op == BinaryOp::Mul && (is_matrix(lhs) || is_matrix(rhs)) && !(lhs.ty.is_scalar() || rhs.ty.is_scalar()) {
            return Some(format!("mul({}, {})", p.expr(rhs), p.expr(lhs)));
        }
        None
    }

    fn constant(&self, name: &str, ty: &Ty, value: String) -> String {
        format!("static const {} {name} = {value};", self.ty(ty))
    }

    /// The struct, then a function constructing it (HLSL has no struct constructors). A host-filled struct is
    /// padded to its offsets.
    fn structure(&self, s: &Struct) -> String {
        let mut out = format!("struct {} {{\n", s.name);
        let mut pad = 0;
        for field in padded(s) {
            match field {
                Field::Member(m, t) => out.push_str(&format!("    {} {m};\n", self.ty(t))),
                Field::Pad(words) => {
                    for _ in 0..words {
                        out.push_str(&format!("    uint shadewire_pad{pad};\n"));
                        pad += 1;
                    }
                }
            }
        }
        out.push_str("};\n");
        if s.layout.is_none() {
            let params: Vec<String> = s.members.iter().map(|(m, t)| format!("{} {m}", self.ty(t))).collect();
            out.push_str(&format!(
                "\n{} shadewire_make_{}({}) {{\n    {} r;\n",
                s.name,
                s.name,
                params.join(", "),
                s.name
            ));
            for (m, _) in &s.members {
                out.push_str(&format!("    r.{m} = {m};\n"));
            }
            out.push_str("    return r;\n}\n");
        }
        out
    }

    fn resource(&self, r: &Resource, _module: &Module) -> String {
        let (class, ty) = match &r.kind {
            ResourceKind::Uniform(s) => ('b', format!("ConstantBuffer<{s}>")),
            ResourceKind::Texture2d => ('t', "Texture2D<float4>".into()),
            ResourceKind::Sampler => ('s', "SamplerState".into()),
        };
        format!("{ty} {} : register({class}{}, space{});", r.name, r.binding, r.group)
    }

    fn prelude(&self, module: &Module) -> String {
        let mut out = String::new();
        for (value, cond) in selects(module) {
            let (v, c) = (self.ty(&value), self.ty(&cond));
            let body = match cond.width() {
                Some(n) if n > 1 => {
                    let parts: Vec<String> =
                        "xyzw"[..n as usize].chars().map(|k| format!("c.{k} ? t.{k} : f.{k}")).collect();
                    format!("{v}({})", parts.join(", "))
                }
                _ => "c ? t : f".to_string(),
            };
            out.push_str(&format!("{v} shadewire_select({v} f, {v} t, {c} c) {{\n    return {body};\n}}\n\n"));
        }
        out
    }
}

/// Prints a module as HLSL.
pub fn print(module: &Module) -> String {
    Printer::new(&Hlsl).module(module)
}

/// Optimize (when asked), reflect, print as an HLSL library.
pub fn compile(dag: &Dag, source: &SourceInfo, options: &CompileOptions) -> Result<Compiled, Error> {
    compile_with(dag, source, options, |module, _| print(module))
}

/// The HLSL type of an IR value type, for host code: `bool`, `int`, `float` or a `floatN`.
pub fn hlsl_type(ty: Type) -> String {
    Hlsl.ty(&Ty::of(ty))
}

#[cfg(test)]
mod tests;
