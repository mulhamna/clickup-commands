//! Time entries (`/v2/team/{id}/time_entries…`).

use serde_json::{json, Value};

use super::{ClickUpClient, Query};
use crate::{error::Result, model::TimeEntry, task_ref::TaskRef};

#[derive(Debug, Clone, Default)]
pub struct TimeQuery {
    pub start: Option<i64>,
    pub end: Option<i64>,
    /// User ids; workspace owners/admins only.
    pub assignees: Vec<String>,
    pub space_id: Option<String>,
    pub folder_id: Option<String>,
    pub list_id: Option<String>,
    pub task: Option<TaskRef>,
    pub billable: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct NewTimeEntry {
    pub task: Option<TaskRef>,
    pub start: i64,
    pub duration: i64,
    pub description: Option<String>,
    pub billable: Option<bool>,
}

fn data<T: serde::de::DeserializeOwned>(v: Value) -> Result<T> {
    let inner = match v {
        Value::Object(mut m) if m.contains_key("data") => m.remove("data").unwrap_or(Value::Null),
        other => other,
    };
    Ok(serde_json::from_value(inner)?)
}

impl ClickUpClient {
    fn time_path(&self, suffix: &str) -> Result<String> {
        Ok(format!(
            "/v2/team/{}/time_entries{suffix}",
            self.workspace_id()?
        ))
    }

    /// `GET /v2/team/{id}/time_entries` — defaults to the last 30 days for
    /// the authenticated user.
    pub async fn time_entries(&self, query: &TimeQuery) -> Result<Vec<TimeEntry>> {
        let mut q = Query::new()
            .push_opt("start_date", query.start)
            .push_opt("end_date", query.end)
            .push_opt("space_id", query.space_id.as_ref())
            .push_opt("folder_id", query.folder_id.as_ref())
            .push_opt("list_id", query.list_id.as_ref())
            .push_opt("is_billable", query.billable)
            .push("include_location_names", "true");
        if !query.assignees.is_empty() {
            q = q.push("assignee", query.assignees.join(","));
        }
        if let Some(t) = &query.task {
            q = q.push("task_id", t.path_id()).extend(self.task_query(t)?);
        }
        let v: Value = self.get(&self.time_path("")?, &q).await?;
        data(v)
    }

    /// `POST /v2/team/{id}/time_entries`
    pub async fn add_time_entry(&self, entry: &NewTimeEntry) -> Result<TimeEntry> {
        let mut body = json!({ "start": entry.start, "duration": entry.duration });
        if let Some(d) = &entry.description {
            body["description"] = json!(d);
        }
        if let Some(b) = entry.billable {
            body["billable"] = json!(b);
        }
        let mut q = Query::new();
        if let Some(t) = &entry.task {
            body["tid"] = json!(t.path_id());
            q = self.task_query(t)?;
        }
        let v: Value = self.post(&self.time_path("")?, &q, body).await?;
        data(v)
    }

    /// `POST /v2/team/{id}/time_entries/start`
    pub async fn start_timer(
        &self,
        task: Option<&TaskRef>,
        description: Option<&str>,
    ) -> Result<TimeEntry> {
        let mut body = json!({});
        if let Some(d) = description {
            body["description"] = json!(d);
        }
        let mut q = Query::new();
        if let Some(t) = task {
            body["tid"] = json!(t.path_id());
            q = self.task_query(t)?;
        }
        let v: Value = self.post(&self.time_path("/start")?, &q, body).await?;
        data(v)
    }

    /// `POST /v2/team/{id}/time_entries/stop`
    pub async fn stop_timer(&self) -> Result<TimeEntry> {
        let v: Value = self
            .post(&self.time_path("/stop")?, &Query::new(), json!({}))
            .await?;
        data(v)
    }

    /// `GET /v2/team/{id}/time_entries/current` — `None` when no timer runs.
    pub async fn current_timer(&self) -> Result<Option<TimeEntry>> {
        let v: Value = self
            .get(&self.time_path("/current")?, &Query::new())
            .await?;
        let entry: Option<Value> = data(v)?;
        match entry {
            Some(e) if e.get("id").is_some() => Ok(Some(serde_json::from_value(e)?)),
            _ => Ok(None),
        }
    }

    /// `DELETE /v2/team/{id}/time_entries/{entry}`
    pub async fn delete_time_entry(&self, entry_id: &str) -> Result<()> {
        self.delete(&self.time_path(&format!("/{entry_id}"))?, &Query::new())
            .await?;
        Ok(())
    }
}
