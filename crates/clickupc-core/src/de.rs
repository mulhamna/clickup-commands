//! Lenient deserializers: ClickUp returns ids and unix-ms timestamps as either
//! numbers or strings, and sometimes `null` where an array is expected.

use serde::{de::Error, Deserialize, Deserializer};
use serde_json::Value;

fn value_to_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => {
            let s = s.trim();
            s.parse::<i64>()
                .ok()
                .or_else(|| s.parse::<f64>().ok().map(|f| f as i64))
        }
        _ => None,
    }
}

pub fn opt_i64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    let v = Option::<Value>::deserialize(d)?;
    Ok(v.as_ref().and_then(value_to_i64))
}

pub fn opt_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    let v = Option::<Value>::deserialize(d)?;
    Ok(match v {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    })
}

pub fn id_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    match Value::deserialize(d)? {
        Value::String(s) => Ok(s),
        Value::Number(n) => Ok(n.to_string()),
        Value::Null => Ok(String::new()),
        other => Err(D::Error::custom(format!("expected id, got {other}"))),
    }
}

pub fn opt_id_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<Value>::deserialize(d)? {
        Some(Value::String(s)) if !s.is_empty() => Some(s),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    })
}

/// `null` → `T::default()`.
pub fn null_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct T {
        #[serde(default, deserialize_with = "opt_i64")]
        ms: Option<i64>,
        #[serde(default, deserialize_with = "id_string")]
        id: String,
        #[serde(default, deserialize_with = "null_default")]
        tags: Vec<String>,
    }

    #[test]
    fn lenient_shapes() {
        let t: T = serde_json::from_str(r#"{"ms":"1700000000000","id":42,"tags":null}"#).unwrap();
        assert_eq!(t.ms, Some(1_700_000_000_000));
        assert_eq!(t.id, "42");
        assert!(t.tags.is_empty());
        let t: T = serde_json::from_str(r#"{"ms":null,"id":"abc"}"#).unwrap();
        assert_eq!(t.ms, None);
        assert_eq!(t.id, "abc");
        let t: T = serde_json::from_str(r#"{"ms":"","id":"x"}"#).unwrap();
        assert_eq!(t.ms, None);
    }
}
