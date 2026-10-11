use crate::{Type, Value};

/// A MaterialX value string that does not match its type.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("cannot read {text:?} as {ty:?}")]
pub struct ParseValueError {
    pub ty: Type,
    pub text: String,
}

impl Value {
    /// Reads a MaterialX value string: comma-separated components, spaces allowed.
    pub fn parse(ty: Type, text: &str) -> Result<Value, ParseValueError> {
        let error = || ParseValueError { ty, text: text.to_string() };
        match ty {
            Type::Boolean => match text.trim() {
                "true" => Ok(Value::Boolean(true)),
                "false" => Ok(Value::Boolean(false)),
                _ => Err(error()),
            },
            Type::Integer => text.trim().parse::<i32>().map(Value::Integer).map_err(|_| error()),
            _ => {
                let mut floats = Vec::new();
                for part in text.split(',') {
                    floats.push(part.trim().parse::<f32>().map_err(|_| error())?);
                }
                if floats.len() != ty.component_count() {
                    return Err(error());
                }
                Ok(Value::from_floats(ty, &floats))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Type, Value};

    #[test]
    fn parses_mtlx_value_strings() {
        assert_eq!(Value::parse(Type::Float, "0.5").unwrap(), Value::Float(0.5));
        assert_eq!(Value::parse(Type::Color3, "0.8, 0.8,0.8").unwrap(), Value::Color3([0.8, 0.8, 0.8]));
        assert_eq!(Value::parse(Type::Integer, "3").unwrap(), Value::Integer(3));
        assert_eq!(Value::parse(Type::Boolean, "true").unwrap(), Value::Boolean(true));
        assert_eq!(Value::parse(Type::Vector2, "1,2").unwrap(), Value::Vector2([1.0, 2.0]));
    }

    #[test]
    fn rejects_wrong_component_count() {
        assert!(Value::parse(Type::Color3, "1, 2").is_err());
        assert!(Value::parse(Type::Float, "abc").is_err());
    }

    #[test]
    fn floats_round_trip() {
        let v = Value::Vector3([1.0, -2.0, 3.5]);
        assert_eq!(Value::from_floats(Type::Vector3, &v.floats()), v);
    }
}
