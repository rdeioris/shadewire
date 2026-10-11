use crate::{NodeId, Type};

#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceField {
    pub name: String,
    pub ty: Type,
    pub node: NodeId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub model: String,
    pub fields: Vec<SurfaceField>,
}
