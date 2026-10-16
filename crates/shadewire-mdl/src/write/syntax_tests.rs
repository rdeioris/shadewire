use shadewire::shader::print::{Printer, Syntax};
use shadewire::shader::*;

use super::names::{mdl_ident, mdl_string};
use super::syntax::Mdl;

fn expr(e: &Expr) -> String {
    Printer::new(&Mdl).expr(e)
}

fn name(n: &str, ty: Ty) -> Expr {
    Expr::name(n, ty)
}

#[test]
fn types_and_literals() {
    assert_eq!(Mdl.ty(&Ty::vec(3)), "float3");
    assert_eq!(Mdl.ty(&Ty::Vector(3, Scalar::I32)), "int3");
    assert_eq!(Mdl.ty(&Ty::Vector(3, Scalar::U32)), "int3");
    assert_eq!(Mdl.ty(&Ty::Matrix(3, 3)), "float3x3");
    assert_eq!(Mdl.ty(&Ty::Texture2d), "texture_2d");
    assert_eq!(expr(&Expr::u32(0xdeadbeef)), "-559038737");
    assert_eq!(expr(&Expr::f32(1.0)), "1.0");
    assert_eq!(expr(&Expr::i32(-3)), "-3");
}

#[test]
fn unsigned_operations() {
    let x = name("x", Ty::U32);
    let shr = Expr::binary(BinaryOp::Shr, x.clone(), Expr::u32(8), Ty::U32);
    assert_eq!(expr(&shr), "x >>> 8");
    assert_eq!(expr(&x.clone().convert(Ty::F32)), "(float(x >>> 1) * 2.0 + float(x & 1))");
    assert_eq!(expr(&x.clone().bitcast(Ty::F32)), "math::int_bits_to_float(x)");
    assert_eq!(expr(&name("f", Ty::F32).bitcast(Ty::U32)), "math::float_bits_to_int(f)");
    assert_eq!(expr(&name("i", Ty::I32).bitcast(Ty::U32)), "i");
    // Unsigned order: the sign bit flipped on both sides.
    let lt = Expr::binary(BinaryOp::Lt, x.clone(), name("y", Ty::U32), Ty::BOOL);
    assert_eq!(expr(&lt), "(x ^ -2147483648) < (y ^ -2147483648)");
}

#[test]
fn swizzles_and_selects() {
    let v = name("v", Ty::vec(3));
    assert_eq!(expr(&v.clone().swizzle("xy")), "float2(v.x, v.y)");
    assert_eq!(expr(&v.clone().swizzle("x")), "v.x");
    let (f, t) = (name("f", Ty::F32), name("t", Ty::F32));
    let sel = Expr::builtin(Builtin::Select, vec![f, t, name("c", Ty::BOOL)], Ty::F32);
    assert_eq!(expr(&sel), "(c ? t : f)");
    let (fv, tv) = (name("f", Ty::vec(3)), name("t", Ty::vec(3)));
    let sel = Expr::builtin(Builtin::Select, vec![fv, tv, name("c", Ty::Vector(3, Scalar::Bool))], Ty::vec(3));
    assert_eq!(expr(&sel), "float3(c.x ? t.x : f.x, c.y ? t.y : f.y, c.z ? t.z : f.z)");
    let lt = Expr::binary(BinaryOp::Lt, name("a", Ty::vec(3)), name("b", Ty::vec(3)), Ty::Vector(3, Scalar::Bool));
    assert_eq!(expr(&lt), "bool3(a.x < b.x, a.y < b.y, a.z < b.z)");
}

#[test]
fn builtins_are_math_functions() {
    let (a, b, c) = (name("a", Ty::F32), name("b", Ty::F32), name("c", Ty::F32));
    let call = |bi: Builtin, args: Vec<Expr>| expr(&Expr::builtin(bi, args, Ty::F32));
    assert_eq!(call(Builtin::Mix, vec![a.clone(), b.clone(), c.clone()]), "math::lerp(a, b, c)");
    assert_eq!(call(Builtin::Fract, vec![a.clone()]), "math::frac(a)");
    assert_eq!(call(Builtin::InverseSqrt, vec![a.clone()]), "math::rsqrt(a)");
    assert_eq!(call(Builtin::Atan2, vec![a.clone(), b.clone()]), "math::atan2(a, b)");
    assert_eq!(call(Builtin::Dpdx, vec![a.clone()]), "0.0");
    assert_eq!(call(Builtin::Fma, vec![a.clone(), b.clone(), c.clone()]), "(a * b + c)");
    assert_eq!(call(Builtin::Trunc, vec![a.clone()]), "(math::sign(a) * math::floor(math::abs(a)))");
    let all = Expr::builtin(Builtin::All, vec![name("m", Ty::Vector(3, Scalar::Bool))], Ty::BOOL);
    assert_eq!(expr(&all), "math::all(m)");
}

#[test]
fn constructors_take_scalars() {
    let v = name("v", Ty::vec(3));
    let wide = Expr::construct(Ty::vec(4), vec![v.clone(), Expr::f32(1.0)]);
    assert_eq!(expr(&wide), "float4(v.x, v.y, v.z, 1.0)");
    assert_eq!(expr(&Expr::construct(Ty::vec(3), vec![Expr::f32(0.5)])), "float3(0.5)");
    assert_eq!(expr(&Expr::construct(Ty::Struct("S".into()), vec![Expr::f32(1.0), v])), "S(1.0, v)");
}

#[test]
fn names() {
    assert_eq!(mdl_ident("color"), "color_");
    assert_eq!(mdl_ident("in"), "in_");
    assert_eq!(mdl_ident("material"), "material_");
    assert_eq!(mdl_ident("base color"), "base_color");
    assert_eq!(mdl_ident("roughness"), "roughness");
    assert_eq!(mdl_string(r#"C:\tex\a "b".png"#), r#""C:/tex/a \"b\".png""#);
}

#[test]
fn a_function_prints_as_mdl() {
    let body = vec![
        Stmt::Let { name: "y".into(), ty: Ty::F32, value: Some(Expr::f32(2.0)), mutable: true },
        Stmt::Return(Some(Expr::binary(BinaryOp::Mul, name("x", Ty::F32), name("y", Ty::F32), Ty::F32))),
    ];
    let f = Function { name: "f".into(), params: vec![("x".into(), Ty::F32)], ret: Ty::F32, body };
    let text = super::syntax::print(&Module { items: vec![Item::Function(f)] });
    assert_eq!(text, "float f(float x) {\n    float y = 2.0;\n    return x * y;\n}\n");
}
