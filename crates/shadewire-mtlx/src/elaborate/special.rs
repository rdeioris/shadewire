//! Categories with no op and no implementation graph (Task 16).

use shadewire_ir::{Error, NodeId};

use super::{Elaborator, Scope};
use crate::document::{Node, NodeDef};

/// True for categories `elaborate_special` handles. None yet.
pub fn is_special(_category: &str) -> bool {
    false
}

impl<'a> Elaborator<'a> {
    pub fn elaborate_special(
        &mut self,
        _scope: &Scope<'a>,
        _node: &'a Node,
        _nd: &'a NodeDef,
        _output: Option<&str>,
    ) -> Result<NodeId, Error> {
        unimplemented!("task 16")
    }
}
