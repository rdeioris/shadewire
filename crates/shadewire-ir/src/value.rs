use serde::Serialize;

use crate::Type;

/// A constant value. Serialized untagged so reflection writes `0.35` or `[1,1,1]`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Value {
    Boolean(bool),
    Integer(i32),
    Float(f32),
    Vector2([f32; 2]),
    Vector3([f32; 3]),
    Vector4([f32; 4]),
    Color3([f32; 3]),
    Color4([f32; 4]),
    Matrix33([f32; 9]),
    Matrix44([f32; 16]),
}

impl Value {
    pub fn ty(&self) -> Type {
        match self {
            Value::Boolean(_) => Type::Boolean,
            Value::Integer(_) => Type::Integer,
            Value::Float(_) => Type::Float,
            Value::Vector2(_) => Type::Vector2,
            Value::Vector3(_) => Type::Vector3,
            Value::Vector4(_) => Type::Vector4,
            Value::Color3(_) => Type::Color3,
            Value::Color4(_) => Type::Color4,
            Value::Matrix33(_) => Type::Matrix33,
            Value::Matrix44(_) => Type::Matrix44,
        }
    }

    /// The components as f32, in order. Booleans are 0.0 or 1.0, integers are cast.
    pub fn floats(&self) -> Vec<f32> {
        match self {
            Value::Boolean(b) => vec![if *b { 1.0 } else { 0.0 }],
            Value::Integer(i) => vec![*i as f32],
            Value::Float(x) => vec![*x],
            Value::Vector2(v) => v.to_vec(),
            Value::Vector3(v) => v.to_vec(),
            Value::Vector4(v) => v.to_vec(),
            Value::Color3(v) => v.to_vec(),
            Value::Color4(v) => v.to_vec(),
            Value::Matrix33(v) => v.to_vec(),
            Value::Matrix44(v) => v.to_vec(),
        }
    }

    /// Builds a value of `ty` from components. Panics if the count is wrong.
    pub fn from_floats(ty: Type, floats: &[f32]) -> Value {
        assert_eq!(floats.len(), ty.component_count(), "wrong component count for {ty:?}");
        match ty {
            Type::Boolean => Value::Boolean(floats[0] != 0.0),
            Type::Integer => Value::Integer(floats[0] as i32),
            Type::Float => Value::Float(floats[0]),
            Type::Vector2 => Value::Vector2([floats[0], floats[1]]),
            Type::Vector3 => Value::Vector3([floats[0], floats[1], floats[2]]),
            Type::Vector4 => Value::Vector4([floats[0], floats[1], floats[2], floats[3]]),
            Type::Color3 => Value::Color3([floats[0], floats[1], floats[2]]),
            Type::Color4 => Value::Color4([floats[0], floats[1], floats[2], floats[3]]),
            Type::Matrix33 => Value::Matrix33(floats.try_into().expect("nine components")),
            Type::Matrix44 => Value::Matrix44(floats.try_into().expect("sixteen components")),
        }
    }

    pub fn splat(ty: Type, x: f32) -> Value {
        Value::from_floats(ty, &vec![x; ty.component_count()])
    }

    pub fn zero(ty: Type) -> Value {
        Value::splat(ty, 0.0)
    }

    pub fn as_f32(&self) -> f32 {
        self.floats()[0]
    }

    pub fn as_bool(&self) -> bool {
        self.as_f32() != 0.0
    }
}
