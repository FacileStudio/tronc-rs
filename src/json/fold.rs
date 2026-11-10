use serde::de::{self, Visitor};
use serde::forward_to_deserialize_any;
use serde_json::{Error, Map, Value};

use super::fold_access::{Entries, Items};

/// A JSON value that matches object keys to struct fields the way Go's `encoding/json` does:
/// an exact match first, then the first case-insensitive one.
pub struct Folded(pub Value);

fn fold_keys(map: Map<String, Value>, fields: &[&str]) -> Map<String, Value> {
    map.into_iter()
        .map(|(key, value)| {
            let field = if fields.contains(&key.as_str()) {
                None
            } else {
                fields.iter().find(|field| field.eq_ignore_ascii_case(&key))
            };
            (field.map_or(key, |field| (*field).to_owned()), value)
        })
        .collect()
}

impl<'de> de::Deserializer<'de> for Folded {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Object(map) => visitor.visit_map(Entries::new(map)),
            Value::Array(items) => visitor.visit_seq(Items(items.into_iter())),
            other => de::Deserializer::deserialize_any(other, visitor),
        }
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.0 {
            Value::Object(map) => Folded(Value::Object(fold_keys(map, fields))).deserialize_any(visitor),
            other => other.deserialize_struct(name, fields, visitor),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Null => visitor.visit_none(),
            other => visitor.visit_some(Folded(other)),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _name: &'static str, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.0.deserialize_enum(name, variants, visitor)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf unit
        unit_struct seq tuple tuple_struct map identifier ignored_any
    }
}
