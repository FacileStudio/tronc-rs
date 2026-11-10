use std::fmt::Debug;

use serde_json::{Map, Value};
use tracing::field::{Field, Visit};

#[derive(Default)]
pub struct Fields {
    pub message: String,
    pub meta: Map<String, Value>,
}

impl Fields {
    fn put(&mut self, field: &Field, value: Value) {
        if field.name() == "message" {
            self.message = value.as_str().map_or_else(|| value.to_string(), str::to_owned);
            return;
        }
        self.meta.insert(field.name().to_owned(), value);
    }
}

impl Visit for Fields {
    fn record_f64(&mut self, field: &Field, value: f64) {
        let value = serde_json::Number::from_f64(value).map_or_else(|| Value::from(value.to_string()), Value::Number);
        self.put(field, value);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.put(field, Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.put(field, Value::from(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.put(field, Value::from(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field, Value::from(value));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.put(field, Value::from(format!("{value:?}")));
    }
}
