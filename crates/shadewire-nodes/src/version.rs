use shadewire::{Error, Location};

use crate::element::Element;

/// Read the `version` attribute of the root; absent means 1.39.
pub fn document_version(root: &Element) -> Result<(u32, u32), Error> {
    let Some(text) = root.attr("version") else {
        return Ok((1, 39));
    };
    let bad = || Error::Parse {
        at: Location { file: None, line: root.line, path: String::new(), source: None },
        message: format!("bad version \"{text}\""),
    };
    let (major, minor) = text.split_once('.').ok_or_else(bad)?;
    let major = major.trim().parse().map_err(|_| bad())?;
    let minor = minor.trim().parse().map_err(|_| bad())?;
    Ok((major, minor))
}

#[cfg(test)]
mod tests {
    use super::document_version;
    use crate::element::Element;
    use shadewire::Error;

    fn root(version: Option<&str>) -> Element {
        Element {
            category: "materialx".into(),
            attributes: version.map(|v| ("version".to_string(), v.to_string())).into_iter().collect(),
            ..Element::default()
        }
    }

    #[test]
    fn reads_major_minor() {
        assert_eq!(document_version(&root(Some("1.39"))).unwrap(), (1, 39));
        assert_eq!(document_version(&root(Some("1.38"))).unwrap(), (1, 38));
    }

    #[test]
    fn missing_means_current() {
        assert_eq!(document_version(&root(None)).unwrap(), (1, 39));
    }

    #[test]
    fn garbage_is_a_parse_error() {
        assert!(matches!(document_version(&root(Some("abc"))), Err(Error::Parse { .. })));
    }
}
