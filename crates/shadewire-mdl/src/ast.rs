//! An MDL module as parsed: declarations, statements and expressions with the line each starts on. Names are not
//! resolved here; built-in types (`float3`, `color`, `material`) are names like any other.

/// `::a::b`, `a::b` or `b`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct QualifiedName {
    pub absolute: bool,
    pub parts: Vec<String>,
}

impl QualifiedName {
    pub fn simple(name: &str) -> QualifiedName {
        QualifiedName { absolute: false, parts: vec![name.to_string()] }
    }

    /// The last part: the name without its module.
    pub fn last(&self) -> &str {
        self.parts.last().map(String::as_str).unwrap_or("")
    }

    pub fn is_simple(&self) -> bool {
        !self.absolute && self.parts.len() == 1
    }
}

impl std::fmt::Display for QualifiedName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.absolute {
            f.write_str("::")?;
        }
        f.write_str(&self.parts.join("::"))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    /// `mdl 1.7;` as `(1, 7)`.
    pub version: (u32, u32),
    pub imports: Vec<Import>,
    pub annotations: Vec<Annotation>,
    pub decls: Vec<Decl>,
}

/// Where an import path starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prefix {
    /// No prefix: the search path, or (MDL before 1.6) the current package.
    None,
    /// `::`
    Absolute,
    /// `.::`
    Current,
    /// `..::` this many times.
    Parent(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportPath {
    pub prefix: Prefix,
    pub parts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Import {
    /// `import a::b::*;` (`name: None`) or `import a::b::name;`.
    Qualified { path: ImportPath, name: Option<String>, line: u32 },
    /// `[export] using a::b import *;` (`names: None`) or `import x, y;`.
    Using { export: bool, path: ImportPath, names: Option<Vec<String>>, line: u32 },
    /// `using name = "package";` or `using name = ::a::b;`: `name` stands for `target` at the start of later
    /// import paths.
    Alias { name: String, target: ImportPath, line: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frequency {
    Uniform,
    Varying,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArraySize {
    /// `T[]`
    Unsized,
    /// `T[<N>]`
    Deferred(String),
    /// `T[n]`
    Sized(Box<Expr>),
}

/// A type as written.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeRef {
    pub frequency: Option<Frequency>,
    /// `None` for `auto`.
    pub name: Option<QualifiedName>,
    pub array: Option<ArraySize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub name: QualifiedName,
    pub args: Vec<Arg>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arg {
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub ty: TypeRef,
    pub name: String,
    pub default: Option<Expr>,
    pub annotations: Vec<Annotation>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    /// A declaration only (the standard modules' intrinsics).
    None,
    Block(Vec<Stmt>),
    /// `= expression;`: an expression-bodied function or a material.
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    pub declarative: bool,
    pub ret: TypeRef,
    pub name: String,
    /// `None` for a variant, `(*)`.
    pub params: Option<Vec<Param>>,
    pub frequency: Option<Frequency>,
    pub annotations: Vec<Annotation>,
    pub body: Body,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub ty: TypeRef,
    pub name: String,
    pub default: Option<Expr>,
    pub annotations: Vec<Annotation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Struct {
    pub declarative: bool,
    pub name: String,
    pub category: Option<QualifiedName>,
    pub annotations: Vec<Annotation>,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumValue {
    pub name: String,
    pub value: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Enum {
    pub name: String,
    pub annotations: Vec<Annotation>,
    pub values: Vec<EnumValue>,
}

/// `name(args)` or `name = value` in a constant or variable declaration.
#[derive(Clone, Debug, PartialEq)]
pub enum Init {
    None,
    Args(Vec<Arg>),
    Value(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Declarator {
    pub name: String,
    pub init: Init,
    pub annotations: Vec<Annotation>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DeclKind {
    Annotation { name: String, params: Vec<Param> },
    StructCategory { name: String },
    Struct(Struct),
    Enum(Enum),
    Typedef { ty: TypeRef, name: String },
    Const { ty: TypeRef, declarators: Vec<Declarator> },
    Function(Function),
    Import(Import),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decl {
    pub export: bool,
    pub kind: DeclKind,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Block(Vec<Stmt>),
    If {
        cond: Expr,
        then: Box<Stmt>,
        otherwise: Option<Box<Stmt>>,
    },
    /// Cases fall through, as in C; `None` is `default`.
    Switch {
        selector: Expr,
        cases: Vec<(Option<Expr>, Vec<Stmt>)>,
    },
    While {
        cond: Expr,
        body: Box<Stmt>,
    },
    DoWhile {
        body: Box<Stmt>,
        cond: Expr,
    },
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        update: Option<Expr>,
        body: Box<Stmt>,
    },
    Break,
    Continue,
    Return(Option<Expr>),
    /// A local struct, enum, typedef or constant.
    Decl(DeclKind),
    Var {
        ty: TypeRef,
        declarators: Vec<Declarator>,
    },
    Expr(Option<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinOp {
    LogicalOr,
    LogicalAnd,
    BitOr,
    BitXor,
    BitAnd,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Shl,
    Shr,
    /// `>>>`
    ShrUnsigned,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::LogicalOr => "||",
            BinOp::LogicalAnd => "&&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::BitAnd => "&",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::ShrUnsigned => ">>>",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Plus,
    Not,
    BitNot,
    PreInc,
    PreDec,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    Bool(bool),
    Int(i64),
    /// The value and whether it was written as a `double` (`d` suffix).
    Float(f64, bool),
    String(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Literal(Literal),
    Name(QualifiedName),
    /// `T[n]`, `T[<N>]` or `T[]`: only as the callee of an array constructor.
    ArrayType {
        elem: QualifiedName,
        size: Option<ArraySize>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Arg>,
    },
    Member {
        base: Box<Expr>,
        name: String,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        arg: Box<Expr>,
    },
    PostInc(Box<Expr>),
    PostDec(Box<Expr>),
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Ternary {
        cond: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    /// `target = value` (`op: None`) or `target op= value`.
    Assign {
        op: Option<BinOp>,
        target: Box<Expr>,
        value: Box<Expr>,
    },
    /// `a, b`: the value of the last.
    Sequence(Vec<Expr>),
    Let {
        decls: Vec<Stmt>,
        body: Box<Expr>,
    },
    Cast {
        ty: TypeRef,
        arg: Box<Expr>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub line: u32,
}

impl Expr {
    pub fn new(kind: ExprKind, line: u32) -> Expr {
        Expr { kind, line }
    }
}
