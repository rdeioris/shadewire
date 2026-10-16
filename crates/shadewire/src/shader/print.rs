//! Printing the tree as source: the walk over items, statements and expressions every back end shares
//! (indentation, comments, operator precedence and parentheses, declaration order), parameterized by a
//! [`Syntax`] for what differs. The defaults are the C family's (GLSL, HLSL and MSL); WGSL overrides most.

use std::collections::{HashMap, HashSet};

use super::ast::*;

/// How one language spells the tree. Each back end implements the required methods and overrides the C-family
/// defaults where it differs.
pub trait Syntax: Sized {
    /// A type's name.
    fn ty(&self, ty: &Ty) -> String;

    /// A built-in's name, for the built-ins `builtin` prints as plain calls.
    fn builtin_name(&self, b: Builtin) -> &'static str;

    /// A literal.
    fn literal(&self, lit: &Literal) -> String {
        match lit {
            Literal::Bool(b) => b.to_string(),
            Literal::I32(i) => i.to_string(),
            Literal::U32(u) => format!("{u}u"),
            Literal::F32(x) => float(*x),
        }
    }

    /// A built-in call. The default prints `name(args)`; override for special forms.
    fn builtin(&self, p: &Printer<Self>, b: Builtin, args: &[Expr], _ty: &Ty) -> String {
        format!("{}({})", self.builtin_name(b), p.list(args))
    }

    /// A value of `ty` from its parts.
    fn construct(&self, p: &Printer<Self>, ty: &Ty, args: &[Expr]) -> String {
        format!("{}({})", self.ty(ty), p.list(args))
    }

    /// A numeric conversion to `ty`.
    fn convert(&self, p: &Printer<Self>, ty: &Ty, arg: &Expr) -> String {
        format!("{}({})", self.ty(ty), p.expr(arg))
    }

    /// The same bits as `ty`.
    fn bitcast(&self, p: &Printer<Self>, ty: &Ty, arg: &Expr) -> String;

    /// A binary expression this language spells differently (a vector comparison as a call, say); `None`
    /// prints the operator.
    fn binary(&self, _p: &Printer<Self>, _op: BinaryOp, _lhs: &Expr, _rhs: &Expr, _ty: &Ty) -> Option<String> {
        None
    }

    /// Components of a vector by `xyzw` letters, of type `ty`.
    fn swizzle(&self, p: &Printer<Self>, base: &Expr, components: &str, _ty: &Ty) -> String {
        format!("{}.{components}", p.operand(base, 12))
    }

    /// What a pointer parameter points to: a reference parameter needs no dereference.
    fn deref(&self, p: &Printer<Self>, inner: &Expr) -> String {
        p.expr(inner)
    }

    /// A local passed to a pointer parameter: references take it as is.
    fn address_of(&self, p: &Printer<Self>, inner: &Expr) -> String {
        p.expr(inner)
    }

    /// A function parameter: a pointer is an `inout` parameter.
    fn param(&self, name: &str, ty: &Ty) -> String {
        match ty {
            Ty::Ptr(t) => format!("inout {} {name}", self.ty(t)),
            t => format!("{} {name}", self.ty(t)),
        }
    }

    /// A function's signature, without the body or `;`.
    fn signature(&self, f: &Function, _module: &Module) -> String {
        let params: Vec<String> = f.params.iter().map(|(n, t)| self.param(n, t)).collect();
        format!("{} {}({})", self.ty(&f.ret), f.name, params.join(", "))
    }

    /// A host function's declaration.
    fn extern_decl(&self, f: &Function, module: &Module) -> Option<String> {
        Some(format!("{};", self.signature(f, module)))
    }

    /// A local: `T name = value;`.
    fn local(&self, name: &str, ty: &Ty, value: Option<String>, _mutable: bool) -> String {
        match value {
            Some(v) => format!("{} {name} = {v};", self.ty(ty)),
            None => format!("{} {name};", self.ty(ty)),
        }
    }

    /// A module constant.
    fn constant(&self, name: &str, ty: &Ty, value: String) -> String {
        format!("const {} {name} = {value};", self.ty(ty))
    }

    fn structure(&self, s: &Struct) -> String {
        let mut out = format!("struct {} {{\n", s.name);
        for (m, t) in &s.members {
            out.push_str(&format!("    {} {m};\n", self.ty(t)));
        }
        out.push_str("};\n");
        out
    }

    /// A resource's declaration.
    fn resource(&self, r: &Resource, module: &Module) -> String;

    /// What the language needs before the items: helper functions for forms it lacks, say.
    fn prelude(&self, _module: &Module) -> String {
        String::new()
    }

    /// Whether a switch case ends with `break` (the C family falls through otherwise).
    fn case_break(&self) -> bool {
        true
    }

    /// A switch case's head.
    fn case_head(&self, p: &Printer<Self>, values: &[Expr]) -> String {
        values.iter().map(|v| format!("case {}:", p.expr(v))).collect::<Vec<_>>().join(" ")
    }

    fn default_head(&self) -> &'static str {
        "default:"
    }

    /// A `for` header from its printed parts (the initializer and update without `;`).
    fn for_head(&self, init: &str, cond: &str, update: &str) -> String {
        format!("for ({init}; {cond}; {update})")
    }

    /// An `if` condition; the C family needs parentheses.
    fn if_head(&self, cond: &str) -> String {
        format!("if ({cond})")
    }

    fn switch_head(&self, selector: &str) -> String {
        format!("switch ({selector})")
    }
}

/// An f32 literal that reads back exactly: Rust's shortest round-trip form, with a `.0` when it has no point
/// or exponent. NaN and the infinities never get here: they are bitcasts.
pub fn float(x: f32) -> String {
    let s = format!("{x:?}");
    if s.contains(['.', 'e', 'E']) || s.contains("inf") || s.contains("NaN") { s } else { format!("{s}.0") }
}

/// A member of a host-filled struct, or padding before one.
#[derive(Clone, Debug, PartialEq)]
pub enum Field<'a> {
    Member(&'a str, &'a Ty),
    /// This many 4-byte words.
    Pad(u32),
}

/// The members of a host-filled struct with padding words so that each sits at its reflected offset, for
/// languages whose packing differs from WGSL's (HLSL's cbuffers, MSL's structs with packed vectors): there every
/// member takes 4 bytes per component, and a WGSL offset is always a legal place for it.
pub fn padded(s: &Struct) -> Vec<Field<'_>> {
    let Some((offsets, size)) = &s.layout else {
        return s.members.iter().map(|(m, t)| Field::Member(m, t)).collect();
    };
    let mut out = Vec::new();
    let mut at = 0;
    for ((m, t), offset) in s.members.iter().zip(offsets) {
        if *offset > at {
            out.push(Field::Pad((offset - at) / 4));
        }
        out.push(Field::Member(m, t));
        at = offset + 4 * u32::from(t.width().unwrap_or(1));
    }
    if *size > at {
        out.push(Field::Pad((size - at) / 4));
    }
    out
}

/// A scalar type as the C family (GLSL, HLSL, MSL) names it.
pub fn c_scalar(s: Scalar) -> &'static str {
    match s {
        Scalar::Bool => "bool",
        Scalar::I32 => "int",
        Scalar::U32 => "uint",
        Scalar::F32 => "float",
    }
}

/// A host-filled struct with padding words so each member sits at its reflected offset (see [`padded`]),
/// spelling each member with `member`.
pub fn padded_struct(s: &Struct, member: impl Fn(&str, &Ty) -> String) -> String {
    let mut out = format!("struct {} {{\n", s.name);
    let mut pad = 0;
    for field in padded(s) {
        match field {
            Field::Member(m, t) => out.push_str(&format!("    {};\n", member(m, t))),
            Field::Pad(words) => {
                for _ in 0..words {
                    out.push_str(&format!("    uint shadewire_pad{pad};\n"));
                    pad += 1;
                }
            }
        }
    }
    out.push_str("};\n");
    out
}

/// The selects a module makes, as (value type, condition type) pairs, for languages that print `select` as a
/// call of a `shadewire_select` overload (see [`select_helpers`]).
pub fn selects(module: &Module) -> Vec<(Ty, Ty)> {
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

/// One `shadewire_select(f, t, cond)` overload per select the module makes, in a C-family syntax. A call evaluates
/// both values, as WGSL's `select` does, so a texture sample in one stays in uniform control flow (a `?:` evaluates
/// one side); a vector condition picks componentwise.
pub fn select_helpers<S: Syntax>(syntax: &S, module: &Module) -> String {
    let mut out = String::new();
    for (value, cond) in selects(module) {
        let (v, c) = (syntax.ty(&value), syntax.ty(&cond));
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

/// Binding strength of an expression, for parentheses.
fn precedence(e: &Expr) -> u8 {
    match &e.kind {
        ExprKind::Binary { op, .. } => match op {
            BinaryOp::Or => 1,
            BinaryOp::And => 2,
            BinaryOp::BitOr => 3,
            BinaryOp::BitXor => 4,
            BinaryOp::BitAnd => 5,
            BinaryOp::Eq | BinaryOp::Ne => 6,
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => 7,
            BinaryOp::Shl | BinaryOp::Shr => 8,
            BinaryOp::Add | BinaryOp::Sub => 9,
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => 10,
        },
        ExprKind::Unary { .. } | ExprKind::Deref(_) | ExprKind::AddressOf(_) => 11,
        ExprKind::Literal(Literal::I32(i)) if *i < 0 => 11,
        ExprKind::Literal(Literal::F32(x)) if x.is_sign_negative() => 11,
        _ => 12,
    }
}

/// Operators whose operands are parenthesized whenever they are binary: WGSL requires it for bitwise, shift
/// and logical operators, and C's precedence of `&` below `==` surprises.
fn strict(op: BinaryOp) -> bool {
    use BinaryOp::*;
    matches!(op, BitAnd | BitOr | BitXor | Shl | Shr | And | Or) || op.is_comparison()
}

/// Prints a tree with a [`Syntax`].
pub struct Printer<'a, S: Syntax> {
    pub syntax: &'a S,
}

impl<'a, S: Syntax> Printer<'a, S> {
    pub fn new(syntax: &'a S) -> Self {
        Printer { syntax }
    }

    pub fn list(&self, args: &[Expr]) -> String {
        args.iter().map(|a| self.expr(a)).collect::<Vec<_>>().join(", ")
    }

    /// `e`, parenthesized if it binds looser than `min`.
    pub fn operand(&self, e: &Expr, min: u8) -> String {
        let s = self.expr(e);
        if precedence(e) < min { format!("({s})") } else { s }
    }

    pub fn expr(&self, e: &Expr) -> String {
        let syntax = self.syntax;
        match &e.kind {
            ExprKind::Literal(lit) => syntax.literal(lit),
            ExprKind::Name(n) => n.clone(),
            ExprKind::Call { function, args } => format!("{function}({})", self.list(args)),
            ExprKind::Builtin { builtin, args } => syntax.builtin(self, *builtin, args, &e.ty),
            ExprKind::Construct(args) => syntax.construct(self, &e.ty, args),
            ExprKind::Convert(arg) => syntax.convert(self, &e.ty, arg),
            ExprKind::Bitcast(arg) => syntax.bitcast(self, &e.ty, arg),
            ExprKind::Unary { op, arg } => {
                let sign = match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::Not => "!",
                    UnaryOp::BitNot => "~",
                };
                format!("{sign}{}", self.operand(arg, 12))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                if let Some(s) = syntax.binary(self, *op, lhs, rhs, &e.ty) {
                    return s;
                }
                let p = precedence(e);
                let side = |child: &Expr, right: bool| {
                    let cp = precedence(child);
                    let nested = matches!(child.kind, ExprKind::Binary { .. });
                    let wrap = cp < p || (right && cp == p) || (nested && (strict(*op) || child_strict(child)));
                    let s = self.expr(child);
                    if wrap { format!("({s})") } else { s }
                };
                format!("{} {} {}", side(lhs, false), op.symbol(), side(rhs, true))
            }
            ExprKind::Member { base, member } => format!("{}.{member}", self.operand(base, 12)),
            ExprKind::Swizzle { base, components } => syntax.swizzle(self, base, components, &e.ty),
            ExprKind::Index { base, index } => format!("{}[{}]", self.operand(base, 12), self.expr(index)),
            ExprKind::Deref(inner) => syntax.deref(self, inner),
            ExprKind::AddressOf(inner) => syntax.address_of(self, inner),
        }
    }

    /// The statements of a block, indented by `depth` levels, one per line (a trailing comment joins the line
    /// before it).
    pub fn block(&self, body: &[Stmt], depth: usize, out: &mut Vec<String>) {
        let pad = "    ".repeat(depth);
        for s in body {
            match s {
                Stmt::Comment { text, trailing: true } if !out.is_empty() => {
                    let last = out.last_mut().expect("a line");
                    last.push_str(&format!("  // {text}"));
                }
                Stmt::Comment { text, .. } => out.push(format!("{pad}// {text}")),
                Stmt::If { .. } => self.if_chain(s, depth, out, false),
                Stmt::For { init, cond, update, body } => {
                    let init = self.simple(init).trim_end_matches(';').to_string();
                    let update = self.simple(update).trim_end_matches(';').to_string();
                    out.push(format!("{pad}{} {{", self.syntax.for_head(&init, &self.expr(cond), &update)));
                    self.block(body, depth + 1, out);
                    out.push(format!("{pad}}}"));
                }
                Stmt::Switch { selector, cases, default } => {
                    out.push(format!("{pad}{} {{", self.syntax.switch_head(&self.expr(selector))));
                    let inner = "    ".repeat(depth + 1);
                    let mut case = |head: String, body: &[Stmt]| {
                        out.push(format!("{inner}{head} {{"));
                        self.block(body, depth + 2, out);
                        if self.syntax.case_break() && !ends(body) {
                            out.push(format!("{inner}    break;"));
                        }
                        out.push(format!("{inner}}}"));
                    };
                    for (values, body) in cases {
                        case(self.syntax.case_head(self, values), body);
                    }
                    case(self.syntax.default_head().to_string(), default);
                    out.push(format!("{pad}}}"));
                }
                other => out.push(format!("{pad}{}", self.simple(other))),
            }
        }
    }

    fn if_chain(&self, s: &Stmt, depth: usize, out: &mut Vec<String>, chained: bool) {
        let pad = "    ".repeat(depth);
        let Stmt::If { cond, then, otherwise } = s else { unreachable!() };
        let head = format!("{} {{", self.syntax.if_head(&self.expr(cond)));
        if chained {
            let last = out.last_mut().expect("the closing brace");
            last.push_str(&format!(" else {head}"));
        } else {
            out.push(format!("{pad}{head}"));
        }
        self.block(then, depth + 1, out);
        out.push(format!("{pad}}}"));
        match otherwise.as_slice() {
            [] => {}
            [only @ Stmt::If { .. }] => self.if_chain(only, depth, out, true),
            rest => {
                out.last_mut().expect("the closing brace").push_str(" else {");
                self.block(rest, depth + 1, out);
                out.push(format!("{pad}}}"));
            }
        }
    }

    /// A one-line statement with its `;`.
    fn simple(&self, s: &Stmt) -> String {
        match s {
            Stmt::Let { name, ty, value, mutable } => {
                self.syntax.local(name, ty, value.as_ref().map(|v| self.expr(v)), *mutable)
            }
            Stmt::Assign { target, op: None, value } => format!("{} = {};", self.expr(target), self.expr(value)),
            Stmt::Assign { target, op: Some(op), value } => {
                format!("{} {}= {};", self.expr(target), op.symbol(), self.expr(value))
            }
            Stmt::Return(Some(v)) => format!("return {};", self.expr(v)),
            Stmt::Return(None) => "return;".into(),
            Stmt::Break => "break;".into(),
            Stmt::Continue => "continue;".into(),
            Stmt::Expr(e) => format!("{};", self.expr(e)),
            other => panic!("{other:?} is not a simple statement"),
        }
    }

    /// One item, with its trailing newline.
    pub fn item(&self, item: &Item, module: &Module) -> String {
        match item {
            Item::Comment(text) => format!("// {text}\n"),
            Item::Struct(s) => self.syntax.structure(s),
            Item::Const { name, ty, value } => format!("{}\n", self.syntax.constant(name, ty, self.expr(value))),
            Item::Resource(r) => {
                let text = self.syntax.resource(r, module);
                if text.is_empty() { text } else { text + "\n" }
            }
            Item::Extern(f) => self.syntax.extern_decl(f, module).map_or(String::new(), |d| format!("{d}\n")),
            Item::Function(f) => {
                let mut lines = vec![format!("{} {{", self.syntax.signature(f, module))];
                self.block(&f.body, 1, &mut lines);
                lines.push("}".into());
                lines.join("\n") + "\n"
            }
        }
    }

    /// The whole module: the prelude, then the items in declaration order (see [`ordered`]), a blank line between
    /// item groups.
    pub fn module(&self, module: &Module) -> String {
        let mut out = self.syntax.prelude(module);
        let items = ordered(&module.items);
        let mut previous: Option<&Item> = None;
        for item in &items {
            let text = self.item(item, module);
            if text.is_empty() {
                continue;
            }
            if let Some(p) = previous
                && separate(p, item)
            {
                out.push('\n');
            }
            out.push_str(&text);
            previous = Some(item);
        }
        out
    }
}

fn child_strict(child: &Expr) -> bool {
    matches!(&child.kind, ExprKind::Binary { op, .. } if strict(*op))
}

/// Whether a body ends with a statement that leaves it.
fn ends(body: &[Stmt]) -> bool {
    body.iter()
        .rev()
        .find(|s| !matches!(s, Stmt::Comment { .. }))
        .is_some_and(|s| matches!(s, Stmt::Return(_) | Stmt::Break | Stmt::Continue))
}

/// A blank line between items, except within a run of comments, resources, externs or constants, and between
/// a comment and what it introduces.
fn separate(previous: &Item, next: &Item) -> bool {
    !matches!(
        (previous, next),
        (Item::Comment(_), _)
            | (Item::Resource(_), Item::Resource(_))
            | (Item::Extern(_), Item::Extern(_))
            | (Item::Const { .. }, Item::Const { .. })
    )
}

/// The items in an order the C family can compile: structs, constants, resources and host declarations as
/// they come, then the functions with every callee before its callers (otherwise in their order). Comments
/// travel with the item after them.
pub fn ordered(items: &[Item]) -> Vec<Item> {
    // Group each run of comments with the item after it.
    let mut groups: Vec<Vec<&Item>> = Vec::new();
    let mut pending: Vec<&Item> = Vec::new();
    for item in items {
        pending.push(item);
        if !matches!(item, Item::Comment(_)) {
            groups.push(std::mem::take(&mut pending));
        }
    }
    fn head<'a>(g: &[&'a Item]) -> &'a Item {
        g.last().expect("a group has its item")
    }
    let mut out: Vec<Item> = Vec::new();
    let mut functions: Vec<&Vec<&Item>> = Vec::new();
    for g in &groups {
        match head(g) {
            Item::Function(_) => functions.push(g),
            _ => out.extend(g.iter().map(|i| (*i).clone())),
        }
    }
    let by_name: HashMap<&str, usize> = functions.iter().enumerate().map(|(i, g)| (fn_name(head(g)), i)).collect();
    let mut done = HashSet::new();
    fn visit(
        i: usize,
        functions: &[&Vec<&Item>],
        by_name: &HashMap<&str, usize>,
        done: &mut HashSet<usize>,
        out: &mut Vec<Item>,
    ) {
        if !done.insert(i) {
            return;
        }
        let Item::Function(f) = functions[i].last().expect("a group has its item") else { unreachable!() };
        let mut callees = Vec::new();
        calls_in(&f.body, &mut callees);
        for c in callees {
            if let Some(&j) = by_name.get(c.as_str()) {
                visit(j, functions, by_name, done, out);
            }
        }
        out.extend(functions[i].iter().map(|i| (*i).clone()));
    }
    for i in 0..functions.len() {
        visit(i, &functions, &by_name, &mut done, &mut out);
    }
    out.extend(pending.into_iter().cloned());
    out
}

fn fn_name(item: &Item) -> &str {
    match item {
        Item::Function(f) => &f.name,
        _ => "",
    }
}

/// The user functions a body calls, in order.
fn calls_in(body: &[Stmt], out: &mut Vec<String>) {
    fn expr(e: &Expr, out: &mut Vec<String>) {
        match &e.kind {
            ExprKind::Call { function, args } => {
                out.push(function.clone());
                args.iter().for_each(|a| expr(a, out));
            }
            ExprKind::Builtin { args, .. } | ExprKind::Construct(args) => args.iter().for_each(|a| expr(a, out)),
            ExprKind::Convert(a)
            | ExprKind::Bitcast(a)
            | ExprKind::Deref(a)
            | ExprKind::AddressOf(a)
            | ExprKind::Unary { arg: a, .. }
            | ExprKind::Member { base: a, .. }
            | ExprKind::Swizzle { base: a, .. } => expr(a, out),
            ExprKind::Binary { lhs, rhs, .. } => {
                expr(lhs, out);
                expr(rhs, out);
            }
            ExprKind::Index { base, index } => {
                expr(base, out);
                expr(index, out);
            }
            ExprKind::Literal(_) | ExprKind::Name(_) => {}
        }
    }
    for s in body {
        match s {
            Stmt::Let { value: Some(v), .. } | Stmt::Return(Some(v)) | Stmt::Expr(v) => expr(v, out),
            Stmt::Assign { target, value, .. } => {
                expr(target, out);
                expr(value, out);
            }
            Stmt::If { cond, then, otherwise } => {
                expr(cond, out);
                calls_in(then, out);
                calls_in(otherwise, out);
            }
            Stmt::For { init, cond, update, body } => {
                calls_in(std::slice::from_ref(init), out);
                expr(cond, out);
                calls_in(std::slice::from_ref(update), out);
                calls_in(body, out);
            }
            Stmt::Switch { selector, cases, default } => {
                expr(selector, out);
                for (values, body) in cases {
                    values.iter().for_each(|v| expr(v, out));
                    calls_in(body, out);
                }
                calls_in(default, out);
            }
            _ => {}
        }
    }
}

/// Every built-in a module calls, each once.
pub fn builtins_used(module: &Module) -> HashSet<Builtin> {
    let mut out = HashSet::new();
    visit_exprs(module, &mut |e| {
        if let ExprKind::Builtin { builtin, .. } = &e.kind {
            out.insert(*builtin);
        }
    });
    out
}

/// Calls `f` on every expression of the module's constants and functions, outermost first.
pub fn visit_exprs(module: &Module, f: &mut impl FnMut(&Expr)) {
    fn expr(e: &Expr, f: &mut impl FnMut(&Expr)) {
        f(e);
        match &e.kind {
            ExprKind::Call { args, .. } | ExprKind::Builtin { args, .. } | ExprKind::Construct(args) => {
                args.iter().for_each(|a| expr(a, f))
            }
            ExprKind::Convert(a)
            | ExprKind::Bitcast(a)
            | ExprKind::Deref(a)
            | ExprKind::AddressOf(a)
            | ExprKind::Unary { arg: a, .. }
            | ExprKind::Member { base: a, .. }
            | ExprKind::Swizzle { base: a, .. } => expr(a, f),
            ExprKind::Binary { lhs, rhs, .. } => {
                expr(lhs, f);
                expr(rhs, f);
            }
            ExprKind::Index { base, index } => {
                expr(base, f);
                expr(index, f);
            }
            ExprKind::Literal(_) | ExprKind::Name(_) => {}
        }
    }
    fn stmts(body: &[Stmt], f: &mut impl FnMut(&Expr)) {
        for s in body {
            match s {
                Stmt::Let { value: Some(v), .. } | Stmt::Return(Some(v)) | Stmt::Expr(v) => expr(v, f),
                Stmt::Assign { target, value, .. } => {
                    expr(target, f);
                    expr(value, f);
                }
                Stmt::If { cond, then, otherwise } => {
                    expr(cond, f);
                    stmts(then, f);
                    stmts(otherwise, f);
                }
                Stmt::For { init, cond, update, body } => {
                    stmts(std::slice::from_ref(init), f);
                    expr(cond, f);
                    stmts(std::slice::from_ref(update), f);
                    stmts(body, f);
                }
                Stmt::Switch { selector, cases, default } => {
                    expr(selector, f);
                    for (values, body) in cases {
                        values.iter().for_each(|v| expr(v, f));
                        stmts(body, f);
                    }
                    stmts(default, f);
                }
                _ => {}
            }
        }
    }
    for item in &module.items {
        match item {
            Item::Const { value, .. } => expr(value, f),
            Item::Function(func) => stmts(&func.body, f),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_read_back_exactly() {
        assert_eq!(float(1.0), "1.0");
        assert_eq!(float(0.1), "0.1");
        assert_eq!(float(5.4856e-13), "5.4856e-13");
        assert_eq!(float(-2.0), "-2.0");
    }

    #[test]
    fn callees_come_before_callers_with_their_comments() {
        let f = |name: &str, calls: &[&str]| {
            Item::Function(Function {
                name: name.into(),
                params: Vec::new(),
                ret: Ty::Void,
                body: calls.iter().map(|c| Stmt::Expr(Expr::call(*c, Vec::new(), Ty::Void))).collect(),
            })
        };
        let items = vec![Item::Comment("a calls b".into()), f("a", &["b"]), Item::Comment("b".into()), f("b", &[])];
        let names: Vec<String> = ordered(&items)
            .iter()
            .map(|i| match i {
                Item::Comment(c) => format!("// {c}"),
                Item::Function(f) => f.name.clone(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(names, ["// b", "b", "// a calls b", "a"]);
    }
}
