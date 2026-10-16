//! Helpers for the writer's tests: `mdlc` (the MDL SDK's compiler) as a second reader of what the writer writes.
#![allow(dead_code)]

use std::process::Command;

/// Checks `text` as MDL module `module` with `mdlc` (`SHADEWIRE_MDLC`, or on PATH): `Ok(false)` when the tool is
/// absent, `Err` with its output and the numbered text when it rejects the module.
pub fn mdlc(text: &str, module: &str) -> Result<bool, String> {
    let Some(exe) = shadewire::shader::fixtures::tool("mdlc") else {
        return Ok(false);
    };
    let dir = std::env::temp_dir().join("shadewire-mdlc").join(module);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{module}.mdl")), text).map_err(|e| e.to_string())?;
    let out = Command::new(exe)
        .arg("-p")
        .arg(&dir)
        .arg("--no-resolve-resources")
        .arg(format!("::{module}"))
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(true);
    }
    let numbered: Vec<String> = text.lines().enumerate().map(|(i, l)| format!("{:4} {l}", i + 1)).collect();
    Err(format!(
        "mdlc rejected {module}:\n{}{}\n\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
        numbered.join("\n")
    ))
}

/// `mdlc` must accept `text` when it is installed.
pub fn assert_mdlc(text: &str, module: &str) {
    match mdlc(text, module) {
        Ok(true) => {}
        Ok(false) => eprintln!("mdlc not found (SHADEWIRE_MDLC): {module} not checked"),
        Err(e) => panic!("{e}"),
    }
}
