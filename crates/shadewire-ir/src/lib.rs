//! Shadewire typed material DAG, optimizer and interpreter

mod context;
mod op;
mod ops;
mod space;
mod texture;
mod types;
mod value;
mod value_parse;

pub use context::{ContextClass, ContextRead};
pub use op::{Op, TransformKind};
pub use ops::{input_names, op_for_category};
pub use space::Space;
pub use texture::{AddressMode, FilterMode, TextureDesc};
pub use types::Type;
pub use value::Value;
pub use value_parse::ParseValueError;
