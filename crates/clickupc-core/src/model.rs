//! Response models. Tolerant by design: unknown fields are ignored, most
//! fields are optional, and ids / timestamps go through `de` helpers.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::de;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Ref {
    #[serde(default, deserialize_with = "de::id_string")]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct User {
    #[serde(default, deserialize_with = "de::id_string")]
    pub id: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub initials: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default, rename = "profilePicture", alias = "profile_picture")]
    pub profile_picture: Option<String>,
}

impl User {
    pub fn display(&self) -> String {
        self.username
            .clone()
            .or_else(|| self.email.clone())
            .unwrap_or_else(|| self.id.clone())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Member {
    pub user: User,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Workspace {
    #[serde(deserialize_with = "de::id_string")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default, deserialize_with = "de::null_default")]
    pub members: Vec<Member>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Status {
    #[serde(default, deserialize_with = "de::opt_id_string")]
    pub id: Option<String>,
    pub status: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub orderindex: Option<i64>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
}

impl Status {
    pub fn is_closed(&self) -> bool {
        matches!(self.kind.as_deref(), Some("closed") | Some("done"))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Space {
    #[serde(deserialize_with = "de::id_string")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default, deserialize_with = "de::null_default")]
    pub statuses: Vec<Status>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct List {
    #[serde(deserialize_with = "de::id_string")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub folder: Option<Ref>,
    #[serde(default)]
    pub space: Option<Ref>,
    #[serde(default, deserialize_with = "de::null_default")]
    pub statuses: Vec<Status>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub task_count: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Folder {
    #[serde(deserialize_with = "de::id_string")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub space: Option<Ref>,
    #[serde(default, deserialize_with = "de::null_default")]
    pub lists: Vec<List>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Priority {
    #[serde(default, deserialize_with = "de::opt_id_string")]
    pub id: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Tag {
    pub name: String,
    #[serde(default)]
    pub tag_fg: Option<String>,
    #[serde(default)]
    pub tag_bg: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChecklistItem {
    #[serde(default, deserialize_with = "de::id_string")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub resolved: bool,
    #[serde(default)]
    pub assignee: Option<User>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Checklist {
    #[serde(default, deserialize_with = "de::id_string")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, deserialize_with = "de::null_default")]
    pub items: Vec<ChecklistItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FieldOption {
    #[serde(default, deserialize_with = "de::id_string")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub orderindex: Option<i64>,
}

/// A custom field definition (from Get List Custom Fields) or a field on a
/// task (same shape plus `value`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomField {
    #[serde(deserialize_with = "de::id_string")]
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub type_config: Value,
    #[serde(default)]
    pub required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

impl CustomField {
    /// Dropdown / label options. Labels use `label` instead of `name`.
    pub fn options(&self) -> Vec<FieldOption> {
        self.type_config
            .get("options")
            .and_then(Value::as_array)
            .map(|opts| {
                opts.iter()
                    .map(|o| FieldOption {
                        id: o
                            .get("id")
                            .map(|v| v.as_str().map(str::to_string).unwrap_or(v.to_string()))
                            .unwrap_or_default(),
                        name: o
                            .get("name")
                            .or_else(|| o.get("label"))
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        orderindex: o.get("orderindex").and_then(|v| {
                            v.as_i64()
                                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                        }),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Human rendering of `value`; `None` when the field is empty.
    pub fn display_value(&self, tz: Option<&str>) -> Option<String> {
        let value = self.value.as_ref()?;
        if value.is_null() {
            return None;
        }
        let options = self.options();
        let option_name = |v: &Value| -> String {
            let by_id = v.as_str().and_then(|s| options.iter().find(|o| o.id == s));
            let by_index = v
                .as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                .and_then(|i| options.iter().find(|o| o.orderindex == Some(i)));
            by_id
                .or(by_index)
                .map(|o| o.name.clone())
                .unwrap_or_else(|| plain(v))
        };
        let rendered = match self.kind.as_str() {
            "drop_down" => option_name(value),
            "labels" => value
                .as_array()
                .map(|a| a.iter().map(option_name).collect::<Vec<_>>().join(", "))
                .unwrap_or_else(|| plain(value)),
            "date" => value
                .as_i64()
                .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
                .map(|ms| crate::time::format_ms(ms, tz))
                .unwrap_or_else(|| plain(value)),
            "users" | "tasks" | "list_relationship" => value
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|u| {
                            ["username", "name", "email", "id"]
                                .iter()
                                .find_map(|k| u.get(*k).filter(|v| !v.is_null()).map(plain))
                                .unwrap_or_else(|| plain(u))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_else(|| plain(value)),
            "location" => value
                .get("formatted_address")
                .map(plain)
                .unwrap_or_else(|| plain(value)),
            "manual_progress" | "automatic_progress" => value
                .get("percent_completed")
                .or_else(|| value.get("current"))
                .map(|v| format!("{}%", plain(v)))
                .unwrap_or_else(|| plain(value)),
            _ => plain(value),
        };
        if rendered.is_empty() {
            None
        } else {
            Some(rendered)
        }
    }
}

fn plain(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Task {
    #[serde(deserialize_with = "de::id_string")]
    pub id: String,
    #[serde(default)]
    pub custom_id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub text_content: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub markdown_description: Option<String>,
    #[serde(default)]
    pub status: Status,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub date_created: Option<i64>,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub date_updated: Option<i64>,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub date_closed: Option<i64>,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub date_done: Option<i64>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub creator: Option<User>,
    #[serde(default, deserialize_with = "de::null_default")]
    pub assignees: Vec<User>,
    #[serde(default, deserialize_with = "de::null_default")]
    pub watchers: Vec<User>,
    #[serde(default, deserialize_with = "de::null_default")]
    pub checklists: Vec<Checklist>,
    #[serde(default, deserialize_with = "de::null_default")]
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub priority: Option<Priority>,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub due_date: Option<i64>,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub start_date: Option<i64>,
    #[serde(default, deserialize_with = "de::opt_f64")]
    pub points: Option<f64>,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub time_estimate: Option<i64>,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub time_spent: Option<i64>,
    #[serde(default, deserialize_with = "de::null_default")]
    pub custom_fields: Vec<CustomField>,
    #[serde(default, deserialize_with = "de::null_default")]
    pub dependencies: Vec<Value>,
    #[serde(default, deserialize_with = "de::null_default")]
    pub linked_tasks: Vec<Value>,
    #[serde(default, deserialize_with = "de::opt_id_string")]
    pub team_id: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub list: Option<Ref>,
    #[serde(default)]
    pub folder: Option<Ref>,
    #[serde(default)]
    pub space: Option<Ref>,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub custom_item_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtasks: Option<Vec<Task>>,
    #[serde(default, deserialize_with = "de::null_default")]
    pub attachments: Vec<Value>,
}

impl Task {
    /// Custom id when present (what people type), else the task id.
    pub fn display_id(&self) -> &str {
        self.custom_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.id)
    }

    pub fn priority_name(&self) -> Option<&str> {
        self.priority.as_ref()?.priority.as_deref()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Comment {
    #[serde(deserialize_with = "de::id_string")]
    pub id: String,
    #[serde(default)]
    pub comment_text: String,
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub date: Option<i64>,
    #[serde(default)]
    pub resolved: bool,
    #[serde(default)]
    pub assignee: Option<User>,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub reply_count: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimeEntry {
    #[serde(deserialize_with = "de::id_string")]
    pub id: String,
    #[serde(default)]
    pub task: Option<Value>,
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default)]
    pub billable: bool,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub start: Option<i64>,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub end: Option<i64>,
    /// Milliseconds; negative while the timer is running.
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub duration: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "de::null_default")]
    pub tags: Vec<Value>,
    /// Present when requested with `include_location_names=true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_location: Option<Value>,
}

impl TimeEntry {
    pub fn is_running(&self) -> bool {
        self.duration.is_some_and(|d| d < 0)
    }

    pub fn task_id(&self) -> Option<String> {
        let t = self.task.as_ref()?;
        t.get("custom_id")
            .and_then(Value::as_str)
            .or_else(|| t.get("id").and_then(Value::as_str))
            .map(str::to_string)
    }

    pub fn task_name(&self) -> Option<String> {
        self.task
            .as_ref()?
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    /// Effective duration; running entries count until `now_ms`.
    pub fn effective_ms(&self, now_ms: i64) -> i64 {
        match (self.duration, self.start) {
            (Some(d), Some(start)) if d < 0 => now_ms - start,
            (Some(d), _) => d,
            _ => 0,
        }
    }
}

/// Priority: 1 urgent, 2 high, 3 normal, 4 low. `Ok(None)` = clear.
pub fn parse_priority(input: &str) -> crate::Result<Option<u8>> {
    match input.trim().to_ascii_lowercase().as_str() {
        "1" | "urgent" => Ok(Some(1)),
        "2" | "high" => Ok(Some(2)),
        "3" | "normal" => Ok(Some(3)),
        "4" | "low" => Ok(Some(4)),
        "none" | "null" | "" => Ok(None),
        other => Err(crate::ClickUpError::Invalid(format!(
            "unknown priority '{other}' (use urgent, high, normal, low or none)"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn task_parses_string_timestamps_and_nulls() {
        let t: Task = serde_json::from_value(json!({
            "id": "abc", "custom_id": null, "name": "x",
            "status": {"status": "open", "type": "open", "orderindex": 0},
            "date_created": "1700000000000", "due_date": null, "points": "3",
            "assignees": [{"id": 1, "username": "a"}], "tags": null,
            "priority": {"id": "2", "priority": "high"},
            "custom_fields": [{"id": "f", "name": "F", "type": "text", "value": "v"}]
        }))
        .unwrap();
        assert_eq!(t.date_created, Some(1_700_000_000_000));
        assert_eq!(t.points, Some(3.0));
        assert_eq!(t.assignees[0].id, "1");
        assert_eq!(t.display_id(), "abc");
        assert_eq!(t.priority_name(), Some("high"));
    }

    #[test]
    fn dropdown_and_labels_display_names() {
        let mut f: CustomField = serde_json::from_value(json!({
            "id": "f", "name": "Severity", "type": "drop_down",
            "type_config": {"options": [
                {"id": "o1", "name": "Low", "orderindex": 0},
                {"id": "o2", "name": "High", "orderindex": 1}
            ]},
            "value": 1
        }))
        .unwrap();
        assert_eq!(f.display_value(None).as_deref(), Some("High"));
        f.value = Some(json!("o1"));
        assert_eq!(f.display_value(None).as_deref(), Some("Low"));

        let l: CustomField = serde_json::from_value(json!({
            "id": "l", "name": "Areas", "type": "labels",
            "type_config": {"options": [{"id": "a", "label": "API"}, {"id": "b", "label": "UI"}]},
            "value": ["a", "b"]
        }))
        .unwrap();
        assert_eq!(l.display_value(None).as_deref(), Some("API, UI"));
    }

    #[test]
    fn priorities() {
        assert_eq!(parse_priority("High").unwrap(), Some(2));
        assert_eq!(parse_priority("none").unwrap(), None);
        assert!(parse_priority("meh").is_err());
    }
}
