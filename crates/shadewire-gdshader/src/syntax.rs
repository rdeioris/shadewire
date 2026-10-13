//! The Godot shading language spelling of the shared shader tree: the C family's defaults with GLSL's names,
//! textures combined with their samplers, and the address modes Godot's samplers lack emulated in the lookup.

use shadewire::reflection::TextureReflection;
use shadewire::shader::print::{Printer, Syntax, c_scalar, select_helpers};
use shadewire::shader::*;
use shadewire::{AddressMode, FilterMode};

/// Godot's syntax, with the textures the module samples (for their address modes).
pub struct Godot<'a> {
    pub textures: &'a [TextureReflection],
}

fn vector_prefix(s: Scalar) -> &'static str {
    match s {
        Scalar::Bool => "b",
        Scalar::I32 => "i",
        Scalar::U32 => "u",
        Scalar::F32 => "",
    }
}

/// The texture index of a `shadewire_t{index}` name.
fn texture_index(name: &str) -> Option<usize> {
    name.strip_prefix("shadewire_t")?.parse().ok()
}

/// Whether a texture's sampler repeats: Godot sets one repeat flag per texture, so a texture repeats when either
/// axis is periodic, and the other axis is emulated in the lookup.
pub fn repeats(t: &TextureReflection) -> bool {
    t.address.contains(&AddressMode::Periodic)
}

/// `uniform sampler2D shadewire_t<i> : <filter>, <repeat>;`. No `source_color`: the generated code converts
/// colorspaces itself, so Godot must not decode the texels.
pub fn texture_uniform(t: &TextureReflection) -> String {
    let filter = match t.filter {
        FilterMode::Closest => "filter_nearest_mipmap",
        FilterMode::Linear | FilterMode::Cubic => "filter_linear_mipmap_anisotropic",
    };
    let repeat = if repeats(t) { "repeat_enable" } else { "repeat_disable" };
    format!("uniform sampler2D shadewire_t{} : {filter}, {repeat};\n", t.index)
}

impl Godot<'_> {
    /// One axis of a lookup coordinate (already V-flipped): mirror is emulated (Godot has no mirrored repeat; a
    /// mirror is symmetric under the flip), clamp and constant on a repeating sampler clamp in the shader.
    fn axis(t: &TextureReflection, coord: String, mode: AddressMode) -> (String, bool) {
        match mode {
            AddressMode::Periodic => (coord, false),
            AddressMode::Mirror => (format!("(1.0 - abs(mod({coord}, 2.0) - 1.0))"), true),
            AddressMode::Clamp | AddressMode::Constant if repeats(t) => (format!("clamp({coord}, 0.0, 1.0)"), true),
            AddressMode::Clamp | AddressMode::Constant => (coord, false),
        }
    }

    /// The coordinate a lookup of `texture` reads, and whether an axis was emulated.
    fn coordinate(&self, texture: &Expr, st: String) -> (String, bool) {
        let ExprKind::Name(name) = &texture.kind else { return (st, false) };
        let Some(t) = texture_index(name).and_then(|i| self.textures.iter().find(|t| t.index == i)) else {
            return (st, false);
        };
        let (u, eu) = Self::axis(t, format!("({st}).x"), t.address[0]);
        let (v, ev) = Self::axis(t, format!("({st}).y"), t.address[1]);
        if eu || ev { (format!("vec2({u}, {v})"), true) } else { (st, false) }
    }
}

impl Syntax for Godot<'_> {
    fn ty(&self, ty: &Ty) -> String {
        match ty {
            Ty::Scalar(s) => c_scalar(*s).into(),
            Ty::Vector(n, s) => format!("{}vec{n}", vector_prefix(*s)),
            Ty::Matrix(c, r) if c == r => format!("mat{c}"),
            Ty::Matrix(c, r) => panic!("Godot has no mat{c}x{r}"),
            Ty::Struct(name) => name.clone(),
            Ty::Ptr(t) => self.ty(t),
            Ty::Texture2d => "sampler2D".into(),
            Ty::Sampler => panic!("Godot samplers are combined with their textures"),
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
            Builtin::Fma => {
                let product = Expr::binary(BinaryOp::Mul, args[0].clone(), args[1].clone(), args[0].ty.clone());
                format!("({})", p.expr(&Expr::binary(BinaryOp::Add, product, args[2].clone(), args[0].ty.clone())))
            }
            // The sampler argument goes: Godot textures carry their sampler. An emulated axis samples with the
            // derivatives of the coordinate before emulation, so the mip level does not jump at the seams.
            Builtin::TextureSample => {
                let st = p.expr(&args[2]);
                match self.coordinate(&args[0], st.clone()) {
                    (uv, true) => format!("textureGrad({}, {uv}, dFdx({st}), dFdy({st}))", p.expr(&args[0])),
                    (uv, false) => format!("texture({}, {uv})", p.expr(&args[0])),
                }
            }
            Builtin::TextureSampleLevel => {
                let (uv, _) = self.coordinate(&args[0], p.expr(&args[2]));
                format!("textureLod({}, {uv}, {})", p.expr(&args[0]), p.expr(&args[3]))
            }
            Builtin::TextureSampleGrad => {
                let (uv, _) = self.coordinate(&args[0], p.expr(&args[2]));
                format!("textureGrad({}, {uv}, {}, {})", p.expr(&args[0]), p.expr(&args[3]), p.expr(&args[4]))
            }
            other => format!("{}({})", self.builtin_name(other), p.list(args)),
        }
    }

    /// Godot builds a matrix from its columns, not from scalars: `mat3(a, b, ..., i)` is
    /// `mat3(vec3(a, b, c), vec3(d, e, f), vec3(g, h, i))`.
    fn construct(&self, p: &Printer<Self>, ty: &Ty, args: &[Expr]) -> String {
        if let Ty::Matrix(columns, rows) = ty
            && args.len() == usize::from(*columns) * usize::from(*rows)
            && args.iter().all(|a| a.ty.is_scalar())
        {
            let column = Ty::vec(*rows);
            let parts: Vec<String> =
                args.chunks(usize::from(*rows)).map(|c| format!("{}({})", self.ty(&column), p.list(c))).collect();
            return format!("{}({})", self.ty(ty), parts.join(", "));
        }
        format!("{}({})", self.ty(ty), p.list(args))
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

    /// Vector comparisons are functions, as in GLSL.
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

    /// Uniforms and textures are named globals the Godot part declares ([`crate::uniforms`]).
    fn resource(&self, _r: &Resource, _module: &Module) -> String {
        String::new()
    }

    fn prelude(&self, module: &Module) -> String {
        select_helpers(self, module)
    }
}

/// Prints the module's items (structs, constants, functions) as Godot shading language.
pub fn print(module: &Module, textures: &[TextureReflection]) -> String {
    Printer::new(&Godot { textures }).module(module)
}
