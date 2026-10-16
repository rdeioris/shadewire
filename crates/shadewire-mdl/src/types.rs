//! MDL types as the evaluator sees them, and how each maps onto the IR's value types.

use shadewire::Type;

/// Index of a struct in `Program::structs`.
pub type StructId = usize;
/// Index of an enum in `Program::enums`.
pub type EnumId = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Scalar {
    Bool,
    Int,
    Float,
    /// Evaluated as `float`: the IR has one precision. Kept apart for overload resolution.
    Double,
}

impl Scalar {
    pub fn name(self) -> &'static str {
        match self {
            Scalar::Bool => "bool",
            Scalar::Int => "int",
            Scalar::Float => "float",
            Scalar::Double => "double",
        }
    }

    /// Whether a value of `self` converts to `to` without a constructor (MDL 6.9.2).
    pub fn converts_to(self, to: Scalar) -> bool {
        use Scalar::*;
        self == to || matches!((self, to), (Bool, Int | Float | Double) | (Int, Float | Double) | (Float, Double))
    }

    pub fn is_floating(self) -> bool {
        matches!(self, Scalar::Float | Scalar::Double)
    }
}

/// The resource a texture names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureShape {
    Tex2d,
    Tex3d,
    Cube,
    Ptex,
}

/// An array's length: known, or a size parameter (`T[<N>]`) still to be bound.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Len {
    Fixed(usize),
    Deferred(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    Scalar(Scalar),
    /// A vector of 2 to 4 components.
    Vector(Scalar, u8),
    /// Columns, rows; `Float` or `Double` components.
    Matrix(Scalar, u8, u8),
    Color,
    String,
    Texture(TextureShape),
    LightProfile,
    BsdfMeasurement,
    Bsdf,
    Edf,
    Vdf,
    HairBsdf,
    Struct(StructId),
    Enum(EnumId),
    Array(Box<Ty>, Len),
}

impl Ty {
    pub const BOOL: Ty = Ty::Scalar(Scalar::Bool);
    pub const INT: Ty = Ty::Scalar(Scalar::Int);
    pub const FLOAT: Ty = Ty::Scalar(Scalar::Float);

    pub fn float_vector(n: u8) -> Ty {
        if n == 1 { Ty::FLOAT } else { Ty::Vector(Scalar::Float, n) }
    }

    /// The type named by one of MDL's reserved type words; `None` for any other name.
    pub fn builtin(name: &str) -> Option<Ty> {
        let scalar = |s: &str| match s {
            "bool" => Some(Scalar::Bool),
            "int" => Some(Scalar::Int),
            "float" => Some(Scalar::Float),
            "double" => Some(Scalar::Double),
            _ => None,
        };
        if let Some(s) = scalar(name) {
            return Some(Ty::Scalar(s));
        }
        let simple = match name {
            "color" => Some(Ty::Color),
            "string" => Some(Ty::String),
            "texture_2d" => Some(Ty::Texture(TextureShape::Tex2d)),
            "texture_3d" => Some(Ty::Texture(TextureShape::Tex3d)),
            "texture_cube" => Some(Ty::Texture(TextureShape::Cube)),
            "texture_ptex" => Some(Ty::Texture(TextureShape::Ptex)),
            "light_profile" => Some(Ty::LightProfile),
            "bsdf_measurement" => Some(Ty::BsdfMeasurement),
            "bsdf" => Some(Ty::Bsdf),
            "edf" => Some(Ty::Edf),
            "vdf" => Some(Ty::Vdf),
            "hair_bsdf" => Some(Ty::HairBsdf),
            _ => None,
        };
        if simple.is_some() {
            return simple;
        }
        // `float3`, `int2`, `double4x3`, ...
        let split = name.find(|c: char| c.is_ascii_digit())?;
        let s = scalar(&name[..split])?;
        let dims = &name[split..];
        let digit = |c: u8| (b'2'..=b'4').contains(&c).then_some(c - b'0');
        match dims.as_bytes() {
            [n] => Some(Ty::Vector(s, digit(*n)?)),
            [c, b'x', r] if s.is_floating() => Some(Ty::Matrix(s, digit(*c)?, digit(*r)?)),
            _ => None,
        }
    }

    pub fn scalar(&self) -> Option<Scalar> {
        match self {
            Ty::Scalar(s) | Ty::Vector(s, _) | Ty::Matrix(s, ..) => Some(*s),
            Ty::Color => Some(Scalar::Float),
            _ => None,
        }
    }

    /// Components of a scalar (1), vector or color.
    pub fn width(&self) -> Option<u8> {
        match self {
            Ty::Scalar(_) => Some(1),
            Ty::Vector(_, n) => Some(*n),
            Ty::Color => Some(3),
            _ => None,
        }
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Ty::Scalar(_) | Ty::Vector(..) | Ty::Color)
    }

    pub fn is_df(&self) -> bool {
        matches!(self, Ty::Bsdf | Ty::Edf | Ty::Vdf | Ty::HairBsdf)
    }

    /// The same shape with another component scalar.
    pub fn with_scalar(&self, s: Scalar) -> Ty {
        match self {
            Ty::Vector(_, n) => Ty::Vector(s, *n),
            Ty::Matrix(_, c, r) => Ty::Matrix(s, *c, *r),
            Ty::Color if s.is_floating() => Ty::Color,
            Ty::Color => Ty::Vector(s, 3),
            _ => Ty::Scalar(s),
        }
    }

    /// The IR type of a value of this type held as one IR value: scalars, float vectors and colors. Integer and
    /// boolean vectors are held by component, matrices by column.
    pub fn ir(&self) -> Option<Type> {
        Some(match self {
            Ty::Scalar(Scalar::Bool) => Type::Boolean,
            Ty::Scalar(Scalar::Int) => Type::Integer,
            Ty::Scalar(_) => Type::Float,
            Ty::Vector(s, 2) if s.is_floating() => Type::Vector2,
            Ty::Vector(s, 3) if s.is_floating() => Type::Vector3,
            Ty::Vector(s, 4) if s.is_floating() => Type::Vector4,
            Ty::Color => Type::Color3,
            _ => return None,
        })
    }

    /// The MDL type of an IR value type.
    pub fn of_ir(ty: Type) -> Ty {
        match ty {
            Type::Boolean => Ty::BOOL,
            Type::Integer => Ty::INT,
            Type::Float => Ty::FLOAT,
            Type::Vector2 => Ty::Vector(Scalar::Float, 2),
            Type::Vector3 => Ty::Vector(Scalar::Float, 3),
            Type::Vector4 | Type::Color4 => Ty::Vector(Scalar::Float, 4),
            Type::Color3 => Ty::Color,
            Type::Matrix33 => Ty::Matrix(Scalar::Float, 3, 3),
            Type::Matrix44 => Ty::Matrix(Scalar::Float, 4, 4),
        }
    }

    pub fn element(&self) -> Option<&Ty> {
        match self {
            Ty::Array(e, _) => Some(e),
            _ => None,
        }
    }
}

/// A type's name as MDL spells it, with struct and enum names supplied by `names`.
pub fn type_name(ty: &Ty, struct_name: &dyn Fn(StructId) -> String, enum_name: &dyn Fn(EnumId) -> String) -> String {
    match ty {
        Ty::Scalar(s) => s.name().to_string(),
        Ty::Vector(s, n) => format!("{}{n}", s.name()),
        Ty::Matrix(s, c, r) => format!("{}{c}x{r}", s.name()),
        Ty::Color => "color".into(),
        Ty::String => "string".into(),
        Ty::Texture(TextureShape::Tex2d) => "texture_2d".into(),
        Ty::Texture(TextureShape::Tex3d) => "texture_3d".into(),
        Ty::Texture(TextureShape::Cube) => "texture_cube".into(),
        Ty::Texture(TextureShape::Ptex) => "texture_ptex".into(),
        Ty::LightProfile => "light_profile".into(),
        Ty::BsdfMeasurement => "bsdf_measurement".into(),
        Ty::Bsdf => "bsdf".into(),
        Ty::Edf => "edf".into(),
        Ty::Vdf => "vdf".into(),
        Ty::HairBsdf => "hair_bsdf".into(),
        Ty::Struct(id) => struct_name(*id),
        Ty::Enum(id) => enum_name(*id),
        Ty::Array(e, Len::Fixed(n)) => format!("{}[{n}]", type_name(e, struct_name, enum_name)),
        Ty::Array(e, Len::Deferred(n)) => format!("{}[<{n}>]", type_name(e, struct_name, enum_name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_names() {
        assert_eq!(Ty::builtin("float3"), Some(Ty::Vector(Scalar::Float, 3)));
        assert_eq!(Ty::builtin("int2"), Some(Ty::Vector(Scalar::Int, 2)));
        assert_eq!(Ty::builtin("double4x3"), Some(Ty::Matrix(Scalar::Double, 4, 3)));
        assert_eq!(Ty::builtin("int2x2"), None);
        assert_eq!(Ty::builtin("float5"), None);
        assert_eq!(Ty::builtin("color"), Some(Ty::Color));
        assert_eq!(Ty::builtin("material"), None);
    }

    #[test]
    fn implicit_scalar_conversions() {
        assert!(Scalar::Int.converts_to(Scalar::Float));
        assert!(Scalar::Bool.converts_to(Scalar::Double));
        assert!(!Scalar::Float.converts_to(Scalar::Int));
        assert!(!Scalar::Double.converts_to(Scalar::Float));
    }

    #[test]
    fn ir_types() {
        assert_eq!(Ty::Vector(Scalar::Double, 3).ir(), Some(Type::Vector3));
        assert_eq!(Ty::Vector(Scalar::Int, 3).ir(), None);
        assert_eq!(Ty::Color.ir(), Some(Type::Color3));
    }
}
