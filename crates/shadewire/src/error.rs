#[derive(Clone, Debug, Default, PartialEq)]
pub struct Location {
    pub file: Option<String>,
    pub line: u32,
    pub path: String,
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{} ({})", self.file.as_deref().unwrap_or("<string>"), self.line, self.path)
    }
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("{at}: parse error: {message}")]
    Parse { at: Location, message: String },
    #[error("{at}: unsupported document version {version}")]
    UnsupportedVersion { at: Location, version: String },
    #[error("{at}: unknown node category {category}")]
    UnknownNode { at: Location, category: String },
    /// `inputs` lists the input types the node gave, such as `in1 vector2, in2 float`
    /// (a boxed str keeps `Error` small).
    #[error("{at}: no nodedef matches {category} -> {output} (inputs: {inputs})")]
    NoMatchingNodeDef { at: Location, category: String, output: String, inputs: Box<str> },
    #[error("{at}: type mismatch: expected {expected}, found {found}")]
    TypeMismatch { at: Location, expected: String, found: String },
    #[error("{at}: unsupported type {ty}")]
    UnsupportedType { at: Location, ty: String },
    #[error("{at}: bad value: {message}")]
    BadValue { at: Location, message: String },
    #[error("{at}: cycle through {node}")]
    Cycle { at: Location, node: String },
    #[error("{at}: no implementation for {nodedef}")]
    MissingLeafImpl { at: Location, nodedef: String },
    #[error("{at}: unsupported surface {node}: {reason}")]
    UnsupportedSurface { at: Location, node: String, reason: String },
    #[error("no translation from {from} to any of {to:?}")]
    NoTranslation { from: String, to: Vec<String> },
    #[error("{at}: unsupported colorspace {colorspace}")]
    UnsupportedColorspace { at: Location, colorspace: String },
    #[error("document has no material")]
    NoMaterial,
    #[error("several materials, pick one of {names:?}")]
    AmbiguousMaterial { names: Vec<String> },
    #[error("unknown material {name}")]
    UnknownMaterial { name: String },
    /// Over a host-set limit for untrusted documents (node count, texture count); the message names the limit,
    /// never document content.
    #[error("{0}")]
    Limit(String),
    #[error("{0}")]
    Io(String),
    /// A back end asked for a language version it cannot print, such as GLSL 300 (desktop) or ES 330.
    #[error("unsupported target: {0}")]
    UnsupportedTarget(String),
}
