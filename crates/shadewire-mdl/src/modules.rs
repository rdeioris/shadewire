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

/// MDL modules, each parsed once with its imports, and their materials lowered to IR.
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
            Ok(self.materials(id))
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
            self.lower(id, material, arguments, options)
        })
    }

    /// Adds the module in `text`, read from `location`, as a main module every call of this adds anew.
    pub(crate) fn add_main(&self, text: &str, location: Option<&str>) -> Result<usize, Error> {
        self.write().program.add_main(text, location)
    }

    pub(crate) fn materials(&self, id: usize) -> Vec<String> {
        self.read().program.materials(id)
    }

    pub(crate) fn lower(
        &self,
        id: usize,
        material: Option<&str>,
        arguments: &[Argument],
        options: &IrOptions,
    ) -> Result<MaterialIr, Error> {
        to_ir_here(&self.read().program, id, material, arguments, options)
    }

    /// The module `module` names, loaded on first use; the error it failed with, on every later use.
    fn load(&self, module: &str) -> Result<usize, Error> {
        let source = Source::of(module);
        if let Some(result) = self.read().sources.get(&source) {
            return result.clone();
        }
        let mut loaded = self.write();
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

    fn read(&self) -> RwLockReadGuard<'_, Loaded> {
        self.loaded.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> RwLockWriteGuard<'_, Loaded> {
        self.loaded.write().unwrap_or_else(PoisonError::into_inner)
    }
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
        let loaded = modules.read();
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
            assert_eq!(modules.read().sources.get(&Source::of(module)), Some(&Err(first)), "{module}");
        }
    }
}
