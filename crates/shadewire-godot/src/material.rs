//! A compiled material to a `ShaderMaterial`: the shader, uniform defaults by name, textures by
//! `shadewire_t<index>`, and the reflection as metadata.

use std::cell::RefCell;
use std::collections::HashMap;

use godot::classes::{Json, Shader, ShaderMaterial};
use godot::prelude::*;
use shadewire::Value;
use shadewire_gdshader::GdshaderOutput;

use crate::compile::reference;
use crate::io::load_texture;

thread_local! {
    /// Shaders by code: materials with the same graph and different values share one `Shader`, so Godot compiles
    /// its pipelines once.
    static SHADERS: RefCell<HashMap<String, Gd<Shader>>> = RefCell::new(HashMap::new());
}

/// Drops the shared shaders (before the engine goes away).
pub fn clear_shader_cache() {
    SHADERS.with_borrow_mut(HashMap::clear);
}

fn shader(code: &str, shared: bool) -> Gd<Shader> {
    let make = || {
        let mut shader = Shader::new_gd();
        shader.set_code(code);
        shader
    };
    if !shared {
        return make();
    }
    SHADERS.with_borrow_mut(|cache| cache.entry(code.to_string()).or_insert_with(make).clone())
}

/// Colors go up as `Color`, which Godot's color uniforms take without conversion (`color_conversion_disabled`).
fn variant(value: &Value) -> Variant {
    match *value {
        Value::Boolean(b) => b.to_variant(),
        Value::Integer(i) => i.to_variant(),
        Value::Float(f) => f.to_variant(),
        Value::Vector2([x, y]) => Vector2::new(x, y).to_variant(),
        Value::Vector3([x, y, z]) => Vector3::new(x, y, z).to_variant(),
        Value::Vector4([x, y, z, w]) => Vector4::new(x, y, z, w).to_variant(),
        Value::Color3([r, g, b]) => Color::from_rgb(r, g, b).to_variant(),
        Value::Color4([r, g, b, a]) => Color::from_rgba(r, g, b, a).to_variant(),
        // Elaboration rejects matrix inputs; nothing to bind.
        Value::Matrix33(_) | Value::Matrix44(_) => Variant::nil(),
    }
}

/// The material and what it could not honour (the back end's approximations, then unloadable textures).
pub struct Built {
    pub material: Gd<ShaderMaterial>,
    pub warnings: Vec<String>,
}

/// `location` is the document's, the base of relative texture files. `shared_shader` takes the shader from the
/// cache (the runtime); the importer saves its own. A texture that cannot load leaves Godot's default bound and
/// warns, or fails the build when `strict_textures`.
pub fn build(
    out: &GdshaderOutput,
    location: Option<&str>,
    strict_textures: bool,
    shared_shader: bool,
) -> Result<Built, String> {
    let mut material = ShaderMaterial::new_gd();
    material.set_shader(&shader(&out.code, shared_shader));
    for (uniform, name) in out.reflection.uniforms.iter().zip(&out.uniform_names) {
        material.set_shader_parameter(name.as_str(), &variant(&uniform.default));
    }
    let mut warnings = out.reflection.approximations.clone();
    for t in &out.reflection.textures {
        match reference(location, &t.file).and_then(|path| load_texture(&path)) {
            Ok(texture) => material.set_shader_parameter(&format!("shadewire_t{}", t.index), &texture.to_variant()),
            Err(e) if strict_textures => return Err(e),
            Err(e) => warnings.push(format!("texture {}: {e}", t.file)),
        }
    }
    let mut reflection = serde_json::to_value(&out.reflection).map_err(|e| e.to_string())?;
    reflection["approximations"] = serde_json::json!(warnings);
    material.set_meta("shadewire_reflection", &Json::parse_string(&reflection.to_string()));
    Ok(Built { material, warnings })
}
