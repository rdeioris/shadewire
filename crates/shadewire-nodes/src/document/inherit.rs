use super::port_type::PortType;
use crate::element::Element;

/// Attributes a child inherits from its ancestors.
#[derive(Clone, Debug, Default)]
pub struct Inherit {
    pub fileprefix: String,
    pub colorspace: Option<String>,
}

impl Inherit {
    /// The context seen inside `element`: its own attributes win.
    pub fn enter(&self, element: &Element) -> Inherit {
        Inherit {
            fileprefix: element.attr("fileprefix").map(str::to_string).unwrap_or_else(|| self.fileprefix.clone()),
            colorspace: element.attr("colorspace").map(str::to_string).or_else(|| self.colorspace.clone()),
        }
    }

    /// The value and colorspace of a port: filename values get the file
    /// prefix, color and filename ports get the nearest colorspace.
    pub fn resolve(
        &self,
        port_type: &PortType,
        own_colorspace: Option<&str>,
        value: Option<&str>,
    ) -> (Option<String>, Option<String>) {
        let colorspace =
            if port_type.has_colorspace() { own(own_colorspace).or_else(|| self.colorspace.clone()) } else { None };
        let value = match (port_type, value) {
            (PortType::Filename, Some(v)) => Some(format!("{}{v}", self.fileprefix)),
            (_, v) => own(v),
        };
        (value, colorspace)
    }
}

/// An optional borrowed string, owned.
pub fn own(value: Option<&str>) -> Option<String> {
    value.map(str::to_string)
}
