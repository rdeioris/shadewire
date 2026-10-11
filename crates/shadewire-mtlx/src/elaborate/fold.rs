//! Folding an elaborated subgraph to a value, for parameters whose
//! authored value went through a conversion (colorspace, units).

use std::collections::HashMap;

use shadewire_ir::interpret::ContextSource;
use shadewire_ir::ops::broadcast::broadcast;
use shadewire_ir::{ContextRead, NodeId, NodeKind, Space, TransformKind, Value, eval_op};

use super::Elaborator;

/// Never called: ops that read context are not folded.
struct NoContext;

impl ContextSource for NoContext {
    fn read(&mut self, _read: &ContextRead) -> Value {
        unreachable!("folding never reads context")
    }
    fn transform(&mut self, _kind: TransformKind, _from: Space, _to: Space, _v: [f32; 3]) -> [f32; 3] {
        unreachable!("folding never transforms")
    }
}

impl<'a> Elaborator<'a> {
    /// The value of `id` when every leaf under it is a `Const`; `None` when it
    /// reaches a uniform, a context read, a sample or a context-reading op.
    pub fn fold_constant(&self, id: NodeId) -> Option<Value> {
        let mut memo = HashMap::new();
        self.fold_node(id, &mut memo)
    }

    fn fold_node(&self, id: NodeId, memo: &mut HashMap<NodeId, Value>) -> Option<Value> {
        if let Some(value) = memo.get(&id) {
            return Some(value.clone());
        }
        let node = self.dag.node(id);
        let value = match &node.kind {
            NodeKind::Const(value) => value.clone(),
            NodeKind::Op { op, args } if !op.reads_context() => {
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.fold_node(*arg, memo)?);
                }
                eval_op(op, &values, node.ty, &mut NoContext)
            }
            NodeKind::Op { .. } | NodeKind::Uniform { .. } | NodeKind::Context(_) | NodeKind::Sample { .. } => {
                return None;
            }
        };
        let value = broadcast(&value, node.ty);
        memo.insert(id, value.clone());
        Some(value)
    }
}
