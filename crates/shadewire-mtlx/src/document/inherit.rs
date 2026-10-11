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
            fileprefix: element
                .attr("fileprefix")
                .map(str::to_string)
                .unwrap_or_else(|| self.fileprefix.clone()),
            colorspace: element
                .attr("colorspace")
                .map(str::to_string)
                .or_else(|| self.colorspace.clone()),
        }
    }
}
