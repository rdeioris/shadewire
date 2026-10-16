//! The evaluator: MDL run at compile time into the IR. Values known now fold; values that vary become IR nodes.
//! Calls are inlined, branches on IR values are predicated (both sides run under a mask and assignments merge
//! through selects), loops unroll until their condition is known false.

pub mod call;
pub mod expr;
pub mod ops;
pub mod stmt;
pub mod typing;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::rc::Rc;

use shadewire::interpret::ContextSource;
use shadewire::{ContextRead, Error, Location, Node, NodeId, NodeKind, Op, Space, TransformKind, Type, Value, eval_op};
use shadewire_nodes::Elaborator;

use crate::program::Program;
use crate::types::{StructId, Ty};
use crate::value::{Num, Val};

/// The most iterations a loop may unroll to before the evaluator gives up on it.
pub const MAX_ITERATIONS: usize = 1024;

/// The deepest call nesting: MDL has no recursion, so past this a module is malformed.
pub const MAX_CALL_DEPTH: u32 = 256;

/// The most statements and calls one material may run: loops each under `MAX_ITERATIONS` still multiply, and
/// calls still fan out, past anything a material needs.
pub const MAX_STEPS: u64 = 1_000_000;

/// A local variable.
#[derive(Clone, Debug)]
pub struct Local {
    pub val: Val,
}

/// The names one function call sees: its module's, its parameters and locals, and the array sizes bound.
pub struct Frame {
    pub module: usize,
    /// For origins and warnings: the function or material being evaluated.
    pub function: String,
    pub scopes: Vec<HashMap<String, Local>>,
    /// Array size names (`N` of `T[<N>]`) bound by the call.
    pub sizes: HashMap<String, usize>,
}

impl Frame {
    pub fn new(module: usize, function: &str) -> Frame {
        Frame { module, function: function.to_string(), scopes: vec![HashMap::new()], sizes: HashMap::new() }
    }

    pub fn lookup(&self, name: &str) -> Option<&Local> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    pub fn lookup_mut(&mut self, name: &str) -> Option<&mut Local> {
        self.scopes.iter_mut().rev().find_map(|s| s.get_mut(name))
    }

    pub fn declare(&mut self, name: &str, val: Val) {
        self.scopes.last_mut().expect("a scope").insert(name.to_string(), Local { val });
    }
}

/// Where control is: the mask under which statements run, and what `return`, `break` and `continue` left.
pub struct Flow {
    /// Statements run where this holds: a known `true`, a known `false` (dead code), or a boolean node.
    pub mask: Num,
    /// The value returned so far: where an earlier `return` ran, its value; elsewhere anything.
    pub ret: Option<Val>,
    /// The type the function returns, which `return` converts to (`None` for `auto`).
    pub ret_ty: Option<Ty>,
    pub loops: Vec<LoopFlow>,
}

/// What `break` and `continue` left in the innermost loop.
pub struct LoopFlow {
    pub broke: Num,
    pub continued: Num,
    /// A `switch`: `break` leaves it, `continue` goes to the loop around it.
    pub switch: bool,
}

impl Flow {
    pub fn new() -> Flow {
        Flow { mask: Num::Known(Value::Boolean(true)), ret: None, ret_ty: None, loops: Vec::new() }
    }

    pub fn dead(&self) -> bool {
        self.mask == Num::Known(Value::Boolean(false))
    }

    pub fn live(&self) -> bool {
        self.mask == Num::Known(Value::Boolean(true))
    }
}

impl Default for Flow {
    fn default() -> Flow {
        Flow::new()
    }
}

/// Never called: folding happens only for ops that read no context.
struct NoContext;

impl ContextSource for NoContext {
    fn read(&mut self, _read: &ContextRead) -> Value {
        unreachable!("folding never reads context")
    }
    fn transform(&mut self, _kind: TransformKind, _from: Space, _to: Space, _v: [f32; 3]) -> [f32; 3] {
        unreachable!("folding never transforms")
    }
}

pub struct Eval<'p, 'r, 'a> {
    pub program: &'p Program<'r>,
    pub e: Elaborator<'a>,
    /// The module and material being read, for warnings.
    pub material: String,
    pub warnings: Vec<String>,
    /// Module constants evaluated so far, by `(module, decl, index)`.
    pub consts: HashMap<(usize, usize, usize), Val>,
    /// Struct field types resolved so far.
    pub field_types: HashMap<StructId, Rc<Vec<Ty>>>,
    /// What `state::normal()` reads: the material's `geometry.normal` while its surface is evaluated.
    pub normal: Option<NodeId>,
    pub depth: u32,
    /// Statements run and calls made so far, against `MAX_STEPS`.
    pub steps: u64,
    /// The first module of the program's own that is not a standard or vendored one, for texture paths.
    pub main: usize,
    /// Context reads made so far: one node each.
    pub contexts: HashMap<ContextRead, NodeId>,
    /// The `Error::Limit` emission hit, kept so a caller that turns errors into messages still reports it.
    pub limit: Option<Error>,
}

impl<'p, 'r, 'a> Eval<'p, 'r, 'a> {
    pub fn new(program: &'p Program<'r>, e: Elaborator<'a>, main: usize, material: &str) -> Eval<'p, 'r, 'a> {
        Eval {
            program,
            e,
            material: material.to_string(),
            warnings: Vec::new(),
            consts: HashMap::new(),
            field_types: HashMap::new(),
            normal: None,
            depth: 0,
            steps: 0,
            main,
            contexts: HashMap::new(),
            limit: None,
        }
    }

    /// A location in module `module` at `line`, for errors.
    pub fn at(&self, module: usize, line: u32, path: &str) -> Location {
        self.program.modules[module].at(line, path)
    }

    /// Counts one statement or call at `line` of `module`; `Error::Limit` past `MAX_STEPS`.
    pub fn step(&mut self, module: usize, line: u32, path: &str) -> Result<(), Error> {
        self.steps += 1;
        if self.steps <= MAX_STEPS {
            return Ok(());
        }
        let limit = Error::Limit(format!(
            "{}: the material runs more than {MAX_STEPS} steps (statements and calls)",
            self.at(module, line, path)
        ));
        self.limit.get_or_insert(limit.clone());
        Err(limit)
    }

    pub fn error(&self, f: &Frame, line: u32, message: impl Into<String>) -> Error {
        self.bad(self.at(f.module, line, &f.function), message)
    }

    /// `Error::BadValue` at `at`; the limit instead once one was hit (its message reached here as text).
    pub fn bad(&self, at: Location, message: impl Into<String>) -> Error {
        match &self.limit {
            Some(limit) => limit.clone(),
            None => Error::BadValue { at, message: message.into() },
        }
    }

    /// Records `{material}: {what}`, once.
    pub fn warn(&mut self, what: impl Into<String>) {
        let warning = format!("{}: {}", self.material, what.into());
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }

    /// `function:line`, the origin of the IR nodes made at a line.
    pub fn origin(f: &Frame, line: u32) -> String {
        format!("{}:{line}", f.function)
    }

    /// The node holding `n`: a constant for a known value.
    pub fn node_of(&mut self, n: &Num, origin: &str) -> NodeId {
        match n {
            Num::Node(id) => *id,
            Num::Known(v) => {
                let ty = v.ty();
                self.e.dag.add(Node { kind: NodeKind::Const(v.clone()), ty, origin: origin.to_string() })
            }
        }
    }

    /// The IR type of `n`.
    pub fn num_type(&self, n: &Num) -> Type {
        match n {
            Num::Known(v) => v.ty(),
            Num::Node(id) => self.e.dag.node(*id).ty,
        }
    }

    /// `op` over `args`, of type `ty`: folded when every argument is known (and the op reads no context), else a
    /// node.
    pub fn emit(&mut self, op: Op, args: &[Num], ty: Type, origin: &str) -> Result<Num, Error> {
        if !op.reads_context() && args.iter().all(|a| a.known().is_some()) {
            let values: Vec<Value> = args.iter().map(|a| a.known().expect("known").clone()).collect();
            let value = eval_op(&op, &values, ty, &mut NoContext);
            return Ok(Num::Known(shadewire::ops::broadcast::broadcast(&value, ty)));
        }
        let mut ids: Vec<NodeId> = args.iter().map(|a| self.node_of(a, origin)).collect();
        self.widen_args(op, &mut ids, ty, origin);
        let id = self.e.dag.add(Node { kind: NodeKind::Op { op, args: ids }, ty, origin: origin.to_string() });
        if let Err(e) = self.e.check_node_limit() {
            self.limit = Some(e.clone());
            return Err(e);
        }
        Ok(Num::Node(id))
    }

    /// A context read, one node however often it is read.
    /// Scalar arguments of a componentwise op over vectors, widened to the result type where MaterialX's nodedefs
    /// have no form taking a scalar there (only the second operand of the arithmetic ops, and the factor of a mix,
    /// may stay scalars), so every graph back end finds a nodedef.
    fn widen_args(&mut self, op: Op, ids: &mut [NodeId], ty: Type, origin: &str) {
        if ty.is_scalar() {
            return;
        }
        let widen: &[usize] = match op {
            Op::Add | Op::Subtract | Op::Multiply | Op::Divide | Op::Modulo | Op::Min | Op::Max | Op::Power => &[0],
            Op::Clamp | Op::Smoothstep => &[0],
            Op::Mix => &[0, 1],
            Op::Absval | Op::Sign | Op::Floor | Op::Ceil | Op::Round | Op::Fract | Op::Sqrt | Op::Exp | Op::Ln => &[0],
            Op::Sin | Op::Cos | Op::Tan | Op::Asin | Op::Acos | Op::Atan2 => &[0, 1],
            Op::IfGreater | Op::IfGreaterEq | Op::IfEqual => &[2, 3],
            _ => &[],
        };
        // Trig and atan2 have no scalar forms at all; the others take a scalar second operand.
        let all = matches!(op, Op::Atan2 | Op::Remap);
        for (i, id) in ids.iter_mut().enumerate() {
            if !(widen.contains(&i) || all) {
                continue;
            }
            let node = self.e.dag.node(*id);
            if !node.ty.is_scalar() {
                continue;
            }
            *id = match &node.kind {
                NodeKind::Const(v) => {
                    let v = Value::splat(ty, v.as_f32());
                    self.e.dag.add(Node { kind: NodeKind::Const(v), ty, origin: origin.to_string() })
                }
                _ => {
                    let float = if node.ty == Type::Float {
                        *id
                    } else {
                        let k = NodeKind::Op { op: Op::Convert, args: vec![*id] };
                        self.e.dag.add(Node { kind: k, ty: Type::Float, origin: origin.to_string() })
                    };
                    let k = NodeKind::Op { op: Op::Convert, args: vec![float] };
                    self.e.dag.add(Node { kind: k, ty, origin: origin.to_string() })
                }
            };
        }
    }

    pub fn context(&mut self, read: ContextRead, origin: &str) -> Num {
        if let Some(&id) = self.contexts.get(&read) {
            return Num::Node(id);
        }
        let id = self.e.context(read.clone(), origin);
        self.contexts.insert(read, id);
        Num::Node(id)
    }

    pub fn struct_name(&self, id: StructId) -> String {
        self.program.structs[id].name.clone()
    }

    /// A type's name for messages.
    pub fn type_name(&self, ty: &Ty) -> String {
        let program = self.program;
        crate::types::type_name(ty, &|s| program.structs[s].name.clone(), &|e| program.enums[e].name.clone())
    }
}
