use serde::Serialize;

/// A texture the material samples. The file is read by the host, not by the compiler.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct TextureDesc {
    pub file: String,
    pub colorspace: Option<String>,
    pub address_u: AddressMode,
    pub address_v: AddressMode,
    pub filter: FilterMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AddressMode {
    Constant,
    Clamp,
    Periodic,
    Mirror,
}

impl AddressMode {
    pub fn from_mtlx(name: &str) -> Option<AddressMode> {
        match name {
            "constant" => Some(AddressMode::Constant),
            "clamp" => Some(AddressMode::Clamp),
            "periodic" => Some(AddressMode::Periodic),
            "mirror" => Some(AddressMode::Mirror),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterMode {
    Closest,
    Linear,
    Cubic,
}

impl FilterMode {
    pub fn from_mtlx(name: &str) -> Option<FilterMode> {
        match name {
            "closest" => Some(FilterMode::Closest),
            "linear" => Some(FilterMode::Linear),
            "cubic" => Some(FilterMode::Cubic),
            _ => None,
        }
    }
}
