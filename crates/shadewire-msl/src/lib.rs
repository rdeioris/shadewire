//! Shadewire MSL back end: the material's shader tree (built by `shadewire::shader`) printed as a Metal Shading
//! Language library, with its reflection. The host appends its own MSL (the host functions, its lights, its
//! entry point) and compiles the whole.
//!
//! Metal has no global resources: the functions that read the material's resources (`shadewire_eval_surface`)
//! take them as trailing parameters, the uniform block first (`constant MaterialUniforms&`), then each texture and
//! its sampler in reflection's order. The uniform block is padded to reflection's offsets (vectors of three are
//! `packed_float3`).
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/shadewire-mark.svg",
    html_favicon_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/png/shadewire-mark-64.png"
)]

use std::collections::HashSet;

use shadewire::shader::print::{Printer, Syntax, c_scalar, float, padded_struct, visit_exprs};
use shadewire::shader::*;
use shadewire::{Dag, Error, SourceInfo, Type};

pub use shadewire::shader::{CompileOptions, Compiled};

/// The MSL syntax.
pub struct Msl;

/// The resources a function reads, in declaration order.
fn resources_read<'m>(f: &Function, module: &'m Module) -> Vec<&'m Resource> {
    let mut names = HashSet::new();
    let alone = Module { items: vec![Item::Function(f.clone())] };
    visit_exprs(&alone, &mut |e| {
        if let ExprKind::Name(n) = &e.kind {
            names.insert(n.clone());
        }
    });
    module.resources().filter(|r| names.contains(&r.name)).collect()
}

impl Syntax for Msl {
    fn ty(&self, ty: &Ty) -> String {
        match ty {
            Ty::Scalar(s) => c_scalar(*s).into(),
            Ty::Vector(n, s) => format!("{}{n}", c_scalar(*s)),
            Ty::Matrix(c, r) => format!("float{c}x{r}"),
            Ty::Struct(name) => name.clone(),
            Ty::Ptr(t) => self.ty(t),
            Ty::Texture2d => "texture2d<float>".into(),
            Ty::Sampler => "sampler".into(),
            Ty::Void => "void".into(),
            Ty::Unknown => panic!("an untyped expression reached the printer"),
        }
    }

    fn builtin_name(&self, b: Builtin) -> &'static str {
        match b {
            Builtin::InverseSqrt => "rsqrt",
            // `rint` rounds halfway cases to even, as WGSL's `round`.
            Builtin::Round => "rint",
            Builtin::Dpdx => "dfdx",
            Builtin::Dpdy => "dfdy",
            other => other.wgsl_name(),
        }
    }

    fn builtin(&self, p: &Printer<Self>, b: Builtin, args: &[Expr], ty: &Ty) -> String {
        let sample = |extra: String| {
            format!("{}.sample({}, {}{extra})", p.operand(&args[0], 12), p.expr(&args[1]), p.expr(&args[2]))
        };
        let degrees = 180.0_f32 / std::f32::consts::PI;
        match b {
            Builtin::TextureSample => sample(String::new()),
            Builtin::TextureSampleLevel => sample(format!(", level({})", p.expr(&args[3]))),
            Builtin::TextureSampleGrad => sample(format!(", gradient2d({}, {})", p.expr(&args[3]), p.expr(&args[4]))),
            // Metal has neither `radians` nor `degrees`.
            Builtin::Radians => {
                format!("({} * {})", p.operand(&args[0], 10), self.literal(&Literal::F32(1.0 / degrees)))
            }
            Builtin::Degrees => format!("({} * {})", p.operand(&args[0], 10), self.literal(&Literal::F32(degrees))),
            other => {
                let _ = ty;
                format!("{}({})", self.builtin_name(other), p.list(args))
            }
        }
    }

    /// A float literal carries its `f`: an unsuffixed one is a double in C++.
    fn literal(&self, lit: &Literal) -> String {
        match lit {
            Literal::Bool(b) => b.to_string(),
            Literal::I32(i) => i.to_string(),
            Literal::U32(u) => format!("{u}u"),
            Literal::F32(x) => format!("{}f", float(*x)),
        }
    }

    fn construct(&self, p: &Printer<Self>, ty: &Ty, args: &[Expr]) -> String {
        match ty {
            Ty::Struct(name) => format!("{name}{{{}}}", p.list(args)),
            _ => format!("{}({})", self.ty(ty), p.list(args)),
        }
    }

    fn bitcast(&self, p: &Printer<Self>, ty: &Ty, arg: &Expr) -> String {
        format!("as_type<{}>({})", self.ty(ty), p.expr(arg))
    }

    fn param(&self, name: &str, ty: &Ty) -> String {
        match ty {
            Ty::Ptr(t) => format!("thread {}& {name}", self.ty(t)),
            t => format!("{} {name}", self.ty(t)),
        }
    }

    /// The resources a function reads follow its parameters.
    fn signature(&self, f: &Function, module: &Module) -> String {
        let mut params: Vec<String> = f.params.iter().map(|(n, t)| self.param(n, t)).collect();
        for r in resources_read(f, module) {
            params.push(match &r.kind {
                ResourceKind::Uniform(s) => format!("constant {s}& {}", r.name),
                ResourceKind::Texture2d => format!("texture2d<float> {}", r.name),
                ResourceKind::Sampler => format!("sampler {}", r.name),
            });
        }
        format!("{} {}({})", self.ty(&f.ret), f.name, params.join(", "))
    }

    fn constant(&self, name: &str, ty: &Ty, value: String) -> String {
        format!("constant {} {name} = {value};", self.ty(ty))
    }

    /// A host-filled struct is padded to its offsets, its vectors of three packed.
    fn structure(&self, s: &Struct) -> String {
        padded_struct(s, |m, t| match t {
            Ty::Vector(3, Scalar::F32) if s.layout.is_some() => format!("packed_float3 {m}"),
            t => format!("{} {m}", self.ty(t)),
        })
    }

    /// Metal has no global resources: they are parameters (see the module doc).
    fn resource(&self, _r: &Resource, _module: &Module) -> String {
        String::new()
    }

    fn prelude(&self, module: &Module) -> String {
        let mut out = String::from("#include <metal_stdlib>\nusing namespace metal;\n\n");
        let names: Vec<String> = module.resources().map(|r| r.name.clone()).collect();
        if !names.is_empty() {
            out.push_str(&format!(
                "// The material's resources are parameters of the functions that read them: {}.\n\n",
                names.join(", ")
            ));
        }
        out
    }
}

/// Prints a module as MSL.
pub fn print(module: &Module) -> String {
    Printer::new(&Msl).module(module)
}

/// Optimize (when asked), reflect, print as an MSL library.
pub fn compile(dag: &Dag, source: &SourceInfo, options: &CompileOptions) -> Result<Compiled, Error> {
    compile_with(dag, source, options, |module, _| print(module))
}

/// The MSL type of an IR value type, for host code: `bool`, `int`, `float` or a `floatN`.
pub fn msl_type(ty: Type) -> String {
    Msl.ty(&Ty::of(ty))
}

pub mod validate;

#[cfg(test)]
mod tests {
    use shadewire::shader::fixtures::{self, reflection};
    use shadewire::shader::material::{MaterialOptions, material};

    use super::*;

    fn msl(dag: &Dag) -> String {
        let r = reflection(dag);
        print(&material(dag, &r, &MaterialOptions::default()))
    }

    /// Every fixture compiles in Apple's Metal compiler, as a library and with a fragment entry, with and without
    /// implicit derivatives (skipped without the compiler).
    #[test]
    fn every_fixture_compiles() {
        for (name, dag) in fixtures::all() {
            let r = reflection(&dag);
            for implicit_derivatives in [true, false] {
                let options = MaterialOptions { implicit_derivatives, ..MaterialOptions::default() };
                let code = print(&material(&dag, &r, &options));
                let tag = format!("{name}_{implicit_derivatives}");
                if !crate::validate::check(&tag, &code, &r).unwrap_or_else(|e| {
                    panic!(
                        "{tag}:
{e}"
                    )
                }) {
                    eprintln!("xcrun metal not found: {tag} not compiled");
                }
            }
        }
    }

    #[test]
    fn resources_are_parameters() {
        let text = msl(&fixtures::everything());
        assert!(text.starts_with("#include <metal_stdlib>\nusing namespace metal;\n"), "{text}");
        assert!(
            text.contains("StandardSurface shadewire_eval_surface(constant MaterialUniforms& shadewire_u, texture2d<float> shadewire_t0, sampler shadewire_s0, texture2d<float> shadewire_t1, sampler shadewire_s1) {"),
            "{text}"
        );
        assert!(text.contains("shadewire_t0.sample(shadewire_s0, float2(n0.x, 1.0f - n0.y)).xyz"), "{text}");
        assert!(text.contains("float3 shadewire_host_transform_normal_object_to_world(float3 v);"), "{text}");
    }

    #[test]
    fn structs_literals_and_pointers() {
        let text = msl(&fixtures::every_lobe());
        assert!(text.contains("return ShadewireBsdf{"), "{text}");
        assert!(text.contains("constant float SHADEWIRE_PI = 3.1415927f;"), "{text}");
        let text = msl(&fixtures::library_ops());
        assert!(text.contains("thread uint& a"), "{text}");
        assert!(text.contains("as_type<uint>("), "{text}");
        assert_eq!(msl_type(Type::Vector2), "float2");
    }

    #[test]
    fn vectors_of_three_are_packed_in_the_uniform_block() {
        let mut dag = Dag::new("m");
        let c = fixtures::add(
            &mut dag,
            shadewire::NodeKind::Uniform { name: "tint".into(), value: shadewire::Value::Color3([1.0; 3]) },
            Type::Color3,
        );
        let w = fixtures::add(
            &mut dag,
            shadewire::NodeKind::Uniform { name: "w".into(), value: shadewire::Value::Float(1.0) },
            Type::Float,
        );
        dag.surface.fields.push(shadewire::SurfaceField { name: "a".into(), ty: Type::Color3, node: c });
        dag.surface.fields.push(shadewire::SurfaceField { name: "b".into(), ty: Type::Float, node: w });
        let text = msl(&dag);
        assert!(text.contains("struct MaterialUniforms {\n    packed_float3 tint;\n    float w;\n};"), "{text}");
    }
}
