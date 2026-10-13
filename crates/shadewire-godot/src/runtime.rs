//! The runtime API: the `Shadewire` class, and the load both it and the resource loader run.

use godot::classes::{DirAccess, ShaderMaterial};
use godot::prelude::*;

use crate::compile::compile;
use crate::io::{read_text, resolver};
use crate::material::{Built, build};
use crate::options::{LoadOptions, Models, models, transparency};

/// Options from a GDScript dictionary: `material`, `models` (`"mapped"`, `"lobes"` or an array of model names),
/// `bake_constants`, `optimize`, `transparency`, `emission_scale`, `strict_textures`. Unknown keys are errors.
pub fn options_from(dictionary: &VarDictionary) -> Result<LoadOptions, String> {
    let mut options = LoadOptions::default();
    for (key, value) in dictionary.iter_shared() {
        let key = key.to_string();
        let bad = |what: &str| format!("option {key}: expected {what}, got {value}");
        match key.as_str() {
            "material" => {
                let name = value.try_to::<GString>().map_err(|_| bad("a string"))?.to_string();
                options.material = (!name.is_empty()).then_some(name);
            }
            "models" => {
                options.models = match value.try_to::<GString>() {
                    Ok(name) => models(&name.to_string())?,
                    Err(_) => Models::List(
                        value
                            .try_to::<PackedStringArray>()
                            .map_err(|_| bad("a string or an array of strings"))?
                            .as_slice()
                            .iter()
                            .map(GString::to_string)
                            .collect(),
                    ),
                }
            }
            "bake_constants" => options.bake_constants = value.try_to().map_err(|_| bad("a bool"))?,
            "optimize" => options.optimize = value.try_to().map_err(|_| bad("a bool"))?,
            "strict_textures" => options.strict_textures = value.try_to().map_err(|_| bad("a bool"))?,
            "transparency" => {
                let name = value.try_to::<GString>().map_err(|_| bad("a string"))?;
                options.transparency = transparency(&name.to_string())?;
            }
            "emission_scale" => {
                options.emission_scale = value.try_to::<f64>().map_err(|_| bad("a number"))? as f32;
            }
            _ => return Err(format!("unknown option {key}")),
        }
    }
    Ok(options)
}

/// Compile `text`, named by `location`, into a material. Texture failures that are not fatal are pushed as
/// warnings.
pub fn load_text(text: &str, location: Option<&str>, options: &LoadOptions) -> Result<Gd<ShaderMaterial>, String> {
    let out = compile(text, location, options, resolver())?;
    let Built { material, warnings } = build(&out, location, options.strict_textures, true)?;
    let name = location.unwrap_or("<string>");
    for warning in warnings {
        godot_warn!("shadewire: {name}: {warning}");
    }
    Ok(material)
}

pub fn load_path(path: &str, options: &LoadOptions) -> Result<Gd<ShaderMaterial>, String> {
    let text = read_text(path)?;
    load_text(&text, Some(path), options)
}

/// The options argument: a dictionary that can be a `#[opt]` default, which gdext allows only for types that
/// callers cannot mutate (the default is made read-only).
#[derive(Clone, Debug, Default)]
pub struct Options(VarDictionary);

impl GodotConvert for Options {
    type Via = VarDictionary;

    fn godot_shape() -> godot::meta::shape::GodotShape {
        VarDictionary::godot_shape()
    }
}

impl ToGodot for Options {
    type Pass = godot::meta::conv::ByValue;

    fn to_godot(&self) -> VarDictionary {
        self.0.clone()
    }
}

impl FromGodot for Options {
    fn try_from_godot(via: VarDictionary) -> Result<Options, godot::meta::error::ConvertError> {
        Ok(Options(via))
    }
}

// SAFETY: the default is a fresh, read-only dictionary per call: no caller can mutate or share it.
unsafe impl godot::meta::GodotImmutable for Options {
    fn into_runtime_immutable(self) -> Options {
        Options(self.0.into_read_only())
    }
}

/// Loads MaterialX documents as `ShaderMaterial`s at runtime, from `res://`, `user://` or any OS path.
///
/// ```gdscript
/// var sw := Shadewire.new()
/// var mat := sw.load_material("D:/assets/rusty_metal.mtlx", {"material": "Rust"})
/// if mat == null:
///     push_error(sw.get_last_error())
/// ```
#[derive(GodotClass)]
#[class(init, base=RefCounted)]
pub struct Shadewire {
    last_error: GString,
}

impl Shadewire {
    fn finish<T>(&mut self, result: Result<T, String>) -> Option<T> {
        match result {
            Ok(value) => {
                self.last_error = GString::new();
                Some(value)
            }
            Err(e) => {
                self.last_error = e.as_str().into();
                None
            }
        }
    }
}

#[godot_api]
impl Shadewire {
    /// The document at `path` as a material, or null (see `get_last_error`). Includes and textures resolve
    /// relative to `path`.
    #[func]
    fn load_material(
        &mut self,
        path: GString,
        #[opt(default = Options::default())] options: Options,
    ) -> Option<Gd<ShaderMaterial>> {
        let result = options_from(&options.0).and_then(|options| load_path(&path.to_string(), &options));
        self.finish(result)
    }

    /// A document held in a string. `base` is where relative includes and textures resolve: a directory or a
    /// document location; empty makes them errors.
    #[func]
    fn compile_string(
        &mut self,
        text: GString,
        #[opt(default = "")] base: GString,
        #[opt(default = Options::default())] options: Options,
    ) -> Option<Gd<ShaderMaterial>> {
        let mut base = base.to_string();
        if !base.is_empty() && !base.ends_with(['/', '\\']) && DirAccess::dir_exists_absolute(&base) {
            base.push('/');
        }
        let location = (!base.is_empty()).then_some(base.as_str());
        let result = options_from(&options.0).and_then(|options| load_text(&text.to_string(), location, &options));
        self.finish(result)
    }

    /// The names of the document's materials, in document order; empty on error (see `get_last_error`).
    #[func]
    fn material_names(&mut self, path: GString) -> PackedStringArray {
        let path = path.to_string();
        let result = read_text(&path).and_then(|text| {
            shadewire_mtlx::material_names(&text, Some(&path), &resolver()).map_err(|e| e.to_string())
        });
        let names = self.finish(result).unwrap_or_default();
        names.iter().map(|n| GString::from(n.as_str())).collect()
    }

    /// The message of the last failed call; empty after a success.
    #[func]
    fn get_last_error(&self) -> GString {
        self.last_error.clone()
    }
}
