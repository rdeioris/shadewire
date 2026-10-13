//! What a document reads besides itself, for hosts that fetch before compiling.

use shadewire::Error;

use crate::document::PortType;
use crate::document::inherit::Inherit;
use crate::element::Element;
use crate::parse::parse_str;

/// The locations one document names, unresolved and in document order without repeats.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dependencies {
    /// `xi:include` hrefs, relative to the document. An included document has its own: list it again once fetched.
    pub includes: Vec<String>,
    /// `filename` input values with `fileprefix` applied, as reflection reports texture files.
    pub files: Vec<String>,
}

/// The includes and files `text` names, so a host can prefetch them all and then compile with an in-memory
/// resolver. Includes are not followed; `location` only names the document in errors.
pub fn list_dependencies(text: &str, location: Option<&str>) -> Result<Dependencies, Error> {
    let root = parse_str(text, location)?;
    let mut out = Dependencies::default();
    walk(&root, &Inherit::default().enter(&root), &mut out);
    Ok(out)
}

fn walk(element: &Element, inherit: &Inherit, out: &mut Dependencies) {
    for child in &element.children {
        if child.category == "xi:include" {
            if let Some(href) = child.attr("href").filter(|h| !h.is_empty()) {
                push(&mut out.includes, href.to_string());
            }
        } else if child.category == "input" && child.attr("type") == Some("filename") {
            // As `build_input`: an input takes its node's context.
            let (value, _) = inherit.resolve(&PortType::Filename, None, child.attr("value"));
            if let Some(file) = value.filter(|f| *f != inherit.fileprefix) {
                push(&mut out.files, file);
            }
        } else {
            walk(child, &inherit.enter(child), out);
        }
    }
}

fn push(list: &mut Vec<String>, item: String) {
    if !list.contains(&item) {
        list.push(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_includes_and_prefixed_files() {
        let text = r#"<materialx version="1.39" fileprefix="tex/">
  <xi:include href="lib/a.mtlx"/>
  <nodegraph name="g" fileprefix="maps/">
    <image name="i" type="color3"><input name="file" type="filename" value="base.png"/></image>
    <image name="j" type="color3"><input name="file" type="filename" value="base.png"/></image>
  </nodegraph>
  <tiledimage name="t" type="float"><input name="file" type="filename" value="rough.png"/></tiledimage>
  <image name="e" type="float"><input name="file" type="filename" value=""/></image>
  <xi:include href="lib/a.mtlx"/>
  <xi:include href="lib/b.mtlx"/>
</materialx>"#;
        let deps = list_dependencies(text, Some("mem://top.mtlx")).unwrap();
        assert_eq!(deps.includes, ["lib/a.mtlx", "lib/b.mtlx"]);
        assert_eq!(deps.files, ["maps/base.png", "tex/rough.png"]);
    }

    #[test]
    fn a_parse_error_names_the_location() {
        let err = list_dependencies("<materialx>", Some("mem://bad.mtlx")).unwrap_err();
        assert!(err.to_string().contains("mem://bad.mtlx"), "{err}");
    }
}
