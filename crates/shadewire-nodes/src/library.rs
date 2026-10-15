//! The vendored MaterialX standard library and Shadewire's own library graphs, embedded in the binary.

use std::sync::OnceLock;

use include_dir::{Dir, include_dir};

use crate::document::Document;
use crate::document::build::build;
use crate::document::flatten::flatten_inheritance;
use crate::parse::parse_str;

static LIBRARIES: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/materialx/libraries");

/// Shadewire's graphs for library nodedefs upstream implements only per language; named `shadewire/...`.
static OWN_LIBRARIES: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/libraries");

pub struct Library {
    pub documents: Vec<Document>,
}

/// Generator directories (genglsl, genosl, ...) hold per-language sources, not definitions.
fn in_gen_dir(path: &std::path::Path) -> bool {
    let Some(dirs) = path.parent() else {
        return false;
    };
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
    let mut own = Vec::new();
    collect(&OWN_LIBRARIES, &mut own);
    own.sort_by_key(|f| f.path().to_path_buf());
    let named =
        files.into_iter().map(|f| (String::new(), f)).chain(own.into_iter().map(|f| ("shadewire/".to_string(), f)));
    let documents = named
        .map(|(prefix, file)| {
            let name = format!("{prefix}{}", file.path().to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"));
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

/// The text of `libraries/shadewire_ir.mtlx`: the nodedefs of the IR ops no MaterialX node expresses, which a
/// written document embeds so other MaterialX tools parse it.
pub fn shadewire_ir_defs_text() -> &'static str {
    OWN_LIBRARIES
        .get_file("shadewire_ir.mtlx")
        .and_then(|file| file.contents_utf8())
        .expect("shadewire_ir.mtlx is embedded as utf-8")
}

#[cfg(test)]
mod tests {
    use super::standard;
    use crate::Document;
    use crate::document::PortType;
    use crate::index::Index;

    #[test]
    fn shadewire_ir_nodedefs_are_in_the_library() {
        let empty = Document::empty();
        let index = Index::new(&empty, standard());
        let coord = index.nodedef("ND_shadewire_hextile_coord_vector2").unwrap();
        assert_eq!(coord.node, "shadewire_hextile_coord");
        assert!(index.is_leaf(coord));
        assert!(super::shadewire_ir_defs_text().contains(r#"name="ND_shadewire_image_grad_color4""#));
    }

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
