//! Shadewire GLSL back end: the material's shader tree (built by `shadewire::shader`) printed as a GLSL library,
//! with its reflection. The host appends its own GLSL (the host functions, its lights, `main`) and compiles the
//! whole.
//!
//! Desktop GLSL from 330 and GLSL ES from 300 (WebGL 2). Textures are combined `sampler2D`s. From GLSL 420 and
//! ES 310 the uniform block sits at binding point `uniform_binding` and texture `t` at unit `t.binding`
//! (reflection's numbers); before, the host binds them by name (`MaterialUniforms`, `shadewire_t{index}`).

use shadewire::shader::print::{Printer, Syntax, visit_exprs};
use shadewire::shader::*;
use shadewire::{Dag, Error, SourceInfo, Type};

pub use shadewire::shader::{CompileOptions, Compiled};

/// The GLSL dialect and version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    /// `#version N core`, N from 330.
    Desktop(u16),
    /// `#version N es`, N from 300.
    Es(u16),
}

impl Default for Version {
    fn default() -> Self {
        Version::Desktop(420)
    }
}

impl Version {
    /// Whether `layout(binding = N)` is available.
    fn explicit_bindings(self) -> bool {
        match self {
            Version::Desktop(v) => v >= 420,
            Version::Es(v) => v >= 310,
        }
    }

    fn has_fma(self) -> bool {
        match self {
            Version::Desktop(v) => v >= 400,
            Version::Es(v) => v >= 320,
        }
    }
}

/// The GLSL syntax at a version.
pub struct Glsl {
    pub version: Version,
}

fn scalar(s: Scalar) -> &'static str {
    match s {
        Scalar::Bool => "bool",
        Scalar::I32 => "int",
        Scalar::U32 => "uint",
        Scalar::F32 => "float",
    }
}

fn vector_prefix(s: Scalar) -> &'static str {
    match s {
        Scalar::Bool => "b",
        Scalar::I32 => "i",
        Scalar::U32 => "u",
        Scalar::F32 => "",
    }
}

/// The selects a module makes, as (value type, condition type) pairs. Each becomes a `shadewire_select`
/// overload: a call evaluates both values, as WGSL's `select` does, so a texture sample in one stays in uniform
/// control flow (a `?:` would evaluate one side); and GLSL has no componentwise select before 4.50 and ES 3.10.
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

impl Syntax for Glsl {
    fn ty(&self, ty: &Ty) -> String {
        match ty {
            Ty::Scalar(s) => scalar(*s).into(),
            Ty::Vector(n, s) => format!("{}vec{n}", vector_prefix(*s)),
            Ty::Matrix(c, r) if c == r => format!("mat{c}"),
            Ty::Matrix(c, r) => format!("mat{c}x{r}"),
            Ty::Struct(name) => name.clone(),
            Ty::Ptr(t) => self.ty(t),
            Ty::Texture2d => "sampler2D".into(),
            Ty::Sampler => panic!("GLSL samplers are combined with their textures"),
            Ty::Void => "void".into(),
            Ty::Unknown => panic!("an untyped expression reached the printer"),
        }
    }

    fn builtin_name(&self, b: Builtin) -> &'static str {
        match b {
            Builtin::InverseSqrt => "inversesqrt",
            Builtin::Atan2 => "atan",
            // WGSL rounds halfway cases to even; GLSL's `round` may go either way.
            Builtin::Round => "roundEven",
            Builtin::Dpdx => "dFdx",
            Builtin::Dpdy => "dFdy",
            other => other.wgsl_name(),
        }
    }

    fn builtin(&self, p: &Printer<Self>, b: Builtin, args: &[Expr], _ty: &Ty) -> String {
        match b {
            Builtin::Select => format!("shadewire_select({})", p.list(args)),
            Builtin::Fma if !self.version.has_fma() => {
                let product = Expr::binary(BinaryOp::Mul, args[0].clone(), args[1].clone(), args[0].ty.clone());
                format!("({})", p.expr(&Expr::binary(BinaryOp::Add, product, args[2].clone(), args[0].ty.clone())))
            }
            // The sampler argument goes: GLSL textures carry their sampler.
            Builtin::TextureSample => format!("texture({}, {})", p.expr(&args[0]), p.expr(&args[2])),
            Builtin::TextureSampleLevel => {
                format!("textureLod({}, {}, {})", p.expr(&args[0]), p.expr(&args[2]), p.expr(&args[3]))
            }
            Builtin::TextureSampleGrad => format!(
                "textureGrad({}, {}, {}, {})",
                p.expr(&args[0]),
                p.expr(&args[2]),
                p.expr(&args[3]),
                p.expr(&args[4])
            ),
            other => format!("{}({})", self.builtin_name(other), p.list(args)),
        }
    }

    fn bitcast(&self, p: &Printer<Self>, ty: &Ty, arg: &Expr) -> String {
        let (from, to) = (arg.ty.scalar().expect("a numeric bitcast"), ty.scalar().expect("a numeric bitcast"));
        let function = match (from, to) {
            (Scalar::U32, Scalar::F32) => "uintBitsToFloat".to_string(),
            (Scalar::I32, Scalar::F32) => "intBitsToFloat".to_string(),
            (Scalar::F32, Scalar::U32) => "floatBitsToUint".to_string(),
            (Scalar::F32, Scalar::I32) => "floatBitsToInt".to_string(),
            // Between the integers a conversion keeps the bits.
            _ => self.ty(ty),
        };
        format!("{function}({})", p.expr(arg))
    }

    /// Vector comparisons are functions in GLSL; a vector `!` is `not`.
    fn binary(&self, p: &Printer<Self>, op: BinaryOp, lhs: &Expr, rhs: &Expr, ty: &Ty) -> Option<String> {
        if !op.is_comparison() || !ty.is_vector() {
            return None;
        }
        let function = match op {
            BinaryOp::Eq => "equal",
            BinaryOp::Ne => "notEqual",
            BinaryOp::Lt => "lessThan",
            BinaryOp::Le => "lessThanEqual",
            BinaryOp::Gt => "greaterThan",
            _ => "greaterThanEqual",
        };
        // Both sides as vectors.
        let width = ty.width().expect("a vector");
        let side = |e: &Expr| {
            if e.ty.is_scalar() {
                p.expr(&Expr::construct(Ty::Vector(width, e.ty.scalar().expect("a scalar")), vec![e.clone()]))
            } else {
                p.expr(e)
            }
        };
        Some(format!("{function}({}, {})", side(lhs), side(rhs)))
    }

    fn constant(&self, name: &str, ty: &Ty, value: String) -> String {
        format!("const {} {name} = {value};", self.ty(ty))
    }

    /// A host-filled struct is the uniform block, which its resource declares.
    fn structure(&self, s: &Struct) -> String {
        if s.layout.is_some() {
            return String::new();
        }
        let mut out = format!("struct {} {{\n", s.name);
        for (m, t) in &s.members {
            out.push_str(&format!("    {} {m};\n", self.ty(t)));
        }
        out.push_str("};\n");
        out
    }

    fn resource(&self, r: &Resource, module: &Module) -> String {
        let binding = |extra: &str| {
            if self.version.explicit_bindings() {
                format!("layout({extra}binding = {}) ", r.binding)
            } else if extra.is_empty() {
                String::new()
            } else {
                format!("layout({}) ", extra.trim_end_matches(", "))
            }
        };
        match &r.kind {
            ResourceKind::Uniform(name) => {
                let s = module.structs().find(|s| &s.name == name).expect("the uniform block's struct");
                let mut out = format!("{}uniform {name} {{\n", binding("std140, "));
                for (m, t) in &s.members {
                    out.push_str(&format!("    {} {m};\n", self.ty(t)));
                }
                out.push_str(&format!("}} {};", r.name));
                out
            }
            ResourceKind::Texture2d => format!("{}uniform sampler2D {};", binding(""), r.name),
            // Combined with its texture.
            ResourceKind::Sampler => String::new(),
        }
    }

    fn prelude(&self, module: &Module) -> String {
        let mut out = match self.version {
            Version::Desktop(v) => format!("#version {v} core\n"),
            Version::Es(v) => format!("#version {v} es\nprecision highp float;\nprecision highp int;\n"),
        };
        out.push('\n');
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

/// Prints a module as GLSL.
pub fn print(module: &Module, version: Version) -> String {
    Printer::new(&Glsl { version }).module(module)
}

/// Optimize (when asked), reflect, print as a GLSL library.
pub fn compile(dag: &Dag, source: &SourceInfo, options: &CompileOptions, version: Version) -> Result<Compiled, Error> {
    compile_with(dag, source, options, |module, _| print(module, version))
}

/// The GLSL type of an IR value type, for host code: `bool`, `int`, `float` or a `vecN`.
pub fn glsl_type(ty: Type) -> String {
    Glsl { version: Version::default() }.ty(&Ty::of(ty))
}

#[cfg(test)]
mod tests;
pub mod validate;
