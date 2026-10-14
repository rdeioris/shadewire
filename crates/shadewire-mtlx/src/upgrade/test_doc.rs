//! Test helper: build a document from a snippet.

use shadewire_nodes::document::Document;
use shadewire_nodes::document::build::build;
use shadewire_nodes::parse::parse_str;

pub fn doc_with_version(version: &str, body: &str) -> Document {
    let text = format!(r#"<?xml version="1.0"?><materialx version="{version}">{body}</materialx>"#);
    build(&parse_str(&text, None).unwrap(), None).unwrap()
}

pub fn doc_138(body: &str) -> Document {
    doc_with_version("1.38", body)
}
