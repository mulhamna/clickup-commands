//! Custom fields: list per list, set / clear on a task, and conversion of
//! human input (option names, dates, numbers) into API values.

use serde::Deserialize;
use serde_json::{json, Value};

use super::{ClickUpClient, Query};
use crate::{
    error::{ClickUpError, Result},
    model::CustomField,
    resolve::suggest,
    task_ref::TaskRef,
};

#[derive(Deserialize)]
struct FieldsResp {
    #[serde(default)]
    fields: Vec<CustomField>,
}

/// A value ready for Set Custom Field Value.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldValue {
    pub value: Value,
    pub value_options: Option<Value>,
}

impl ClickUpClient {
    /// `GET /v2/list/{id}/field`
    pub async fn list_fields(&self, list_id: &str) -> Result<Vec<CustomField>> {
        Ok(self
            .get::<FieldsResp>(&format!("/v2/list/{list_id}/field"), &Query::new())
            .await?
            .fields)
    }

    /// `POST /v2/task/{id}/field/{field_id}`
    pub async fn set_field(
        &self,
        task: &TaskRef,
        field_id: &str,
        value: &FieldValue,
    ) -> Result<()> {
        let mut body = json!({ "value": value.value });
        if let Some(opts) = &value.value_options {
            body["value_options"] = opts.clone();
        }
        self.post::<Value>(
            &format!("/v2/task/{}/field/{field_id}", task.path_id()),
            &self.task_query(task)?,
            body,
        )
        .await?;
        Ok(())
    }

    /// `DELETE /v2/task/{id}/field/{field_id}`
    pub async fn clear_field(&self, task: &TaskRef, field_id: &str) -> Result<()> {
        self.delete(
            &format!("/v2/task/{}/field/{field_id}", task.path_id()),
            &self.task_query(task)?,
        )
        .await?;
        Ok(())
    }
}

fn option_id(field: &CustomField, input: &str) -> Result<String> {
    let options = field.options();
    let input = input.trim();
    if let Some(o) = options
        .iter()
        .find(|o| o.id == input || o.name.eq_ignore_ascii_case(input))
    {
        return Ok(o.id.clone());
    }
    let names: Vec<String> = options.iter().map(|o| o.name.clone()).collect();
    let mut suggestions = suggest(input, &names);
    if suggestions.is_empty() {
        suggestions = names;
    }
    Err(ClickUpError::resolve("option", input, suggestions))
}

fn parse_bool(raw: &str) -> Result<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "y" | "1" | "on" | "checked" | "x" => Ok(true),
        "false" | "no" | "n" | "0" | "off" | "unchecked" | "" => Ok(false),
        other => Err(ClickUpError::Invalid(format!(
            "'{other}' is not a checkbox value (use true/false)"
        ))),
    }
}

fn parse_number(raw: &str) -> Result<Value> {
    let cleaned: String = raw
        .trim()
        .chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-'))
        .collect();
    let n: f64 = cleaned
        .parse()
        .map_err(|_| ClickUpError::Invalid(format!("'{raw}' is not a number")))?;
    Ok(if n.fract() == 0.0 && n.abs() < 1e15 {
        json!(n as i64)
    } else {
        json!(n)
    })
}

/// Convert human input to an API value for every type that doesn't need a
/// network lookup. `users` / `tasks` take already-resolved ids via
/// [`relation_value`].
pub fn convert_field_value(field: &CustomField, raw: &str, tz: Option<&str>) -> Result<FieldValue> {
    let plain = |v: Value| FieldValue {
        value: v,
        value_options: None,
    };
    let raw_t = raw.trim();
    Ok(match field.kind.as_str() {
        "drop_down" => plain(json!(option_id(field, raw_t)?)),
        "labels" => {
            let ids = raw_t
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| option_id(field, s))
                .collect::<Result<Vec<_>>>()?;
            plain(json!(ids))
        }
        "date" => {
            let ms = crate::time::parse_when(raw_t, tz)?;
            let has_time = crate::time::format_ms(ms, tz).contains(':');
            FieldValue {
                value: json!(ms),
                value_options: has_time.then(|| json!({ "time": true })),
            }
        }
        "number" | "currency" | "emoji" => plain(parse_number(raw_t)?),
        "checkbox" => plain(json!(parse_bool(raw_t)?)),
        "manual_progress" => plain(json!({ "current": parse_number(raw_t)? })),
        "button" => plain(json!(true)),
        "users" | "tasks" | "list_relationship" => {
            return Err(ClickUpError::Invalid(format!(
                "field '{}' ({}) needs resolved ids; use relation_value",
                field.name, field.kind
            )))
        }
        "formula" | "automatic_progress" | "rollup" => {
            return Err(ClickUpError::Invalid(format!(
                "field '{}' is computed by ClickUp ({}) and cannot be set",
                field.name, field.kind
            )))
        }
        "location" => {
            return Err(ClickUpError::Invalid(
                "location fields are not supported yet; use `clickupc api`".into(),
            ))
        }
        // text, short_text, url, email, phone, and anything new.
        _ => plain(json!(raw_t)),
    })
}

/// `{ "add": [...], "rem": [...] }` for people / task relationship fields.
pub fn relation_value(add: Vec<Value>, rem: Vec<Value>) -> FieldValue {
    FieldValue {
        value: json!({ "add": add, "rem": rem }),
        value_options: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(kind: &str, config: Value) -> CustomField {
        CustomField {
            id: "f".into(),
            name: "F".into(),
            kind: kind.into(),
            type_config: config,
            required: None,
            value: None,
        }
    }

    fn opts() -> Value {
        json!({ "options": [
            { "id": "u-low", "name": "Low", "orderindex": 0 },
            { "id": "u-high", "name": "High", "orderindex": 1 }
        ]})
    }

    fn conv(kind: &str, cfg: Value, raw: &str) -> Result<FieldValue> {
        convert_field_value(&field(kind, cfg), raw, Some("UTC"))
    }

    #[test]
    fn dropdown_by_name_or_id() {
        assert_eq!(
            conv("drop_down", opts(), "high").unwrap().value,
            json!("u-high")
        );
        assert_eq!(
            conv("drop_down", opts(), "u-low").unwrap().value,
            json!("u-low")
        );
        match conv("drop_down", opts(), "Hgh") {
            Err(ClickUpError::Resolve { suggestions, .. }) => {
                assert_eq!(suggestions, vec!["High".to_string()])
            }
            other => panic!("{other:?}"),
        }
        match conv("drop_down", opts(), "zzz") {
            Err(ClickUpError::Resolve { suggestions, .. }) => assert_eq!(suggestions.len(), 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn labels() {
        let cfg =
            json!({ "options": [{ "id": "a", "label": "API" }, { "id": "b", "label": "UI" }] });
        assert_eq!(
            conv("labels", cfg, "api, UI").unwrap().value,
            json!(["a", "b"])
        );
    }

    #[test]
    fn dates() {
        let v = conv("date", Value::Null, "2026-10-20").unwrap();
        assert_eq!(v.value, json!(1_792_454_400_000i64));
        assert_eq!(v.value_options, None);
        let v = conv("date", Value::Null, "2026-10-20 14:00").unwrap();
        assert_eq!(v.value_options, Some(json!({ "time": true })));
    }

    #[test]
    fn numbers_money_checkbox_progress() {
        assert_eq!(conv("number", Value::Null, "3").unwrap().value, json!(3));
        assert_eq!(
            conv("number", Value::Null, "2.5").unwrap().value,
            json!(2.5)
        );
        assert_eq!(
            conv("currency", Value::Null, "$1,200").unwrap().value,
            json!(1200)
        );
        assert_eq!(conv("emoji", Value::Null, "4").unwrap().value, json!(4));
        assert!(conv("number", Value::Null, "abc").is_err());
        assert_eq!(
            conv("checkbox", Value::Null, "yes").unwrap().value,
            json!(true)
        );
        assert_eq!(
            conv("checkbox", Value::Null, "false").unwrap().value,
            json!(false)
        );
        assert!(conv("checkbox", Value::Null, "maybe").is_err());
        assert_eq!(
            conv("manual_progress", Value::Null, "40").unwrap().value,
            json!({ "current": 40 })
        );
    }

    #[test]
    fn text_like_and_unsupported() {
        for kind in ["text", "short_text", "url", "email", "phone"] {
            assert_eq!(conv(kind, Value::Null, " hi ").unwrap().value, json!("hi"));
        }
        assert!(conv("formula", Value::Null, "1").is_err());
        assert!(conv("users", Value::Null, "me").is_err());
        assert_eq!(
            relation_value(vec![json!(1)], vec![]).value,
            json!({ "add": [1], "rem": [] })
        );
    }
}
