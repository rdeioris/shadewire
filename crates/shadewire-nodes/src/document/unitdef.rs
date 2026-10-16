use shadewire::{Error, Location};

use crate::element::Element;

#[derive(Clone, Debug, PartialEq)]
pub struct UnitDef {
    pub unittype: String,
    pub scales: Vec<(String, f32)>,
}

pub fn build_unitdef(element: &Element, file: Option<&str>) -> Result<UnitDef, Error> {
    let mut scales = Vec::new();
    for unit in element.children.iter().filter(|c| c.category == "unit") {
        let text = unit.attr("scale").unwrap_or("");
        let scale: f32 = text.trim().parse().map_err(|_| Error::BadValue {
            at: Location { file: file.map(str::to_string), line: unit.line, path: unit.name().to_string(), source: None },
            message: format!("bad unit scale \"{text}\""),
        })?;
        scales.push((unit.name().to_string(), scale));
    }
    Ok(UnitDef { unittype: element.attr("unittype").unwrap_or("").to_string(), scales })
}
