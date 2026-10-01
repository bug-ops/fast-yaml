//! Type conversion between Rust YAML values and JavaScript objects.
//!
//! This module provides bidirectional conversion utilities for translating
//! between `fast_yaml_core::Value` and NAPI-RS JavaScript values.

use fast_yaml_core::value::quote_key;
use fast_yaml_core::{DumpBudget, Float, LimitKind, Mapping, MaxDepth, Set, Value};
use napi::{Result as NapiResult, bindgen_prelude::*};
use std::collections::HashSet;

/// Sets `key` on `object`; `__proto__` is defined as an own property so it cannot replace the prototype.
fn set_own(env: Env, object: &mut Object, key: &str, value: Unknown) -> NapiResult<()> {
    if key == "__proto__" {
        return object
            .define_properties(&[Property::new().with_name(&env, key)?.with_value(&value)]);
    }
    object.set(key, value)
}

/// Convert a YAML value to a JavaScript value.
///
/// Handles all YAML 1.2.2 Core Schema types including special float values.
///
/// # Type Mapping
///
/// - `Value::Null` → `null`
/// - `Value::Bool` → `boolean`
/// - `Value::Int` → `number`
/// - `Value::Float` → `number`
/// - `Value::String` → `string`
/// - `Value::Sequence` → `Array`
/// - `Value::Mapping` → `Object`
///
/// # Errors
///
/// Returns an error if conversion fails or encounters invalid YAML values.
pub fn yaml_to_js<'env>(env: &'env Env, yaml: &Value) -> NapiResult<Unknown<'env>> {
    match yaml {
        Value::Null => Null.into_unknown(env),
        Value::Bool(b) => (*b).into_unknown(env),
        Value::Int(i) => (*i).into_unknown(env),
        Value::BigInt(big) => big.canonical().into_unknown(env),
        Value::Float(f) => f.get().into_unknown(env),
        Value::String(s) => s.as_str().into_unknown(env),

        Value::Sequence(arr) => {
            let arr_len = u32::try_from(arr.len()).map_err(|_| {
                napi::Error::from_reason("array too large for JavaScript (max 2^32 elements)")
            })?;

            // Pre-allocate JavaScript array with known capacity to avoid reallocation.
            // NAPI-RS env.create_array(len) hints to V8 the final array size, enabling
            // efficient memory allocation and reducing overhead for large arrays.
            let mut js_array = env.create_array(arr_len)?;
            for (i, item) in arr.iter().enumerate() {
                let js_value = yaml_to_js(env, item)?;
                let idx = u32::try_from(i)
                    .map_err(|_| napi::Error::from_reason("array index too large"))?;
                js_array.set(idx, js_value)?;
            }
            js_array.into_unknown(env)
        }

        Value::Set(set) => {
            let mut js_obj = Object::new(env)?;
            let mut seen = collision_table(set.iter(), set.len());
            for member in set {
                let key_str = yaml_key_to_string(member)?;
                record_property(seen.as_mut(), &key_str)?;
                set_own(*env, &mut js_obj, &key_str, Null.into_unknown(env)?)?;
            }
            js_obj.into_unknown(env)
        }

        Value::Mapping(map) => {
            let mut js_obj = Object::new(env)?;
            let mut seen = collision_table(map.keys(), map.len());
            for (k, v) in map {
                let key_str = yaml_key_to_string(k)?;
                record_property(seen.as_mut(), &key_str)?;
                let js_value = yaml_to_js(env, v)?;
                set_own(*env, &mut js_obj, &key_str, js_value)?;
            }
            js_obj.into_unknown(env)
        }
    }
}

/// Allocates a table of property names only when a key is not a string.
///
/// The loader already rejects keys that share a property name; this guards values built some
/// other way, and string-only keys cannot collide.
fn collision_table<'a>(
    mut keys: impl Iterator<Item = &'a Value>,
    capacity: usize,
) -> Option<HashSet<String>> {
    keys.any(|key| !matches!(key, Value::String(_)))
        .then(|| HashSet::with_capacity(capacity))
}

/// Records a property name, failing when an earlier distinct YAML key produced it as well.
///
/// With no table every key is a string, so a name can only repeat as the same key, which the
/// mapping has already collapsed.
fn record_property(seen: Option<&mut HashSet<String>>, name: &str) -> NapiResult<()> {
    if let Some(seen) = seen
        && !seen.insert(name.to_owned())
    {
        return Err(napi::Error::from_reason(format!(
            "distinct YAML keys convert to the same JavaScript property {}",
            quote_key(name)
        )));
    }
    Ok(())
}

/// Convert a YAML key to a string for use as JavaScript object property.
///
/// YAML keys can be any type, but JavaScript object keys must be strings.
fn yaml_key_to_string(yaml: &Value) -> NapiResult<String> {
    yaml.key_text()
        .map(std::borrow::Cow::into_owned)
        .ok_or_else(|| {
            napi::Error::from_reason(
                "YAML complex keys (sequences or mappings as keys) are not supported as JavaScript object keys",
            )
        })
}

/// Convert a JavaScript value to a YAML value.
///
/// Handles JavaScript types including special float values (Infinity, -Infinity, NaN).
///
/// # Type Mapping
///
/// - `null`, `undefined` → `Value::Null`
/// - `boolean` → `Value::Bool`
/// - `number` (integer) → `Value::Int`
/// - `number` (float) → `Value::Float`
/// - `string` → `Value::String`
/// - `Array` → `Value::Sequence`
/// - `Object` → `Value::Mapping`
/// - `Set` → `Value::Set` (a `!!set`)
/// - `Map` → `Value::Mapping`
///
/// A `Set` or `Map` is recognised by its `Symbol.toStringTag`, so one from another realm works
/// too. An object that claims the tag without being one is an error.
///
/// # Errors
///
/// Returns an error if the JavaScript value contains non-serializable types, two `Set` members
/// or `Map` keys are the same YAML value, or converting it would exceed `budget`.
pub fn js_to_yaml(env: Env, js_value: Unknown, budget: &mut DumpBudget) -> NapiResult<Value> {
    let mut stack: Vec<OpenContainer> = Vec::new();
    let mut keyed = KeyedCollections::new(env);
    match classify(js_value, 0, budget, &mut keyed)? {
        Classified::Scalar(value) => return Ok(value),
        Classified::Container(open) => stack.push(open),
    }
    // The walk is iterative (explicit heap stack), so host stack size does not bound
    // nesting; `MaxDepth::DEFAULT` does, which also catches self-referential values.
    loop {
        let depth = stack.len();
        let Some(top) = stack.last_mut() else {
            return Err(napi::Error::from_reason(
                "internal error: empty conversion stack",
            ));
        };
        if let Some(child) = top.next_child() {
            match classify(child.value, depth, budget, &mut keyed)? {
                Classified::Scalar(value) => top.accept(child.key, value)?,
                Classified::Container(open) => {
                    top.pending_key = child.key;
                    stack.push(open);
                }
            }
        } else if let Some(finished) = stack.pop() {
            let value = finished.finish();
            match stack.last_mut() {
                Some(parent) => {
                    let key = parent.pending_key.take();
                    parent.accept(key, value)?;
                }
                None => return Ok(value),
            }
        }
    }
}

/// A child value plus, for object members, its property name.
struct Child<'a> {
    key: Option<String>,
    value: Unknown<'a>,
}

/// A container whose children are still being converted.
struct OpenContainer<'a> {
    children: std::vec::IntoIter<Child<'a>>,
    /// Property name of the child container currently being converted.
    pending_key: Option<String>,
    done: Done,
}

enum Done {
    Sequence(Vec<Value>),
    Mapping(Mapping),
    Set(Set),
    /// A `Map`: children alternate key, value; `key` holds a key awaiting its value.
    Map {
        entries: Mapping,
        key: Option<Value>,
    },
}

impl<'a> OpenContainer<'a> {
    fn next_child(&mut self) -> Option<Child<'a>> {
        self.children.next()
    }

    fn accept(&mut self, key: Option<String>, value: Value) -> NapiResult<()> {
        match (&mut self.done, key) {
            (Done::Mapping(map), Some(key)) => {
                map.insert(Value::String(key), value);
            }
            (Done::Sequence(items), _) => items.push(value),
            (Done::Mapping(_), None) => {}
            (Done::Set(set), _) => {
                if !set.insert(value) {
                    return Err(napi::Error::from_reason(
                        "cannot serialize to YAML: two Set members are the same YAML value",
                    ));
                }
            }
            (Done::Map { entries, key }, _) => match key.take() {
                None => *key = Some(value),
                Some(key) => {
                    if entries.insert(key, value).is_some() {
                        return Err(napi::Error::from_reason(
                            "cannot serialize to YAML: two Map keys are the same YAML key",
                        ));
                    }
                }
            },
        }
        Ok(())
    }

    fn finish(self) -> Value {
        match self.done {
            Done::Sequence(items) => Value::Sequence(items),
            Done::Mapping(map) => Value::Mapping(map),
            Done::Set(set) => Value::Set(set),
            Done::Map { entries, .. } => Value::Mapping(entries),
        }
    }
}

/// Which keyed built-in a `Symbol.toStringTag` names.
#[derive(Clone, Copy)]
enum KeyedKind {
    Set,
    Map,
}

impl KeyedKind {
    const fn name(self) -> &'static str {
        match self {
            Self::Set => "Set",
            Self::Map => "Map",
        }
    }

    /// Nodes charged per entry: a member, or a key and a value.
    const fn nodes_per_entry(self) -> usize {
        match self {
            Self::Set => 1,
            Self::Map => 2,
        }
    }
}

/// JavaScript run once per dump to capture the `Set` and `Map` intrinsics.
///
/// `size` and `entries` brand-check their receiver through the built-in `size` getter, so an
/// object that only claims the tag throws a `TypeError`, a real `Set` or `Map` from any realm
/// works, and the user's own `size`, `length` and `Symbol.iterator` are never consulted. `entries`
/// takes at most `size` steps of the built-in iterator.
const KEYED_INTRINSICS: &str = r"(() => {
  const apply = Reflect.apply;
  const sizeOf = (proto) => Object.getOwnPropertyDescriptor(proto, 'size').get;
  const sizes = [sizeOf(Set.prototype), sizeOf(Map.prototype)];
  const openers = [Set.prototype.values, Map.prototype.entries];
  const nexts = [new Set().values().next, new Map().entries().next];
  return {
    size: (isMap, object) => apply(sizes[+isMap], object, []),
    entries: (isMap, object, size) => {
      const iterator = apply(openers[+isMap], object, []);
      const out = [];
      for (let i = 0; i < size; i++) {
        const step = apply(nexts[+isMap], iterator, []);
        if (step.done) break;
        if (isMap) out.push(step.value[0], step.value[1]);
        else out.push(step.value);
      }
      return out;
    },
  };
})()";

/// Handles needed to recognise and read a `Set` or `Map`, fetched on the first plain object.
struct KeyedHandles<'a> {
    to_string_tag: Unknown<'a>,
    size: Function<'a, FnArgs<(bool, Object<'a>)>, f64>,
    entries: Function<'a, FnArgs<(bool, Object<'a>, f64)>, Object<'a>>,
}

/// Recognises `Set` and `Map` objects by `Symbol.toStringTag`, which also holds across realms.
struct KeyedCollections<'a> {
    env: Env,
    handles: Option<KeyedHandles<'a>>,
}

impl<'a> KeyedCollections<'a> {
    const fn new(env: Env) -> Self {
        Self { env, handles: None }
    }

    fn handles(&mut self) -> NapiResult<&KeyedHandles<'a>> {
        if self.handles.is_none() {
            let global = self.env.get_global()?;
            let symbol: Function<Unknown, Unknown> = global.get_named_property("Symbol")?;
            let intrinsics: Object = self.env.run_script(KEYED_INTRINSICS)?;
            self.handles = Some(KeyedHandles {
                to_string_tag: symbol.get_named_property("toStringTag")?,
                size: intrinsics.get_named_property("size")?,
                entries: intrinsics.get_named_property("entries")?,
            });
        }
        self.handles
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("internal error: missing Set/Map handles"))
    }

    fn kind(&mut self, object: Object<'a>) -> NapiResult<Option<KeyedKind>> {
        let tag: Unknown = object.get_property(self.handles()?.to_string_tag)?;
        if tag.get_type()? != ValueType::String {
            return Ok(None);
        }
        let tag: String = FromNapiValue::from_unknown(tag)?;
        Ok(match tag.as_str() {
            "Set" => Some(KeyedKind::Set),
            "Map" => Some(KeyedKind::Map),
            _ => None,
        })
    }

    /// Reads the members of a `Set`, or the flattened keys and values of a `Map`, charging the
    /// budget from the built-in `size` before anything is copied.
    fn entries(
        &mut self,
        object: Object<'a>,
        kind: KeyedKind,
        budget: &mut DumpBudget,
    ) -> NapiResult<Vec<Unknown<'a>>> {
        let is_map = matches!(kind, KeyedKind::Map);
        let handles = self.handles()?;
        let size = handles.size.call(FnArgs::from((is_map, object)))?;
        if !(size.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&size)) {
            return Err(napi::Error::from_reason(format!(
                "cannot serialize to YAML: a {} reports an invalid size",
                kind.name()
            )));
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = size as usize;
        budget
            .charge_nodes(count.saturating_mul(kind.nodes_per_entry()))
            .map_err(limit_error)?;
        let list = handles.entries.call(FnArgs::from((is_map, object, size)))?;
        let len = list.get_array_length()?;
        if len as usize > count.saturating_mul(kind.nodes_per_entry()) {
            return Err(napi::Error::from_reason(format!(
                "cannot serialize to YAML: a {} yielded more entries than its size",
                kind.name()
            )));
        }
        (0..len).map(|i| list.get_element(i)).collect()
    }
}

enum Classified<'a> {
    Scalar(Value),
    Container(OpenContainer<'a>),
}

/// Maps a dump limit violation to a JS error; depth overruns hint at self-reference.
fn limit_error(kind: LimitKind) -> napi::Error {
    napi::Error::from_reason(match kind {
        LimitKind::Depth(_) => format!("cannot serialize to YAML: {kind} (circular reference?)"),
        _ => format!("cannot serialize to YAML: {kind}"),
    })
}

/// Enter one more container level, failing past [`MaxDepth::DEFAULT`].
///
/// The bound also catches self-referential containers, which would recurse forever.
fn enter_container(depth: usize) -> NapiResult<usize> {
    MaxDepth::DEFAULT.descend(depth).map_err(limit_error)
}

/// Convert a scalar, or open a container whose children are `depth` levels deep.
///
/// A node's fixed cost is charged by its parent, before any storage for the children is
/// allocated; the root is free because its output is covered by its children and text.
fn classify<'a>(
    js_value: Unknown<'a>,
    depth: usize,
    budget: &mut DumpBudget,
    keyed: &mut KeyedCollections<'a>,
) -> NapiResult<Classified<'a>> {
    let js_type = js_value.get_type()?;

    match js_type {
        ValueType::Null | ValueType::Undefined => Ok(Classified::Scalar(Value::Null)),

        ValueType::Boolean => {
            let b: bool = FromNapiValue::from_unknown(js_value)?;
            Ok(Classified::Scalar(Value::Bool(b)))
        }

        ValueType::Number => {
            let num: f64 = FromNapiValue::from_unknown(js_value)?;
            Ok(Classified::Scalar(number_to_scalar(num)))
        }

        ValueType::String => {
            let s: String = FromNapiValue::from_unknown(js_value)?;
            budget.charge(s.len()).map_err(limit_error)?;
            Ok(Classified::Scalar(Value::String(s)))
        }

        ValueType::Object => {
            let js_obj: Object = FromNapiValue::from_unknown(js_value)?;
            enter_container(depth)?;

            if js_obj.is_array()? {
                let len: u32 = js_obj.get_array_length()?;
                budget.charge_nodes(len as usize).map_err(limit_error)?;
                let mut children = Vec::with_capacity(len as usize);
                for i in 0..len {
                    children.push(Child {
                        key: None,
                        value: js_obj.get_element(i)?,
                    });
                }
                return Ok(Classified::Container(OpenContainer {
                    children: children.into_iter(),
                    pending_key: None,
                    done: Done::Sequence(Vec::with_capacity(len as usize)),
                }));
            }

            if let Some(kind) = keyed.kind(js_obj)? {
                let entries = keyed.entries(js_obj, kind, budget)?;
                let capacity = entries.len() / kind.nodes_per_entry();
                return Ok(Classified::Container(OpenContainer {
                    children: entries
                        .into_iter()
                        .map(|value| Child { key: None, value })
                        .collect::<Vec<_>>()
                        .into_iter(),
                    pending_key: None,
                    done: match kind {
                        KeyedKind::Set => Done::Set(Set::new()),
                        KeyedKind::Map => Done::Map {
                            entries: Mapping::with_capacity(capacity),
                            key: None,
                        },
                    },
                }));
            }

            let property_names = js_obj.get_property_names()?;
            let len = property_names.get_array_length()?;
            // Each member costs a key node and a value node.
            budget
                .charge_nodes((len as usize).saturating_mul(2))
                .map_err(limit_error)?;
            let mut children = Vec::with_capacity(len as usize);
            for i in 0..len {
                let key: Unknown = property_names.get_element(i)?;
                let key_str: String = FromNapiValue::from_unknown(key)?;
                budget.charge(key_str.len()).map_err(limit_error)?;
                let value: Unknown = js_obj.get_named_property(&key_str)?;
                children.push(Child {
                    key: Some(key_str),
                    value,
                });
            }
            Ok(Classified::Container(OpenContainer {
                children: children.into_iter(),
                pending_key: None,
                done: Done::Mapping(Mapping::with_capacity(len as usize)),
            }))
        }

        _ => Err(napi::Error::from_reason(format!(
            "cannot serialize JavaScript value of type {js_type:?} to YAML"
        ))),
    }
}

/// Integral numbers within the exact `f64` range become integers; the rest stay floats.
fn number_to_scalar(num: f64) -> Value {
    // Safe integer range for f64 is -(2^53) to 2^53
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_992.0; // 2^53
    #[allow(clippy::cast_possible_truncation)]
    if num.fract() == 0.0 && num.is_finite() && num.abs() <= MAX_SAFE_INTEGER {
        return Value::Int(num as i64);
    }
    // Float value (including inf, -inf, nan)
    Value::Float(Float::new(num))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_key_to_string() {
        assert_eq!(
            yaml_key_to_string(&Value::String("test".to_string())).unwrap(),
            "test"
        );
        assert_eq!(yaml_key_to_string(&Value::Int(42)).unwrap(), "42");
        assert_eq!(yaml_key_to_string(&Value::Bool(true)).unwrap(), "true");
        assert_eq!(yaml_key_to_string(&Value::Null).unwrap(), "null");
    }
}
