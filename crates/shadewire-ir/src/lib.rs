//! Shadewire typed material DAG, optimizer and interpreter

mod context;
pub mod classify;
pub mod dag;
mod dump;
mod error;
pub mod interpret;
mod node;
mod op;
pub mod ops;
pub mod optimize;
pub mod reflection;
mod space;
mod structure_hash;
mod surface;
mod texture;
mod types;
mod value;
mod value_parse;

pub use classify::ParamKind;
pub use context::{ContextClass, ContextRead};
pub use dag::Dag;
pub use error::{Error, Location};
pub use node::{Node, NodeId, NodeKind};
pub use op::{Op, TransformKind};
pub use ops::{eval_op, input_names, op_for_category};
pub use reflection::{reflect, BindingOptions, Reflection, SourceInfo};
pub use space::Space;
pub use structure_hash::structure_hash;
pub use surface::{Surface, SurfaceField};
pub use texture::{AddressMode, FilterMode, TextureDesc};
pub use types::Type;
pub use value::Value;
pub use value_parse::ParseValueError;
