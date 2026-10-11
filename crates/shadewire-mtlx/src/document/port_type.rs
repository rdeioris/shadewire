use shadewire_ir::Type;

/// The declared type of a port: a value type or one of the non-value kinds.
#[derive(Clone, Debug, PartialEq)]
pub enum PortType {
    Value(Type),
    String,
    Filename,
    SurfaceShader,
    DisplacementShader,
    Material,
    Closure(String),
    MultiOutput,
    Other(String),
}

impl PortType {
    pub fn parse(name: &str) -> PortType {
        match name {
            "surfaceshader" => PortType::SurfaceShader,
            "displacementshader" => PortType::DisplacementShader,
            "material" => PortType::Material,
            "BSDF" | "EDF" | "VDF" => PortType::Closure(name.to_string()),
            "multioutput" => PortType::MultiOutput,
            "string" => PortType::String,
            "filename" => PortType::Filename,
            _ => match Type::from_mtlx_name(name) {
                Some(ty) => PortType::Value(ty),
                None => PortType::Other(name.to_string()),
            },
        }
    }

    /// Types whose values carry a colorspace.
    pub fn has_colorspace(&self) -> bool {
        matches!(
            self,
            PortType::Filename | PortType::Value(Type::Color3) | PortType::Value(Type::Color4)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::PortType;
    use shadewire_ir::Type;

    #[test]
    fn parses_each_family() {
        assert_eq!(PortType::parse("float"), PortType::Value(Type::Float));
        assert_eq!(PortType::parse("filename"), PortType::Filename);
        assert_eq!(PortType::parse("BSDF"), PortType::Closure("BSDF".into()));
        assert_eq!(PortType::parse("surfaceshader"), PortType::SurfaceShader);
        assert_eq!(PortType::parse("volumeshader"), PortType::Other("volumeshader".into()));
    }
}
