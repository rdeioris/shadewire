//! The shader syntax tree every back end prints: items (structs, constants, resources, functions), statements
//! and typed expressions. The material code is built as a tree ([`super::material`]); the support libraries are
//! parsed into one ([`super::parse`]) and typed ([`super::infer`]).

use crate::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scalar {
    Bool,
    I32,
    U32,
    F32,
}

/// The type of a value. Vectors and matrices are of `f32` unless a vector says otherwise.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    Scalar(Scalar),
    Vector(u8, Scalar),
    /// Columns, rows.
    Matrix(u8, u8),
    Struct(String),
    /// A parameter the callee writes through (`ptr<function, T>`, an `inout` parameter).
    Ptr(Box<Ty>),
    Texture2d,
    Sampler,
    Void,
    /// Not inferred yet: what the parser leaves for [`super::infer`].
    Unknown,
}

impl Ty {
    pub const BOOL: Ty = Ty::Scalar(Scalar::Bool);
    pub const I32: Ty = Ty::Scalar(Scalar::I32);
    pub const U32: Ty = Ty::Scalar(Scalar::U32);
    pub const F32: Ty = Ty::Scalar(Scalar::F32);

    pub fn vec(n: u8) -> Ty {
        Ty::Vector(n, Scalar::F32)
    }

    /// The shader type of an IR value type: colors are vectors.
    ///
    /// # Panics
    /// On matrices: elaboration rejects matrix-typed values.
    pub fn of(ty: Type) -> Ty {
        match ty {
            Type::Boolean => Ty::BOOL,
            Type::Integer => Ty::I32,
            Type::Float => Ty::F32,
            Type::Vector2 => Ty::vec(2),
            Type::Vector3 | Type::Color3 => Ty::vec(3),
            Type::Vector4 | Type::Color4 => Ty::vec(4),
            Type::Matrix33 | Type::Matrix44 => panic!("matrix types are rejected in elaboration"),
        }
    }

    /// The component scalar of a scalar, vector or matrix.
    pub fn scalar(&self) -> Option<Scalar> {
        match self {
            Ty::Scalar(s) | Ty::Vector(_, s) => Some(*s),
            Ty::Matrix(..) => Some(Scalar::F32),
            _ => None,
        }
    }

    /// Components of a scalar (1) or vector (n).
    pub fn width(&self) -> Option<u8> {
        match self {
            Ty::Scalar(_) => Some(1),
            Ty::Vector(n, _) => Some(*n),
            _ => None,
        }
    }

    pub fn is_scalar(&self) -> bool {
        matches!(self, Ty::Scalar(_))
    }

    pub fn is_vector(&self) -> bool {
        matches!(self, Ty::Vector(..))
    }

    /// The same shape with another component scalar.
    pub fn with_scalar(&self, s: Scalar) -> Ty {
        match self {
            Ty::Vector(n, _) => Ty::Vector(*n, s),
            _ => Ty::Scalar(s),
        }
    }

    /// What a pointer points to; any other type is itself.
    pub fn pointee(&self) -> &Ty {
        match self {
            Ty::Ptr(t) => t,
            other => other,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Literal {
    Bool(bool),
    I32(i32),
    U32(u32),
    F32(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinaryOp {
    pub fn is_comparison(self) -> bool {
        matches!(self, BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge)
    }

    /// The operator as WGSL, GLSL, HLSL and MSL all spell it.
    pub fn symbol(self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Rem => "%",
            BinaryOp::BitAnd => "&",
            BinaryOp::BitOr => "|",
            BinaryOp::BitXor => "^",
            BinaryOp::Shl => "<<",
            BinaryOp::Shr => ">>",
            BinaryOp::Eq => "==",
            BinaryOp::Ne => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Le => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::Ge => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
        }
    }
}

/// The built-in functions the shader code calls, named as WGSL names them; each back end spells its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Builtin {
    Abs,
    Sign,
    Floor,
    Ceil,
    /// Rounds halfway cases to even, as WGSL's `round`.
    Round,
    Fract,
    Trunc,
    Pow,
    Sqrt,
    InverseSqrt,
    Exp,
    Exp2,
    Log,
    Log2,
    Min,
    Max,
    Clamp,
    Mix,
    Smoothstep,
    Step,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Atan2,
    Radians,
    Degrees,
    Normalize,
    Length,
    Distance,
    Dot,
    Cross,
    Reflect,
    Transpose,
    Fma,
    /// `select(f, t, cond)`: componentwise when `cond` is a vector.
    Select,
    All,
    Any,
    Dpdx,
    Dpdy,
    /// `textureSample(texture, sampler, coords)`: fragment stage only.
    TextureSample,
    /// `textureSampleLevel(texture, sampler, coords, level)`.
    TextureSampleLevel,
    /// `textureSampleGrad(texture, sampler, coords, ddx, ddy)`.
    TextureSampleGrad,
}

impl Builtin {
    /// Every built-in with its WGSL name.
    pub const ALL: [(Builtin, &'static str); 45] = [
        (Builtin::Abs, "abs"),
        (Builtin::Sign, "sign"),
        (Builtin::Floor, "floor"),
        (Builtin::Ceil, "ceil"),
        (Builtin::Round, "round"),
        (Builtin::Fract, "fract"),
        (Builtin::Trunc, "trunc"),
        (Builtin::Pow, "pow"),
        (Builtin::Sqrt, "sqrt"),
        (Builtin::InverseSqrt, "inverseSqrt"),
        (Builtin::Exp, "exp"),
        (Builtin::Exp2, "exp2"),
        (Builtin::Log, "log"),
        (Builtin::Log2, "log2"),
        (Builtin::Min, "min"),
        (Builtin::Max, "max"),
        (Builtin::Clamp, "clamp"),
        (Builtin::Mix, "mix"),
        (Builtin::Smoothstep, "smoothstep"),
        (Builtin::Step, "step"),
        (Builtin::Sin, "sin"),
        (Builtin::Cos, "cos"),
        (Builtin::Tan, "tan"),
        (Builtin::Asin, "asin"),
        (Builtin::Acos, "acos"),
        (Builtin::Atan, "atan"),
        (Builtin::Atan2, "atan2"),
        (Builtin::Radians, "radians"),
        (Builtin::Degrees, "degrees"),
        (Builtin::Normalize, "normalize"),
        (Builtin::Length, "length"),
        (Builtin::Distance, "distance"),
        (Builtin::Dot, "dot"),
        (Builtin::Cross, "cross"),
        (Builtin::Reflect, "reflect"),
        (Builtin::Transpose, "transpose"),
        (Builtin::Fma, "fma"),
        (Builtin::Select, "select"),
        (Builtin::All, "all"),
        (Builtin::Any, "any"),
        (Builtin::Dpdx, "dpdx"),
        (Builtin::Dpdy, "dpdy"),
        (Builtin::TextureSample, "textureSample"),
        (Builtin::TextureSampleLevel, "textureSampleLevel"),
        (Builtin::TextureSampleGrad, "textureSampleGrad"),
    ];

    pub fn from_wgsl(name: &str) -> Option<Builtin> {
        Builtin::ALL.iter().find(|(_, n)| *n == name).map(|(b, _)| *b)
    }

    pub fn wgsl_name(self) -> &'static str {
        Builtin::ALL.iter().find(|(b, _)| *b == self).map(|(_, n)| *n).expect("every built-in has a name")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: Ty,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Literal(Literal),
    /// A local, a parameter, a constant or a resource.
    Name(String),
    /// A call of a function defined in shader code (or by the host).
    Call {
        function: String,
        args: Vec<Expr>,
    },
    Builtin {
        builtin: Builtin,
        args: Vec<Expr>,
    },
    /// A value of the expression's type from its parts: a vector from scalars and vectors (one scalar splats,
    /// one same-width vector relabels), a matrix from columns, a struct from its members in order.
    Construct(Vec<Expr>),
    /// A numeric conversion to the expression's type, componentwise (`f32(i)`, `bool(x)`).
    Convert(Box<Expr>),
    /// The same bits as the expression's type.
    Bitcast(Box<Expr>),
    Unary {
        op: UnaryOp,
        arg: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Member {
        base: Box<Expr>,
        member: String,
    },
    /// Components by `xyzw` letters.
    Swizzle {
        base: Box<Expr>,
        components: String,
    },
    /// A vector component or a matrix column.
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    /// What a pointer parameter points to.
    Deref(Box<Expr>),
    /// A local passed to a pointer parameter.
    AddressOf(Box<Expr>),
}

impl Expr {
    pub fn new(kind: ExprKind, ty: Ty) -> Expr {
        Expr { kind, ty }
    }

    pub fn name(name: impl Into<String>, ty: Ty) -> Expr {
        Expr::new(ExprKind::Name(name.into()), ty)
    }

    pub fn f32(x: f32) -> Expr {
        Expr::new(ExprKind::Literal(Literal::F32(x)), Ty::F32)
    }

    pub fn i32(x: i32) -> Expr {
        Expr::new(ExprKind::Literal(Literal::I32(x)), Ty::I32)
    }

    pub fn u32(x: u32) -> Expr {
        Expr::new(ExprKind::Literal(Literal::U32(x)), Ty::U32)
    }

    pub fn bool(x: bool) -> Expr {
        Expr::new(ExprKind::Literal(Literal::Bool(x)), Ty::BOOL)
    }

    pub fn call(function: impl Into<String>, args: Vec<Expr>, ty: Ty) -> Expr {
        Expr::new(ExprKind::Call { function: function.into(), args }, ty)
    }

    pub fn builtin(builtin: Builtin, args: Vec<Expr>, ty: Ty) -> Expr {
        Expr::new(ExprKind::Builtin { builtin, args }, ty)
    }

    pub fn construct(ty: Ty, args: Vec<Expr>) -> Expr {
        Expr::new(ExprKind::Construct(args), ty)
    }

    pub fn convert(self, ty: Ty) -> Expr {
        Expr::new(ExprKind::Convert(Box::new(self)), ty)
    }

    pub fn bitcast(self, ty: Ty) -> Expr {
        Expr::new(ExprKind::Bitcast(Box::new(self)), ty)
    }

    pub fn unary(op: UnaryOp, arg: Expr) -> Expr {
        let ty = arg.ty.clone();
        Expr::new(ExprKind::Unary { op, arg: Box::new(arg) }, ty)
    }

    pub fn binary(op: BinaryOp, lhs: Expr, rhs: Expr, ty: Ty) -> Expr {
        Expr::new(ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) }, ty)
    }

    pub fn member(self, member: impl Into<String>, ty: Ty) -> Expr {
        Expr::new(ExprKind::Member { base: Box::new(self), member: member.into() }, ty)
    }

    pub fn swizzle(self, components: &str) -> Expr {
        let scalar = self.ty.scalar().expect("swizzle of a vector");
        let ty = if components.len() == 1 { Ty::Scalar(scalar) } else { Ty::Vector(components.len() as u8, scalar) };
        Expr::new(ExprKind::Swizzle { base: Box::new(self), components: components.into() }, ty)
    }

    pub fn index(self, index: Expr) -> Expr {
        let ty = match &self.ty {
            Ty::Vector(_, s) => Ty::Scalar(*s),
            Ty::Matrix(_, rows) => Ty::vec(*rows),
            other => panic!("index of {other:?}"),
        };
        Expr::new(ExprKind::Index { base: Box::new(self), index: Box::new(index) }, ty)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    /// `let` (immutable) or `var` (mutable), with an initial value unless a `var` has none.
    Let {
        name: String,
        ty: Ty,
        value: Option<Expr>,
        mutable: bool,
    },
    /// `target = value`, or `target op= value`.
    Assign {
        target: Expr,
        op: Option<BinaryOp>,
        value: Expr,
    },
    If {
        cond: Expr,
        then: Vec<Stmt>,
        /// An `else if` is an `If` alone in here.
        otherwise: Vec<Stmt>,
    },
    For {
        init: Box<Stmt>,
        cond: Expr,
        update: Box<Stmt>,
        body: Vec<Stmt>,
    },
    /// Cases never fall through.
    Switch {
        selector: Expr,
        cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
        default: Vec<Stmt>,
    },
    Return(Option<Expr>),
    Break,
    Continue,
    /// A call whose value is dropped.
    Expr(Expr),
    /// A comment line, or (`trailing`) one ending the statement before it.
    Comment {
        text: String,
        trailing: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    pub name: String,
    pub params: Vec<(String, Ty)>,
    pub ret: Ty,
    pub body: Vec<Stmt>,
}

/// What a resource is.
#[derive(Clone, Debug, PartialEq)]
pub enum ResourceKind {
    /// A uniform block of the struct named here.
    Uniform(String),
    Texture2d,
    Sampler,
}

/// A resource the host binds: WGSL's `@group(group) @binding(binding)`, which each back end maps to its own.
#[derive(Clone, Debug, PartialEq)]
pub struct Resource {
    pub name: String,
    pub kind: ResourceKind,
    pub group: u32,
    pub binding: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Struct {
    pub name: String,
    pub members: Vec<(String, Ty)>,
    /// Byte offsets of the members and the size, for a struct the host fills (a uniform block): back ends
    /// whose layout rules differ pad to match.
    pub layout: Option<(Vec<u32>, u32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Comment(String),
    Struct(Struct),
    Const {
        name: String,
        ty: Ty,
        value: Expr,
    },
    Resource(Resource),
    /// A function the host defines (`shadewire_host_*`): declared, not defined.
    Extern(Function),
    Function(Function),
}

/// A sequence of items; printing one prints them in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Module {
    pub items: Vec<Item>,
}

impl Module {
    pub fn functions(&self) -> impl Iterator<Item = &Function> {
        self.items.iter().filter_map(|i| if let Item::Function(f) = i { Some(f) } else { None })
    }

    pub fn structs(&self) -> impl Iterator<Item = &Struct> {
        self.items.iter().filter_map(|i| if let Item::Struct(s) = i { Some(s) } else { None })
    }

    pub fn resources(&self) -> impl Iterator<Item = &Resource> {
        self.items.iter().filter_map(|i| if let Item::Resource(r) = i { Some(r) } else { None })
    }
}
