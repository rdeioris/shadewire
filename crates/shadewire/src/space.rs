/// Coordinate space of a position, normal or direction read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Space {
    Model,
    Object,
    World,
}

impl Space {
    pub fn from_mtlx(name: &str) -> Option<Space> {
        match name {
            "model" => Some(Space::Model),
            "object" => Some(Space::Object),
            "world" => Some(Space::World),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Space::Model => "model",
            Space::Object => "object",
            Space::World => "world",
        }
    }
}
