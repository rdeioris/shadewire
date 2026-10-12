//! Joins a host prelude to generated WGSL and validates the whole module.

use naga::Module;
use naga::valid::{Capabilities, ModuleInfo, ValidationFlags, Validator};
use shadewire::Error;

/// Parses and validates WGSL; errors carry naga's line and caret rendering of `source`.
pub(crate) fn parse_and_validate(source: &str) -> Result<(Module, ModuleInfo), Error> {
    let module = naga::front::wgsl::parse_str(source).map_err(|e| Error::Naga(e.emit_to_string(source)))?;
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|e| Error::Naga(e.emit_to_string(source)))?;
    Ok((module, info))
}

/// `host_prelude + "\n" + generated`, parsed and validated with naga.
pub fn link(generated: &str, host_prelude: &str) -> Result<String, Error> {
    let linked = format!("{host_prelude}\n{generated}");
    parse_and_validate(&linked)?;
    Ok(linked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrong_host_signature_names_the_line() {
        let prelude = "fn mx_texcoord_0() -> f32 { return 0.0; }\n";
        let generated = "fn f() -> vec2<f32> {\n    let a: vec2<f32> = mx_texcoord_0();\n    return a;\n}\n";
        let Err(Error::Naga(text)) = link(generated, prelude) else { panic!("expected a naga error") };
        assert!(text.contains("let a: vec2<f32> = mx_texcoord_0();"), "{text}");
        assert!(text.contains("wgsl:4:9"), "{text}");
    }
}
