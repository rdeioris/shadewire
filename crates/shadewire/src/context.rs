use crate::{Space, Type};

/// A value the host supplies per fragment, per frame or per geometry.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ContextRead {
    Position(Space),
    Normal(Space),
    Tangent(Space),
    Bitangent(Space),
    TexCoord { index: u32, ty: Type },
    GeomColor { index: u32, ty: Type },
    GeomProp { name: String, ty: Type },
    ViewDirection(Space),
    Time,
    Frame,
}

/// How the host must provide a read: per vertex, computed from others, once per frame, or per geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextClass {
    Varying,
    Derived,
    Global,
    GeomProp,
}

impl ContextRead {
    pub fn ty(&self) -> Type {
        match self {
            ContextRead::Position(_)
            | ContextRead::Normal(_)
            | ContextRead::Tangent(_)
            | ContextRead::Bitangent(_)
            | ContextRead::ViewDirection(_) => Type::Vector3,
            ContextRead::TexCoord { ty, .. } | ContextRead::GeomColor { ty, .. } | ContextRead::GeomProp { ty, .. } => {
                *ty
            }
            ContextRead::Time | ContextRead::Frame => Type::Float,
        }
    }

    /// `position_world`, `texcoord_0`, `geomprop_displayColor`, `time` and so on.
    pub fn reflection_name(&self) -> String {
        match self {
            ContextRead::Position(s) => format!("position_{}", s.name()),
            ContextRead::Normal(s) => format!("normal_{}", s.name()),
            ContextRead::Tangent(s) => format!("tangent_{}", s.name()),
            ContextRead::Bitangent(s) => format!("bitangent_{}", s.name()),
            ContextRead::TexCoord { index, .. } => format!("texcoord_{index}"),
            ContextRead::GeomColor { index, .. } => format!("geomcolor_{index}"),
            ContextRead::GeomProp { name, .. } => format!("geomprop_{name}"),
            ContextRead::ViewDirection(s) => format!("view_direction_{}", s.name()),
            ContextRead::Time => "time".to_string(),
            ContextRead::Frame => "frame".to_string(),
        }
    }

    /// The host function the generated WGSL calls.
    pub fn function_name(&self) -> String {
        format!("shadewire_{}", self.reflection_name())
    }

    pub fn class(&self) -> ContextClass {
        match self {
            ContextRead::ViewDirection(_) => ContextClass::Derived,
            ContextRead::Time | ContextRead::Frame => ContextClass::Global,
            ContextRead::GeomProp { .. } => ContextClass::GeomProp,
            _ => ContextClass::Varying,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{ContextRead, Space, Type};

    #[test]
    fn host_function_names() {
        assert_eq!(ContextRead::Position(Space::World).function_name(), "shadewire_position_world");
        assert_eq!(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }.function_name(), "shadewire_texcoord_0");
        assert_eq!(ContextRead::ViewDirection(Space::World).function_name(), "shadewire_view_direction_world");
        assert_eq!(
            ContextRead::GeomProp { name: "displayColor".into(), ty: Type::Color3 }.function_name(),
            "shadewire_geomprop_displayColor"
        );
        assert_eq!(ContextRead::Time.function_name(), "shadewire_time");
    }
}
