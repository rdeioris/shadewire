//! Shadewire core: the material IR (a typed DAG), optimizer, CPU interpreter and reflection

pub mod classify;
mod context;
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
pub use op::{Op, TransformKind, transform_name};
pub use ops::{eval_op, input_names, op_for_category};
pub use reflection::{BindingOptions, ContextEntry, Reflection, SourceInfo, reflect};
pub use space::Space;
pub use structure_hash::structure_hash;
pub use surface::{Surface, SurfaceField, model_struct_name};
pub use texture::{AddressMode, FilterMode, TextureDesc};
pub use types::Type;
pub use value::Value;
pub use value_parse::ParseValueError;
