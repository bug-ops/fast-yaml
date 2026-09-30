//! Bounded conversion of JavaScript values into JSON for rule configuration.
//!
//! napi's own `serde_json::Value` conversion recurses without a limit, so a deeply nested or
//! cyclic JS value overflows the native stack before any validation runs. This conversion is
//! type-exact and capped in depth and node count, so hostile input fails with `InvalidArg`.

use napi::{
    Error, Result, Status, ValueType,
    bindgen_prelude::{Array, FromNapiValue, Object, TypeName, ValidateNapiValue, sys},
};
use serde_json::{Map, Number, Value};

/// Maximum nesting depth of a rule configuration value, mirroring the Python binding.
const MAX_DEPTH: usize = 16;
/// Maximum number of values (scalars, arrays and objects) in a rule configuration.
const MAX_NODES: usize = 100_000;

/// A rule configuration read from JavaScript under depth and node limits.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleInput(pub Value);

struct RawValue(sys::napi_value);

impl FromNapiValue for RawValue {
    unsafe fn from_napi_value(_: sys::napi_env, value: sys::napi_value) -> Result<Self> {
        Ok(Self(value))
    }
}

fn invalid_arg(reason: String) -> Error {
    Error::new(Status::InvalidArg, reason)
}

struct Converter {
    env: sys::napi_env,
    nodes: usize,
}

impl Converter {
    fn charge(&mut self, count: usize) -> Result<()> {
        self.nodes = self.nodes.saturating_add(count);
        if self.nodes > MAX_NODES {
            return Err(invalid_arg(format!(
                "rule configuration has more than {MAX_NODES} values"
            )));
        }
        Ok(())
    }

    unsafe fn convert(&mut self, value: sys::napi_value, depth: usize) -> Result<Value> {
        if depth > MAX_DEPTH {
            return Err(invalid_arg(format!(
                "rule configuration is nested deeper than {MAX_DEPTH} levels (or contains a cycle)"
            )));
        }
        self.charge(1)?;
        let env = self.env;
        let kind = napi::type_of!(env, value)?;
        unsafe {
            match kind {
                ValueType::Null => Ok(Value::Null),
                ValueType::Boolean => Ok(Value::Bool(bool::from_napi_value(env, value)?)),
                ValueType::Number => Ok(Value::Number(Number::from_napi_value(env, value)?)),
                ValueType::String => Ok(Value::String(String::from_napi_value(env, value)?)),
                ValueType::Object => self.convert_object(value, depth),
                other => Err(invalid_arg(format!(
                    "unsupported value of type '{other}' in rule configuration"
                ))),
            }
        }
    }

    unsafe fn convert_object(&mut self, value: sys::napi_value, depth: usize) -> Result<Value> {
        let env = self.env;
        let mut is_array = false;
        napi::check_status!(
            unsafe { sys::napi_is_array(env, value, &raw mut is_array) },
            "Failed to detect whether given js is an array"
        )?;
        if is_array {
            let items = unsafe { Array::from_napi_value(env, value)? };
            self.charge(items.len() as usize)?;
            let mut out = Vec::with_capacity(items.len() as usize);
            for index in 0..items.len() {
                if let Some(RawValue(item)) = items.get(index)? {
                    out.push(unsafe { self.convert(item, depth + 1)? });
                }
            }
            return Ok(Value::Array(out));
        }
        let object = unsafe { Object::from_napi_value(env, value)? };
        let keys = Object::keys(&object)?;
        self.charge(keys.len())?;
        let mut map = Map::new();
        for key in keys {
            if let Some(RawValue(item)) = object.get(&key)? {
                map.insert(key, unsafe { self.convert(item, depth + 1)? });
            }
        }
        Ok(Value::Object(map))
    }
}

impl TypeName for RuleInput {
    fn type_name() -> &'static str {
        "RuleInput"
    }

    fn value_type() -> ValueType {
        ValueType::Unknown
    }
}

impl ValidateNapiValue for RuleInput {}

impl FromNapiValue for RuleInput {
    unsafe fn from_napi_value(env: sys::napi_env, value: sys::napi_value) -> Result<Self> {
        let mut converter = Converter { env, nodes: 0 };
        unsafe { converter.convert(value, 0) }.map(Self)
    }
}
