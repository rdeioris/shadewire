//! A parser for the WGSL subset the support libraries are written in: constants, structs and functions;
//! `let`/`var`, assignments, `if`, `for`, `switch`, `return`; the usual operators, calls, constructors,
//! conversions, `bitcast`, member access, indexing and function-local pointers. Comment lines are kept.
//! Expressions come out untyped ([`Ty::Unknown`]); [`super::infer`] types them.

use super::ast::*;

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    /// A number with its suffix (`u`, `i`, `f` or none) and whether it is a float.
    Number {
        text: String,
        suffix: Option<char>,
        float: bool,
    },
    Punct(&'static str),
    /// A `//` comment and whether code precedes it on its line.
    Comment {
        text: String,
        trailing: bool,
    },
}

#[derive(Clone, Debug)]
struct Token {
    tok: Tok,
    line: usize,
}

/// A parse error with its line.
#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

const PUNCTS: [&str; 41] = [
    "<<=", ">>=", "->", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=",
    "++", "--", "+", "-", "*", "/", "%", "&", "|", "^", "!", "~", "<", ">", "=", "(", ")", "{", "}", ",", ";", ":",
];
const MORE_PUNCTS: [&str; 3] = [".", "[", "]"];

fn lex(src: &str) -> Result<Vec<Token>> {
    let mut out = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut line = 1;
    let mut code_on_line = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            code_on_line = false;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            let start = i + 2;
            let mut end = start;
            while end < chars.len() && chars[end] != '\n' {
                end += 1;
            }
            let text: String = chars[start..end].iter().collect();
            out.push(Token { tok: Tok::Comment { text: text.trim().to_string(), trailing: code_on_line }, line });
            i = end;
            continue;
        }
        code_on_line = true;
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Token { tok: Tok::Ident(chars[start..i].iter().collect()), line });
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            let mut float = false;
            if c == '0' && matches!(chars.get(i + 1), Some('x') | Some('X')) {
                i += 2;
                while i < chars.len() && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
            } else {
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    float |= chars[i] == '.';
                    i += 1;
                }
                if matches!(chars.get(i), Some('e') | Some('E')) {
                    float = true;
                    i += 1;
                    if matches!(chars.get(i), Some('+') | Some('-')) {
                        i += 1;
                    }
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let text: String = chars[start..i].iter().collect();
            let suffix = match chars.get(i) {
                Some(&s @ ('u' | 'i' | 'f')) => {
                    i += 1;
                    Some(s)
                }
                _ => None,
            };
            float |= suffix == Some('f');
            out.push(Token { tok: Tok::Number { text, suffix, float }, line });
            continue;
        }
        let rest: String = chars[i..chars.len().min(i + 3)].iter().collect();
        let Some(p) = PUNCTS.iter().chain(&MORE_PUNCTS).find(|p| rest.starts_with(**p)) else {
            return Err(ParseError { line, message: format!("unexpected character {c:?}") });
        };
        out.push(Token { tok: Tok::Punct(p), line });
        i += p.chars().count();
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

type Result<T> = std::result::Result<T, ParseError>;

impl Parser {
    /// The next non-comment token.
    fn peek_at(&self, ahead: usize) -> Option<&Tok> {
        self.tokens[self.pos..].iter().filter(|t| !matches!(t.tok, Tok::Comment { .. })).nth(ahead).map(|t| &t.tok)
    }

    fn peek(&self) -> Option<&Tok> {
        self.peek_at(0)
    }

    fn line(&self) -> usize {
        self.tokens.get(self.pos).or(self.tokens.last()).map_or(0, |t| t.line)
    }

    fn error<T>(&self, message: impl Into<String>) -> Result<T> {
        Err(ParseError { line: self.line(), message: message.into() })
    }

    /// The next non-comment token, consumed (comments before it are dropped).
    fn next(&mut self) -> Option<Tok> {
        while let Some(t) = self.tokens.get(self.pos) {
            self.pos += 1;
            if !matches!(t.tok, Tok::Comment { .. }) {
                return Some(t.tok.clone());
            }
        }
        None
    }

    /// The comments before the next token, consumed.
    fn comments(&mut self) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        while let Some(Token { tok: Tok::Comment { text, trailing }, .. }) = self.tokens.get(self.pos) {
            out.push((text.clone(), *trailing));
            self.pos += 1;
        }
        out
    }

    fn is(&self, p: &str) -> bool {
        matches!(self.peek(), Some(Tok::Punct(q)) if *q == p)
    }

    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(s)) if s == w)
    }

    fn eat(&mut self, p: &str) -> bool {
        if self.is(p) {
            self.next();
            true
        } else {
            false
        }
    }

    fn eat_word(&mut self, w: &str) -> bool {
        if self.is_word(w) {
            self.next();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, p: &str) -> Result<()> {
        if self.eat(p) { Ok(()) } else { self.error(format!("expected `{p}`, found {:?}", self.peek())) }
    }

    /// A closing `>` of a type, splitting the `>>` that closes two.
    fn close_angle(&mut self) -> Result<()> {
        while matches!(self.tokens.get(self.pos), Some(Token { tok: Tok::Comment { .. }, .. })) {
            self.pos += 1;
        }
        match self.tokens.get_mut(self.pos) {
            Some(t) if t.tok == Tok::Punct(">>") => {
                t.tok = Tok::Punct(">");
                Ok(())
            }
            _ => self.expect(">"),
        }
    }

    fn ident(&mut self) -> Result<String> {
        match self.next() {
            Some(Tok::Ident(s)) => Ok(s),
            other => self.error(format!("expected a name, found {other:?}")),
        }
    }

    // ------------------------------------------------------------------------------------------------- types

    fn scalar_name(name: &str) -> Option<Scalar> {
        match name {
            "f32" => Some(Scalar::F32),
            "i32" => Some(Scalar::I32),
            "u32" => Some(Scalar::U32),
            "bool" => Some(Scalar::Bool),
            _ => None,
        }
    }

    /// `vecN<T>`, `matCxR<f32>`, `ptr<function, T>`, a scalar or a struct name.
    fn ty(&mut self) -> Result<Ty> {
        let name = self.ident()?;
        if let Some(s) = Parser::scalar_name(&name) {
            return Ok(Ty::Scalar(s));
        }
        if let Some(n) = name.strip_prefix("vec").and_then(|n| n.parse::<u8>().ok()) {
            self.expect("<")?;
            let inner = self.ident()?;
            let s = Parser::scalar_name(&inner).ok_or(()).or_else(|_| self.error(format!("vector of {inner}")))?;
            self.close_angle()?;
            return Ok(Ty::Vector(n, s));
        }
        if let Some(dims) = name.strip_prefix("mat") {
            let (c, r) = dims.split_once('x').unwrap_or(("", ""));
            let (Ok(c), Ok(r)) = (c.parse::<u8>(), r.parse::<u8>()) else {
                return self.error(format!("matrix type {name}"));
            };
            self.expect("<")?;
            if self.ident()? != "f32" {
                return self.error("matrices are of f32");
            }
            self.close_angle()?;
            return Ok(Ty::Matrix(c, r));
        }
        if name == "ptr" {
            self.expect("<")?;
            if self.ident()? != "function" {
                return self.error("only function pointers");
            }
            self.expect(",")?;
            let inner = self.ty()?;
            self.close_angle()?;
            return Ok(Ty::Ptr(Box::new(inner)));
        }
        Ok(Ty::Struct(name))
    }

    /// Whether the tokens ahead start a type constructor or conversion (`vec3<f32>(`, `f32(`, `mat3x3<f32>(`).
    fn type_name(name: &str) -> bool {
        Parser::scalar_name(name).is_some()
            || name.strip_prefix("vec").is_some_and(|n| n.parse::<u8>().is_ok())
            || name.strip_prefix("mat").is_some_and(|d| d.contains('x'))
    }

    // ------------------------------------------------------------------------------------------------- items

    fn module(&mut self) -> Result<Module> {
        let mut items = Vec::new();
        loop {
            for (text, _) in self.comments() {
                items.push(Item::Comment(text));
            }
            let Some(tok) = self.peek().cloned() else {
                return Ok(Module { items });
            };
            match tok {
                Tok::Ident(w) if w == "const" => items.push(self.constant()?),
                Tok::Ident(w) if w == "struct" => items.push(self.structure()?),
                Tok::Ident(w) if w == "fn" => items.push(Item::Function(self.function()?)),
                other => return self.error(format!("expected `const`, `struct` or `fn`, found {other:?}")),
            }
        }
    }

    fn constant(&mut self) -> Result<Item> {
        self.next();
        let name = self.ident()?;
        let ty = if self.eat(":") { self.ty()? } else { Ty::Unknown };
        self.expect("=")?;
        let value = self.expr()?;
        self.expect(";")?;
        Ok(Item::Const { name, ty, value })
    }

    fn structure(&mut self) -> Result<Item> {
        self.next();
        let name = self.ident()?;
        self.expect("{")?;
        let mut members = Vec::new();
        while !self.eat("}") {
            let member = self.ident()?;
            self.expect(":")?;
            members.push((member, self.ty()?));
            if !self.eat(",") {
                self.expect("}")?;
                break;
            }
        }
        self.eat(";");
        Ok(Item::Struct(Struct { name, members, layout: None }))
    }

    fn function(&mut self) -> Result<Function> {
        self.next();
        let name = self.ident()?;
        self.expect("(")?;
        let mut params = Vec::new();
        while !self.eat(")") {
            let p = self.ident()?;
            self.expect(":")?;
            params.push((p, self.ty()?));
            if !self.eat(",") {
                self.expect(")")?;
                break;
            }
        }
        let ret = if self.eat("->") { self.ty()? } else { Ty::Void };
        let body = self.block()?;
        Ok(Function { name, params, ret, body })
    }

    // ------------------------------------------------------------------------------------------------- statements

    fn block(&mut self) -> Result<Vec<Stmt>> {
        self.expect("{")?;
        let mut out = Vec::new();
        loop {
            for (text, trailing) in self.comments() {
                out.push(Stmt::Comment { text, trailing });
            }
            if self.eat("}") {
                return Ok(out);
            }
            out.push(self.statement()?);
        }
    }

    fn statement(&mut self) -> Result<Stmt> {
        if self.eat_word("let") {
            return self.binding(false);
        }
        if self.eat_word("var") {
            return self.binding(true);
        }
        if self.eat_word("return") {
            let value = if self.is(";") { None } else { Some(self.expr()?) };
            self.expect(";")?;
            return Ok(Stmt::Return(value));
        }
        if self.eat_word("break") {
            self.expect(";")?;
            return Ok(Stmt::Break);
        }
        if self.eat_word("continue") {
            self.expect(";")?;
            return Ok(Stmt::Continue);
        }
        if self.eat_word("if") {
            return self.if_statement();
        }
        if self.eat_word("for") {
            self.expect("(")?;
            let init = if self.eat_word("var") { self.binding(true)? } else { self.simple()? };
            let cond = self.expr()?;
            self.expect(";")?;
            let update = self.update()?;
            self.expect(")")?;
            let body = self.block()?;
            return Ok(Stmt::For { init: Box::new(init), cond, update: Box::new(update), body });
        }
        if self.eat_word("switch") {
            return self.switch();
        }
        let s = self.update()?;
        self.expect(";")?;
        Ok(s)
    }

    /// After `let`/`var`: `name[: T] [= value];`.
    fn binding(&mut self, mutable: bool) -> Result<Stmt> {
        let name = self.ident()?;
        let ty = if self.eat(":") { self.ty()? } else { Ty::Unknown };
        let value = if self.eat("=") { Some(self.expr()?) } else { None };
        self.expect(";")?;
        if value.is_none() && !mutable {
            return self.error("a `let` needs a value");
        }
        Ok(Stmt::Let { name, ty, value, mutable })
    }

    /// An assignment or a call, with its `;`.
    fn simple(&mut self) -> Result<Stmt> {
        let s = self.update()?;
        self.expect(";")?;
        Ok(s)
    }

    /// An assignment, an increment or a call, without `;`.
    fn update(&mut self) -> Result<Stmt> {
        let target = self.expr()?;
        const COMPOUND: [(&str, BinaryOp); 10] = [
            ("+=", BinaryOp::Add),
            ("-=", BinaryOp::Sub),
            ("*=", BinaryOp::Mul),
            ("/=", BinaryOp::Div),
            ("%=", BinaryOp::Rem),
            ("&=", BinaryOp::BitAnd),
            ("|=", BinaryOp::BitOr),
            ("^=", BinaryOp::BitXor),
            ("<<=", BinaryOp::Shl),
            (">>=", BinaryOp::Shr),
        ];
        if self.eat("=") {
            return Ok(Stmt::Assign { target, op: None, value: self.expr()? });
        }
        for (p, op) in COMPOUND {
            if self.eat(p) {
                return Ok(Stmt::Assign { target, op: Some(op), value: self.expr()? });
            }
        }
        let one = Expr::new(ExprKind::Literal(Literal::I32(1)), Ty::Unknown);
        if self.eat("++") {
            return Ok(Stmt::Assign { target, op: Some(BinaryOp::Add), value: one });
        }
        if self.eat("--") {
            return Ok(Stmt::Assign { target, op: Some(BinaryOp::Sub), value: one });
        }
        match target.kind {
            ExprKind::Call { .. } | ExprKind::Builtin { .. } => Ok(Stmt::Expr(target)),
            _ => self.error("expected an assignment or a call"),
        }
    }

    fn if_statement(&mut self) -> Result<Stmt> {
        let cond = self.expr()?;
        let then = self.block()?;
        let otherwise = if self.eat_word("else") {
            if self.eat_word("if") { vec![self.if_statement()?] } else { self.block()? }
        } else {
            Vec::new()
        };
        Ok(Stmt::If { cond, then, otherwise })
    }

    fn switch(&mut self) -> Result<Stmt> {
        let selector = self.expr()?;
        self.expect("{")?;
        let mut cases = Vec::new();
        let mut default = Vec::new();
        loop {
            self.comments();
            if self.eat("}") {
                break;
            }
            if self.eat_word("default") {
                self.eat(":");
                default = self.block()?;
                continue;
            }
            if !self.eat_word("case") {
                return self.error("expected `case` or `default`");
            }
            let mut values = vec![self.expr()?];
            while self.eat(",") {
                if self.is(":") || self.is("{") {
                    break;
                }
                values.push(self.expr()?);
            }
            self.eat(":");
            cases.push((values, self.block()?));
        }
        Ok(Stmt::Switch { selector, cases, default })
    }

    // ------------------------------------------------------------------------------------------------- expressions

    fn expr(&mut self) -> Result<Expr> {
        self.binary(0)
    }

    /// Binary operators by precedence, lowest first.
    fn binary(&mut self, level: usize) -> Result<Expr> {
        const LEVELS: [&[(&str, BinaryOp)]; 10] = [
            &[("||", BinaryOp::Or)],
            &[("&&", BinaryOp::And)],
            &[("|", BinaryOp::BitOr)],
            &[("^", BinaryOp::BitXor)],
            &[("&", BinaryOp::BitAnd)],
            &[("==", BinaryOp::Eq), ("!=", BinaryOp::Ne)],
            &[("<=", BinaryOp::Le), (">=", BinaryOp::Ge), ("<", BinaryOp::Lt), (">", BinaryOp::Gt)],
            &[("<<", BinaryOp::Shl), (">>", BinaryOp::Shr)],
            &[("+", BinaryOp::Add), ("-", BinaryOp::Sub)],
            &[("*", BinaryOp::Mul), ("/", BinaryOp::Div), ("%", BinaryOp::Rem)],
        ];
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut lhs = self.binary(level + 1)?;
        'outer: loop {
            for (p, op) in LEVELS[level] {
                if self.is(p) {
                    self.next();
                    let rhs = self.binary(level + 1)?;
                    lhs = Expr::binary(*op, lhs, rhs, Ty::Unknown);
                    continue 'outer;
                }
            }
            return Ok(lhs);
        }
    }

    fn unary(&mut self) -> Result<Expr> {
        let op = if self.eat("-") {
            UnaryOp::Neg
        } else if self.eat("!") {
            UnaryOp::Not
        } else if self.eat("~") {
            UnaryOp::BitNot
        } else if self.eat("*") {
            let inner = self.unary()?;
            return Ok(Expr::new(ExprKind::Deref(Box::new(inner)), Ty::Unknown));
        } else if self.eat("&") {
            let inner = self.unary()?;
            return Ok(Expr::new(ExprKind::AddressOf(Box::new(inner)), Ty::Unknown));
        } else {
            return self.postfix();
        };
        let arg = self.unary()?;
        Ok(Expr::new(ExprKind::Unary { op, arg: Box::new(arg) }, Ty::Unknown))
    }

    fn postfix(&mut self) -> Result<Expr> {
        let mut e = self.primary()?;
        loop {
            if self.eat(".") {
                let member = self.ident()?;
                e = Expr::new(ExprKind::Member { base: Box::new(e), member }, Ty::Unknown);
            } else if self.eat("[") {
                let index = self.expr()?;
                self.expect("]")?;
                e = Expr::new(ExprKind::Index { base: Box::new(e), index: Box::new(index) }, Ty::Unknown);
            } else {
                return Ok(e);
            }
        }
    }

    fn args(&mut self) -> Result<Vec<Expr>> {
        self.expect("(")?;
        let mut out = Vec::new();
        while !self.eat(")") {
            out.push(self.expr()?);
            if !self.eat(",") {
                self.expect(")")?;
                break;
            }
        }
        Ok(out)
    }

    fn primary(&mut self) -> Result<Expr> {
        match self.peek().cloned() {
            Some(Tok::Punct("(")) => {
                self.next();
                let e = self.expr()?;
                self.expect(")")?;
                Ok(e)
            }
            Some(Tok::Number { text, suffix, float }) => {
                self.next();
                self.number(&text, suffix, float)
            }
            Some(Tok::Ident(name)) if name == "true" || name == "false" => {
                self.next();
                Ok(Expr::bool(name == "true"))
            }
            Some(Tok::Ident(name)) if name == "bitcast" => {
                self.next();
                self.expect("<")?;
                let ty = self.ty()?;
                self.expect(">")?;
                let mut args = self.args()?;
                if args.len() != 1 {
                    return self.error("bitcast takes one argument");
                }
                Ok(args.remove(0).bitcast(ty))
            }
            Some(Tok::Ident(name)) if Parser::type_name(&name) => {
                let ty = self.ty()?;
                let mut args = self.args()?;
                // A scalar type of one argument converts; anything else constructs.
                if ty.is_scalar() && args.len() == 1 {
                    Ok(args.remove(0).convert(ty))
                } else {
                    Ok(Expr::construct(ty, args))
                }
            }
            Some(Tok::Ident(name)) => {
                self.next();
                if !self.is("(") {
                    return Ok(Expr::name(name, Ty::Unknown));
                }
                let args = self.args()?;
                Ok(match Builtin::from_wgsl(&name) {
                    Some(b) => Expr::builtin(b, args, Ty::Unknown),
                    // A struct name or a function: `infer` tells them apart.
                    None => Expr::call(name, args, Ty::Unknown),
                })
            }
            other => self.error(format!("expected an expression, found {other:?}")),
        }
    }

    /// A literal. Without a suffix it is abstract ([`Ty::Unknown`]) until `infer` gives it a type.
    fn number(&self, text: &str, suffix: Option<char>, float: bool) -> Result<Expr> {
        if float {
            let x: f64 = text.parse().or_else(|_| self.error(format!("bad float {text}")))?;
            let ty = if suffix == Some('f') { Ty::F32 } else { Ty::Unknown };
            return Ok(Expr::new(ExprKind::Literal(Literal::F32(x as f32)), ty));
        }
        let value = match text.strip_prefix("0x").or(text.strip_prefix("0X")) {
            Some(hex) => i64::from_str_radix(hex, 16),
            None => text.parse::<i64>(),
        }
        .or_else(|_| self.error(format!("bad integer {text}")))?;
        match suffix {
            Some('u') => {
                let v = u32::try_from(value).or_else(|_| self.error(format!("{text}u is out of range")))?;
                Ok(Expr::u32(v))
            }
            Some('i') => {
                let v = i32::try_from(value).or_else(|_| self.error(format!("{text}i is out of range")))?;
                Ok(Expr::i32(v))
            }
            _ => {
                let v = i32::try_from(value).or_else(|_| self.error(format!("{text} is out of range")))?;
                Ok(Expr::new(ExprKind::Literal(Literal::I32(v)), Ty::Unknown))
            }
        }
    }
}

/// Parses shader source in the library subset of WGSL into untyped items.
pub fn parse(src: &str) -> std::result::Result<Module, ParseError> {
    let mut p = Parser { tokens: lex(src)?, pos: 0 };
    p.module()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_function_with_comments_and_control_flow() {
        let m = parse(
            "// Header.\nconst K: f32 = 0.5 - 2.0;\nstruct S { a: vec3<f32>, b: f32, }\n\
             fn f(x: f32, p: ptr<function, i32>) -> f32 {\n    // Lead.\n    var y = x;  // trailing\n\
                 for (var i = 0; i < 4; i++) { y += f32(i); }\n    if (y > 1.0) { *p = 1; } else if (y < 0.0) { return 0.0; }\n\
                 switch *p { case 0, 1: { y = 2.0; } default: { } }\n    return select(y, -y, y < 0.0) * K;\n}\n",
        )
        .unwrap();
        assert!(matches!(&m.items[0], Item::Comment(c) if c == "Header."));
        assert!(matches!(&m.items[2], Item::Struct(s) if s.members.len() == 2));
        let Item::Function(f) = &m.items[3] else { panic!() };
        assert_eq!(f.params[1].1, Ty::Ptr(Box::new(Ty::I32)));
        assert!(matches!(&f.body[0], Stmt::Comment { text, trailing: false } if text == "Lead."));
        assert!(matches!(&f.body[2], Stmt::Comment { text, trailing: true } if text == "trailing"));
        assert!(matches!(&f.body[3], Stmt::For { .. }));
        assert!(matches!(&f.body[4], Stmt::If { otherwise, .. } if matches!(otherwise[0], Stmt::If { .. })));
        assert!(matches!(&f.body[5], Stmt::Switch { cases, .. } if cases[0].0.len() == 2));
    }

    #[test]
    fn precedence_and_literals() {
        let m = parse("const A = 1 + 2 * 3u << 1;").unwrap();
        let Item::Const { value, .. } = &m.items[0] else { panic!() };
        // (1 + (2 * 3u)) << 1
        let ExprKind::Binary { op: BinaryOp::Shl, lhs, .. } = &value.kind else { panic!("{value:?}") };
        assert!(matches!(&lhs.kind, ExprKind::Binary { op: BinaryOp::Add, .. }));
    }

    #[test]
    fn conversions_constructors_and_bitcasts() {
        let m = parse("const A = vec3<f32>(f32(1u), 2.0, bitcast<f32>(0x7fc00000u));").unwrap();
        let Item::Const { value, .. } = &m.items[0] else { panic!() };
        let ExprKind::Construct(args) = &value.kind else { panic!() };
        assert!(matches!(args[0].kind, ExprKind::Convert(_)));
        assert!(matches!(args[2].kind, ExprKind::Bitcast(_)));
        assert_eq!(value.ty, Ty::vec(3));
    }

    #[test]
    fn errors_name_the_line() {
        let e = parse("fn f() {\n  let x = ;\n}").unwrap_err();
        assert_eq!(e.line, 2);
    }
}
