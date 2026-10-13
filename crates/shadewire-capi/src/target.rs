//! Output language selection at the C boundary: which back end compiles.

use crate::strings::ArgError;

/// Which language `shadewire_target.kind` selects. Passed in from C, so a plain `u32`.
pub type shadewire_target_kind = u32;
pub const SHADEWIRE_TARGET_WGSL: shadewire_target_kind = 0;
pub const SHADEWIRE_TARGET_GLSL: shadewire_target_kind = 1;
pub const SHADEWIRE_TARGET_GLSL_ES: shadewire_target_kind = 2;
pub const SHADEWIRE_TARGET_HLSL: shadewire_target_kind = 3;
pub const SHADEWIRE_TARGET_MSL: shadewire_target_kind = 4;

/// A target language and its version.
///
/// `version` is the `#version` number for GLSL (330, 400, 410, 420, 430, 440, 450, 460; 0 for 420) and GLSL ES
/// (300, 310, 320; 0 for 300), and ignored for WGSL, HLSL and MSL. Any other version is
/// SHADEWIRE_ERROR_INVALID_ARGUMENT.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct shadewire_target {
    pub kind: shadewire_target_kind,
    pub version: u32,
}

/// A back end, with what it needs to know.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Backend {
    Wgsl,
    Glsl(shadewire_glsl::Version),
    Hlsl,
    Msl,
}

fn bad_version(language: &str, version: u32) -> ArgError {
    ArgError(format!("unsupported {language} version {version}"))
}

/// The back end a C target selects, rejecting unknown kinds and versions.
pub(crate) fn backend(target: shadewire_target) -> Result<Backend, ArgError> {
    use shadewire_glsl::Version;
    let version = target.version;
    match target.kind {
        SHADEWIRE_TARGET_WGSL => Ok(Backend::Wgsl),
        SHADEWIRE_TARGET_GLSL => match version {
            0 => Ok(Backend::Glsl(Version::Desktop(420))),
            330 | 400 | 410 | 420 | 430 | 440 | 450 | 460 => Ok(Backend::Glsl(Version::Desktop(version as u16))),
            _ => Err(bad_version("GLSL", version)),
        },
        SHADEWIRE_TARGET_GLSL_ES => match version {
            0 => Ok(Backend::Glsl(Version::Es(300))),
            300 | 310 | 320 => Ok(Backend::Glsl(Version::Es(version as u16))),
            _ => Err(bad_version("GLSL ES", version)),
        },
        SHADEWIRE_TARGET_HLSL => Ok(Backend::Hlsl),
        SHADEWIRE_TARGET_MSL => Ok(Backend::Msl),
        other => Err(ArgError(format!("unknown target kind {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire_glsl::Version;

    fn t(kind: shadewire_target_kind, version: u32) -> Result<Backend, ArgError> {
        backend(shadewire_target { kind, version })
    }

    #[test]
    fn wgsl_hlsl_and_msl_ignore_the_version() {
        assert_eq!(t(SHADEWIRE_TARGET_WGSL, 12345), Ok(Backend::Wgsl));
        assert_eq!(t(SHADEWIRE_TARGET_HLSL, 60), Ok(Backend::Hlsl));
        assert_eq!(t(SHADEWIRE_TARGET_MSL, 7), Ok(Backend::Msl));
    }

    #[test]
    fn glsl_versions() {
        for v in [330, 400, 410, 420, 430, 440, 450, 460] {
            assert_eq!(t(SHADEWIRE_TARGET_GLSL, v), Ok(Backend::Glsl(Version::Desktop(v as u16))));
        }
        assert_eq!(t(SHADEWIRE_TARGET_GLSL, 0), Ok(Backend::Glsl(Version::Desktop(420))));
        for bad in [300, 320, 470, 70000] {
            assert!(t(SHADEWIRE_TARGET_GLSL, bad).is_err(), "GLSL {bad}");
        }
    }

    #[test]
    fn glsl_es_versions() {
        for v in [300, 310, 320] {
            assert_eq!(t(SHADEWIRE_TARGET_GLSL_ES, v), Ok(Backend::Glsl(Version::Es(v as u16))));
        }
        assert_eq!(t(SHADEWIRE_TARGET_GLSL_ES, 0), Ok(Backend::Glsl(Version::Es(300))));
        assert!(t(SHADEWIRE_TARGET_GLSL_ES, 330).is_err());
    }

    #[test]
    fn unknown_kinds_are_rejected() {
        assert!(t(5, 0).is_err());
        assert!(t(u32::MAX, 0).is_err());
    }
}
