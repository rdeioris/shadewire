use super::*;

fn module(text: &str) -> Module {
    parse_module(text, None).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn the_vendored_modules_parse() {
    for (name, text) in [
        ("anno", include_str!("../../mdl/std/anno.mdl")),
        ("debug", include_str!("../../mdl/std/debug.mdl")),
        ("df", include_str!("../../mdl/std/df.mdl")),
        ("limits", include_str!("../../mdl/std/limits.mdl")),
        ("math", include_str!("../../mdl/std/math.mdl")),
        ("scene", include_str!("../../mdl/std/scene.mdl")),
        ("state", include_str!("../../mdl/std/state.mdl")),
        ("tex", include_str!("../../mdl/std/tex.mdl")),
        ("base", include_str!("../../mdl/base.mdl")),
        ("core_definitions", include_str!("../../mdl/nvidia/core_definitions.mdl")),
    ] {
        if let Err(e) = parse_module(text, Some(name)) {
            panic!("{name}: {e}");
        }
    }
}

#[test]
fn imports_split_and_keep_their_prefix() {
    let m = module("mdl 1.7;\nimport ::df::*, .::local::thing;\nusing ::anno import hard_range, soft_range;\n");
    assert_eq!(m.version, (1, 7));
    assert_eq!(m.imports.len(), 3);
    let Import::Qualified { path, name, .. } = &m.imports[0] else { panic!() };
    assert_eq!((path.prefix, path.parts.as_slice(), name), (Prefix::Absolute, &["df".to_string()][..], &None));
    let Import::Qualified { path, name, .. } = &m.imports[1] else { panic!() };
    assert_eq!(path.prefix, Prefix::Current);
    assert_eq!(name.as_deref(), Some("thing"));
    let Import::Using { names, .. } = &m.imports[2] else { panic!() };
    assert_eq!(names.as_deref(), Some(&["hard_range".to_string(), "soft_range".to_string()][..]));
}

#[test]
fn binary_operators_follow_c_precedence() {
    let e = parse_expr("a + b * c << 1 == d && e || f").unwrap();
    let ExprKind::Binary { op: BinOp::LogicalOr, lhs, .. } = e.kind else { panic!("{e:?}") };
    let ExprKind::Binary { op: BinOp::LogicalAnd, lhs, .. } = lhs.kind else { panic!() };
    let ExprKind::Binary { op: BinOp::Eq, lhs, .. } = lhs.kind else { panic!() };
    let ExprKind::Binary { op: BinOp::Shl, lhs, .. } = lhs.kind else { panic!() };
    let ExprKind::Binary { op: BinOp::Add, rhs, .. } = lhs.kind else { panic!() };
    assert!(matches!(rhs.kind, ExprKind::Binary { op: BinOp::Mul, .. }));
}

#[test]
fn named_arguments_and_ternaries() {
    let e = parse_expr("df::diffuse_reflection_bsdf(tint: c ? a : b, 0.5)").unwrap();
    let ExprKind::Call { callee, args } = e.kind else { panic!() };
    let name = QualifiedName { absolute: false, parts: vec!["df".into(), "diffuse_reflection_bsdf".into()] };
    assert_eq!(callee.kind, ExprKind::Name(name));
    assert_eq!(args[0].name.as_deref(), Some("tint"));
    assert!(matches!(args[0].value.kind, ExprKind::Ternary { .. }));
    assert_eq!(args[1].name, None);
}

#[test]
fn array_constructors_and_literals() {
    let e = parse_expr("df::bsdf_component[](df::bsdf_component(0.5, a))").unwrap();
    let ExprKind::Call { callee, .. } = e.kind else { panic!() };
    assert!(matches!(callee.kind, ExprKind::ArrayType { size: None, .. }));
    let e = parse_expr("0x1F + 017 + 1.5f + .5 + 2e3d").unwrap();
    fn walk(e: &Expr, out: &mut Vec<Literal>) {
        match &e.kind {
            ExprKind::Binary { lhs, rhs, .. } => {
                walk(lhs, out);
                walk(rhs, out);
            }
            ExprKind::Literal(l) => out.push(l.clone()),
            _ => panic!(),
        }
    }
    let mut values = Vec::new();
    walk(&e, &mut values);
    assert_eq!(
        values,
        [
            Literal::Int(31),
            Literal::Int(15),
            Literal::Float(1.5, false),
            Literal::Float(0.5, false),
            Literal::Float(2000.0, true)
        ]
    );
    assert_eq!(parse_expr(r#""a\"b" "c""#).unwrap().kind, ExprKind::Literal(Literal::String("a\"bc".into())));
}

#[test]
fn materials_lets_and_statements() {
    let m = module(
        r#"mdl 1.6;
import ::df::*;
export material m(color tint = color(0.5) [[ anno::hard_range(color(0), color(1)) ]], uniform bool on = true)
  [[ anno::display_name("M") ]]
  = let {
      bsdf b = df::diffuse_reflection_bsdf(tint: tint);
      material_surface s(scattering: b);
  } in material(surface: s);
export material v(*) = m(color(1.0));
float f(float x) {
    float y = 0.0, z;
    for (int i = 0; i < 4; ++i) { if (i == 2) continue; else y += x; }
    switch (int(x)) { case 0: y = 1; break; default: y = 2; }
    while (y > 1.0) y /= 2.0;
    do { y++; } while (false);
    return y > 0.5 ? y : -y;
}
float g(float x) = x * x;
"#,
    );
    assert_eq!(m.decls.len(), 4);
    let DeclKind::Function(f) = &m.decls[0].kind else { panic!() };
    assert!(m.decls[0].export);
    assert_eq!(f.name, "m");
    let params = f.params.as_ref().unwrap();
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].annotations[0].name.last(), "hard_range");
    assert_eq!(params[1].ty.frequency, Some(Frequency::Uniform));
    assert_eq!(f.annotations[0].name.last(), "display_name");
    let Body::Expr(Expr { kind: ExprKind::Let { decls, .. }, .. }) = &f.body else { panic!() };
    assert_eq!(decls.len(), 2);
    let DeclKind::Function(v) = &m.decls[1].kind else { panic!() };
    assert_eq!(v.params, None);
    let DeclKind::Function(f) = &m.decls[2].kind else { panic!() };
    let Body::Block(body) = &f.body else { panic!() };
    assert_eq!(body.len(), 6);
    assert!(matches!(body[1].kind, StmtKind::For { .. }));
    assert!(matches!(body[2].kind, StmtKind::Switch { ref cases, .. } if cases.len() == 2));
    let DeclKind::Function(g) = &m.decls[3].kind else { panic!() };
    assert!(matches!(g.body, Body::Expr(_)));
}

#[test]
fn errors_name_the_line() {
    let e = parse_module("mdl 1.6;\nexport float f() {\n  return 1.0 +;\n}\n", Some("bad.mdl")).unwrap_err();
    let Error::Parse { at, .. } = e else { panic!("{e:?}") };
    assert_eq!(at.line, 3);
    assert_eq!(at.file.as_deref(), Some("bad.mdl"));
}
