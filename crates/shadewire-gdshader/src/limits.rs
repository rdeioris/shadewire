//! Limits for untrusted documents on what the shader costs the GPU: how many textures it samples, and how often a
//! loop whose count the document controls may run (fractal noise reads its octaves from an input, which can be a
//! uniform a game script sets).

use std::collections::HashSet;

use shadewire::Error;
use shadewire::shader::*;

/// `Error::Limit` when the material samples more than `max` textures.
pub fn check_textures(count: usize, max: Option<usize>) -> Result<(), Error> {
    match max {
        Some(max) if count > max => {
            Err(Error::Limit(format!("the material samples {count} textures, more than {max} (max_textures)")))
        }
        _ => Ok(()),
    }
}

/// Bounds every `for` loop of the module to `max` iterations: a loop `i < n` (or `<=`) whose bound is not a literal
/// or a module constant runs to `min(n, max)` instead. A loop of any other shape is an error, so a library loop this
/// pass cannot bound is found by the tests instead of emitted unbounded.
pub fn clamp_loops(module: &mut Module, max: u32) -> Result<(), Error> {
    let constants: HashSet<String> = module
        .items
        .iter()
        .filter_map(|item| if let Item::Const { name, .. } = item { Some(name.clone()) } else { None })
        .collect();
    for item in &mut module.items {
        if let Item::Function(function) = item {
            clamp_body(&mut function.body, &constants, max, &function.name)?;
        }
    }
    Ok(())
}

fn clamp_body(body: &mut [Stmt], constants: &HashSet<String>, max: u32, function: &str) -> Result<(), Error> {
    for stmt in body {
        match stmt {
            Stmt::If { then, otherwise, .. } => {
                clamp_body(then, constants, max, function)?;
                clamp_body(otherwise, constants, max, function)?;
            }
            Stmt::Switch { cases, default, .. } => {
                for (_, case) in cases {
                    clamp_body(case, constants, max, function)?;
                }
                clamp_body(default, constants, max, function)?;
            }
            Stmt::For { cond, body, .. } => {
                clamp_bound(cond, constants, max, function)?;
                clamp_body(body, constants, max, function)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn clamp_bound(cond: &mut Expr, constants: &HashSet<String>, max: u32, function: &str) -> Result<(), Error> {
    let ExprKind::Binary { op: BinaryOp::Lt | BinaryOp::Le, lhs, rhs } = &mut cond.kind else {
        return Err(unbounded(function));
    };
    if !matches!(lhs.kind, ExprKind::Name(_)) {
        return Err(unbounded(function));
    }
    let fixed = match &rhs.kind {
        ExprKind::Literal(_) => true,
        ExprKind::Name(name) => constants.contains(name),
        _ => false,
    };
    if fixed {
        return Ok(());
    }
    let limit = match rhs.ty {
        Ty::Scalar(Scalar::I32) => Expr::i32(max.min(i32::MAX as u32) as i32),
        Ty::Scalar(Scalar::U32) => Expr::u32(max),
        _ => return Err(unbounded(function)),
    };
    let ty = rhs.ty.clone();
    let bound = std::mem::replace(rhs.as_mut(), Expr::i32(0));
    **rhs = Expr::builtin(Builtin::Min, vec![bound, limit], ty);
    Ok(())
}

fn unbounded(function: &str) -> Error {
    Error::Limit(format!("{function}: a loop max_loop_iterations cannot bound"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn function(cond: Expr) -> Module {
        let init = Stmt::Let { name: "i".into(), ty: Ty::I32, value: Some(Expr::i32(0)), mutable: true };
        let update = Stmt::Assign { target: Expr::name("i", Ty::I32), op: Some(BinaryOp::Add), value: Expr::i32(1) };
        let body = vec![Stmt::For { init: Box::new(init), cond, update: Box::new(update), body: Vec::new() }];
        let f = Function { name: "f".into(), params: vec![("n".into(), Ty::I32)], ret: Ty::Void, body };
        Module { items: vec![Item::Const { name: "K".into(), ty: Ty::I32, value: Expr::i32(2) }, Item::Function(f)] }
    }

    fn cond(op: BinaryOp, bound: Expr) -> Expr {
        Expr::binary(op, Expr::name("i", Ty::I32), bound, Ty::BOOL)
    }

    fn loop_cond(module: &Module) -> &Expr {
        let Some(Stmt::For { cond, .. }) = module.functions().next().unwrap().body.first() else { panic!() };
        cond
    }

    #[test]
    fn a_parameter_bound_is_clamped() {
        let mut module = function(cond(BinaryOp::Lt, Expr::name("n", Ty::I32)));
        clamp_loops(&mut module, 16).unwrap();
        let want =
            cond(BinaryOp::Lt, Expr::builtin(Builtin::Min, vec![Expr::name("n", Ty::I32), Expr::i32(16)], Ty::I32));
        assert_eq!(loop_cond(&module), &want);
    }

    #[test]
    fn literal_and_constant_bounds_stay() {
        for bound in [Expr::i32(1), Expr::name("K", Ty::I32)] {
            let mut module = function(cond(BinaryOp::Le, bound));
            let before = module.clone();
            clamp_loops(&mut module, 16).unwrap();
            assert_eq!(module, before);
        }
    }

    #[test]
    fn a_loop_of_another_shape_is_an_error() {
        let mut module = function(cond(BinaryOp::Gt, Expr::name("n", Ty::I32)));
        let err = clamp_loops(&mut module, 16).unwrap_err();
        assert_eq!(err, Error::Limit("f: a loop max_loop_iterations cannot bound".into()));
    }

    /// Every loop of the shared library is one this pass bounds; only fractal noise's count is the document's.
    #[test]
    fn every_library_loop_is_bounded() {
        for name in library::FILES {
            let mut module = library::module(name).clone();
            clamp_loops(&mut module, 16).unwrap_or_else(|e| panic!("{name}: {e}"));
            let original = library::module(name).functions();
            let clamped = module.functions().zip(original).filter(|(after, before)| after != before).count();
            assert_eq!(clamped, if name == "noise_fractal" { 4 } else { 0 }, "{name}");
        }
    }

    #[test]
    fn texture_counts_over_the_limit_fail() {
        assert!(check_textures(16, Some(16)).is_ok());
        assert!(check_textures(17, None).is_ok());
        let err = check_textures(17, Some(16)).unwrap_err();
        assert_eq!(err.to_string(), "the material samples 17 textures, more than 16 (max_textures)");
    }
}
