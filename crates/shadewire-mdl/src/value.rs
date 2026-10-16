//! What the evaluator computes: a value of an MDL type, each part either known at compile time or an IR node.

use std::rc::Rc;

use shadewire::{NodeId, Value};

use crate::types::{TextureShape, Ty};

/// A number known now, or the IR node computing it.
#[derive(Clone, Debug, PartialEq)]
pub enum Num {
    Known(Value),
    Node(NodeId),
}

impl Num {
    pub fn known(&self) -> Option<&Value> {
        match self {
            Num::Known(v) => Some(v),
            Num::Node(_) => None,
        }
    }
}

/// How a texture's texels are read: MDL's `tex::gamma_mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gamma {
    Default,
    Linear,
    Srgb,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    pub shape: TextureShape,
    /// The path as resolved for the host: as written for the main module, joined to its module's folder for an
    /// imported one.
    pub file: String,
    pub gamma: Gamma,
}

/// A distribution function: MDL's `bsdf`, `edf`, `vdf` or `hair_bsdf` value, kept as written until the material
/// is finished (`material.rs` maps it onto pbrlib lobes).
#[derive(Clone, Debug, PartialEq)]
pub enum Df {
    /// `bsdf()`, `edf()`, `vdf()`, `hair_bsdf()`: black, or (`vdf`) isotropic.
    Default,
    /// A `::df` function with every parameter bound, defaults included, by name.
    Call { name: String, args: Vec<(String, Val)>, line: u32 },
    /// `cond ? then : otherwise` on a run-time condition (a boolean node).
    Select { cond: NodeId, then: Rc<Df>, otherwise: Rc<Df> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum V {
    /// A scalar, a float vector or a color: one IR value.
    Num(Num),
    /// An integer or boolean vector: one scalar per component.
    Parts(Vec<Num>),
    /// A matrix: one float vector per column.
    Columns(Vec<Num>),
    Str(String),
    Enum(i32),
    Texture(Texture),
    /// A light profile or BSDF measurement: its file.
    Resource(String),
    /// Field values in declaration order.
    Struct(Vec<Val>),
    Array(Vec<Val>),
    Df(Rc<Df>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Val {
    pub ty: Ty,
    pub v: V,
}

impl Val {
    pub fn new(ty: Ty, v: V) -> Val {
        Val { ty, v }
    }

    pub fn known(ty: Ty, value: Value) -> Val {
        Val { ty, v: V::Num(Num::Known(value)) }
    }

    pub fn node(ty: Ty, id: NodeId) -> Val {
        Val { ty, v: V::Num(Num::Node(id)) }
    }

    pub fn bool(b: bool) -> Val {
        Val::known(Ty::BOOL, Value::Boolean(b))
    }

    pub fn int(i: i32) -> Val {
        Val::known(Ty::INT, Value::Integer(i))
    }

    pub fn float(x: f32) -> Val {
        Val::known(Ty::FLOAT, Value::Float(x))
    }

    pub fn string(s: &str) -> Val {
        Val::new(Ty::String, V::Str(s.to_string()))
    }

    pub fn df(ty: Ty, df: Df) -> Val {
        Val { ty, v: V::Df(Rc::new(df)) }
    }

    pub fn num(&self) -> Option<&Num> {
        match &self.v {
            V::Num(n) => Some(n),
            _ => None,
        }
    }

    /// The value when known now: a scalar, float vector or color.
    pub fn known_value(&self) -> Option<&Value> {
        self.num().and_then(Num::known)
    }

    pub fn known_bool(&self) -> Option<bool> {
        match self.known_value() {
            Some(Value::Boolean(b)) => Some(*b),
            _ => None,
        }
    }

    pub fn known_int(&self) -> Option<i32> {
        match (&self.v, self.known_value()) {
            (_, Some(Value::Integer(i))) => Some(*i),
            (V::Enum(e), _) => Some(*e),
            _ => None,
        }
    }

    pub fn known_f32(&self) -> Option<f32> {
        match self.known_value() {
            Some(Value::Float(x)) => Some(*x),
            Some(Value::Integer(i)) => Some(*i as f32),
            _ => None,
        }
    }

    pub fn str(&self) -> Option<&str> {
        match &self.v {
            V::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn fields(&self) -> Option<&[Val]> {
        match &self.v {
            V::Struct(f) => Some(f),
            _ => None,
        }
    }

    pub fn elements(&self) -> Option<&[Val]> {
        match &self.v {
            V::Array(e) => Some(e),
            _ => None,
        }
    }

    pub fn df_value(&self) -> Option<&Rc<Df>> {
        match &self.v {
            V::Df(d) => Some(d),
            _ => None,
        }
    }
}
