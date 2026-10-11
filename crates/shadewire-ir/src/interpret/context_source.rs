use crate::{ContextRead, Space, TransformKind, Value};

/// Supplies everything the host provides to a shader: geometry, frame and light data.
pub trait ContextSource {
    fn read(&mut self, read: &ContextRead) -> Value;
    fn transform(&mut self, kind: TransformKind, from: Space, to: Space, v: [f32; 3]) -> [f32; 3];
}
