use serde::Serialize;

/// A value type. MaterialX keeps `color3` and `vector3` apart; both are `vec3<f32>` in WGSL.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Type {
    Boolean,
    Integer,
    Float,
    Vector2,
    Vector3,
    Vector4,
    Color3,
    Color4,
    Matrix33,
    Matrix44,
}

impl Type {
    pub fn component_count(self) -> usize {
        match self {
            Type::Boolean | Type::Integer | Type::Float => 1,
            Type::Vector2 => 2,
            Type::Vector3 | Type::Color3 => 3,
            Type::Vector4 | Type::Color4 => 4,
            Type::Matrix33 => 9,
            Type::Matrix44 => 16,
        }
    }

    pub fn is_scalar(self) -> bool {
        self.component_count() == 1
    }

    pub fn is_matrix(self) -> bool {
        matches!(self, Type::Matrix33 | Type::Matrix44)
    }

    pub fn mtlx_name(self) -> &'static str {
        match self {
            Type::Boolean => "boolean",
            Type::Integer => "integer",
            Type::Float => "float",
            Type::Vector2 => "vector2",
            Type::Vector3 => "vector3",
            Type::Vector4 => "vector4",
            Type::Color3 => "color3",
            Type::Color4 => "color4",
            Type::Matrix33 => "matrix33",
            Type::Matrix44 => "matrix44",
        }
    }

    pub fn from_mtlx_name(name: &str) -> Option<Type> {
        match name {
            "boolean" => Some(Type::Boolean),
            "integer" => Some(Type::Integer),
            "float" => Some(Type::Float),
            "vector2" => Some(Type::Vector2),
            "vector3" => Some(Type::Vector3),
            "vector4" => Some(Type::Vector4),
            "color3" => Some(Type::Color3),
            "color4" => Some(Type::Color4),
            "matrix33" => Some(Type::Matrix33),
            "matrix44" => Some(Type::Matrix44),
            _ => None,
        }
    }
}
