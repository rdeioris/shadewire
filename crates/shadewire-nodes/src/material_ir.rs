//! What a front end returns: one material as IR, with where it came from and what the front end had to approximate.

use shadewire::{Dag, SourceInfo};

/// One material lowered to the IR, with where it came from (for reflection) and the front end's warnings: what it
/// dropped or approximated, one line each, in lowering order (`dag.approximations` is the elaborator's own list).
#[derive(Clone, Debug)]
pub struct MaterialIr {
    pub dag: Dag,
    pub source: SourceInfo,
    pub warnings: Vec<String>,
}
