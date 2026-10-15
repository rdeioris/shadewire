//! Blender socket values and types as IR values and types, and the names uniforms get from sockets.

use serde_json::Value as Json;
use shadewire::{Type, Value};

use crate::document::Socket;

/// The IR type of a Blender socket type; `None` for sockets that never hold a value (`STRING`, `SHADER`, `CUSTOM`).
pub fn socket_type(kind: &str) -> Option<Type> {
    match kind {
        "VALUE" => Some(Type::Float),
        "INT" => Some(Type::Integer),
        "BOOLEAN" => Some(Type::Boolean),
        "VECTOR" | "ROTATION" => Some(Type::Vector3),
        "RGBA" => Some(Type::Color3),
        _ => None,
    }
}

/// The socket's value as its IR type (an `RGBA` loses its alpha); `None` when the type holds no value or the JSON
/// is not a value of that type.
pub fn socket_value(socket: &Socket) -> Option<Value> {
    let ty = socket_type(&socket.kind)?;
    let json = socket.value.as_ref()?;
    match ty {
        Type::Float => Some(Value::Float(json.as_f64()? as f32)),
        Type::Integer => {
            let i = json.as_i64().or_else(|| json.as_f64().map(|f| f as i64))?;
            Some(Value::Integer(i.clamp(i32::MIN as i64, i32::MAX as i64) as i32))
        }
        Type::Boolean => json.as_bool().or_else(|| json.as_f64().map(|f| f != 0.0)).map(Value::Boolean),
        Type::Vector3 => floats(json).map(Value::Vector3),
        Type::Color3 => floats(json).map(Value::Color3),
        _ => None,
    }
}

/// The first three numbers of a JSON array of at least three.
fn floats(json: &Json) -> Option<[f32; 3]> {
    let list = json.as_array()?;
    if list.len() < 3 {
        return None;
    }
    let mut out = [0.0; 3];
    for (o, x) in out.iter_mut().zip(list) {
        *o = x.as_f64()? as f32;
    }
    Some(out)
}

/// `name` as an identifier: every character but ASCII letters and digits becomes `_`, and a leading digit (or an
/// empty name) gets a leading `_`.
pub fn sanitize(name: &str) -> String {
    let out: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    if out.chars().next().is_none_or(|c| c.is_ascii_digit()) { format!("_{out}") } else { out }
}

/// The uniform name of a socket's authored value: `{node}_{socket}`, sanitized.
pub fn uniform_name(node: &str, socket: &str) -> String {
    sanitize(&format!("{node}_{socket}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn socket(kind: &str, value: Json) -> Socket {
        Socket { identifier: "s".into(), name: "s".into(), kind: kind.into(), enabled: true, value: Some(value) }
    }

    #[test]
    fn socket_values_take_their_ir_type_and_rgba_drops_its_alpha() {
        assert_eq!(socket_value(&socket("VALUE", json!(0.5))), Some(Value::Float(0.5)));
        assert_eq!(socket_value(&socket("INT", json!(3))), Some(Value::Integer(3)));
        assert_eq!(socket_value(&socket("BOOLEAN", json!(true))), Some(Value::Boolean(true)));
        assert_eq!(socket_value(&socket("VECTOR", json!([1, 2, 3]))), Some(Value::Vector3([1.0, 2.0, 3.0])));
        assert_eq!(socket_value(&socket("RGBA", json!([1, 0.5, 0, 0.25]))), Some(Value::Color3([1.0, 0.5, 0.0])));
        assert_eq!(socket_value(&socket("STRING", json!("uv"))), None);
        assert_eq!(socket_value(&socket("VALUE", json!(null))), None);
        assert_eq!(socket_type("SHADER"), None);
    }

    #[test]
    fn names_are_sanitized_into_identifiers() {
        assert_eq!(uniform_name("Principled BSDF", "Base Color"), "Principled_BSDF_Base_Color");
        assert_eq!(sanitize("2 Gain.Socket_0"), "_2_Gain_Socket_0");
        assert_eq!(sanitize(""), "_");
    }
}
