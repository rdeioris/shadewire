//! The vendored MaterialX standard library, embedded in the binary.

use std::sync::OnceLock;

use include_dir::{include_dir, Dir};

use crate::document::build::build;
use crate::document::flatten::flatten_inheritance;
use crate::document::Document;
use crate::parse::parse_str;

static LIBRARIES: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../vendor/materialx/libraries");

pub struct Library {
    pub documents: Vec<Document>,
}

/// Generator directories (genglsl, genosl, ...) hold per-language sources, not definitions.
fn in_gen_dir(path: &std::path::Path) -> bool {
    let Some(dirs) = path.parent() else { return false };
    dirs.components().any(|c| c.as_os_str().to_str().is_some_and(|s| s.starts_with("gen")))
}

fn collect<'a>(dir: &'a Dir<'static>, out: &mut Vec<&'a include_dir::File<'static>>) {
    for file in dir.files() {
        let path = file.path();
        if path.extension().and_then(|e| e.to_str()) == Some("mtlx") && !in_gen_dir(path) {
            out.push(file);
        }
    }
    for sub in dir.dirs() {
        collect(sub, out);
    }
}

fn load() -> Library {
    let mut files = Vec::new();
    collect(&LIBRARIES, &mut files);
    files.sort_by_key(|f| f.path().to_path_buf());
    let documents = files
        .into_iter()
        .map(|file| {
            let name = file.path().to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
            let text = file.contents_utf8().unwrap_or_else(|| panic!("{name} is not utf-8"));
            let root = parse_str(text, Some(&name)).unwrap_or_else(|e| panic!("{name}: {e}"));
            build(&root, Some(&name)).unwrap_or_else(|e| panic!("{name}: {e}"))
        })
        .collect();
    let mut library = Library { documents };
    if let Err(message) = flatten_inheritance(&mut library.documents) {
        panic!("standard library: {message}");
    }
    library
}

pub fn standard() -> &'static Library {
    static STANDARD: OnceLock<Library> = OnceLock::new();
    STANDARD.get_or_init(load)
}

#[cfg(test)]
mod tests {
    use super::standard;
    use crate::document::PortType;
    use crate::index::Index;
    use crate::Document;

    #[test]
    fn nodedefs_carry_their_inherited_ports() {
        let empty = Document::empty();
        let index = Index::new(&empty, standard());
        let current = index.nodedef("ND_standard_surface_surfaceshader").unwrap();
        let base = index.nodedef("ND_standard_surface_surfaceshader_100").unwrap();
        assert_eq!(current.inputs.len(), base.inputs.len());
        assert_eq!(current.inputs[0].name, "base");
        // The newer version's own default overrides the inherited one.
        assert_eq!(current.inputs[0].value.as_deref(), Some("1.0"));
        assert_eq!(base.inputs[0].value.as_deref(), Some("0.8"));
        assert_eq!(current.outputs[0].port_type, PortType::SurfaceShader);
    }
}
