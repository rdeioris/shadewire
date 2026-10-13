//! Types parsed shader code: every expression gets its type, untyped `let`s and constants take their value's,
//! abstract literals (`1`, `0.5`) take the type their context needs, a call of a struct name becomes its
//! constructor and a member of a vector becomes a swizzle.

use std::collections::HashMap;

use super::ast::*;

/// A type error with the function or item it is in.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeError {
    pub context: String,
    pub message: String,
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.context, self.message)
    }
}

type Result<T> = std::result::Result<T, String>;

/// What the code can refer to besides its own locals: structs, constants, resources and function signatures.
#[derive(Clone, Debug, Default)]
pub struct Env {
    pub structs: HashMap<String, Vec<(String, Ty)>>,
    pub globals: HashMap<String, Ty>,
    pub functions: HashMap<String, (Vec<Ty>, Ty)>,
}

impl Env {
    /// Adds the declarations of `module` (its items stay as they are).
    pub fn declare(&mut self, module: &Module) {
        for item in &module.items {
            match item {
                Item::Struct(s) => {
                    self.structs.insert(s.name.clone(), s.members.clone());
                }
                Item::Const { name, ty, .. } if *ty != Ty::Unknown => {
                    self.globals.insert(name.clone(), ty.clone());
                }
                Item::Resource(r) => {
                    let ty = match &r.kind {
                        ResourceKind::Uniform(s) => Ty::Struct(s.clone()),
                        ResourceKind::Texture2d => Ty::Texture2d,
                        ResourceKind::Sampler => Ty::Sampler,
                    };
                    self.globals.insert(r.name.clone(), ty);
                }
                Item::Function(f) | Item::Extern(f) => {
                    let params = f.params.iter().map(|(_, t)| t.clone()).collect();
                    self.functions.insert(f.name.clone(), (params, f.ret.clone()));
                }
                _ => {}
            }
        }
    }
}

/// Types `module` in place against `env` plus its own declarations, which it adds to `env`.
pub fn infer(module: &mut Module, env: &mut Env) -> std::result::Result<(), TypeError> {
    env.declare(module);
    for item in &mut module.items {
        match item {
            Item::Const { name, ty, value } => {
                let context = format!("const {name}");
                let mut cx = Cx { env: &*env, scopes: vec![HashMap::new()] };
                cx.expr(value, known(ty)).map_err(|message| TypeError { context, message })?;
                if *ty == Ty::Unknown {
                    *ty = value.ty.clone();
                }
                env.globals.insert(name.clone(), ty.clone());
            }
            Item::Function(f) => {
                let context = format!("fn {}", f.name);
                let mut cx = Cx { env: &*env, scopes: vec![f.params.iter().cloned().collect()] };
                cx.block(&mut f.body, &f.ret).map_err(|message| TypeError { context, message })?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn known(ty: &Ty) -> Option<&Ty> {
    (*ty != Ty::Unknown).then_some(ty)
}

/// Whether an expression is made of abstract literals only, so its type comes from its context.
fn is_abstract(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Literal(_) => e.ty == Ty::Unknown,
        ExprKind::Unary { arg, .. } => is_abstract(arg),
        ExprKind::Binary { lhs, rhs, .. } => is_abstract(lhs) && is_abstract(rhs),
        _ => false,
    }
}

struct Cx<'a> {
    env: &'a Env,
    scopes: Vec<HashMap<String, Ty>>,
}

impl Cx<'_> {
    fn lookup(&self, name: &str) -> Result<Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(t) = scope.get(name) {
                return Ok(t.clone());
            }
        }
        self.env.globals.get(name).cloned().ok_or_else(|| format!("unknown name `{name}`"))
    }

    fn block(&mut self, body: &mut [Stmt], ret: &Ty) -> Result<()> {
        self.scopes.push(HashMap::new());
        for s in body.iter_mut() {
            self.stmt(s, ret)?;
        }
        self.scopes.pop();
        Ok(())
    }

    fn stmt(&mut self, s: &mut Stmt, ret: &Ty) -> Result<()> {
        match s {
            Stmt::Let { name, ty, value, .. } => {
                if let Some(v) = value {
                    self.expr(v, known(ty))?;
                    if *ty == Ty::Unknown {
                        *ty = v.ty.clone();
                    }
                } else if *ty == Ty::Unknown {
                    return Err(format!("`var {name}` needs a type or a value"));
                }
                self.scopes.last_mut().expect("a scope").insert(name.clone(), ty.clone());
            }
            Stmt::Assign { target, value, .. } => {
                self.expr(target, None)?;
                let want = target.ty.pointee().clone();
                self.expr(value, Some(&want))?;
            }
            Stmt::If { cond, then, otherwise } => {
                self.expr(cond, Some(&Ty::BOOL))?;
                self.block(then, ret)?;
                self.block(otherwise, ret)?;
            }
            Stmt::For { init, cond, update, body } => {
                self.scopes.push(HashMap::new());
                self.stmt(init, ret)?;
                self.expr(cond, Some(&Ty::BOOL))?;
                self.stmt(update, ret)?;
                self.block(body, ret)?;
                self.scopes.pop();
            }
            Stmt::Switch { selector, cases, default } => {
                self.expr(selector, None)?;
                let want = selector.ty.clone();
                for (values, body) in cases.iter_mut() {
                    for v in values.iter_mut() {
                        self.expr(v, Some(&want))?;
                    }
                    self.block(body, ret)?;
                }
                self.block(default, ret)?;
            }
            Stmt::Return(Some(v)) => self.expr(v, Some(ret))?,
            Stmt::Expr(e) => self.expr(e, None)?,
            Stmt::Return(None) | Stmt::Break | Stmt::Continue | Stmt::Comment { .. } => {}
        }
        Ok(())
    }

    /// Types `args` that are not abstract, then the abstract ones as `want`.
    fn args_like(&mut self, args: &mut [Expr], want: Option<&Ty>) -> Result<()> {
        for a in args.iter_mut().filter(|a| !is_abstract(a)) {
            self.expr(a, None)?;
        }
        let first = args.iter().find(|a| a.ty != Ty::Unknown).map(|a| a.ty.clone());
        let like = first.or(want.cloned());
        for a in args.iter_mut().filter(|a| a.ty == Ty::Unknown) {
            self.expr(a, like.as_ref())?;
        }
        Ok(())
    }

    fn expr(&mut self, e: &mut Expr, want: Option<&Ty>) -> Result<()> {
        let ty = match &mut e.kind {
            ExprKind::Literal(lit) => {
                if e.ty != Ty::Unknown {
                    return Ok(());
                }
                let scalar = want.and_then(Ty::scalar).unwrap_or(match lit {
                    Literal::F32(_) => Scalar::F32,
                    _ => Scalar::I32,
                });
                *lit = match (*lit, scalar) {
                    (Literal::I32(v), Scalar::F32) => Literal::F32(v as f32),
                    (Literal::I32(v), Scalar::U32) => {
                        Literal::U32(u32::try_from(v).map_err(|_| format!("{v} is not a u32"))?)
                    }
                    (Literal::I32(v), Scalar::I32) => Literal::I32(v),
                    (Literal::F32(v), Scalar::F32) => Literal::F32(v),
                    (l, s) => return Err(format!("literal {l:?} as {s:?}")),
                };
                Ty::Scalar(scalar)
            }
            ExprKind::Name(name) => self.lookup(name)?,
            ExprKind::Call { function, args } => {
                if let Some(members) = self.env.structs.get(function.as_str()) {
                    let members = members.clone();
                    if members.len() != args.len() {
                        return Err(format!("{function} has {} members, given {}", members.len(), args.len()));
                    }
                    for (a, (_, t)) in args.iter_mut().zip(&members) {
                        self.expr(a, Some(t))?;
                    }
                    let ty = Ty::Struct(function.clone());
                    let args = std::mem::take(args);
                    e.kind = ExprKind::Construct(args);
                    ty
                } else {
                    let (params, ret) = self
                        .env
                        .functions
                        .get(function.as_str())
                        .cloned()
                        .ok_or(format!("unknown function `{function}`"))?;
                    if params.len() != args.len() {
                        return Err(format!("{function} takes {} arguments, given {}", params.len(), args.len()));
                    }
                    for (a, p) in args.iter_mut().zip(&params) {
                        self.expr(a, Some(p))?;
                    }
                    ret
                }
            }
            ExprKind::Builtin { builtin, args } => self.builtin(*builtin, args, want)?,
            ExprKind::Construct(args) => {
                let part = match &e.ty {
                    Ty::Vector(_, s) => Ty::Scalar(*s),
                    Ty::Matrix(..) => Ty::F32,
                    Ty::Struct(name) => {
                        let members = self.env.structs.get(name).cloned().ok_or(format!("unknown struct {name}"))?;
                        for (a, (_, t)) in args.iter_mut().zip(&members) {
                            self.expr(a, Some(t))?;
                        }
                        return Ok(());
                    }
                    other => return Err(format!("construct {other:?}")),
                };
                for a in args.iter_mut() {
                    self.expr(a, Some(&part))?;
                }
                return Ok(());
            }
            ExprKind::Convert(inner) | ExprKind::Bitcast(inner) => {
                self.expr(inner, None)?;
                return Ok(());
            }
            ExprKind::Unary { op, arg } => {
                self.expr(arg, want)?;
                match op {
                    UnaryOp::Not => arg.ty.with_scalar(Scalar::Bool),
                    _ => arg.ty.clone(),
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let op = *op;
                let arith_want =
                    if op.is_comparison() || matches!(op, BinaryOp::And | BinaryOp::Or) { None } else { want };
                match (is_abstract(lhs), is_abstract(rhs)) {
                    (true, true) => {
                        self.expr(lhs, arith_want)?;
                        self.expr(rhs, arith_want)?;
                    }
                    (true, false) => {
                        self.expr(rhs, None)?;
                        let like = rhs.ty.clone();
                        self.expr(lhs, Some(&like))?;
                    }
                    (false, true) => {
                        self.expr(lhs, None)?;
                        // A shift amount is u32 whatever the shifted value is.
                        let like = if matches!(op, BinaryOp::Shl | BinaryOp::Shr) { Ty::U32 } else { lhs.ty.clone() };
                        self.expr(rhs, Some(&like))?;
                    }
                    (false, false) => {
                        self.expr(lhs, None)?;
                        self.expr(rhs, None)?;
                    }
                }
                binary_type(op, &lhs.ty, &rhs.ty)?
            }
            ExprKind::Member { base, member } => {
                self.expr(base, None)?;
                match &base.ty {
                    Ty::Vector(..) => {
                        let components: String = member
                            .chars()
                            .map(|c| match c {
                                'r' => 'x',
                                'g' => 'y',
                                'b' => 'z',
                                'a' => 'w',
                                other => other,
                            })
                            .collect();
                        let base = std::mem::replace(base, Box::new(Expr::bool(false)));
                        *e = (*base).swizzle(&components);
                        return Ok(());
                    }
                    Ty::Struct(name) => {
                        let members = self.env.structs.get(name).ok_or(format!("unknown struct {name}"))?;
                        members
                            .iter()
                            .find(|(m, _)| m == member)
                            .map(|(_, t)| t.clone())
                            .ok_or(format!("{name} has no member {member}"))?
                    }
                    other => return Err(format!("member {member} of {other:?}")),
                }
            }
            ExprKind::Swizzle { base, components } => {
                self.expr(base, None)?;
                let s = base.ty.scalar().ok_or("swizzle of a non-vector")?;
                if components.len() == 1 { Ty::Scalar(s) } else { Ty::Vector(components.len() as u8, s) }
            }
            ExprKind::Index { base, index } => {
                self.expr(base, None)?;
                self.expr(index, Some(&Ty::I32))?;
                match base.ty.pointee() {
                    Ty::Vector(_, s) => Ty::Scalar(*s),
                    Ty::Matrix(_, rows) => Ty::vec(*rows),
                    other => return Err(format!("index of {other:?}")),
                }
            }
            ExprKind::Deref(inner) => {
                self.expr(inner, None)?;
                match &inner.ty {
                    Ty::Ptr(t) => (**t).clone(),
                    other => return Err(format!("dereference of {other:?}")),
                }
            }
            ExprKind::AddressOf(inner) => {
                self.expr(inner, want.map(Ty::pointee))?;
                Ty::Ptr(Box::new(inner.ty.clone()))
            }
        };
        e.ty = ty;
        Ok(())
    }

    fn builtin(&mut self, b: Builtin, args: &mut [Expr], want: Option<&Ty>) -> Result<Ty> {
        use Builtin::*;
        match b {
            TextureSample | TextureSampleLevel | TextureSampleGrad => {
                for (i, a) in args.iter_mut().enumerate() {
                    let t = if i == 3 && b == TextureSampleLevel { Ty::F32 } else { Ty::vec(2) };
                    self.expr(a, if i < 2 { None } else { Some(&t) })?;
                }
            }
            Select if args.len() == 3 => {
                self.args_like(&mut args[..2], want)?;
                let ty = args[0].ty.with_scalar(Scalar::Bool);
                self.expr(&mut args[2], Some(&ty))?;
            }
            Mix if args.len() == 3 => {
                self.args_like(&mut args[..2], want)?;
                // The amount may be a scalar for a vector mix.
                let amount = if is_abstract(&args[2]) { Ty::F32 } else { args[0].ty.clone() };
                self.expr(&mut args[2], Some(&amount))?;
            }
            Cross => self.args_like(args, Some(&Ty::vec(3)))?,
            Length | Distance | Dot | All | Any | Transpose => self.args_like(args, None)?,
            _ => self.args_like(args, want)?,
        }
        builtin_type(b, args)
    }
}

/// The type a built-in returns for typed arguments.
pub fn builtin_type(b: Builtin, args: &[Expr]) -> Result<Ty> {
    use Builtin::*;
    let arity = match b {
        TextureSample | Clamp | Mix | Smoothstep | Select | Fma => 3,
        TextureSampleLevel => 4,
        TextureSampleGrad => 5,
        Pow | Min | Max | Step | Atan2 | Distance | Dot | Cross | Reflect => 2,
        _ => 1,
    };
    if args.len() != arity {
        return Err(format!("{} takes {arity} arguments, given {}", b.wgsl_name(), args.len()));
    }
    Ok(match b {
        TextureSample | TextureSampleLevel | TextureSampleGrad => Ty::vec(4),
        All | Any => Ty::BOOL,
        Length | Distance | Dot => Ty::Scalar(args[0].ty.scalar().ok_or("a numeric argument")?),
        Cross => Ty::vec(3),
        Transpose => match args[0].ty {
            Ty::Matrix(c, r) => Ty::Matrix(r, c),
            ref other => return Err(format!("transpose of {other:?}")),
        },
        // The result is the type of x, the last argument.
        Smoothstep => args[2].ty.clone(),
        _ => args[0].ty.clone(),
    })
}

/// The type of a binary expression.
pub fn binary_type(op: BinaryOp, l: &Ty, r: &Ty) -> Result<Ty> {
    use BinaryOp::*;
    Ok(match op {
        And | Or => Ty::BOOL,
        Eq | Ne | Lt | Le | Gt | Ge => {
            let width = l.width().max(r.width()).ok_or(format!("compare {l:?} and {r:?}"))?;
            if width == 1 { Ty::BOOL } else { Ty::Vector(width, Scalar::Bool) }
        }
        Shl | Shr | BitAnd | BitOr | BitXor => l.clone(),
        Mul => match (l, r) {
            (Ty::Matrix(_, rows), Ty::Vector(..)) => Ty::vec(*rows),
            (Ty::Vector(..), Ty::Matrix(cols, _)) => Ty::vec(*cols),
            (Ty::Matrix(_, rows), Ty::Matrix(cols, _)) => Ty::Matrix(*cols, *rows),
            (Ty::Matrix(..), _) => l.clone(),
            (_, Ty::Matrix(..)) => r.clone(),
            _ => wider(l, r),
        },
        Add | Sub | Div | Rem => wider(l, r),
    })
}

/// A scalar meets a vector as the vector.
fn wider(l: &Ty, r: &Ty) -> Ty {
    if l.is_scalar() { r.clone() } else { l.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader::parse::parse;

    fn typed(src: &str) -> Module {
        let mut m = parse(src).unwrap();
        infer(&mut m, &mut Env::default()).unwrap();
        m
    }

    fn function<'a>(m: &'a Module, name: &str) -> &'a Function {
        m.functions().find(|f| f.name == name).unwrap()
    }

    #[test]
    fn abstract_literals_take_their_context() {
        let m = typed("fn f(v: vec3<f32>, k: u32) -> vec3<f32> { let a = k << 2; let b = 1 - v; return b * 2; }");
        let f = function(&m, "f");
        let Stmt::Let { ty, value: Some(v), .. } = &f.body[0] else { panic!() };
        assert_eq!(*ty, Ty::U32);
        let ExprKind::Binary { rhs, .. } = &v.kind else { panic!() };
        assert_eq!(rhs.kind, ExprKind::Literal(Literal::U32(2)));
        let Stmt::Let { ty, value: Some(v), .. } = &f.body[1] else { panic!() };
        assert_eq!(*ty, Ty::vec(3));
        let ExprKind::Binary { lhs, .. } = &v.kind else { panic!() };
        assert_eq!(lhs.kind, ExprKind::Literal(Literal::F32(1.0)));
    }

    #[test]
    fn struct_calls_construct_and_vector_members_swizzle() {
        let m = typed(
            "struct S { a: vec3<f32>, b: f32 }\nfn f(c: vec4<f32>) -> S { let s = S(c.rgb, 1); return S(s.a.zyx, c.a); }",
        );
        let f = function(&m, "f");
        let Stmt::Let { ty, value: Some(v), .. } = &f.body[0] else { panic!() };
        assert_eq!(*ty, Ty::Struct("S".into()));
        let ExprKind::Construct(args) = &v.kind else { panic!("{v:?}") };
        assert!(matches!(&args[0].kind, ExprKind::Swizzle { components, .. } if components == "xyz"));
        assert_eq!(args[1].kind, ExprKind::Literal(Literal::F32(1.0)));
    }

    #[test]
    fn pointers_comparisons_and_matrices() {
        let m = typed(
            "fn g(p: ptr<function, u32>) { *p += 1; }\nfn f(m: mat3x3<f32>, v: vec3<f32>) -> vec3<bool> {\n\
                 var k = 0u; g(&k); let w = transpose(m) * v; return w < vec3<f32>(f32(k)); }",
        );
        let f = function(&m, "f");
        let Stmt::Let { ty, .. } = &f.body[2] else { panic!() };
        assert_eq!(*ty, Ty::vec(3));
        let Stmt::Return(Some(r)) = &f.body[3] else { panic!() };
        assert_eq!(r.ty, Ty::Vector(3, Scalar::Bool));
        let g = function(&m, "g");
        let Stmt::Assign { value, .. } = &g.body[0] else { panic!() };
        assert_eq!(value.ty, Ty::U32);
    }

    #[test]
    fn unknown_names_are_errors_with_their_function() {
        let mut m = parse("fn f() -> f32 { return nope; }").unwrap();
        let e = infer(&mut m, &mut Env::default()).unwrap_err();
        assert_eq!(e.to_string(), "fn f: unknown name `nope`");
    }
}
