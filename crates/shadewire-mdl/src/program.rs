//! The modules a material needs, loaded once each: the user's module, what it imports from the search path, and
//! the vendored ones (the standard modules' declarations, `::base`, `::nvidia::core_definitions` and the prelude
//! holding `material` and its parts). Each module's names: what it declares, exports and imports.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use shadewire::{Error, Location};

use crate::ast::*;
use crate::parse::parse_module;
use crate::types::{EnumId, StructId};

/// Where `import` finds modules: the MDL search path. A module `::a::b` is `a/b.mdl` under the first path holding
/// it; the standard modules are always the vendored ones, and `::base` and `::nvidia::core_definitions` the
/// vendored ones unless the search path has them. A module on the search path that imports its own name (a stub
/// holding `export using ::base import *;`, as Kit's are) imports the vendored one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resolver {
    pub paths: Vec<PathBuf>,
}

impl Resolver {
    pub fn new(paths: Vec<PathBuf>) -> Resolver {
        Resolver { paths }
    }
}

/// The name of the module holding the reserved material types.
pub const PRELUDE: &str = "::<builtins>";

/// The material types and their parts, as MDL's specification declares them (13.1, 13.3).
const PRELUDE_SOURCE: &str = r#"mdl 1.11;
import ::state::normal;
export enum intensity_mode { intensity_radiant_exitance, intensity_power };
export declarative struct material_emission {
    edf emission = edf();
    color intensity = color(0.0);
    uniform intensity_mode mode = intensity_radiant_exitance;
};
export declarative struct material_surface {
    bsdf scattering = bsdf();
    material_emission emission = material_emission();
};
export declarative struct material_volume {
    vdf scattering = vdf();
    color absorption_coefficient = color();
    color scattering_coefficient = color();
    color emission_intensity = color();
};
export declarative struct material_geometry {
    float3 displacement = float3(0.0);
    float cutout_opacity = 1.0;
    float3 normal = state::normal();
};
export declarative struct material {
    uniform bool thin_walled = false;
    material_surface surface = material_surface();
    material_surface backface = material_surface();
    color ior = color(1.0);
    material_volume volume = material_volume();
    material_geometry geometry = material_geometry();
    hair_bsdf hair = hair_bsdf();
};
"#;

/// The standard modules, whose functions are intrinsics: their declarations only.
pub const STANDARD: [&str; 9] =
    ["::anno", "::debug", "::df", "::limits", "::math", "::scene", "::state", "::std", "::tex"];

fn vendored(name: &str) -> Option<&'static str> {
    Some(match name {
        "::anno" => include_str!("../mdl/std/anno.mdl"),
        "::debug" => include_str!("../mdl/std/debug.mdl"),
        "::df" => include_str!("../mdl/std/df.mdl"),
        "::limits" => include_str!("../mdl/std/limits.mdl"),
        "::math" => include_str!("../mdl/std/math.mdl"),
        "::scene" => include_str!("../mdl/std/scene.mdl"),
        "::state" => include_str!("../mdl/std/state.mdl"),
        "::std" => include_str!("../mdl/std/std.mdl"),
        "::tex" => include_str!("../mdl/std/tex.mdl"),
        "::base" => include_str!("../mdl/base.mdl"),
        "::nvidia::core_definitions" => include_str!("../mdl/nvidia/core_definitions.mdl"),
        PRELUDE => PRELUDE_SOURCE,
        _ => return None,
    })
}

/// A declared thing a name can refer to.
#[derive(Clone, Debug, PartialEq)]
pub enum Entity {
    Function { module: usize, decl: usize },
    Struct(StructId),
    Enum(EnumId),
    EnumValue(EnumId, i32),
    Const { module: usize, decl: usize, index: usize },
    Typedef { module: usize, decl: usize },
    Annotation,
}

/// `import a::b::*` (`only: None`) or `import a::b::c`, and the module it names.
#[derive(Clone, Debug)]
pub struct QualifiedImport {
    /// The path as written, without `.` and `..`: what qualified names in the module start with.
    pub written: Vec<String>,
    pub module: usize,
    pub only: Option<HashSet<String>>,
}

#[derive(Debug)]
pub struct ModuleData {
    /// `::a::b`.
    pub name: String,
    pub ast: Module,
    pub file: Option<String>,
    /// The folder of the module's file, for relative imports and texture paths.
    pub dir: Option<PathBuf>,
    /// The packages the module is in, outermost first.
    pub package: Vec<String>,
    /// Whether this is a standard module: its functions are intrinsics.
    pub standard: bool,
    pub own: HashMap<String, Vec<Entity>>,
    pub exported: HashSet<String>,
    /// Names imported unqualified (`using`), with what they name.
    pub unqualified: HashMap<String, Vec<Entity>>,
    pub qualified: Vec<QualifiedImport>,
    /// Package aliases (`using name = "package";`) by name.
    pub aliases: HashMap<String, ImportPath>,
}

impl ModuleData {
    pub fn version(&self) -> (u32, u32) {
        self.ast.version
    }

    pub fn at(&self, line: u32, path: &str) -> Location {
        Location {
            file: self.file.clone().or_else(|| Some(self.name.clone())),
            line,
            path: path.to_string(),
            source: None,
        }
    }
}

#[derive(Debug)]
pub struct StructDef {
    pub name: String,
    pub module: usize,
    pub decl: usize,
    pub declarative: bool,
}

#[derive(Debug)]
pub struct EnumDef {
    pub name: String,
    pub module: usize,
    pub values: Vec<(String, i32)>,
}

pub struct Program {
    resolver: Resolver,
    pub modules: Vec<ModuleData>,
    /// The modules a name means wherever it is imported: the vendored ones and those under a search path root.
    by_name: HashMap<String, usize>,
    /// The modules read from files, by path: next to a main module on no search path, a name means the file in
    /// that module's folder, so two such folders may each have their own module of a name.
    by_file: HashMap<PathBuf, usize>,
    /// The vendored modules a module of the same name on the search path imports, by name.
    shadowed: HashMap<String, usize>,
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
    /// While a main module under no search path root loads, its folder: a root after them, so its relative imports
    /// resolve next to it.
    main_root: Option<PathBuf>,
}

/// `path` as the file system names it (relative paths and `..` resolved), or as given when it does not exist. On
/// Windows a drive path keeps its usual form, not the `\\?\` one, for the file names errors cite.
pub(crate) fn canonical(path: &Path) -> PathBuf {
    let Ok(full) = std::fs::canonicalize(path) else { return path.to_path_buf() };
    match full.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(drive) if !drive.starts_with("UNC") => PathBuf::from(drive),
        _ => full,
    }
}

/// How much a program holds, to return to when a load fails.
#[derive(Clone, Copy)]
struct Mark {
    modules: usize,
    structs: usize,
    enums: usize,
}

/// What an installed module is found by: the file it was read from, and whether its name means it everywhere.
struct Keys {
    file: Option<PathBuf>,
    by_name: bool,
}

/// The file of the module whose path is `parts`, below a search path root.
fn module_file(parts: &[&str]) -> PathBuf {
    let mut file = PathBuf::new();
    for (i, p) in parts.iter().enumerate() {
        if i + 1 == parts.len() { file.push(format!("{p}.mdl")) } else { file.push(p) }
    }
    file
}

impl Program {
    pub fn new(resolver: Resolver) -> Result<Program, Error> {
        let mut program = Program {
            resolver,
            modules: Vec::new(),
            by_name: HashMap::new(),
            by_file: HashMap::new(),
            shadowed: HashMap::new(),
            structs: Vec::new(),
            enums: Vec::new(),
            main_root: None,
        };
        program.module(PRELUDE, &Location::default())?;
        Ok(program)
    }

    pub fn prelude(&self) -> usize {
        self.by_name[PRELUDE]
    }

    /// Adds the user's module from `text`; `location` is its file, whose folder relative imports start from. Nothing
    /// it loads stays when it fails.
    pub fn add_main(&mut self, text: &str, location: Option<&str>) -> Result<usize, Error> {
        let result = self.all_or_nothing(|program| program.add_main_loading(text, location));
        self.main_root = None;
        result
    }

    fn mark(&self) -> Mark {
        Mark { modules: self.modules.len(), structs: self.structs.len(), enums: self.enums.len() }
    }

    /// `load`, or when it fails, the program as it was before.
    fn all_or_nothing(&mut self, load: impl FnOnce(&mut Program) -> Result<usize, Error>) -> Result<usize, Error> {
        let mark = self.mark();
        let result = load(self);
        if result.is_err() {
            self.modules.truncate(mark.modules);
            self.structs.truncate(mark.structs);
            self.enums.truncate(mark.enums);
            self.by_name.retain(|_, id| *id < mark.modules);
            self.by_file.retain(|_, id| *id < mark.modules);
            self.shadowed.retain(|_, id| *id < mark.modules);
        }
        result
    }

    fn add_main_loading(&mut self, text: &str, location: Option<&str>) -> Result<usize, Error> {
        let ast = parse_module(text, location)?;
        let path = location.map(Path::new);
        let stem = path.and_then(|p| p.file_stem()).and_then(|s| s.to_str()).unwrap_or("main");
        let dir = path.and_then(|p| p.parent()).map(Path::to_path_buf);
        let (package, on_search_path) = match dir.as_deref().map(|d| (d, self.package_of(d))) {
            Some((_, Some(package))) => (package, true),
            Some((d, None)) => {
                self.main_root = Some(canonical(d));
                (Vec::new(), false)
            }
            None => (Vec::new(), false),
        };
        let mut name = String::new();
        for p in package.iter().chain(std::iter::once(&stem.to_string())) {
            name.push_str("::");
            name.push_str(p);
        }
        let keys = Keys { file: path.map(canonical), by_name: on_search_path };
        self.install(name, ast, location.map(str::to_string), dir, package, false, keys)
    }

    /// The packages of a folder: its path below the search path root holding it; `None` when no root holds it.
    fn package_of(&self, dir: &Path) -> Option<Vec<String>> {
        let dir = canonical(dir);
        self.roots().find_map(|(root, _)| {
            let rest = dir.strip_prefix(&root).ok()?;
            Some(rest.components().filter_map(|c| c.as_os_str().to_str().map(str::to_string)).collect())
        })
    }

    /// Where modules are looked for: the search path, then the main module's folder when it is on none of it; each
    /// with whether it is on the search path.
    fn roots(&self) -> impl Iterator<Item = (PathBuf, bool)> + '_ {
        let search_path = self.resolver.paths.iter().map(|p| (canonical(p), true));
        search_path.chain(self.main_root.clone().map(|root| (root, false)))
    }

    pub fn by_name(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }

    /// The module `name` (`::a::b`), loaded with its imports on first use. Nothing it loads stays when it fails.
    pub fn module(&mut self, name: &str, at: &Location) -> Result<usize, Error> {
        self.all_or_nothing(|program| program.load(name, at))
    }

    fn load(&mut self, name: &str, at: &Location) -> Result<usize, Error> {
        let parts: Vec<&str> = name.trim_start_matches("::").split("::").collect();
        let standard = STANDARD.contains(&name);
        let found = if standard || name == PRELUDE { None } else { self.find_file(&parts) };
        let loaded = match &found {
            Some((path, _)) => self.by_file.get(path),
            None => self.by_name.get(name),
        };
        if let Some(&id) = loaded {
            return Ok(id);
        }
        let (text, file, dir, keys) = match found {
            Some((path, on_search_path)) => {
                let text = std::fs::read_to_string(&path).map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
                let dir = path.parent().map(Path::to_path_buf);
                (text, Some(path.display().to_string()), dir, Keys { file: Some(path), by_name: on_search_path })
            }
            None => match vendored(name) {
                Some(text) => (text.to_string(), None, None, Keys { file: None, by_name: true }),
                None => return Err(Error::Io(format!("{at}: no MDL module {name} on the search path"))),
            },
        };
        let ast = parse_module(&text, Some(file.as_deref().unwrap_or(name)))?;
        let package = parts[..parts.len() - 1].iter().map(|s| s.to_string()).collect();
        self.install(name.to_string(), ast, file, dir, package, standard, keys)
    }

    /// The vendored module `name`, which a module of that name on the search path stands in for everywhere else.
    fn load_shadowed(&mut self, name: &str) -> Result<usize, Error> {
        if let Some(&id) = self.shadowed.get(name) {
            return Ok(id);
        }
        let text = vendored(name).expect("a vendored module");
        let ast = parse_module(text, Some(name))?;
        let parts: Vec<&str> = name.trim_start_matches("::").split("::").collect();
        let package = parts[..parts.len() - 1].iter().map(|s| s.to_string()).collect();
        self.shadowed.insert(name.to_string(), self.modules.len());
        self.install(name.to_string(), ast, None, None, package, false, Keys { file: None, by_name: false })
    }

    /// The file of the module whose path is `parts` under the first root holding it, and whether that root is on the
    /// search path (not the main module's folder).
    fn find_file(&self, parts: &[&str]) -> Option<(PathBuf, bool)> {
        let relative = module_file(parts);
        self.roots()
            .map(|(root, on_search_path)| (root.join(&relative), on_search_path))
            .find(|(path, _)| path.is_file())
    }

    /// Whether the module `name` is on the search path, next to the main module or vendored.
    fn exists(&self, name: &str) -> bool {
        let parts: Vec<&str> = name.trim_start_matches("::").split("::").collect();
        let on_disk = !STANDARD.contains(&name) && name != PRELUDE && self.find_file(&parts).is_some();
        on_disk || vendored(name).is_some()
    }

    /// Registers a parsed module under `keys`: its imports loaded first, then its declarations named.
    #[allow(clippy::too_many_arguments)]
    fn install(
        &mut self,
        name: String,
        ast: Module,
        file: Option<String>,
        dir: Option<PathBuf>,
        package: Vec<String>,
        standard: bool,
        keys: Keys,
    ) -> Result<usize, Error> {
        let id = self.modules.len();
        if keys.by_name {
            self.by_name.insert(name.clone(), id);
        }
        if let Some(file) = keys.file {
            self.by_file.insert(file, id);
        }
        let imports: Vec<Import> = ast
            .imports
            .iter()
            .cloned()
            .chain(ast.decls.iter().filter_map(|d| match &d.kind {
                DeclKind::Import(i) => Some(i.clone()),
                _ => None,
            }))
            .collect();
        self.modules.push(ModuleData {
            name,
            ast,
            file,
            dir,
            package,
            standard,
            own: HashMap::new(),
            exported: HashSet::new(),
            unqualified: HashMap::new(),
            qualified: Vec::new(),
            aliases: imports
                .iter()
                .filter_map(|i| match i {
                    Import::Alias { name, target, .. } => Some((name.clone(), target.clone())),
                    _ => None,
                })
                .collect(),
        });
        self.declare(id);
        for import in &imports {
            self.import(id, import)?;
        }
        Ok(id)
    }

    /// The module's own declarations, by name.
    fn declare(&mut self, id: usize) {
        let decls = std::mem::take(&mut self.modules[id].ast.decls);
        for (index, decl) in decls.iter().enumerate() {
            let add = |program: &mut Program, name: &str, entity: Entity| {
                let m = &mut program.modules[id];
                m.own.entry(name.to_string()).or_default().push(entity);
                if decl.export {
                    m.exported.insert(name.to_string());
                }
            };
            match &decl.kind {
                DeclKind::Function(f) => add(self, &f.name, Entity::Function { module: id, decl: index }),
                DeclKind::Struct(s) => {
                    let sid = self.structs.len();
                    self.structs.push(StructDef {
                        name: s.name.clone(),
                        module: id,
                        decl: index,
                        declarative: s.declarative,
                    });
                    add(self, &s.name, Entity::Struct(sid));
                }
                DeclKind::Enum(e) => {
                    let eid = self.enums.len();
                    let mut values = Vec::new();
                    let mut next = 0i32;
                    for v in &e.values {
                        if let Some(Expr { kind: ExprKind::Literal(Literal::Int(i)), .. }) = &v.value {
                            next = *i as i32;
                        } else if let Some(value) = &v.value {
                            next = enum_value(value, &values).unwrap_or(next);
                        }
                        values.push((v.name.clone(), next));
                        next = next.wrapping_add(1);
                    }
                    self.enums.push(EnumDef { name: e.name.clone(), module: id, values: values.clone() });
                    add(self, &e.name, Entity::Enum(eid));
                    for (value_name, value) in values {
                        add(self, &value_name, Entity::EnumValue(eid, value));
                    }
                }
                DeclKind::Const { declarators, .. } => {
                    for (i, d) in declarators.iter().enumerate() {
                        add(self, &d.name, Entity::Const { module: id, decl: index, index: i });
                    }
                }
                DeclKind::Typedef { name, .. } => add(self, name, Entity::Typedef { module: id, decl: index }),
                DeclKind::Annotation { name, .. } => add(self, name, Entity::Annotation),
                DeclKind::StructCategory { .. } | DeclKind::Import(_) => {}
            }
        }
        self.modules[id].ast.decls = decls;
    }

    /// The absolute names of the modules an import path may name from module `from`, the first that exists being the
    /// one. A path without prefix is weakly relative: the module beside the importer first, then the search path root.
    fn candidates(&self, from: usize, path: &ImportPath) -> Vec<String> {
        let m = &self.modules[from];
        // A leading alias stands for its package path.
        if path.prefix == Prefix::None
            && let Some(target) = path.parts.first().and_then(|p| m.aliases.get(p))
        {
            let mut expanded = target.clone();
            expanded.parts.extend(path.parts[1..].iter().cloned());
            return self.candidates(from, &expanded);
        }
        let mut parts: Vec<String> = match path.prefix {
            Prefix::Absolute => Vec::new(),
            Prefix::Current => m.package.clone(),
            Prefix::Parent(n) => {
                let keep = m.package.len().saturating_sub(n as usize);
                m.package[..keep].to_vec()
            }
            Prefix::None => Vec::new(),
        };
        // An alias of one package name stands for it anywhere in the path.
        for p in &path.parts {
            match m.aliases.get(p) {
                Some(target) if target.prefix == Prefix::None && target.parts.len() == 1 => {
                    parts.push(target.parts[0].clone())
                }
                _ => parts.push(p.clone()),
            }
        }
        let name = |parts: &[String]| -> String { parts.iter().map(|p| format!("::{p}")).collect() };
        let mut names = vec![name(&parts)];
        if path.prefix == Prefix::None && !m.package.is_empty() {
            names.insert(0, name(&[m.package.as_slice(), parts.as_slice()].concat()));
        }
        names
    }

    fn import(&mut self, id: usize, import: &Import) -> Result<(), Error> {
        let (path, line) = match import {
            Import::Qualified { path, line, .. } | Import::Using { path, line, .. } => (path, *line),
            Import::Alias { .. } => return Ok(()),
        };
        let at = self.modules[id].at(line, "import");
        let names = self.candidates(id, path);
        let name = match names.iter().find(|n| self.exists(n)) {
            Some(name) => name,
            None if names.len() > 1 => {
                return Err(Error::Io(format!("{at}: no MDL module {} on the search path", names.join(" or "))));
            }
            None => &names[0],
        };
        let mut target = self.load(name, &at)?;
        if target == id && vendored(name).is_some() {
            target = self.load_shadowed(name)?;
        }
        let written = path.parts.clone();
        match import {
            Import::Qualified { name, .. } => {
                let only = name.as_ref().map(|n| HashSet::from([n.clone()]));
                self.modules[id].qualified.push(QualifiedImport { written, module: target, only });
            }
            Import::Alias { .. } => unreachable!("aliases are recorded before imports"),
            Import::Using { export, names, .. } => {
                let names: Vec<String> = match names {
                    Some(names) => names.clone(),
                    None => self.export_names(target),
                };
                // An enum brings its values with it.
                let mut names = names;
                for n in names.clone() {
                    for e in self.exports(target, &n) {
                        if let Entity::Enum(id) = e {
                            for (value, _) in &self.enums[id].values {
                                if !names.contains(value) {
                                    names.push(value.clone());
                                }
                            }
                        }
                    }
                }
                for n in &names {
                    let entities = self.exports(target, n);
                    let m = &mut self.modules[id];
                    let slot = m.unqualified.entry(n.clone()).or_default();
                    for e in entities {
                        if !slot.contains(&e) {
                            slot.push(e);
                        }
                    }
                    if *export {
                        m.exported.insert(n.clone());
                    }
                }
                let only = Some(names.into_iter().collect());
                self.modules[id].qualified.push(QualifiedImport { written, module: target, only });
            }
        }
        Ok(())
    }

    /// Every name module `id` exports.
    pub fn export_names(&self, id: usize) -> Vec<String> {
        let mut names: Vec<String> = self.modules[id].exported.iter().cloned().collect();
        names.sort();
        names
    }

    /// What module `id` exports under `name`: its own declarations and the unqualified imports it re-exports.
    pub fn exports(&self, id: usize, name: &str) -> Vec<Entity> {
        let m = &self.modules[id];
        if !m.exported.contains(name) {
            return Vec::new();
        }
        let mut out: Vec<Entity> = m.own.get(name).cloned().unwrap_or_default();
        for e in m.unqualified.get(name).into_iter().flatten() {
            if !out.contains(e) {
                out.push(e.clone());
            }
        }
        out
    }

    /// What `name` refers to in module `from`: its own declarations, its unqualified imports, then the prelude;
    /// a qualified name through the module it names.
    pub fn lookup(&self, from: usize, name: &QualifiedName) -> Vec<Entity> {
        if name.is_simple() {
            let n = name.last();
            let m = &self.modules[from];
            let mut out: Vec<Entity> = m.own.get(n).cloned().unwrap_or_default();
            for e in m.unqualified.get(n).into_iter().flatten() {
                if !out.contains(e) {
                    out.push(e.clone());
                }
            }
            if out.is_empty() {
                out = self.exports(self.prelude(), n);
            }
            return out;
        }
        let qualifier = &name.parts[..name.parts.len() - 1];
        let last = name.last();
        match self.qualified_module(from, qualifier, name.absolute, last) {
            Some(target) => self.exports(target, last),
            None => Vec::new(),
        }
    }

    /// The module a qualifier names from module `from`: an import as written, or a loaded module's absolute name.
    fn qualified_module(&self, from: usize, qualifier: &[String], absolute: bool, last: &str) -> Option<usize> {
        let m = &self.modules[from];
        if !absolute {
            let found = m
                .qualified
                .iter()
                .find(|q| q.written.as_slice() == qualifier && q.only.as_ref().is_none_or(|only| only.contains(last)));
            if let Some(q) = found {
                return Some(q.module);
            }
        }
        let full: String = qualifier.iter().map(|p| format!("::{p}")).collect();
        if let Some(found) = m.qualified.iter().find(|q| self.modules[q.module].name == full) {
            return Some(found.module);
        }
        // A name of a module the program has loaded, imported here or not.
        self.by_name.get(&full).copied()
    }

    pub fn function(&self, module: usize, decl: usize) -> &Function {
        match &self.modules[module].ast.decls[decl].kind {
            DeclKind::Function(f) => f,
            other => panic!("not a function: {other:?}"),
        }
    }

    pub fn struct_decl(&self, id: StructId) -> &Struct {
        let def = &self.structs[id];
        match &self.modules[def.module].ast.decls[def.decl].kind {
            DeclKind::Struct(s) => s,
            other => panic!("not a struct: {other:?}"),
        }
    }

    /// The exported material definitions of module `id`, in declaration order.
    pub fn materials(&self, id: usize) -> Vec<String> {
        let m = &self.modules[id];
        let mut out = Vec::new();
        for d in &m.ast.decls {
            if let DeclKind::Function(f) = &d.kind
                && d.export
                && f.ret.name.as_ref().is_some_and(|n| n.last() == "material" && n.parts.len() == 1)
                && !out.contains(&f.name)
            {
                out.push(f.name.clone());
            }
        }
        out
    }
}

/// An enum value written as a reference to an earlier value of the same enum (`b = a`), or `a + 1`.
fn enum_value(e: &Expr, values: &[(String, i32)]) -> Option<i32> {
    match &e.kind {
        ExprKind::Literal(Literal::Int(i)) => Some(*i as i32),
        ExprKind::Name(n) if n.is_simple() => values.iter().find(|(name, _)| name == n.last()).map(|(_, v)| *v),
        ExprKind::Unary { op: UnaryOp::Neg, arg } => enum_value(arg, values).map(|v| -v),
        ExprKind::Binary { op, lhs, rhs } => {
            let (a, b) = (enum_value(lhs, values)?, enum_value(rhs, values)?);
            match op {
                BinOp::Add => Some(a.wrapping_add(b)),
                BinOp::Sub => Some(a.wrapping_sub(b)),
                BinOp::Mul => Some(a.wrapping_mul(b)),
                BinOp::Shl => Some(a.wrapping_shl(b as u32)),
                BinOp::BitOr => Some(a | b),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prelude_declares_the_material_types() {
        let resolver = Resolver::default();
        let program = Program::new(resolver).unwrap();
        let prelude = program.prelude();
        for name in ["material", "material_surface", "material_emission", "material_volume", "material_geometry"] {
            assert!(matches!(program.exports(prelude, name).as_slice(), [Entity::Struct(_)]), "{name}");
        }
        assert!(matches!(program.exports(prelude, "intensity_power").as_slice(), [Entity::EnumValue(_, 1)]));
    }

    #[test]
    fn qualified_and_unqualified_imports() {
        let resolver = Resolver::default();
        let mut program = Program::new(resolver).unwrap();
        let main = program
            .add_main(
                "mdl 1.7;\nimport ::df::*;\nusing ::anno import hard_range;\nexport material m() = material();\n",
                None,
            )
            .unwrap();
        let q = |s: &str| QualifiedName {
            absolute: s.starts_with("::"),
            parts: s.trim_start_matches("::").split("::").map(str::to_string).collect(),
        };
        assert!(!program.lookup(main, &q("df::diffuse_reflection_bsdf")).is_empty());
        assert!(!program.lookup(main, &q("::df::scatter_reflect")).is_empty());
        assert!(program.lookup(main, &q("diffuse_reflection_bsdf")).is_empty());
        assert!(!program.lookup(main, &q("hard_range")).is_empty());
        assert!(!program.lookup(main, &q("anno::hard_range")).is_empty());
        // Lenient where MDL is strict: a loaded module answers qualified names even where not imported.
        assert!(!program.lookup(main, &q("anno::soft_range")).is_empty());
        assert!(matches!(program.lookup(main, &q("material")).as_slice(), [Entity::Struct(_)]));
        assert_eq!(program.materials(main), ["m"]);
    }

    #[test]
    fn imports_through_a_package_alias() {
        let root = std::env::temp_dir().join("shadewire-mdl-alias");
        std::fs::create_dir_all(root.join("odd pkg")).unwrap();
        std::fs::write(
            root.join("odd pkg/m.mdl"),
            "mdl 1.6;
export color c() = color(0.5);
",
        )
        .unwrap();
        let resolver = Resolver::new(vec![root]);
        let mut program = Program::new(resolver).unwrap();
        let main = program
            .add_main(
                "mdl 1.6;
using odd = \"odd pkg\";
import odd::m::*;
",
                None,
            )
            .unwrap();
        assert!(program.by_name("::odd pkg::m").is_some());
        let name = QualifiedName { absolute: false, parts: vec!["odd".into(), "m".into(), "c".into()] };
        assert!(matches!(program.lookup(main, &name).as_slice(), [Entity::Function { .. }]));
    }

    #[test]
    fn an_alias_inside_an_absolute_path() {
        let root = std::env::temp_dir().join("shadewire-mdl-alias-inner");
        std::fs::create_dir_all(root.join("p/odd pkg")).unwrap();
        std::fs::write(
            root.join("p/odd pkg/m.mdl"),
            "mdl 1.6;
export color c() = color(0.5);
",
        )
        .unwrap();
        let resolver = Resolver::new(vec![root]);
        let mut program = Program::new(resolver).unwrap();
        program
            .add_main(
                "mdl 1.6;
using odd = \"odd pkg\";
import ::p::odd::m::*;
",
                None,
            )
            .unwrap();
        assert!(program.by_name("::p::odd pkg::m").is_some());
    }

    #[test]
    fn base_and_core_definitions_load_with_their_imports() {
        let resolver = Resolver::default();
        let mut program = Program::new(resolver).unwrap();
        let at = Location::default();
        let core = program.module("::nvidia::core_definitions", &at).unwrap();
        assert!(program.by_name("::base").is_some());
        assert!(!program.export_names(core).is_empty());
    }
}
