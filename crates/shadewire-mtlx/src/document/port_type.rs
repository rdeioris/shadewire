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

    /// The type as MaterialX spells it; an untyped input is `untyped`.
    pub fn mtlx_name(&self) -> String {
        match self {
            PortType::Value(ty) => ty.mtlx_name().to_string(),
            PortType::String => "string".to_string(),
            PortType::Filename => "filename".to_string(),
            PortType::SurfaceShader => "surfaceshader".to_string(),
            PortType::DisplacementShader => "displacementshader".to_string(),
            PortType::Material => "material".to_string(),
            PortType::MultiOutput => "multioutput".to_string(),
            PortType::Closure(name) | PortType::Other(name) if !name.is_empty() => name.clone(),
            PortType::Closure(_) | PortType::Other(_) => "untyped".to_string(),
        }
    }

    /// Types whose values carry a colorspace.
    pub fn has_colorspace(&self) -> bool {
        matches!(self, PortType::Filename | PortType::Value(Type::Color3) | PortType::Value(Type::Color4))
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

    #[test]
    fn mtlx_name_spells_what_parse_reads() {
        for name in ["float", "color3", "filename", "BSDF", "surfaceshader", "multioutput", "volumeshader"] {
            assert_eq!(PortType::parse(name).mtlx_name(), name);
        }
        assert_eq!(PortType::Other(String::new()).mtlx_name(), "untyped");
    }
}
