//! A minimal XML element tree and its serializer.

/// An XML element: a tag, ordered attributes and child elements.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    /// The tag name.
    pub tag: String,
    /// The attributes in insertion order; the serializer reorders them.
    pub attrs: Vec<(String, String)>,
    /// The child elements.
    pub children: Vec<Element>,
}

impl Element {
    /// An element with this tag and no attributes or children.
    pub fn new(tag: &str) -> Element {
        Element { tag: tag.to_string(), attrs: Vec::new(), children: Vec::new() }
    }

    /// Sets an attribute; a repeated key replaces the earlier value.
    pub fn attr(mut self, key: &str, value: impl Into<String>) -> Element {
        let value = value.into();
        match self.attrs.iter_mut().find(|(k, _)| k == key) {
            Some(existing) => existing.1 = value,
            None => self.attrs.push((key.to_string(), value)),
        }
        self
    }

    /// Appends a child element (builder form).
    pub fn child(mut self, child: Element) -> Element {
        self.children.push(child);
        self
    }

    /// Appends a child element.
    pub fn push(&mut self, child: Element) {
        self.children.push(child);
    }
}

/// Attributes that come first, in this order; every other attribute follows alphabetically.
const ATTR_ORDER: [&str; 8] = ["name", "type", "nodedef", "nodename", "nodegraph", "output", "interfacename", "value"];

fn attr_rank(key: &str) -> (usize, &str) {
    match ATTR_ORDER.iter().position(|k| *k == key) {
        Some(index) => (index, ""),
        None => (ATTR_ORDER.len(), key),
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The XML header followed by `root`, indented by two spaces per level, one element per line.
pub fn to_text(root: &Element) -> String {
    let mut out = String::from("<?xml version=\"1.0\"?>\n");
    write_element(&mut out, root, 0);
    out
}

fn write_element(out: &mut String, element: &Element, depth: usize) {
    let indent = "  ".repeat(depth);
    out.push_str(&indent);
    out.push('<');
    out.push_str(&element.tag);
    let mut attrs: Vec<&(String, String)> = element.attrs.iter().collect();
    attrs.sort_by(|a, b| attr_rank(&a.0).cmp(&attr_rank(&b.0)));
    for (key, value) in attrs {
        out.push_str(&format!(" {key}=\"{}\"", escape(value)));
    }
    if element.children.is_empty() {
        out.push_str(" />\n");
        return;
    }
    out.push_str(">\n");
    for child in &element.children {
        write_element(out, child, depth + 1);
    }
    out.push_str(&format!("{indent}</{}>\n", element.tag));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_header_indentation_attribute_order_and_escaping() {
        let doc = Element::new("materialx").attr("version", "1.39").child(
            Element::new("nodegraph").attr("name", "NG").child(
                Element::new("constant").attr("type", "string").attr("name", "c").child(
                    Element::new("input").attr("value", "a<b&\"c\"").attr("type", "string").attr("name", "value"),
                ),
            ),
        );
        assert_eq!(
            to_text(&doc),
            "<?xml version=\"1.0\"?>\n<materialx version=\"1.39\">\n  <nodegraph name=\"NG\">\n    <constant name=\"c\" type=\"string\">\n      <input name=\"value\" type=\"string\" value=\"a&lt;b&amp;&quot;c&quot;\" />\n    </constant>\n  </nodegraph>\n</materialx>\n"
        );
    }

    #[test]
    fn an_element_without_children_is_self_closing_and_a_repeated_attr_replaces() {
        let e = Element::new("time").attr("name", "t").attr("type", "float").attr("name", "t2");
        assert_eq!(to_text(&e), "<?xml version=\"1.0\"?>\n<time name=\"t2\" type=\"float\" />\n");
    }
}
