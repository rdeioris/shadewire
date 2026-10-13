use crate::element::Element;

/// The ui attributes of an input or port, as authored text. `uiname` is not read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiAttrs {
    pub min: Option<String>,
    pub max: Option<String>,
    pub soft_min: Option<String>,
    pub soft_max: Option<String>,
    pub folder: Option<String>,
}

impl UiAttrs {
    pub fn read(element: &Element) -> UiAttrs {
        let get = |key| element.attr(key).filter(|v| !v.is_empty()).map(str::to_string);
        UiAttrs {
            min: get("uimin"),
            max: get("uimax"),
            soft_min: get("uisoftmin"),
            soft_max: get("uisoftmax"),
            folder: get("uifolder"),
        }
    }

    /// Each attribute from `self`, else from `fallback` (a node input over its nodedef port).
    pub fn or(&self, fallback: &UiAttrs) -> UiAttrs {
        let pick = |own: &Option<String>, other: &Option<String>| own.clone().or_else(|| other.clone());
        UiAttrs {
            min: pick(&self.min, &fallback.min),
            max: pick(&self.max, &fallback.max),
            soft_min: pick(&self.soft_min, &fallback.soft_min),
            soft_max: pick(&self.soft_max, &fallback.soft_max),
            folder: pick(&self.folder, &fallback.folder),
        }
    }
}
