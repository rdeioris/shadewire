//! Joins a host prelude to generated WGSL and validates the whole module.

use naga::valid::{Capabilities, ValidationFlags, Validator};
use shadewire_ir::Error;

/// `host_prelude + "\n" + generated`, parsed and validated with naga.
pub fn link(generated: &str, host_prelude: &str) -> Result<String, Error> {
    let linked = format!("{host_prelude}\n{generated}");
    let module = naga::front::wgsl::parse_str(&linked).map_err(|e| Error::Naga(e.emit_to_string(&linked)))?;
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|e| Error::Naga(format!("{e:?}")))?;
    Ok(linked)
}
