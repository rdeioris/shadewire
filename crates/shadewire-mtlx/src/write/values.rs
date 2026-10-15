//! MaterialX value text.

use shadewire::Value;

/// A value as MaterialX attribute text: `true`, `3`, `0.5`, `1, 0.5, 0.25`.
pub fn value_text(value: &Value) -> String {
    match value {
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        other => other.floats().iter().map(|f| format!("{f}")).collect::<Vec<String>>().join(", "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire::{Type, Value};

    #[test]
    fn values_print_as_materialx_text() {
        assert_eq!(value_text(&Value::Boolean(true)), "true");
        assert_eq!(value_text(&Value::Integer(-3)), "-3");
        assert_eq!(value_text(&Value::Float(0.5)), "0.5");
        assert_eq!(value_text(&Value::Float(2.0)), "2");
        assert_eq!(value_text(&Value::Color3([1.0, 0.5, 0.25])), "1, 0.5, 0.25");
        assert_eq!(value_text(&Value::Vector2([0.0, -1.5])), "0, -1.5");
        assert_eq!(
            Value::parse(Type::Color3, &value_text(&Value::Color3([0.1, 0.2, 0.3]))).unwrap(),
            Value::Color3([0.1, 0.2, 0.3])
        );
    }
}
