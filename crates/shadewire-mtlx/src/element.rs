/// One XML element of a MaterialX document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    pub category: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Element>,
    pub line: u32,
}

impl Element {
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attributes.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// The `name` attribute, or the empty string.
    pub fn name(&self) -> &str {
        self.attr("name").unwrap_or("")
    }
}
