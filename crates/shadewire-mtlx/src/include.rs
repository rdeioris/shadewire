use std::path::Path;

use shadewire_ir::{Error, Location};

use crate::element::Element;
use crate::parse::parse_str;

const MAX_DEPTH: u32 = 16;

fn fail(file: Option<&str>, line: u32, message: String) -> Error {
    Error::Parse { at: Location { file: file.map(str::to_string), line, path: String::new() }, message }
}

/// Replace each `xi:include` child with the children of the included document.
pub fn resolve_includes(root: Element, base_dir: Option<&Path>, file: Option<&str>) -> Result<Element, Error> {
    resolve(root, base_dir, file, 0)
}

fn resolve(mut root: Element, base_dir: Option<&Path>, file: Option<&str>, depth: u32) -> Result<Element, Error> {
    let children = std::mem::take(&mut root.children);
    for child in children {
        if child.category != "xi:include" {
            root.children.push(child);
            continue;
        }
        if depth >= MAX_DEPTH {
            return Err(fail(file, child.line, "include depth".into()));
        }
        let href = child.attr("href").unwrap_or("");
        let path = match base_dir {
            Some(dir) => dir.join(href),
            None => Path::new(href).to_path_buf(),
        };
        let text = std::fs::read_to_string(&path)
            .map_err(|_| fail(file, child.line, format!("cannot read {}", path.display())))?;
        let name = path.display().to_string();
        let included = parse_str(&text, Some(&name))?;
        let included = resolve(included, path.parent(), Some(&name), depth + 1)?;
        root.children.extend(included.children);
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::resolve_includes;
    use crate::parse::parse_str;
    use shadewire_ir::Error;

    #[test]
    fn splices_included_children_and_reports_missing() {
        let dir = std::env::temp_dir().join(format!("shadewire-include-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("other.mtlx"),
            "<materialx version=\"1.39\"><constant name=\"c\" type=\"float\"/></materialx>",
        )
        .unwrap();

        let root =
            parse_str("<materialx><xi:include href=\"other.mtlx\"/><add name=\"a\"/></materialx>", None).unwrap();
        let out = resolve_includes(root, Some(&dir), None).unwrap();
        let cats: Vec<_> = out.children.iter().map(|c| c.category.as_str()).collect();
        assert_eq!(cats, ["constant", "add"]);

        let bad = parse_str("<materialx><xi:include href=\"nope.mtlx\"/></materialx>", None).unwrap();
        let err = resolve_includes(bad, Some(&dir), Some("top.mtlx")).unwrap_err();
        assert!(matches!(err, Error::Parse { .. }));
        assert!(err.to_string().contains("cannot read"));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
