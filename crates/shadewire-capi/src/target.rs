//! Output language selection at the C boundary.

use shadewire_wgsl::{GlslVersion, HlslShaderModel, MslVersion, Target};

use crate::strings::ArgError;

/// Which language `shadewire_target.kind` selects. Passed in from C, so a plain `u32`.
pub type shadewire_target_kind = u32;
pub const SHADEWIRE_TARGET_WGSL: shadewire_target_kind = 0;
pub const SHADEWIRE_TARGET_HLSL: shadewire_target_kind = 1;
pub const SHADEWIRE_TARGET_GLSL: shadewire_target_kind = 2;
pub const SHADEWIRE_TARGET_GLSL_ES: shadewire_target_kind = 3;
pub const SHADEWIRE_TARGET_WEBGL2: shadewire_target_kind = 4;
pub const SHADEWIRE_TARGET_MSL: shadewire_target_kind = 5;
pub const SHADEWIRE_TARGET_SPIRV: shadewire_target_kind = 6;

/// A target language and its version.
///
/// `version` is the shader model times ten for HLSL (50, 51, 60..69), the `#version` number for GLSL (330, 400, 410,
/// 420, 430, 450) and GLSL ES (300, 310), `major * 100 + minor` for MSL (e.g. 203 for 2.3), and ignored for WGSL,
/// WEBGL2 and SPIRV.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct shadewire_target {
    pub kind: shadewire_target_kind,
    pub version: u32,
}

fn bad_version(language: &str, version: u32) -> ArgError {
    ArgError(format!("unsupported {language} version {version}"))
}

fn hlsl_model(version: u32) -> Result<HlslShaderModel, ArgError> {
    match version {
        50 => Ok(HlslShaderModel::Sm5_0),
        51 => Ok(HlslShaderModel::Sm5_1),
        60 => Ok(HlslShaderModel::Sm6_0),
        61 => Ok(HlslShaderModel::Sm6_1),
        62 => Ok(HlslShaderModel::Sm6_2),
        63 => Ok(HlslShaderModel::Sm6_3),
        64 => Ok(HlslShaderModel::Sm6_4),
        65 => Ok(HlslShaderModel::Sm6_5),
        66 => Ok(HlslShaderModel::Sm6_6),
        67 => Ok(HlslShaderModel::Sm6_7),
        68 => Ok(HlslShaderModel::Sm6_8),
        69 => Ok(HlslShaderModel::Sm6_9),
        _ => Err(bad_version("HLSL", version)),
    }
}

/// Converts a C target to the Rust one, rejecting unknown kinds and versions.
pub fn to_target(target: shadewire_target) -> Result<Target, ArgError> {
    let version = target.version;
    match target.kind {
        SHADEWIRE_TARGET_WGSL => Ok(Target::Wgsl),
        SHADEWIRE_TARGET_HLSL => hlsl_model(version).map(Target::Hlsl),
        SHADEWIRE_TARGET_GLSL => match version {
            330 | 400 | 410 | 420 | 430 | 450 => Ok(Target::Glsl(GlslVersion::Desktop(version as u16))),
            _ => Err(bad_version("GLSL", version)),
        },
        SHADEWIRE_TARGET_GLSL_ES => match version {
            300 | 310 => Ok(Target::Glsl(GlslVersion::Es(version as u16))),
            _ => Err(bad_version("GLSL ES", version)),
        },
        SHADEWIRE_TARGET_WEBGL2 => Ok(Target::Glsl(GlslVersion::WebGl2)),
        SHADEWIRE_TARGET_MSL => {
            let major = u8::try_from(version / 100).map_err(|_| bad_version("MSL", version))?;
            let minor = (version % 100) as u8;
            Ok(Target::Msl(MslVersion { major, minor }))
        }
        SHADEWIRE_TARGET_SPIRV => Ok(Target::SpirV),
        other => Err(ArgError(format!("unknown target kind {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(kind: shadewire_target_kind, version: u32) -> Result<Target, ArgError> {
        to_target(shadewire_target { kind, version })
    }

    #[test]
    fn wgsl_webgl2_and_spirv_ignore_the_version() {
        assert_eq!(t(SHADEWIRE_TARGET_WGSL, 12345), Ok(Target::Wgsl));
        assert_eq!(t(SHADEWIRE_TARGET_WEBGL2, 7), Ok(Target::Glsl(GlslVersion::WebGl2)));
        assert_eq!(t(SHADEWIRE_TARGET_SPIRV, 0), Ok(Target::SpirV));
    }

    #[test]
    fn hlsl_models() {
        assert_eq!(t(SHADEWIRE_TARGET_HLSL, 50), Ok(Target::Hlsl(HlslShaderModel::Sm5_0)));
        assert_eq!(t(SHADEWIRE_TARGET_HLSL, 51), Ok(Target::Hlsl(HlslShaderModel::Sm5_1)));
        assert_eq!(t(SHADEWIRE_TARGET_HLSL, 60), Ok(Target::Hlsl(HlslShaderModel::Sm6_0)));
        assert_eq!(t(SHADEWIRE_TARGET_HLSL, 66), Ok(Target::Hlsl(HlslShaderModel::Sm6_6)));
        assert_eq!(t(SHADEWIRE_TARGET_HLSL, 69), Ok(Target::Hlsl(HlslShaderModel::Sm6_9)));
        for bad in [0, 40, 52, 59, 70, 600] {
            assert!(t(SHADEWIRE_TARGET_HLSL, bad).is_err(), "HLSL {bad}");
        }
    }

    #[test]
    fn glsl_versions() {
        for v in [330, 400, 410, 420, 430, 450] {
            assert_eq!(t(SHADEWIRE_TARGET_GLSL, v), Ok(Target::Glsl(GlslVersion::Desktop(v as u16))));
        }
        for bad in [0, 300, 440, 460, 70000] {
            assert!(t(SHADEWIRE_TARGET_GLSL, bad).is_err(), "GLSL {bad}");
        }
    }

    #[test]
    fn glsl_es_versions() {
        assert_eq!(t(SHADEWIRE_TARGET_GLSL_ES, 300), Ok(Target::Glsl(GlslVersion::Es(300))));
        assert_eq!(t(SHADEWIRE_TARGET_GLSL_ES, 310), Ok(Target::Glsl(GlslVersion::Es(310))));
        assert!(t(SHADEWIRE_TARGET_GLSL_ES, 320).is_err());
        assert!(t(SHADEWIRE_TARGET_GLSL_ES, 330).is_err());
    }

    #[test]
    fn msl_splits_major_and_minor() {
        assert_eq!(t(SHADEWIRE_TARGET_MSL, 203), Ok(Target::Msl(MslVersion { major: 2, minor: 3 })));
        assert_eq!(t(SHADEWIRE_TARGET_MSL, 310), Ok(Target::Msl(MslVersion { major: 3, minor: 10 })));
        assert!(t(SHADEWIRE_TARGET_MSL, 100_000).is_err());
    }

    #[test]
    fn unknown_kinds_are_rejected() {
        assert!(t(7, 0).is_err());
        assert!(t(99, 330).is_err());
        assert!(t(u32::MAX, 0).is_err());
    }
}
