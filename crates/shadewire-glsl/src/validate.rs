//! Checking generated GLSL with glslangValidator (found by `shadewire::shader::fixtures::tool`): the library,
//! stub definitions of every host function the reflection lists, and a `main` calling everything a host may.

use std::process::Command;

use shadewire::shader::Module;
use shadewire::shader::fixtures::{host_stubs, tool};
use shadewire::shader::print::Printer;
use shadewire::{Reflection, model_struct_name};

use crate::{Glsl, Version};

/// `code` completed with stub host functions and a fragment `main`.
pub fn whole_shader(code: &str, reflection: &Reflection, version: Version) -> String {
    let empty = Module::default();
    let syntax = Glsl { version };
    let printer = Printer::new(&syntax);
    let mut text = code.to_string();
    for stub in host_stubs(reflection) {
        text.push_str(&printer.item(&stub, &empty));
    }
    let model = model_struct_name(&reflection.model);
    let mut calls = format!("    {model} s = shadewire_eval_surface();\n");
    if code.contains("shadewire_eval_bsdf(") {
        calls.push_str(
            "    vec3 n = vec3(0.0, 0.0, 1.0);\n    ShadewireBsdf b = shadewire_eval_bsdf(s, n, n, n);\n    \
             vec3 a = shadewire_eval_albedo(s, n, n);\n    vec3 e = shadewire_eval_emission(s, n, n);\n",
        );
    }
    text.push_str(&format!(
        "\nlayout(location = 0) out vec4 shadewire_test_color;\nvoid main() {{\n{calls}    shadewire_test_color = vec4(0.0);\n}}\n"
    ));
    text
}

/// Compiles `text` as a fragment shader in glslangValidator: `Ok(false)` when it is not installed.
pub fn glslang(name: &str, text: &str) -> Result<bool, String> {
    let Some(validator) = tool("glslangValidator") else {
        return Ok(false);
    };
    let dir = std::env::temp_dir().join("shadewire-glsl-check");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{name}.frag"));
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    let out = Command::new(validator).arg("-S").arg("frag").arg(&path).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(true)
    } else {
        Err(format!("{}\n\n{}", String::from_utf8_lossy(&out.stdout), numbered(text)))
    }
}

/// Checks `code` completed by [`whole_shader`]; `Ok(false)` when glslangValidator is not installed.
pub fn check(name: &str, code: &str, reflection: &Reflection, version: Version) -> Result<bool, String> {
    glslang(name, &whole_shader(code, reflection, version))
}

fn numbered(text: &str) -> String {
    text.lines().enumerate().map(|(i, l)| format!("{:4} {l}", i + 1)).collect::<Vec<_>>().join("\n")
}
