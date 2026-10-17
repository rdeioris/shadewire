//! MDL modules loaded once each, and the materials in them lowered as often as asked, with arguments in place of
//! parameter defaults, from any number of threads at once.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use shadewire::{Error, Location, Value};
use shadewire_nodes::{IrOptions, MaterialIr};

use crate::material::{on_reader_stack, to_ir_here};
use crate::program::{Program, Resolver, canonical};

/// A material parameter's value, given in place of its default.
#[derive(Clone, Debug, PartialEq)]
pub struct Argument {
    pub name: String,
    pub value: ArgValue,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArgValue {
    /// A value, converted to the parameter's type as its default would be.
    Value(Value),
    /// `texture_2d(file, gamma)`: `sRGB` is `::tex::gamma_srgb`, `raw` is `::tex::gamma_linear`, any other
    /// colorspace or none is `::tex::gamma_default`.
    Texture { file: String, colorspace: Option<String> },
    /// An int, or the value of an enum with that integer.
    Int(i64),
}

/// A material argument as a host holds it (a USD `Shader` prim's `inputs:*`), converted to its parameter's declared
/// type by MDL's rules; see [`MdlArgValue`]. Given to [`crate::to_ir_with_args`], where a name that is no parameter is an
/// error.
#[derive(Clone, Debug, PartialEq)]
pub struct MdlArgument {
    pub name: String,
    pub value: MdlArgValue,
}

/// The value of an [`MdlArgument`]. A value that does not convert to its parameter's type is an error naming the
/// parameter and both types.
#[derive(Clone, Debug, PartialEq)]
pub enum MdlArgValue {
    /// One float for a `float` (or `double`), two to four for a `float2` to `float4` (or `double`), three for a
    /// `color`.
    Float(Vec<f32>),
    /// An `int`, an enum's value with that integer, or a `float` (or `double`), MDL's implicit conversion.
    Int(i32),
    /// A `bool`.
    Bool(bool),
    /// A `string`.
    String(String),
    /// A `texture_2d(path, gamma)`: `sRGB` is `::tex::gamma_srgb`, `raw` is `::tex::gamma_linear`, `auto`, any other
    /// colorspace or none is `::tex::gamma_default`.
    Texture { path: String, colorspace: Option<String> },
}

/// The arguments of one lowering: lenient ([`Argument`], an unknown name a warning) or typed ([`MdlArgument`], an
/// unknown name an error).
#[derive(Clone, Copy)]
pub(crate) enum Arguments<'a> {
    Lenient(&'a [Argument]),
    Typed(&'a [MdlArgument]),
}

/// MDL modules, each parsed once with its imports, and their materials lowered to IR. A load that panics leaves the
/// modules unusable: every later call is an error.
pub struct Modules {
    loaded: RwLock<Loaded>,
}

struct Loaded {
    program: Program,
    /// What each module given loaded as: its id, or the error loading it failed with, which is not retried.
    sources: HashMap<Source, Result<usize, Error>>,
}

/// What `Modules` is given: a module name (`::a::b`) or the path of a `.mdl` file.
#[derive(Debug, PartialEq, Eq, Hash)]
enum Source {
    Name(String),
    File(PathBuf),
}

impl Source {
    fn of(module: &str) -> Source {
        if module.starts_with("::") {
            Source::Name(module.to_string())
        } else {
            Source::File(canonical(Path::new(module)))
        }
    }
}

impl Modules {
    /// No modules loaded yet; `resolver` is where imports and module names are found.
    pub fn new(resolver: Resolver) -> Result<Modules, Error> {
        let program = Program::new(resolver)?;
        Ok(Modules { loaded: RwLock::new(Loaded { program, sources: HashMap::new() }) })
    }

    /// The exported materials of `module`: a module name (`::OmniPBR`) or a path to a `.mdl` file, loaded once.
    pub fn material_names(&self, module: &str) -> Result<Vec<String>, Error> {
        on_reader_stack(|| {
            let id = self.load(module)?;
            self.materials(id)
        })
    }

    /// Material `material` of `module` (its only one when `None`) as IR for `options`, `arguments` in place of
    /// parameter defaults. `module` is a module name (`::OmniPBR`) or a path to a `.mdl` file, loaded once. An
    /// argument naming no parameter is a warning; one that does not convert to its parameter's type, an error.
    pub fn to_ir(
        &self,
        module: &str,
        material: Option<&str>,
        arguments: &[Argument],
        options: &IrOptions,
    ) -> Result<MaterialIr, Error> {
        on_reader_stack(|| {
            let id = self.load(module)?;
            self.lower(id, material, Arguments::Lenient(arguments), options)
        })
    }

    /// As [`Modules::to_ir`] with typed arguments, as [`crate::to_ir_with_args`] takes them: an argument naming no
    /// parameter is an error too.
    pub fn to_ir_with_args(
        &self,
        module: &str,
        material: Option<&str>,
        arguments: &[MdlArgument],
        options: &IrOptions,
    ) -> Result<MaterialIr, Error> {
        on_reader_stack(|| {
            let id = self.load(module)?;
            self.lower(id, material, Arguments::Typed(arguments), options)
        })
    }

    /// Adds the module in `text`, read from `location`, as a main module every call of this adds anew.
    pub(crate) fn add_main(&self, text: &str, location: Option<&str>) -> Result<usize, Error> {
        self.write()?.program.add_main(text, location)
    }

    pub(crate) fn materials(&self, id: usize) -> Result<Vec<String>, Error> {
        Ok(self.read()?.program.materials(id))
    }

    pub(crate) fn lower(
        &self,
        id: usize,
        material: Option<&str>,
        arguments: Arguments,
        options: &IrOptions,
    ) -> Result<MaterialIr, Error> {
        to_ir_here(&self.read()?.program, id, material, arguments, options)
    }

    /// The module `module` names, loaded on first use; the error it failed with, on every later use.
    fn load(&self, module: &str) -> Result<usize, Error> {
        let source = Source::of(module);
        if let Some(result) = self.read()?.sources.get(&source) {
            return result.clone();
        }
        let mut loaded = self.write()?;
        if let Some(result) = loaded.sources.get(&source) {
            return result.clone();
        }
        let result = match &source {
            Source::Name(name) => loaded.program.module(name, &Location::default()),
            Source::File(path) => match std::fs::read_to_string(path) {
                Ok(text) => loaded.program.add_main(&text, Some(module)),
                Err(e) => Err(Error::Io(format!("{module}: {e}"))),
            },
        };
        loaded.sources.insert(source, result.clone());
        result
    }

    /// The modules, unless a load panicked while it held them: what it left may be half built.
    fn read(&self) -> Result<RwLockReadGuard<'_, Loaded>, Error> {
        self.loaded.read().map_err(|_| panicked())
    }

    fn write(&self) -> Result<RwLockWriteGuard<'_, Loaded>, Error> {
        self.loaded.write().map_err(|_| panicked())
    }
}

fn panicked() -> Error {
    Error::Io("the MDL modules cannot be used: an earlier load panicked".to_string())
}

impl Drop for Modules {
    /// The modules are dropped on the reader's stack, as they were parsed.
    fn drop(&mut self) {
        let loaded = self.loaded.get_mut().unwrap_or_else(PoisonError::into_inner);
        let modules = Mutex::new(std::mem::take(&mut loaded.program.modules));
        on_reader_stack(|| drop(std::mem::take(&mut *modules.lock().unwrap_or_else(PoisonError::into_inner))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How many modules, structs and enums the program holds.
    fn size(modules: &Modules) -> (usize, usize, usize) {
        let loaded = modules.read().unwrap();
        (loaded.program.modules.len(), loaded.program.structs.len(), loaded.program.enums.len())
    }

    #[test]
    fn a_failed_load_is_kept_and_leaves_nothing_behind() {
        let dir = std::env::temp_dir().join("shadewire-mdl-failed-load");
        std::fs::create_dir_all(&dir).unwrap();
        let text = "mdl 1.7;\nimport ::df::*;\nexport enum e { a };\nexport struct s { float x = 0.0; };\n\
                    import ::missing::*;\nexport material m() = material();\n";
        std::fs::write(dir.join("broken.mdl"), text).unwrap();
        let file = dir.join("broken.mdl").to_string_lossy().to_string();
        let modules = Modules::new(Resolver::new(vec![dir])).unwrap();
        let options = IrOptions::default();
        let before = size(&modules);
        for module in [file.as_str(), "::broken"] {
            let first = modules.to_ir(module, None, &[], &options).unwrap_err();
            assert!(first.to_string().contains("::missing"), "{first}");
            for _ in 0..5 {
                assert_eq!(modules.to_ir(module, None, &[], &options).unwrap_err(), first);
                assert_eq!(modules.material_names(module).unwrap_err(), first);
            }
            assert_eq!(size(&modules), before, "{module}");
            assert_eq!(modules.read().unwrap().sources.get(&Source::of(module)), Some(&Err(first)), "{module}");
        }
    }

    #[test]
    fn a_load_that_panics_leaves_every_later_call_an_error() {
        let dir = std::env::temp_dir().join("shadewire-mdl-panicked-load");
        std::fs::create_dir_all(&dir).unwrap();
        let text = "mdl 1.7;\nexport material m() = material();\n";
        std::fs::write(dir.join("whole.mdl"), text).unwrap();
        let file = dir.join("whole.mdl").to_string_lossy().to_string();
        let modules = Modules::new(Resolver::new(vec![dir])).unwrap();
        let options = IrOptions::default();
        // A load that panics halfway: the module is in the program, but not in the sources.
        let panicked = std::thread::scope(|threads| {
            threads
                .spawn(|| {
                    let mut loaded = modules.loaded.write().unwrap();
                    loaded.program.add_main(text, Some(&file)).unwrap();
                    panic!("a load panics halfway");
                })
                .join()
        });
        assert!(panicked.is_err());
        for module in [file.as_str(), "::whole"] {
            for error in [
                modules.to_ir(module, None, &[], &options).map(|_| ()).unwrap_err(),
                modules.material_names(module).map(|_| ()).unwrap_err(),
            ] {
                assert!(error.to_string().contains("an earlier load panicked"), "{module}: {error}");
            }
        }
    }
}
