//! How WGSL spells the shader tree: `let n: T = v;`, `fn f(a: T) -> R`, `vec3<f32>(..)`, `@group @binding`
//! resources and function-local pointers.

use shadewire::shader::print::{Printer, Syntax};
use shadewire::shader::*;
use shadewire::{Type, Value};

/// The WGSL syntax.
pub struct Wgsl;

fn scalar(s: Scalar) -> &'static str {
    match s {
        Scalar::Bool => "bool",
        Scalar::I32 => "i32",
        Scalar::U32 => "u32",
        Scalar::F32 => "f32",
    }
}

impl Syntax for Wgsl {
    fn ty(&self, ty: &Ty) -> String {
        match ty {
            Ty::Scalar(s) => scalar(*s).into(),
            Ty::Vector(n, s) => format!("vec{n}<{}>", scalar(*s)),
            Ty::Matrix(c, r) => format!("mat{c}x{r}<f32>"),
            Ty::Struct(name) => name.clone(),
            Ty::Ptr(t) => format!("ptr<function, {}>", self.ty(t)),
            Ty::Texture2d => "texture_2d<f32>".into(),
            Ty::Sampler => "sampler".into(),
            Ty::Void => String::new(),
            Ty::Unknown => panic!("an untyped expression reached the printer"),
        }
    }

    fn builtin_name(&self, b: Builtin) -> &'static str {
        b.wgsl_name()
    }

    fn literal(&self, lit: &Literal) -> String {
        match lit {
            Literal::I32(i) => format!("{i}i"),
            Literal::U32(u) => format!("{u}u"),
            Literal::Bool(b) => b.to_string(),
            Literal::F32(x) => shadewire::shader::print::float(*x),
        }
    }

    fn bitcast(&self, p: &Printer<Self>, ty: &Ty, arg: &Expr) -> String {
        format!("bitcast<{}>({})", self.ty(ty), p.expr(arg))
    }

    fn deref(&self, p: &Printer<Self>, inner: &Expr) -> String {
        format!("*{}", p.operand(inner, 12))
    }

    fn address_of(&self, p: &Printer<Self>, inner: &Expr) -> String {
        format!("&{}", p.operand(inner, 12))
    }

    fn param(&self, name: &str, ty: &Ty) -> String {
        format!("{name}: {}", self.ty(ty))
    }

    fn signature(&self, f: &Function, _module: &Module) -> String {
        let params: Vec<String> = f.params.iter().map(|(n, t)| self.param(n, t)).collect();
        let ret = if f.ret == Ty::Void { String::new() } else { format!(" -> {}", self.ty(&f.ret)) };
        format!("fn {}({}){ret}", f.name, params.join(", "))
    }

    /// WGSL has no declarations: the host's definitions are linked in.
    fn extern_decl(&self, _f: &Function, _module: &Module) -> Option<String> {
        None
    }

    fn local(&self, name: &str, ty: &Ty, value: Option<String>, mutable: bool) -> String {
        let keyword = if mutable { "var" } else { "let" };
        match value {
            Some(v) => format!("{keyword} {name}: {} = {v};", self.ty(ty)),
            None => format!("{keyword} {name}: {};", self.ty(ty)),
        }
    }

    fn constant(&self, name: &str, ty: &Ty, value: String) -> String {
        format!("const {name}: {} = {value};", self.ty(ty))
    }

    fn structure(&self, s: &Struct) -> String {
        let mut out = format!("struct {} {{\n", s.name);
        for (m, t) in &s.members {
            out.push_str(&format!("    {m}: {},\n", self.ty(t)));
        }
        out.push_str("}\n");
        out
    }

    fn resource(&self, r: &Resource, _module: &Module) -> String {
        let at = format!("@group({}) @binding({})", r.group, r.binding);
        match &r.kind {
            ResourceKind::Uniform(s) => format!("{at} var<uniform> {}: {s};", r.name),
            ResourceKind::Texture2d => format!("{at} var {}: texture_2d<f32>;", r.name),
            ResourceKind::Sampler => format!("{at} var {}: sampler;", r.name),
        }
    }

    /// WGSL cases never fall through.
    fn case_break(&self) -> bool {
        false
    }

    fn case_head(&self, p: &Printer<Self>, values: &[Expr]) -> String {
        format!("case {}:", p.list(values))
    }
}

/// Prints a module as WGSL.
pub fn print(module: &Module) -> String {
    Printer::new(&Wgsl).module(module)
}

/// The WGSL type of an IR value type, for host code: `bool`, `i32`, `f32` or a `vecN<f32>`.
///
/// # Panics
/// On matrices, which elaboration rejects.
pub fn wgsl_type(ty: Type) -> String {
    Wgsl.ty(&Ty::of(ty))
}

/// A WGSL expression for `value`, for host code.
///
/// # Panics
/// On matrices, which elaboration rejects.
pub fn literal(value: &Value) -> String {
    Printer::new(&Wgsl).expr(&shadewire::shader::material::values::literal(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_and_literals() {
        assert_eq!(wgsl_type(Type::Color3), "vec3<f32>");
        assert_eq!(literal(&Value::Float(f32::NAN)), "bitcast<f32>(2143289344u)");
        assert_eq!(literal(&Value::Integer(i32::MIN)), "-2147483647i - 1i");
        assert_eq!(literal(&Value::Vector2([0.5, 1.0])), "vec2<f32>(0.5, 1.0)");
    }

    #[test]
    fn library_round_trips_through_naga() {
        // Every library file printed back as WGSL, all together, is valid WGSL.
        let items = shadewire::shader::library::FILES
            .iter()
            .flat_map(|f| shadewire::shader::library::module(f).items.iter().cloned())
            .collect();
        let text = print(&Module { items });
        let module = naga::front::wgsl::parse_str(&text).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&text)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{text}\n{e:?}"));
    }
}
