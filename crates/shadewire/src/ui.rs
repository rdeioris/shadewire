use serde::Serialize;

use crate::Value;

/// MaterialX ui attributes of an authored input, carried to the uniform it becomes. `uiname` is not carried.
/// Limits have the input's type; serialized under their MaterialX names, each only when present.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct UiHints {
    #[serde(rename = "uimin", skip_serializing_if = "Option::is_none")]
    pub min: Option<Value>,
    #[serde(rename = "uimax", skip_serializing_if = "Option::is_none")]
    pub max: Option<Value>,
    #[serde(rename = "uisoftmin", skip_serializing_if = "Option::is_none")]
    pub soft_min: Option<Value>,
    #[serde(rename = "uisoftmax", skip_serializing_if = "Option::is_none")]
    pub soft_max: Option<Value>,
    /// `/`-separated folder path, as authored.
    #[serde(rename = "uifolder", skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

impl UiHints {
    pub fn is_empty(&self) -> bool {
        *self == UiHints::default()
    }
}
