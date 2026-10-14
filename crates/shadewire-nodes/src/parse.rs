use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use shadewire::{Error, Location};

use crate::element::Element;

/// Line numbers for increasing byte positions; counts each byte once.
struct Lines<'a> {
    text: &'a str,
    pos: usize,
    line: u32,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Lines<'a> {
        Lines { text, pos: 0, line: 1 }
    }

    fn at(&mut self, pos: usize) -> u32 {
        let end = pos.min(self.text.len());
        if end < self.pos {
            self.pos = 0;
            self.line = 1;
        }
        let new = self.text.as_bytes()[self.pos..end].iter().filter(|&&b| b == 0x0A).count();
        self.line += new as u32;
        self.pos = end;
        self.line
    }
}

/// Deeper than any MaterialX document nests; past it the element tree is refused
/// so building and dropping it cannot overflow the stack.
const MAX_NESTING: usize = 64;

fn fail(file: Option<&str>, line: u32, message: String) -> Error {
    Error::Parse { at: Location { file: file.map(str::to_string), line, path: String::new() }, message }
}

fn open(start: &BytesStart, line: u32, file: Option<&str>) -> Result<Element, Error> {
    let category = String::from_utf8_lossy(start.name().as_ref()).into_owned();
    let mut attributes = Vec::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|e| fail(file, line, e.to_string()))?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
        let value = attr.unescape_value().map_err(|e| fail(file, line, e.to_string()))?.into_owned();
        attributes.push((key, value));
    }
    Ok(Element { category, attributes, children: Vec::new(), line })
}

/// Parse a MaterialX document and return its `<materialx>` root.
pub fn parse_str(text: &str, file: Option<&str>) -> Result<Element, Error> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut lines = Lines::new(text);
    loop {
        let start_pos = reader.buffer_position() as usize;
        let event = match reader.read_event() {
            Ok(event) => event,
            Err(e) => {
                return Err(fail(file, lines.at(reader.buffer_position() as usize), e.to_string()));
            }
        };
        // trim_text skips leading whitespace, so find the real start of the tag
        let tag_pos = text[start_pos.min(text.len())..].find('<').map_or(start_pos, |o| start_pos + o);
        let line = lines.at(tag_pos);
        if matches!(event, Event::Start(_) | Event::Empty(_)) && stack.len() >= MAX_NESTING {
            return Err(fail(file, line, format!("elements nested deeper than {MAX_NESTING}")));
        }
        match event {
            Event::Start(s) => stack.push(open(&s, line, file)?),
            Event::Empty(s) => {
                let element = open(&s, line, file)?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(element),
                    None => root = Some(element),
                }
            }
            Event::End(_) => {
                let element = stack.pop().ok_or_else(|| fail(file, line, "unexpected end tag".into()))?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(element),
                    None => root = Some(element),
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if let Some(open) = stack.last() {
        return Err(fail(file, open.line, format!("unclosed element {}", open.category)));
    }
    root.ok_or_else(|| fail(file, 1, "no root element".into()))
}

#[cfg(test)]
mod tests {
    use super::parse_str;

    #[test]
    fn reads_nested_elements_with_lines() {
        let text = "<?xml version=\"1.0\"?>\n<materialx version=\"1.39\">\n  <nodegraph name=\"g\">\n    <add name=\"a\" type=\"float\"/>\n  </nodegraph>\n</materialx>\n";
        let root = parse_str(text, None).unwrap();
        assert_eq!(root.category, "materialx");
        assert_eq!(root.attr("version"), Some("1.39"));
        let graph = &root.children[0];
        assert_eq!(graph.name(), "g");
        assert_eq!(graph.children[0].category, "add");
        assert_eq!(graph.children[0].line, 4);
    }

    #[test]
    fn reports_bad_xml_with_a_line() {
        let err = parse_str("<materialx>\n<a>\n</materialx>", Some("bad.mtlx")).unwrap_err();
        assert!(err.to_string().contains("bad.mtlx"));
    }

    fn nested(depth: usize) -> String {
        let mut text = String::from("<materialx>");
        text.push_str(&"<a>".repeat(depth - 1));
        text.push_str(&"</a>".repeat(depth - 1));
        text.push_str("</materialx>");
        text
    }

    #[test]
    fn nesting_up_to_the_limit_is_read() {
        assert!(parse_str(&nested(64), None).is_ok());
    }

    #[test]
    fn deep_nesting_is_a_parse_error() {
        for depth in [200_000, 65] {
            let err = parse_str(&nested(depth), None).unwrap_err();
            assert!(err.to_string().contains("elements nested deeper than 64"), "{err}");
        }
    }

    #[test]
    fn unescapes_attribute_values() {
        let root = parse_str("<materialx a=\"x&amp;y\"/>", None).unwrap();
        assert_eq!(root.attr("a"), Some("x&y"));
    }
}
