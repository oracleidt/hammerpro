use std::collections::HashMap;
use std::sync::OnceLock;

use crate::dmx::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParameterType {
    Int,
    Float,
    Bool,
    String,
    Color,
    Vec3,
    Vec4,
}

impl ParameterType {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Float => "float",
            Self::Bool => "bool",
            Self::String => "string",
            Self::Color => "color",
            Self::Vec3 => "vector3",
            Self::Vec4 => "vector4",
        }
    }

    pub(crate) fn accepts(self, value: &Value) -> bool {
        matches!(
            (self, value),
            (Self::Int, Value::Int(_))
                | (Self::Float, Value::Float(_))
                | (Self::Bool, Value::Bool(_))
                | (Self::String, Value::Str(_))
                | (Self::Color, Value::Color(_))
                | (Self::Vec3, Value::Vec3(_))
                | (Self::Vec4, Value::Vec4(_))
        )
    }

    pub(crate) fn normalize(self, value: Value) -> Result<Value, Self> {
        if self.accepts(&value) {
            return Ok(value);
        }
        match (self, value) {
            (Self::Float, Value::Int(v)) => Ok(Value::Float(v as f32)),
            (Self::Int, Value::Float(v))
                if v.is_finite() && v.fract() == 0.0 && (-2147483648.0..2147483648.0).contains(&f64::from(v)) =>
            {
                Ok(Value::Int(v as i32))
            }
            (Self::Bool, Value::Int(v)) if v == 0 || v == 1 => Ok(Value::Bool(v == 1)),
            (Self::Bool, Value::Float(v)) if v == 0.0 || v == 1.0 => Ok(Value::Bool(v == 1.0)),
            _ => Err(self),
        }
    }
}

type Parameters = HashMap<String, HashMap<String, ParameterType>>;

fn schemas() -> &'static Parameters {
    static SCHEMAS: OnceLock<Parameters> = OnceLock::new();
    SCHEMAS.get_or_init(|| {
        let mut result = Parameters::new();
        let mut operator = None;
        for line in include_str!("operator_parameters.tsv").lines() {
            if let Some((kind, name)) = line.split_once('\t') {
                let kind = match kind {
                    "2" => ParameterType::Int,
                    "3" => ParameterType::Float,
                    "4" => ParameterType::Bool,
                    "5" => ParameterType::String,
                    "8" => ParameterType::Color,
                    "10" => ParameterType::Vec3,
                    "11" => ParameterType::Vec4,
                    _ => panic!("invalid built-in particle parameter type"),
                };
                result
                    .get_mut(operator.as_ref().expect("parameter group"))
                    .expect("parameter group")
                    .insert(name.to_ascii_lowercase(), kind);
            } else {
                let name = line.to_ascii_lowercase();
                result.insert(name.clone(), HashMap::new());
                operator = Some(name);
            }
        }
        result
    })
}

pub(crate) fn expected(function: &str, key: &str) -> Option<ParameterType> {
    let schema = schemas();
    let operator = schema.get(&function.to_ascii_lowercase())?;
    let key = key.to_ascii_lowercase();
    operator.get(&key).or_else(|| schema.get("*")?.get(&key)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_catalog_covers_each_registered_operator_and_common_timing_fields() {
        let schema = schemas();
        assert_eq!(schema.len(), 161);
        assert_eq!(schema.values().map(HashMap::len).sum::<usize>(), 1016);
        assert_eq!(expected("RENDER_ANIMATED_SPRITES", "animation rate"), Some(ParameterType::Float));
        assert_eq!(expected("Sequence Two Random", "sequence_min"), Some(ParameterType::Int));
        assert_eq!(expected("Movement Lock to Bone", "operator start fadein"), Some(ParameterType::Float));
        assert_eq!(expected("unknown extension", "operator start fadein"), None);
    }

    #[test]
    fn conversions_reject_lossy_integer_or_ambiguous_boolean_values() {
        assert_eq!(ParameterType::Int.normalize(Value::Float(-2147483648.0)), Ok(Value::Int(i32::MIN)));
        for value in [1.5, 2147483648.0, f32::NAN, f32::INFINITY] {
            assert_eq!(ParameterType::Int.normalize(Value::Float(value)), Err(ParameterType::Int));
        }
        assert_eq!(ParameterType::Bool.normalize(Value::Int(2)), Err(ParameterType::Bool));
        assert_eq!(ParameterType::Bool.normalize(Value::Str("true".into())), Err(ParameterType::Bool));
    }
}
