//! Unit conversion of authored values. Only `distance` is converted.

use shadewire_ir::{Error, Location, Node, NodeId, NodeKind, Op, Type, Value};

use super::Elaborator;

impl<'a> Elaborator<'a> {
    /// Converts a value authored in `unit` to `target_unit` (the nodedef
    /// port's unit). Distances are multiplied by `scale(unit) / scale(target)`;
    /// no unit or no target leaves the value unchanged. Other unit types are
    /// not converted and recorded as an approximation.
    pub fn convert_unit(
        &mut self,
        value: NodeId,
        ty: Type,
        unit: Option<&str>,
        unittype: Option<&str>,
        target_unit: Option<&str>,
        at: &Location,
    ) -> Result<NodeId, Error> {
        let (Some(unit), Some(unittype)) = (unit, unittype) else { return Ok(value) };
        if unittype != "distance" {
            let note = format!("unit {unittype} ignored");
            if !self.dag.approximations.contains(&note) {
                self.dag.approximations.push(note);
            }
            return Ok(value);
        }
        let Some(target) = target_unit else { return Ok(value) };
        let from = self.distance_scale(unit, at)?;
        let to = self.distance_scale(target, at)?;
        let factor = from / to;
        if factor == 1.0 {
            return Ok(value);
        }
        let origin = at.path.clone();
        let scaled = match &self.dag.node(value).kind {
            NodeKind::Const(v) => NodeKind::Const(scale(v, factor, at)?),
            _ => {
                let splat = scale(&Value::parse(ty, &splat_text(ty)).map_err(|e| bad(at, e.to_string()))?, factor, at)?;
                let factor = self.dag.add(Node { kind: NodeKind::Const(splat), ty, origin: origin.clone() });
                NodeKind::Op { op: Op::Multiply, args: vec![value, factor] }
            }
        };
        Ok(self.dag.add(Node { kind: scaled, ty, origin }))
    }

    fn distance_scale(&self, unit: &str, at: &Location) -> Result<f32, Error> {
        self.index.unit_scale("distance", unit).ok_or_else(|| bad(at, format!("unknown distance unit {unit}")))
    }
}

fn bad(at: &Location, message: String) -> Error {
    Error::BadValue { at: at.clone(), message }
}

/// `1` in every component of a `ty` value, as MaterialX text.
fn splat_text(ty: Type) -> String {
    let n = match ty {
        Type::Vector2 => 2,
        Type::Vector3 => 3,
        Type::Vector4 => 4,
        _ => 1,
    };
    vec!["1"; n].join(",")
}

/// A float or vector value with every component multiplied by `factor`.
fn scale(value: &Value, factor: f32, at: &Location) -> Result<Value, Error> {
    Ok(match value {
        Value::Float(x) => Value::Float(x * factor),
        Value::Vector2(v) => Value::Vector2(v.map(|x| x * factor)),
        Value::Vector3(v) => Value::Vector3(v.map(|x| x * factor)),
        Value::Vector4(v) => Value::Vector4(v.map(|x| x * factor)),
        other => return Err(bad(at, format!("a distance unit on a {} value", other.ty().mtlx_name()))),
    })
}
