use serde::de::value::StringDeserializer;
use serde::de::{DeserializeSeed, IntoDeserializer, MapAccess, SeqAccess};
use serde_json::{Error, Map, Value};

use super::fold::Folded;

pub struct Entries {
    iter: serde_json::map::IntoIter,
    value: Option<Value>,
}

impl Entries {
    pub fn new(map: Map<String, Value>) -> Self {
        Self { iter: map.into_iter(), value: None }
    }
}

impl<'de> MapAccess<'de> for Entries {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, Error> {
        let Some((key, value)) = self.iter.next() else { return Ok(None) };
        self.value = Some(value);
        let key: StringDeserializer<Error> = key.into_deserializer();
        seed.deserialize(key).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        seed.deserialize(Folded(self.value.take().unwrap_or(Value::Null)))
    }
}

pub struct Items(pub std::vec::IntoIter<Value>);

impl<'de> SeqAccess<'de> for Items {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>, Error> {
        self.0.next().map(|value| seed.deserialize(Folded(value))).transpose()
    }
}
