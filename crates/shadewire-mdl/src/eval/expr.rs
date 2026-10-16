//! Expressions: names, members and indices, operators, conditionals, assignments, `let` and casts.

use shadewire::{Error, Value};

use super::{Eval, Flow, Frame};
use crate::ast::{Arg, BinOp, DeclKind, Expr, ExprKind, Literal, UnaryOp};
use crate::program::Entity;
use crate::types::{Len, Scalar, Ty};
use crate::value::{Num, V, Val};

/// One step from a variable to the part of it an assignment writes.
#[derive(Clone, Debug)]
pub enum Step {
    Field(usize),
    /// A vector component or matrix column, by number.
    Component(u32),
    /// An array element, by a value known now or at run time.
    Index(Val),
}

const COMPONENTS: [&str; 4] = ["x", "y", "z", "w"];

impl<'p, 'r, 'a> Eval<'p, 'r, 'a> {
    pub fn expr(&mut self, f: &mut Frame, flow: &mut Flow, e: &Expr) -> Result<Val, Error> {
        let origin = Self::origin(f, e.line);
        match &e.kind {
            ExprKind::Literal(l) => Ok(match l {
                Literal::Bool(b) => Val::bool(*b),
                Literal::Int(i) => Val::int(*i as i32),
                Literal::Float(x, double) => {
                    let s = if *double { Scalar::Double } else { Scalar::Float };
                    Val::known(Ty::Scalar(s), Value::Float(*x as f32))
                }
                Literal::String(s) => Val::string(s),
            }),
            ExprKind::Name(name) => {
                if name.is_simple()
                    && let Some(local) = f.lookup(name.last())
                {
                    return Ok(local.val.clone());
                }
                if name.is_simple()
                    && let Some(&n) = f.sizes.get(name.last())
                {
                    return Ok(Val::int(n as i32));
                }
                for entity in self.program.lookup(f.module, name) {
                    match entity {
                        Entity::EnumValue(id, v) => return Ok(Val::new(Ty::Enum(id), V::Enum(v))),
                        Entity::Const { module, decl, index } => return self.constant(module, decl, index),
                        _ => {}
                    }
                }
                if let Some(v) = crate::intrinsics::compiler_constant(name.last()) {
                    return Ok(v);
                }
                Err(self.error(f, e.line, format!("unknown name {name}")))
            }
            ExprKind::ArrayType { .. } => Err(self.error(f, e.line, "an array type is not a value")),
            ExprKind::Call { callee, args } => self.call(f, flow, callee, args, e.line),
            ExprKind::Member { base, name } => {
                let v = self.expr(f, flow, base)?;
                self.member(f, &v, name, e.line)
            }
            ExprKind::Index { base, index } => {
                let v = self.expr(f, flow, base)?;
                let i = self.expr(f, flow, index)?;
                self.index(f, &v, &i, e.line)
            }
            ExprKind::Unary { op: op @ (UnaryOp::PreInc | UnaryOp::PreDec), arg } => {
                let bin = if *op == UnaryOp::PreInc { BinOp::Add } else { BinOp::Sub };
                self.assign(f, flow, Some(bin), arg, None, e.line).map(|(new, _)| new)
            }
            ExprKind::Unary { op, arg } => {
                let v = self.expr(f, flow, arg)?;
                self.unary(*op, v, &origin).map_err(|m| self.error(f, e.line, m))
            }
            ExprKind::PostInc(arg) | ExprKind::PostDec(arg) => {
                let bin = if matches!(e.kind, ExprKind::PostInc(_)) { BinOp::Add } else { BinOp::Sub };
                self.assign(f, flow, Some(bin), arg, None, e.line).map(|(_, old)| old)
            }
            ExprKind::Binary { op: op @ (BinOp::LogicalAnd | BinOp::LogicalOr), lhs, rhs } => {
                let a = self.expr(f, flow, lhs)?;
                let a = self.convert_val(a, &Ty::BOOL, true, &origin).map_err(|m| self.error(f, e.line, m))?;
                // Short circuit on a known left side.
                match (op, a.known_bool()) {
                    (BinOp::LogicalAnd, Some(false)) => return Ok(Val::bool(false)),
                    (BinOp::LogicalOr, Some(true)) => return Ok(Val::bool(true)),
                    _ => {}
                }
                let b = self.expr(f, flow, rhs)?;
                self.binary(*op, a, b, &origin).map_err(|m| self.error(f, e.line, m))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let a = self.expr(f, flow, lhs)?;
                // `0 & x` is zero whatever x is, even an x the IR cannot compute.
                if *op == BinOp::BitAnd && a.known_int() == Some(0) && a.ty == Ty::INT {
                    return Ok(Val::int(0));
                }
                let b = self.expr(f, flow, rhs)?;
                self.binary(*op, a, b, &origin).map_err(|m| self.error(f, e.line, m))
            }
            ExprKind::Ternary { cond, then, otherwise } => {
                let c = self.expr(f, flow, cond)?;
                let c = self.convert_val(c, &Ty::BOOL, true, &origin).map_err(|m| self.error(f, e.line, m))?;
                let c = c.num().expect("a bool").clone();
                if let Some(known) = c.known() {
                    return self.expr(f, flow, if known.as_bool() { then } else { otherwise });
                }
                let outer = flow.mask.clone();
                flow.mask = self.and(&outer, &c, &origin).map_err(|m| self.error(f, e.line, m))?;
                let a = self.expr(f, flow, then);
                let not_c = self.not(&c, &origin).map_err(|m| self.error(f, e.line, m))?;
                flow.mask = self.and(&outer, &not_c, &origin).map_err(|m| self.error(f, e.line, m))?;
                let b = self.expr(f, flow, otherwise);
                flow.mask = outer;
                let (a, b) = (a?, b?);
                let (a, b) = self.unify(f, a, b, e.line)?;
                self.select(&c, a, b, &origin).map_err(|m| self.error(f, e.line, m))
            }
            ExprKind::Assign { op, target, value } => {
                self.assign(f, flow, *op, target, Some(value), e.line).map(|(new, _)| new)
            }
            ExprKind::Sequence(items) => {
                let mut last = Val::bool(false);
                for item in items {
                    last = self.expr(f, flow, item)?;
                }
                Ok(last)
            }
            ExprKind::Let { decls, body } => {
                f.scopes.push(Default::default());
                let result = (|| {
                    for d in decls {
                        self.exec(f, flow, d)?;
                    }
                    self.expr(f, flow, body)
                })();
                f.scopes.pop();
                result
            }
            ExprKind::Cast { ty, arg } => {
                let v = self.expr(f, flow, arg)?;
                let to = self.resolve_type(f, ty, e.line)?.ok_or_else(|| self.error(f, e.line, "a cast to auto"))?;
                self.cast(f, v, &to, e.line)
            }
        }
    }

    /// The two sides of a conditional as one type: an integer side meets a float one as float.
    fn unify(&mut self, f: &Frame, a: Val, b: Val, line: u32) -> Result<(Val, Val), Error> {
        if a.ty == b.ty {
            return Ok((a, b));
        }
        let origin = Self::origin(f, line);
        if self.implicit_cost(&b.ty, &a.ty).is_some() {
            let ty = a.ty.clone();
            let b = self.convert_val(b, &ty, false, &origin).map_err(|m| self.error(f, line, m))?;
            return Ok((a, b));
        }
        if self.implicit_cost(&a.ty, &b.ty).is_some() {
            let ty = b.ty.clone();
            let a = self.convert_val(a, &ty, false, &origin).map_err(|m| self.error(f, line, m))?;
            return Ok((a, b));
        }
        // Arrays of the same element type and different lengths (a size-deferred variable).
        if let (Ty::Array(x, _), Ty::Array(y, _)) = (&a.ty, &b.ty)
            && x == y
        {
            return Ok((a, b));
        }
        Err(self.error(f, line, format!("{} and {} in one conditional", self.type_name(&a.ty), self.type_name(&b.ty))))
    }

    /// `cast<T>(v)`: a struct to a struct of the same field types, or a value to a type it converts to.
    fn cast(&mut self, f: &Frame, v: Val, to: &Ty, line: u32) -> Result<Val, Error> {
        if let (Ty::Struct(_), Ty::Struct(_), V::Struct(fields)) = (&v.ty, to, &v.v) {
            return Ok(Val::new(to.clone(), V::Struct(fields.clone())));
        }
        let origin = Self::origin(f, line);
        self.convert_val(v, to, true, &origin).map_err(|m| self.error(f, line, m))
    }

    /// The value of a module constant, evaluated once in its module.
    pub fn constant(&mut self, module: usize, decl: usize, index: usize) -> Result<Val, Error> {
        if let Some(v) = self.consts.get(&(module, decl, index)) {
            return Ok(v.clone());
        }
        let DeclKind::Const { ty, declarators } = &self.program.modules[module].ast.decls[decl].kind else {
            unreachable!("a constant entity names a constant")
        };
        let (ty, d) = (ty.clone(), declarators[index].clone());
        let mut frame = Frame::new(module, &d.name);
        let mut flow = Flow::new();
        let v = self.declarator_value(&mut frame, &mut flow, &ty, &d)?;
        self.consts.insert((module, decl, index), v.clone());
        Ok(v)
    }

    /// The value a declarator of type `ty` starts with: its initializer, its constructor arguments, or the
    /// type's default.
    pub fn declarator_value(
        &mut self,
        f: &mut Frame,
        flow: &mut Flow,
        ty: &crate::ast::TypeRef,
        d: &crate::ast::Declarator,
    ) -> Result<Val, Error> {
        let origin = Self::origin(f, d.line);
        let declared = self.resolve_type(f, ty, d.line)?;
        match (&d.init, declared) {
            (crate::ast::Init::Value(e), None) => self.expr(f, flow, e),
            (crate::ast::Init::Value(e), Some(t)) => {
                let v = self.expr(f, flow, e)?;
                let t = self.complete_array(&t, &v);
                self.convert(v, &t, false, d.line, f.module).map_err(|err| self.error_at(f, d.line, err))
            }
            (crate::ast::Init::Args(args), Some(t)) => self.construct_by_args(f, flow, &t, args, d.line),
            (crate::ast::Init::None, Some(t)) => self.default_value(&t, d.line),
            (_, None) => Err(self.error(f, d.line, format!("{} = auto without a value ({origin})", d.name))),
        }
    }

    /// `t` with a size-deferred array length taken from `v`.
    pub fn complete_array(&self, t: &Ty, v: &Val) -> Ty {
        match (t, &v.ty) {
            (Ty::Array(e, Len::Deferred(_)), Ty::Array(_, Len::Fixed(n))) => Ty::Array(e.clone(), Len::Fixed(*n)),
            _ => t.clone(),
        }
    }

    fn error_at(&self, f: &Frame, line: u32, err: Error) -> Error {
        match err {
            Error::BadValue { message, .. } => self.error(f, line, message),
            other => other,
        }
    }

    /// `v` converted implicitly (or with `explicit`, as a constructor would) to `to`, errors placed in `module`.
    pub fn convert(&mut self, v: Val, to: &Ty, explicit: bool, line: u32, module: usize) -> Result<Val, Error> {
        let origin = format!("{}:{line}", self.program.modules[module].name);
        self.convert_val(v, to, explicit, &origin).map_err(|message| self.bad(self.at(module, line, ""), message))
    }

    /// `T name(args)`: a variable built by its type's constructor.
    fn construct_by_args(
        &mut self,
        f: &mut Frame,
        flow: &mut Flow,
        t: &Ty,
        args: &[Arg],
        line: u32,
    ) -> Result<Val, Error> {
        let values = self.args(f, flow, args)?;
        self.construct_type(f, t, values, line)
    }

    /// Member `name` of `v`: a struct field or a vector component.
    pub fn member(&mut self, f: &Frame, v: &Val, name: &str, line: u32) -> Result<Val, Error> {
        let origin = Self::origin(f, line);
        if let Ty::Struct(_) = v.ty {
            return self
                .field(v, name)
                .cloned()
                .ok_or_else(|| self.error(f, line, format!("{} has no field {name}", self.type_name(&v.ty))));
        }
        let Some(i) = COMPONENTS.iter().position(|c| *c == name) else {
            return Err(self.error(f, line, format!("{} has no member {name}", self.type_name(&v.ty))));
        };
        self.component(f, v, i as u32, line, &origin)
    }

    fn component(&mut self, f: &Frame, v: &Val, i: u32, line: u32, origin: &str) -> Result<Val, Error> {
        let scalar = v.ty.scalar().map(Ty::Scalar);
        match (&v.v, scalar) {
            (V::Columns(c), _) => {
                let rows = match v.ty {
                    Ty::Matrix(_, _, r) => r,
                    _ => 0,
                };
                c.get(i as usize)
                    .map(|col| Val::new(Ty::float_vector(rows), V::Num(col.clone())))
                    .ok_or_else(|| self.error(f, line, format!("column {i} out of range")))
            }
            (V::Parts(p), Some(s)) => p
                .get(i as usize)
                .map(|n| Val::new(s, V::Num(n.clone())))
                .ok_or_else(|| self.error(f, line, format!("component {i} out of range"))),
            (V::Num(n), Some(s)) if v.ty.width().is_some_and(|w| w > 1 && i < w as u32) => {
                let c = self.extract(n, i, origin).map_err(|m| self.error(f, line, m))?;
                let s = if s == Ty::Scalar(Scalar::Double) { s } else { Ty::FLOAT };
                Ok(Val::new(s, V::Num(c)))
            }
            _ => Err(self.error(f, line, format!("component {i} of {}", self.type_name(&v.ty)))),
        }
    }

    /// `v[i]`: an array element, a vector component or a matrix column, by a known or run-time index.
    pub fn index(&mut self, f: &Frame, v: &Val, i: &Val, line: u32) -> Result<Val, Error> {
        let origin = Self::origin(f, line);
        let count = match &v.v {
            V::Array(e) => e.len(),
            V::Columns(c) => c.len(),
            V::Parts(p) => p.len(),
            V::Num(_) => v.ty.width().unwrap_or(1) as usize,
            _ => return Err(self.error(f, line, format!("{} is not indexable", self.type_name(&v.ty)))),
        };
        let pick = |s: &mut Self, k: usize| -> Result<Val, Error> {
            match &v.v {
                V::Array(e) => Ok(e[k].clone()),
                _ => s.component(f, v, k as u32, line, &origin),
            }
        };
        if let Some(k) = i.known_int() {
            if k < 0 || k as usize >= count {
                // MDL leaves an out-of-range index undefined; the last element is as good as any.
                self.warn(format!("{}: index {k} out of range", Self::origin(f, line)));
                return pick(self, count.saturating_sub(1));
            }
            return pick(self, k as usize);
        }
        if count > 64 {
            return Err(self.error(f, line, format!("an index known only at run time into {count} elements")));
        }
        // A chain of selects, the last element as the fallback.
        let i = self.convert_val(i.clone(), &Ty::INT, false, &origin).map_err(|m| self.error(f, line, m))?;
        let i = i.num().expect("an int").clone();
        let mut out = pick(self, count - 1)?;
        for k in (0..count - 1).rev() {
            let hit = self.compare_eq_int(&i, k as i32, &origin).map_err(|m| self.error(f, line, m))?;
            let element = pick(self, k)?;
            out = self.select(&hit, element, out, &origin).map_err(|m| self.error(f, line, m))?;
        }
        Ok(out)
    }

    fn compare_eq_int(&mut self, i: &Num, k: i32, origin: &str) -> Result<Num, String> {
        let r = self.binary(BinOp::Eq, Val::new(Ty::INT, V::Num(i.clone())), Val::int(k), origin)?;
        Ok(r.num().expect("a bool").clone())
    }

    /// `target op= value` (`op: None` for `=`; `value: None` for an increment by one): the new and the old value.
    pub fn assign(
        &mut self,
        f: &mut Frame,
        flow: &mut Flow,
        op: Option<BinOp>,
        target: &Expr,
        value: Option<&Expr>,
        line: u32,
    ) -> Result<(Val, Val), Error> {
        let origin = Self::origin(f, line);
        let (var, steps) = self.lvalue(f, flow, target)?;
        let whole = f.lookup(&var).ok_or_else(|| self.error(f, line, format!("{var} is not a variable")))?.val.clone();
        let old = self.read_steps(f, &whole, &steps, line)?;
        let rhs = match value {
            Some(e) => self.expr(f, flow, e)?,
            None => Val::int(1),
        };
        let new = match op {
            Some(op) => self.binary(op, old.clone(), rhs, &origin).map_err(|m| self.error(f, line, m))?,
            None => rhs,
        };
        let new = self.convert(new, &old.ty, false, line, f.module).map_err(|e| self.error_at(f, line, e))?;
        if flow.dead() {
            return Ok((new, old));
        }
        let updated = self.write_steps(f, &whole, &steps, new.clone(), line)?;
        let stored = if flow.live() {
            updated
        } else {
            let mask = flow.mask.clone();
            self.select(&mask, updated, whole, &origin).map_err(|m| self.error(f, line, m))?
        };
        f.lookup_mut(&var).expect("a variable").val = stored;
        Ok((new, old))
    }

    /// The variable an assignment target names and the steps into it.
    fn lvalue(&mut self, f: &mut Frame, flow: &mut Flow, target: &Expr) -> Result<(String, Vec<Step>), Error> {
        match &target.kind {
            ExprKind::Name(n) if n.is_simple() && f.lookup(n.last()).is_some() => {
                Ok((n.last().to_string(), Vec::new()))
            }
            ExprKind::Member { base, name } => {
                let (var, mut steps) = self.lvalue(f, flow, base)?;
                let whole = f.lookup(&var).expect("a variable").val.clone();
                let inner = self.read_steps(f, &whole, &steps, target.line)?;
                match inner.ty {
                    Ty::Struct(id) => {
                        let i = self.field_index(id, name).ok_or_else(|| {
                            self.error(f, target.line, format!("{} has no field {name}", self.type_name(&inner.ty)))
                        })?;
                        steps.push(Step::Field(i));
                    }
                    _ => {
                        let i = COMPONENTS
                            .iter()
                            .position(|c| c == name)
                            .ok_or_else(|| self.error(f, target.line, format!("no member {name}")))?;
                        steps.push(Step::Component(i as u32));
                    }
                }
                Ok((var, steps))
            }
            ExprKind::Index { base, index } => {
                let (var, mut steps) = self.lvalue(f, flow, base)?;
                let i = self.expr(f, flow, index)?;
                steps.push(Step::Index(i));
                Ok((var, steps))
            }
            _ => Err(self.error(f, target.line, "not an assignable value")),
        }
    }

    fn read_steps(&mut self, f: &Frame, v: &Val, steps: &[Step], line: u32) -> Result<Val, Error> {
        let mut cur = v.clone();
        for s in steps {
            cur = match s {
                Step::Field(i) => {
                    cur.fields().and_then(|x| x.get(*i)).cloned().ok_or_else(|| self.error(f, line, "no such field"))?
                }
                Step::Component(i) => self.component(f, &cur, *i, line, &Self::origin(f, line))?,
                Step::Index(i) => self.index(f, &cur, i, line)?,
            };
        }
        Ok(cur)
    }

    fn write_steps(&mut self, f: &Frame, v: &Val, steps: &[Step], new: Val, line: u32) -> Result<Val, Error> {
        let origin = Self::origin(f, line);
        let Some((first, rest)) = steps.split_first() else { return Ok(new) };
        match first {
            Step::Field(i) => {
                let mut fields = v.fields().expect("a struct").to_vec();
                fields[*i] = self.write_steps(f, &fields[*i].clone(), rest, new, line)?;
                Ok(Val::new(v.ty.clone(), V::Struct(fields)))
            }
            Step::Component(i) => {
                if let V::Columns(cols) = &v.v {
                    let rows = match v.ty {
                        Ty::Matrix(_, _, r) => r,
                        _ => 0,
                    };
                    let column = Val::new(Ty::float_vector(rows), V::Num(cols[*i as usize].clone()));
                    let column = self.write_steps(f, &column, rest, new, line)?;
                    let mut cols = cols.clone();
                    cols[*i as usize] = column.num().expect("a column").clone();
                    return Ok(Val::new(v.ty.clone(), V::Columns(cols)));
                }
                let mut parts = self.parts(v, &origin).map_err(|m| self.error(f, line, m))?;
                let s = v.ty.scalar().expect("numeric");
                let current = Val::new(Ty::Scalar(s), V::Num(parts[*i as usize].clone()));
                let value = self.write_steps(f, &current, rest, new, line)?;
                let value = self.convert(value, &Ty::Scalar(s), true, line, f.module)?;
                parts[*i as usize] = value.num().expect("a scalar").clone();
                self.from_parts(&v.ty, parts, &origin).map_err(|m| self.error(f, line, m))
            }
            // An index into a vector or matrix: a component known now, else a select over the components.
            Step::Index(i) if !matches!(v.v, V::Array(_)) => match i.known_int() {
                Some(k) => self.write_steps(f, v, &[&[Step::Component(k.max(0) as u32)], rest].concat(), new, line),
                None => {
                    let count = match &v.v {
                        V::Columns(c) => c.len(),
                        _ => v.ty.width().unwrap_or(1) as usize,
                    };
                    let i = i.num().cloned().ok_or_else(|| self.error(f, line, "a non-numeric index"))?;
                    let mut out = v.clone();
                    for k in 0..count {
                        let steps = [&[Step::Component(k as u32)], rest].concat();
                        let written = self.write_steps(f, &out, &steps, new.clone(), line)?;
                        let hit = self.compare_eq_int(&i, k as i32, &origin).map_err(|m| self.error(f, line, m))?;
                        out = self.select(&hit, written, out, &origin).map_err(|m| self.error(f, line, m))?;
                    }
                    Ok(out)
                }
            },
            Step::Index(i) => {
                let elements = v.elements().ok_or_else(|| self.error(f, line, "not an array"))?.to_vec();
                let mut out = Vec::with_capacity(elements.len());
                match i.known_int() {
                    Some(k) => {
                        for (j, e) in elements.into_iter().enumerate() {
                            out.push(if j as i32 == k { self.write_steps(f, &e, rest, new.clone(), line)? } else { e });
                        }
                    }
                    None => {
                        let i = i.num().cloned().ok_or_else(|| self.error(f, line, "a non-numeric index"))?;
                        for (j, e) in elements.into_iter().enumerate() {
                            let written = self.write_steps(f, &e, rest, new.clone(), line)?;
                            let hit = self.compare_eq_int(&i, j as i32, &origin).map_err(|m| self.error(f, line, m))?;
                            out.push(self.select(&hit, written, e, &origin).map_err(|m| self.error(f, line, m))?);
                        }
                    }
                }
                Ok(Val::new(v.ty.clone(), V::Array(out)))
            }
        }
    }
}
