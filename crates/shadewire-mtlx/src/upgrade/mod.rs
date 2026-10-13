//! Document upgrades: bring older MaterialX documents to 1.39.
//!
//! Ported from `Document::upgradeVersion` in upstream `MaterialXCore/Version.cpp`
//! (the 1.38 to 1.39 section), restricted to the nodes Shadewire handles.

pub mod atan2;
pub mod channels;
pub mod normalmap;
pub mod scope;
pub mod subsurface;
pub mod switch;
pub mod swizzle;

#[cfg(test)]
pub mod test_doc;

use shadewire::{Error, Location};

use crate::document::Document;

/// Upgrade `doc` to 1.39. 1.39 passes through; 1.38 gets the rules; anything else is refused.
pub fn upgrade(mut doc: Document) -> Result<Document, Error> {
    match doc.version {
        (1, 39) => Ok(doc),
        (1, 38) => {
            channels::apply(&mut doc);
            swizzle::apply(&mut doc);
            atan2::apply(&mut doc);
            switch::apply(&mut doc);
            normalmap::apply(&mut doc)?;
            subsurface::apply(&mut doc);
            doc.version = (1, 39);
            Ok(doc)
        }
        (major, minor) => Err(Error::UnsupportedVersion {
            at: Location { file: doc.file.clone(), line: 0, path: String::new() },
            version: format!("{major}.{minor}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::test_doc::doc_with_version;
    use super::upgrade;
    use shadewire::Error;

    #[test]
    fn current_version_passes_through() {
        let doc = doc_with_version("1.39", "");
        assert_eq!(upgrade(doc.clone()).unwrap(), doc);
    }

    #[test]
    fn older_than_1_38_is_refused() {
        let doc = doc_with_version("1.37", "");
        assert!(matches!(upgrade(doc), Err(Error::UnsupportedVersion { .. })));
    }

    #[test]
    fn newer_than_1_39_is_refused() {
        let doc = doc_with_version("1.40", "");
        assert!(matches!(upgrade(doc), Err(Error::UnsupportedVersion { .. })));
    }

    #[test]
    fn upgrading_1_38_sets_the_version_and_applies_every_rule() {
        let body = r#"<atan2 name="a" type="float"><input name="in1" type="float" value="1"/><input name="in2" type="float" value="2"/></atan2>
                      <swizzle name="s" type="float"><input name="in" type="color3" nodename="c"/><input name="channels" type="string" value="g"/></swizzle>"#;
        let out = upgrade(doc_with_version("1.38", body)).unwrap();
        assert_eq!(out.version, (1, 39));
        let atan = out.nodes.iter().find(|n| n.name == "a").unwrap();
        assert_eq!(atan.inputs[0].name, "iny");
        let s = out.nodes.iter().find(|n| n.name == "s").unwrap();
        assert_eq!(s.category, "extract");
    }
}
