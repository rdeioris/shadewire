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
    /// The modules read from files, by canonical path.
    files: HashMap<PathBuf, usize>,
}

/// What `Modules` is given: a module name (`::a::b`) or the path of a `.mdl` file.
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

impl Loaded {
    fn find(&self, source: &Source) -> Option<usize> {
        match source {
            Source::Name(name) => self.program.by_name(name),
            Source::File(path) => self.files.get(path).copied(),
        }
    }
}

impl Modules {
    /// No modules loaded yet; `resolver` is where imports and module names are found.
    pub fn new(resolver: Resolver) -> Result<Modules, Error> {
        let program = Program::new(resolver)?;
        Ok(Modules { loaded: RwLock::new(Loaded { program, files: HashMap::new() }) })
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

    /// The module `module` names, loaded on first use.
    fn load(&self, module: &str) -> Result<usize, Error> {
        let source = Source::of(module);
        if let Some(id) = self.read().find(&source) {
            return Ok(id);
        }
        let mut loaded = self.write();
        if let Some(id) = loaded.find(&source) {
            return Ok(id);
        }
        match source {
            Source::Name(name) => loaded.program.module(&name, &Location::default()),
            Source::File(path) => {
                let text = std::fs::read_to_string(&path).map_err(|e| Error::Io(format!("{module}: {e}")))?;
                let id = loaded.program.add_main(&text, Some(module))?;
                loaded.files.insert(path, id);
                Ok(id)
            }
        }
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
