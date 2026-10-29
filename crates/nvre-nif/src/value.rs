//! What a decoded block holds: its fields, and what they can be.

use serde_json::{Map, Value as Json};

/// A value read out of a NIF.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A number — every integer and float in a NIF is one of these.
    Num(f64),
    /// A 64-bit integer that exceeds f64's 53-bit mantissa precision.
    Int64(u64),
    /// Text, in Windows-1252.
    Str(String),
    /// A flag.
    Bool(bool),
    /// A block index, -1 for none.
    Ref(i32),
    /// Raw bytes.
    Bytes(Vec<u8>),
    /// A list of values.
    List(Vec<Value>),
    /// A struct: its fields, by name.
    Struct(Struct),
}

impl Value {
    /// The value as a number, if it is one.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            Value::Int64(n) => Some(*n as f64),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    /// The value as a whole number, if it is one.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Num(n) => Some(*n as i64),
            Value::Int64(n) => Some(*n as i64),
            _ => self.as_number().map(|n| n as i64),
        }
    }

    /// The value as text, if it is.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The value as a flag, if it is one.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Num(n) => Some(*n != 0.0),
            _ => None,
        }
    }

    /// The value as a block index, if it is one.
    pub fn as_ref(&self) -> Option<i32> {
        match self {
            Value::Ref(r) => Some(*r),
            Value::Num(n) => Some(*n as i32),
            _ => None,
        }
    }

    /// The value as a list, if it is one.
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(v) => Some(v),
            _ => None,
        }
    }

    /// The value as a struct, if it is one.
    pub fn as_struct(&self) -> Option<&Struct> {
        match self {
            Value::Struct(s) => Some(s),
            _ => None,
        }
    }

    /// The value as raw bytes, if it is.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(b) => Some(b),
            _ => None,
        }
    }

    /// Whether the value is "true" for a condition: a non-zero number, or a
    /// true flag.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0,
            Value::Int64(n) => *n != 0,
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Bytes(b) => !b.is_empty(),
            Value::Struct(s) => !s.fields.is_empty(),
            // A block index: -1 is none, so it is false.
            Value::Ref(r) => *r >= 0,
        }
    }

    /// The value as JSON, for the editor's inspectors.
    pub fn to_json(&self) -> Json {
        match self {
            Value::Num(n) => {
                if n.fract() == 0.0 && *n >= (i64::MIN as f64) && *n <= (i64::MAX as f64) {
                    Json::from(*n as i64)
                } else {
                    Json::from(*n)
                }
            }
            Value::Int64(n) => Json::from(*n),
            Value::Str(s) => Json::from(s.clone()),
            Value::Bool(b) => Json::from(*b),
            Value::Ref(r) => Json::from(*r),
            Value::Bytes(b) => Json::from(crate::codec::to_hex(b)),
            Value::List(l) => Json::Array(l.iter().map(|v| v.to_json()).collect()),
            Value::Struct(s) => s.to_json(),
        }
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Value {
        Value::Num(n)
    }
}
impl From<i64> for Value {
    fn from(n: i64) -> Value {
        Value::Num(n as f64)
    }
}
impl From<i32> for Value {
    fn from(n: i32) -> Value {
        Value::Num(n as f64)
    }
}
impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value::Bool(b)
    }
}
impl From<&str> for Value {
    fn from(s: &str) -> Value {
        Value::Str(s.to_string())
    }
}
impl From<String> for Value {
    fn from(s: String) -> Value {
        Value::Str(s)
    }
}

/// A struct: a block's or a struct type's fields, by name.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Struct {
    /// The fields, in the order they were read.
    pub fields: Vec<(String, Value)>,
}

impl Struct {
    /// An empty struct.
    pub fn new() -> Struct {
        Struct::default()
    }

    /// The value of a field.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// The value of a field, as a number.
    pub fn num(&self, name: &str) -> Option<f64> {
        self.get(name).and_then(|v| v.as_number())
    }

    /// The value of a field, as a whole number.
    pub fn int(&self, name: &str) -> Option<i64> {
        self.get(name).and_then(|v| v.as_int())
    }

    /// The value of a field, as text.
    pub fn text(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(|v| v.as_str())
    }

    /// The value of a field, as a flag.
    pub fn flag(&self, name: &str) -> Option<bool> {
        self.get(name).and_then(|v| v.as_bool())
    }

    /// The value of a field, as a block index.
    pub fn reference(&self, name: &str) -> Option<i32> {
        self.get(name).and_then(|v| v.as_ref())
    }

    /// The value of a field, as a list.
    pub fn list(&self, name: &str) -> Option<&[Value]> {
        self.get(name).and_then(|v| v.as_list())
    }

    /// The value of a field, as a struct.
    pub fn struct_at(&self, name: &str) -> Option<&Struct> {
        self.get(name).and_then(|v| v.as_struct())
    }

    /// The block indices a field holds, if it holds a list of them.
    pub fn refs(&self, name: &str) -> Option<Vec<i32>> {
        self.list(name)
            .map(|l| l.iter().filter_map(|v| v.as_ref()).collect())
    }

    /// The block index a field holds, if it holds one.
    pub fn ref_at(&self, name: &str) -> Option<i32> {
        self.reference(name)
    }

    /// Add a field.
    pub fn set(&mut self, name: &str, value: Value) {
        self.fields.push((name.to_string(), value));
    }

    /// The struct as JSON, for the editor's inspectors.
    pub fn to_json(&self) -> Json {
        let mut m = Map::new();
        for (k, v) in &self.fields {
            m.insert(k.clone(), v.to_json());
        }
        Json::Object(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_is_found_by_name() {
        let mut s = Struct::new();
        s.set("count", Value::Num(3.0));
        s.set("name", Value::Str("hello".into()));
        assert_eq!(s.int("count"), Some(3));
        assert_eq!(s.text("name"), Some("hello"));
        assert_eq!(s.int("missing"), None);
    }

    #[test]
    fn a_number_is_a_whole_number_when_it_can_be() {
        assert_eq!(Value::Num(3.0).to_json(), Json::from(3));
        assert_eq!(Value::Num(3.5).to_json(), Json::from(3.5));
    }

    #[test]
    fn truthiness_follows_the_value() {
        assert!(Value::Num(1.0).truthy());
        assert!(!Value::Num(0.0).truthy());
        assert!(Value::Bool(true).truthy());
        assert!(!Value::Str(String::new()).truthy());
    }
}
