//! What a front end returns: one material as IR, with where it came from and what the front end had to approximate.

use shadewire::{Dag, SourceInfo};

use crate::surface::distill::NOTE_PREFIX;

/// One material lowered to the IR, with where it came from (for reflection) and the front end's warnings: what it
/// dropped or approximated, one line each, in lowering order (`dag.approximations` is the elaborator's own list).
#[derive(Clone, Debug)]
pub struct MaterialIr {
    pub dag: Dag,
    pub source: SourceInfo,
    pub warnings: Vec<String>,
}

impl MaterialIr {
    /// The IR with the front end's `warnings`, then the distiller's notes from `dag.approximations` (see
    /// `surface::distill`): a host reading only the warnings still learns what distilling dropped.
    pub fn new(dag: Dag, source: SourceInfo, mut warnings: Vec<String>) -> MaterialIr {
        for note in dag.approximations.iter().filter(|a| a.starts_with(NOTE_PREFIX)) {
            if !warnings.contains(note) {
                warnings.push(note.clone());
            }
        }
        MaterialIr { dag, source, warnings }
    }
}
