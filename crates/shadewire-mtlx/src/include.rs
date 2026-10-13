use std::fmt;
use std::path::Path;
use std::sync::Arc;

use shadewire::{Error, Location};

use crate::element::Element;
use crate::parse::parse_str;

const MAX_DEPTH: u32 = 16;

/// An include the resolver found: where it is (the base of its own includes) and its text.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub location: String,
    pub text: String,
}

/// The signature of a resolver: the including document's location and the `href`, to the included document.
pub type ResolveFn = dyn Fn(&str, &str) -> Result<Resolved, String> + Send + Sync;

/// Joins and reads `xi:include`s, so the front end never interprets locations: they are opaque strings (an OS path,
/// `res://…`, `https://…`). The default reads the file system relative to the including document's directory.
#[derive(Clone)]
pub struct Resolver(pub Arc<ResolveFn>);

impl Resolver {
    pub fn new(f: impl Fn(&str, &str) -> Result<Resolved, String> + Send + Sync + 'static) -> Resolver {
        Resolver(Arc::new(f))
    }

    /// `href` joined to the directory of `base` and read with `std::fs`.
    pub fn file_system() -> Resolver {
        Resolver::new(|base, href| {
            let path = Path::new(base).parent().unwrap_or(Path::new("")).join(href);
            let text = std::fs::read_to_string(&path).map_err(|_| format!("cannot read {}", path.display()))?;
            Ok(Resolved { location: path.display().to_string(), text })
        })
    }

    pub fn resolve(&self, base: &str, href: &str) -> Result<Resolved, String> {
        (self.0)(base, href)
    }
}

impl Default for Resolver {
    fn default() -> Resolver {
        Resolver::file_system()
    }
}

impl fmt::Debug for Resolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Resolver")
    }
}

/// Two resolvers are equal when they are the same function.
impl PartialEq for Resolver {
    fn eq(&self, other: &Resolver) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

fn fail(location: Option<&str>, line: u32, message: String) -> Error {
    Error::Parse { at: Location { file: location.map(str::to_string), line, path: String::new() }, message }
}

/// Replace each `xi:include` child with the children of the included document. `location` names the document and
/// is the base its includes resolve against; without one, an include is an error.
pub fn resolve_includes(root: Element, location: Option<&str>, resolver: &Resolver) -> Result<Element, Error> {
    resolve(root, location, resolver, 0)
}

fn resolve(mut root: Element, location: Option<&str>, resolver: &Resolver, depth: u32) -> Result<Element, Error> {
    let children = std::mem::take(&mut root.children);
    for child in children {
        if child.category != "xi:include" {
            root.children.push(child);
            continue;
        }
        if depth >= MAX_DEPTH {
            return Err(fail(location, child.line, "include depth".into()));
        }
        let Some(base) = location else {
            return Err(fail(None, child.line, "includes need a document location to resolve against".into()));
        };
        let href = child.attr("href").unwrap_or("");
        let resolved = resolver.resolve(base, href).map_err(|message| fail(location, child.line, message))?;
        let included = parse_included(&resolved)?;
        let included = resolve(included, Some(&resolved.location), resolver, depth + 1)?;
        root.children.extend(included.children);
    }
    Ok(root)
}

/// An included document's tree. An include can name any file the resolver reaches, so its errors never quote the
/// file (the XML parser's messages name tags and attributes) and anything but a `<materialx>` root is refused before
/// its elements could reach a message.
fn parse_included(resolved: &Resolved) -> Result<Element, Error> {
    let location = Some(resolved.location.as_str());
    let root = parse_str(&resolved.text, location).map_err(|e| match e {
        Error::Parse { at, .. } => Error::Parse { at, message: "not well-formed XML".into() },
        other => other,
    })?;
    if root.category != "materialx" {
        return Err(fail(location, root.line, "not a MaterialX document".into()));
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::{Resolved, Resolver, resolve_includes};
    use crate::parse::parse_str;
    use shadewire::Error;

    #[test]
    fn splices_included_children_and_reports_missing() {
        let dir = std::env::temp_dir().join(format!("shadewire-include-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("other.mtlx"),
            "<materialx version=\"1.39\"><constant name=\"c\" type=\"float\"/></materialx>",
        )
        .unwrap();
        let top = dir.join("top.mtlx").display().to_string();

        let root =
            parse_str("<materialx><xi:include href=\"other.mtlx\"/><add name=\"a\"/></materialx>", None).unwrap();
        let out = resolve_includes(root, Some(&top), &Resolver::default()).unwrap();
        let cats: Vec<_> = out.children.iter().map(|c| c.category.as_str()).collect();
        assert_eq!(cats, ["constant", "add"]);

        let bad = parse_str("<materialx><xi:include href=\"nope.mtlx\"/></materialx>", None).unwrap();
        let err = resolve_includes(bad, Some(&top), &Resolver::default()).unwrap_err();
        assert!(matches!(err, Error::Parse { .. }));
        assert!(err.to_string().contains("cannot read"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_custom_resolver_sees_each_documents_own_location() {
        let resolver = Resolver::new(|base, href| {
            let location = format!("{}/{href}", base.rsplit_once('/').map_or("", |(dir, _)| dir));
            let text = match href {
                "sub/a.mtlx" => r#"<materialx><xi:include href="b.mtlx"/></materialx>"#,
                "b.mtlx" => r#"<materialx><constant name="c" type="float"/></materialx>"#,
                _ => return Err(format!("no {location}")),
            };
            Ok(Resolved { location, text: text.to_string() })
        });
        let root = parse_str(r#"<materialx><xi:include href="sub/a.mtlx"/></materialx>"#, None).unwrap();
        let out = resolve_includes(root, Some("mem://lib/top.mtlx"), &resolver).unwrap();
        assert_eq!(out.children[0].category, "constant");

        let bad = parse_str(r#"<materialx><xi:include href="x.mtlx"/></materialx>"#, None).unwrap();
        let err = resolve_includes(bad, Some("mem://lib/top.mtlx"), &resolver).unwrap_err();
        assert!(err.to_string().contains("no mem://lib/x.mtlx"), "{err}");
    }

    #[test]
    fn errors_in_an_included_file_do_not_quote_it() {
        let resolver = Resolver::new(|_, href| {
            let text = match href {
                "tags.xml" => "<config><password value=\"hunter2\"/></config>",
                "broken.xml" => {
                    "<config>
<hunter2></password></config>"
                }
                _ => "password=hunter2",
            };
            Ok(Resolved { location: format!("mem://{href}"), text: text.to_string() })
        });
        for href in ["tags.xml", "broken.xml", "secret.txt"] {
            let root = parse_str(&format!(r#"<materialx><xi:include href="{href}"/></materialx>"#), None).unwrap();
            let err = resolve_includes(root, Some("mem://top.mtlx"), &resolver).unwrap_err().to_string();
            assert!(err.starts_with(&format!("mem://{href}:")), "{err}");
            assert!(!err.contains("hunter2") && !err.contains("password") && !err.contains("config"), "{err}");
        }
    }
}
