//! Calls: constructors, overload resolution by MDL's rules, argument binding with defaults, and inlining (or an
//! intrinsic's handler for the standard modules).

use std::collections::HashMap;

use shadewire::Error;

use super::{Eval, Flow, Frame, MAX_CALL_DEPTH};
use crate::ast::{Annotation, Arg, Body, Expr, ExprKind, Function, Literal};
use crate::program::Entity;
use crate::types::{Len, Ty};
use crate::value::{V, Val};

/// Arguments as evaluated: a name for a named one.
pub type Args = Vec<(Option<String>, Val)>;

/// `since(1, 4)` or `removed(1, 6)` among a declaration's annotations, as `(1, 4)`.
fn version_annotation(annotations: &[Annotation], name: &str) -> Option<(u32, u32)> {
    let a = annotations.iter().find(|a| a.name.last() == name)?;
    let number = |i: usize| match a.args.get(i).map(|x| &x.value.kind) {
        Some(ExprKind::Literal(Literal::Int(n))) => Some(*n as u32),
        _ => None,
    };
    Some((number(0)?, number(1).unwrap_or(0)))
}

/// Whether a declaration exists for a module of `version`: since its `since`, before its `removed`.
pub fn visible(annotations: &[Annotation], version: (u32, u32)) -> bool {
    let since = version_annotation(annotations, "since").unwrap_or((0, 0));
    let removed = version_annotation(annotations, "removed").unwrap_or((u32::MAX, 0));
    since <= version && version < removed
}

/// A candidate's arguments bound to its parameters, and its conversion cost.
struct Binding {
    module: usize,
    decl: usize,
    args: Vec<Option<Val>>,
    sizes: HashMap<String, usize>,
    cost: u32,
}

impl<'p, 'r, 'a> Eval<'p, 'r, 'a> {
    /// The arguments of a call, evaluated in the caller.
    pub fn args(&mut self, f: &mut Frame, flow: &mut Flow, args: &[Arg]) -> Result<Args, Error> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            let v = self.expr(f, flow, &a.value)?;
            out.push((a.name.clone(), v));
        }
        Ok(out)
    }

    pub fn call(
        &mut self,
        f: &mut Frame,
        flow: &mut Flow,
        callee: &Expr,
        args: &[Arg],
        line: u32,
    ) -> Result<Val, Error> {
        let values = self.args(f, flow, args)?;
        match &callee.kind {
            ExprKind::ArrayType { elem, size } => {
                let elem = self.named_type(f.module, elem, line)?;
                let size = match size {
                    Some(crate::ast::ArraySize::Sized(e)) => {
                        let n = self.expr(f, flow, e)?;
                        Some(n.known_int().ok_or_else(|| self.error(f, line, "an array size must be known"))? as usize)
                    }
                    _ => None,
                };
                if values.iter().any(|(n, _)| n.is_some()) {
                    return Err(self.error(f, line, "named arguments to an array constructor"));
                }
                let origin = Self::origin(f, line);
                let given = values.into_iter().map(|(_, v)| v).collect();
                let mut array =
                    self.construct_array(&elem, size, given, &origin).map_err(|m| self.error(f, line, m))?;
                // Elements not given take the type's default.
                if let (V::Array(e), Ty::Array(_, Len::Fixed(n))) = (&mut array.v, &array.ty) {
                    let n = *n;
                    while e.len() < n {
                        e.push(self.default_value(&elem, line)?);
                    }
                }
                Ok(array)
            }
            ExprKind::Name(name) => {
                if name.is_simple()
                    && let Some(ty) = Ty::builtin(name.last())
                {
                    return self.construct_type(f, &ty, values, line);
                }
                let entities = self.program.lookup(f.module, name);
                let mut functions = Vec::new();
                for entity in &entities {
                    match entity {
                        Entity::Struct(id) => return self.construct_type(f, &Ty::Struct(*id), values, line),
                        Entity::Typedef { .. } | Entity::Enum(_) => {
                            let ty = self.named_type(f.module, name, line)?;
                            return self.construct_type(f, &ty, values, line);
                        }
                        Entity::Function { module, decl } => functions.push((*module, *decl)),
                        _ => {}
                    }
                }
                if functions.is_empty() {
                    return Err(self.error(f, line, format!("unknown function {name}")));
                }
                self.call_overloads(f, &name.to_string(), &functions, values, line)
            }
            _ => Err(self.error(f, line, "only named functions can be called")),
        }
    }

    /// `ty(args)`: a struct from its fields, or a built-in type's constructor.
    pub fn construct_type(&mut self, f: &Frame, ty: &Ty, values: Args, line: u32) -> Result<Val, Error> {
        let origin = Self::origin(f, line);
        match ty {
            Ty::Struct(id) => {
                let decl = self.program.struct_decl(*id);
                let names: Vec<String> = decl.fields.iter().map(|x| x.name.clone()).collect();
                let mut given = Vec::new();
                let mut positional = 0;
                for (name, v) in values {
                    let index = match name {
                        Some(n) => names
                            .iter()
                            .position(|x| *x == n)
                            .ok_or_else(|| self.error(f, line, format!("{} has no field {n}", self.type_name(ty))))?,
                        None => {
                            positional += 1;
                            positional - 1
                        }
                    };
                    if index >= names.len() {
                        return Err(self.error(f, line, format!("too many values for {}", self.type_name(ty))));
                    }
                    given.push((index, v));
                }
                self.construct_struct(*id, given, line)
            }
            Ty::Array(elem, len) => {
                let size = match len {
                    Len::Fixed(n) => Some(*n),
                    Len::Deferred(_) => None,
                };
                let given = values.into_iter().map(|(_, v)| v).collect();
                let mut array = self.construct_array(elem, size, given, &origin).map_err(|m| self.error(f, line, m))?;
                if let (V::Array(e), Some(n)) = (&mut array.v, size) {
                    while e.len() < n {
                        e.push(self.default_value(elem, line)?);
                    }
                }
                Ok(array)
            }
            Ty::Enum(_) => {
                let v = values.into_iter().next().map(|(_, v)| v);
                match v.as_ref().and_then(Val::known_int) {
                    Some(i) => Ok(Val::new(ty.clone(), V::Enum(i))),
                    None if v.is_none() => self.default_value(ty, line),
                    None => Err(self.error(f, line, "an enum from a value known only at run time")),
                }
            }
            _ => {
                if values.iter().any(|(n, _)| n.is_some()) {
                    // `texture_2d(name: ...)` and the like: by position, in declaration order.
                    let order = ["name", "gamma", "selector"];
                    let mut sorted = values;
                    sorted.sort_by_key(|(n, _)| n.as_deref().and_then(|n| order.iter().position(|o| *o == n)));
                    let given = sorted.into_iter().map(|(_, v)| v).collect();
                    return self.construct(ty, given, &origin).map_err(|m| self.error(f, line, m));
                }
                let given = values.into_iter().map(|(_, v)| v).collect();
                self.construct(ty, given, &origin).map_err(|m| self.error(f, line, m))
            }
        }
    }

    /// The overload of `name` among `functions` the arguments select, called.
    fn call_overloads(
        &mut self,
        f: &Frame,
        name: &str,
        functions: &[(usize, usize)],
        values: Args,
        line: u32,
    ) -> Result<Val, Error> {
        let version = self.program.modules[f.module].version();
        let mut best: Option<Binding> = None;
        let mut reasons = Vec::new();
        for &(module, decl) in functions {
            let func = self.program.function(module, decl);
            if !visible(&func.annotations, version) && functions.len() > 1 {
                continue;
            }
            match self.bind(module, decl, &values, line)? {
                Ok(b) => {
                    if best.as_ref().is_none_or(|x| b.cost < x.cost) {
                        best = Some(b);
                    }
                }
                Err(reason) => reasons.push(reason),
            }
        }
        let Some(b) = best else {
            let given: Vec<String> = values
                .iter()
                .map(|(n, v)| match n {
                    Some(n) => format!("{n}: {}", self.type_name(&v.ty)),
                    None => self.type_name(&v.ty),
                })
                .collect();
            let why = reasons.first().map(|r| format!(" ({r})")).unwrap_or_default();
            return Err(self.error(f, line, format!("no overload of {name} takes ({}){why}", given.join(", "))));
        };
        self.invoke(b.module, b.decl, b.args, b.sizes, line)
    }

    /// The arguments bound to the parameters of function `decl` of `module`, with the cost of their conversions;
    /// `Ok(Err(reason))` when they do not fit.
    fn bind(&mut self, module: usize, decl: usize, values: &Args, line: u32) -> Result<Result<Binding, String>, Error> {
        let func = self.program.function(module, decl);
        let Some(params) = &func.params else {
            // A variant takes what its body's callee takes; it is checked when the body is evaluated.
            let args = values.iter().map(|(_, v)| Some(v.clone())).collect();
            return Ok(Ok(Binding { module, decl, args, sizes: HashMap::new(), cost: 0 }));
        };
        let params = params.clone();
        let mut bound: Vec<Option<Val>> = vec![None; params.len()];
        let mut positional = 0;
        for (name, v) in values {
            let index = match name {
                Some(n) => match params.iter().position(|p| p.name == *n) {
                    Some(i) => i,
                    None => return Ok(Err(format!("no parameter {n}"))),
                },
                None => {
                    positional += 1;
                    positional - 1
                }
            };
            if index >= params.len() {
                return Ok(Err(format!("{} parameters", params.len())));
            }
            if bound[index].is_some() {
                return Ok(Err(format!("{} given twice", params[index].name)));
            }
            bound[index] = Some(v.clone());
        }
        let mut frame = Frame::new(module, &func.name.clone());
        let mut sizes = HashMap::new();
        let mut cost = 0;
        for (p, arg) in params.iter().zip(&bound) {
            let Some(arg) = arg else {
                if p.default.is_none() {
                    return Ok(Err(format!("{} has no default", p.name)));
                }
                continue;
            };
            frame.sizes = sizes.clone();
            let Some(ty) = self.resolve_type(&mut frame, &p.ty, line)? else { continue };
            // A size-deferred array parameter binds its size name.
            if let (Ty::Array(pe, Len::Deferred(n)), Ty::Array(ae, Len::Fixed(k))) = (&ty, &arg.ty) {
                if pe != ae && self.implicit_cost(ae, pe).is_none() {
                    return Ok(Err(format!("{} takes {}", p.name, self.type_name(&ty))));
                }
                if !n.is_empty() {
                    match sizes.get(n) {
                        Some(&s) if s != *k => return Ok(Err(format!("{n} is {s} and {k}"))),
                        _ => {
                            sizes.insert(n.clone(), *k);
                        }
                    }
                }
                continue;
            }
            match self.implicit_cost(&arg.ty, &ty) {
                Some(c) => cost += c,
                None => {
                    return Ok(Err(format!(
                        "{} takes {}, not {}",
                        p.name,
                        self.type_name(&ty),
                        self.type_name(&arg.ty)
                    )));
                }
            }
        }
        Ok(Ok(Binding { module, decl, args: bound, sizes, cost }))
    }

    /// Function `decl` of `module` called with `args` (by parameter; `None` takes the default).
    pub fn invoke(
        &mut self,
        module: usize,
        decl: usize,
        args: Vec<Option<Val>>,
        sizes: HashMap<String, usize>,
        line: u32,
    ) -> Result<Val, Error> {
        if self.depth >= MAX_CALL_DEPTH {
            return Err(Error::Limit(format!(
                "{}: calls nested more than {MAX_CALL_DEPTH} deep",
                self.at(module, line, "")
            )));
        }
        self.depth += 1;
        let result = self.invoke_here(module, decl, args, sizes, line);
        self.depth -= 1;
        result
    }

    fn invoke_here(
        &mut self,
        module: usize,
        decl: usize,
        args: Vec<Option<Val>>,
        sizes: HashMap<String, usize>,
        line: u32,
    ) -> Result<Val, Error> {
        let func: Function = self.program.function(module, decl).clone();
        let Some(params) = &func.params else {
            return self.invoke_variant(module, &func, args, line);
        };
        let mut frame = Frame::new(module, &func.name);
        frame.sizes = sizes;
        let mut flow = Flow::new();
        let mut bound = Vec::new();
        for (p, arg) in params.iter().zip(args) {
            let ty = self.resolve_type(&mut frame, &p.ty, p.line)?;
            let v = match arg {
                Some(v) => v,
                None => {
                    let default = p.default.as_ref().expect("a bound parameter without argument has a default");
                    self.expr(&mut frame, &mut flow, default)?
                }
            };
            let v = match ty {
                Some(t) => {
                    let t = self.complete_array(&t, &v);
                    self.convert(v, &t, false, p.line, module)?
                }
                None => v,
            };
            frame.declare(&p.name, v.clone());
            bound.push((p.name.clone(), v));
        }
        let ret_ty = self.resolve_type(&mut frame, &func.ret, func.line)?;
        if self.program.modules[module].standard {
            return self.intrinsic(&frame, &func, &bound, ret_ty, line);
        }
        if let Some(v) = self.native(&frame, &func, &bound, ret_ty.as_ref(), line)? {
            return Ok(v);
        }
        let v = match &func.body {
            Body::Expr(e) => self.expr(&mut frame, &mut flow, e)?,
            Body::Block(body) => {
                flow.ret_ty = ret_ty.clone();
                for s in body {
                    self.exec(&mut frame, &mut flow, s)?;
                }
                flow.ret
                    .take()
                    .ok_or_else(|| self.error(&frame, func.line, format!("{} returns nothing", func.name)))?
            }
            Body::None => return Err(self.error(&frame, func.line, format!("{} has no body", func.name))),
        };
        match ret_ty {
            Some(t) => {
                let t = self.complete_array(&t, &v);
                self.convert(v, &t, false, func.line, module)
            }
            None => Ok(v),
        }
    }

    /// A variant (`material m(*) = base(...)`): the base called with the variant's arguments and the caller's,
    /// the caller's winning.
    fn invoke_variant(
        &mut self,
        module: usize,
        func: &Function,
        args: Vec<Option<Val>>,
        line: u32,
    ) -> Result<Val, Error> {
        let mut frame = Frame::new(module, &func.name);
        let mut flow = Flow::new();
        let Body::Expr(Expr { kind: ExprKind::Call { callee, args: base_args }, line: call_line }) = &func.body else {
            return Err(self.error(&frame, func.line, "a variant must call a function"));
        };
        if args.iter().any(Option::is_some) {
            self.warn(format!("{}: arguments to a variant are ignored", func.name));
        }
        self.call(&mut frame, &mut flow, callee, base_args, *call_line).map_err(|e| match e {
            Error::BadValue { message, .. } => self.error(&frame, line, message),
            other => other,
        })
    }
}
