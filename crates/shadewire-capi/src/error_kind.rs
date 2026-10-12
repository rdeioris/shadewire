//! The category of an error, as seen from C.

/// The category of a failed call.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum shadewire_error_kind {
    /// An argument passed by the caller is invalid.
    SHADEWIRE_ERROR_INVALID_ARGUMENT = 1,
    /// The document could not be parsed.
    SHADEWIRE_ERROR_PARSE = 2,
    /// The document uses something Shadewire does not support.
    SHADEWIRE_ERROR_UNSUPPORTED = 3,
    /// The document has no usable material, or the named one does not exist.
    SHADEWIRE_ERROR_MATERIAL = 4,
    /// The node graph is invalid.
    SHADEWIRE_ERROR_GRAPH = 5,
    /// The generated shader failed validation.
    SHADEWIRE_ERROR_SHADER = 6,
    /// Reading or writing failed.
    SHADEWIRE_ERROR_IO = 7,
    /// An internal fault: a panic caught at the boundary, or a library bug the caller cannot fix.
    SHADEWIRE_ERROR_PANIC = 8,
}

/// The kind of a core error.
pub(crate) fn kind_of(error: &shadewire::Error) -> shadewire_error_kind {
    use shadewire::Error;
    match error {
        Error::Parse { .. } => shadewire_error_kind::SHADEWIRE_ERROR_PARSE,
        Error::UnsupportedVersion { .. }
        | Error::UnsupportedType { .. }
        | Error::UnsupportedSurface { .. }
        | Error::UnsupportedColorspace { .. }
        | Error::NoTranslation { .. } => shadewire_error_kind::SHADEWIRE_ERROR_UNSUPPORTED,
        Error::NoMaterial | Error::AmbiguousMaterial { .. } | Error::UnknownMaterial { .. } => {
            shadewire_error_kind::SHADEWIRE_ERROR_MATERIAL
        }
        Error::UnknownNode { .. }
        | Error::NoMatchingNodeDef { .. }
        | Error::TypeMismatch { .. }
        | Error::Cycle { .. }
        | Error::MissingLeafImpl { .. }
        | Error::BadValue { .. } => shadewire_error_kind::SHADEWIRE_ERROR_GRAPH,
        Error::Naga(_) => shadewire_error_kind::SHADEWIRE_ERROR_SHADER,
        Error::Io(_) => shadewire_error_kind::SHADEWIRE_ERROR_IO,
    }
}

#[cfg(test)]
mod tests {
    use super::shadewire_error_kind as K;
    use super::*;
    use shadewire::{Error, Location};

    fn at() -> Location {
        Location::default()
    }

    #[test]
    fn parse_is_parse() {
        assert_eq!(kind_of(&Error::Parse { at: at(), message: "m".into() }), K::SHADEWIRE_ERROR_PARSE);
    }

    #[test]
    fn unsupported_group() {
        let all = [
            Error::UnsupportedVersion { at: at(), version: "9".into() },
            Error::UnsupportedType { at: at(), ty: "t".into() },
            Error::UnsupportedSurface { at: at(), node: "n".into(), reason: "r".into() },
            Error::UnsupportedColorspace { at: at(), colorspace: "c".into() },
            Error::NoTranslation { from: "a".into(), to: vec!["b".into()] },
        ];
        for e in &all {
            assert_eq!(kind_of(e), K::SHADEWIRE_ERROR_UNSUPPORTED, "{e}");
        }
    }

    #[test]
    fn material_group() {
        let all = [
            Error::NoMaterial,
            Error::AmbiguousMaterial { names: vec!["a".into()] },
            Error::UnknownMaterial { name: "x".into() },
        ];
        for e in &all {
            assert_eq!(kind_of(e), K::SHADEWIRE_ERROR_MATERIAL, "{e}");
        }
    }

    #[test]
    fn graph_group() {
        let all = [
            Error::UnknownNode { at: at(), category: "c".into() },
            Error::NoMatchingNodeDef { at: at(), category: "c".into(), output: "o".into(), inputs: "i".into() },
            Error::TypeMismatch { at: at(), expected: "e".into(), found: "f".into() },
            Error::Cycle { at: at(), node: "n".into() },
            Error::MissingLeafImpl { at: at(), nodedef: "d".into() },
            Error::BadValue { at: at(), message: "m".into() },
        ];
        for e in &all {
            assert_eq!(kind_of(e), K::SHADEWIRE_ERROR_GRAPH, "{e}");
        }
    }

    #[test]
    fn shader_and_io() {
        assert_eq!(kind_of(&Error::Naga("x".into())), K::SHADEWIRE_ERROR_SHADER);
        assert_eq!(kind_of(&Error::Io("x".into())), K::SHADEWIRE_ERROR_IO);
    }
}
