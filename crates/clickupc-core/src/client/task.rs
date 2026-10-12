//! Task CRUD, search, move, TIML, merge, time in status.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::{ClickUpClient, Query};
use crate::{
    error::{ClickUpError, Result},
    filter::TaskFilter,
    model::Task,
    task_ref::TaskRef,
};

pub const PAGE_SIZE: usize = 100;
pub const MAX_PAGES: usize = 500;

#[derive(Debug, Clone, Default, Serialize)]
pub struct CreateTask {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown_content: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub assignees: Vec<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date_time: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date_time: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_estimate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_item_id: Option<i64>,
    /// `[{ "id": field_id, "value": … }]`
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub custom_fields: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_required_custom_fields: Option<bool>,
}

/// `None` = leave unchanged; `Some(None)` = clear (sent as `null`).
#[derive(Debug, Clone, Default)]
pub struct UpdateTask {
    pub name: Option<String>,
    pub markdown_content: Option<String>,
    pub status: Option<String>,
    pub priority: Option<Option<u8>>,
    pub due_date: Option<Option<i64>>,
    pub due_date_time: Option<bool>,
    pub start_date: Option<Option<i64>>,
    pub start_date_time: Option<bool>,
    pub time_estimate: Option<Option<i64>>,
    pub points: Option<f64>,
    pub parent: Option<String>,
    pub custom_item_id: Option<i64>,
    pub archived: Option<bool>,
    pub assignees_add: Vec<i64>,
    pub assignees_rem: Vec<i64>,
    pub watchers_add: Vec<i64>,
    pub watchers_rem: Vec<i64>,
}

impl UpdateTask {
    pub fn is_empty(&self) -> bool {
        self.body().as_object().is_none_or(Map::is_empty)
    }

    pub fn body(&self) -> Value {
        let mut m = Map::new();
        let mut put = |k: &str, v: Value| {
            m.insert(k.to_string(), v);
        };
        if let Some(v) = &self.name {
            put("name", json!(v));
        }
        if let Some(v) = &self.markdown_content {
            put("markdown_content", json!(v));
        }
        if let Some(v) = &self.status {
            put("status", json!(v));
        }
        if let Some(v) = self.priority {
            put("priority", json!(v));
        }
        if let Some(v) = self.due_date {
            put("due_date", json!(v));
        }
        if let Some(v) = self.due_date_time {
            put("due_date_time", json!(v));
        }
        if let Some(v) = self.start_date {
            put("start_date", json!(v));
        }
        if let Some(v) = self.start_date_time {
            put("start_date_time", json!(v));
        }
        if let Some(v) = self.time_estimate {
            put("time_estimate", json!(v));
        }
        if let Some(v) = self.points {
            put("points", json!(v));
        }
        if let Some(v) = &self.parent {
            put("parent", json!(v));
        }
        if let Some(v) = self.custom_item_id {
            put("custom_item_id", json!(v));
        }
        if let Some(v) = self.archived {
            put("archived", json!(v));
        }
        // ClickUp's schema marks both `add` and `rem` required.
        if !self.assignees_add.is_empty() || !self.assignees_rem.is_empty() {
            put(
                "assignees",
                json!({ "add": self.assignees_add, "rem": self.assignees_rem }),
            );
        }
        if !self.watchers_add.is_empty() || !self.watchers_rem.is_empty() {
            put(
                "watchers",
                json!({ "add": self.watchers_add, "rem": self.watchers_rem }),
            );
        }
        Value::Object(m)
    }
}

#[derive(Debug, Clone, Default)]
pub struct MoveTask {
    pub move_custom_fields: Option<bool>,
    /// `(source_status_id, destination_status_id)`
    pub status_mappings: Vec<(String, String)>,
}

#[derive(Deserialize)]
struct TasksPage {
    #[serde(default)]
    tasks: Vec<Task>,
    #[serde(default)]
    last_page: Option<bool>,
}

impl ClickUpClient {
    /// `custom_task_ids` + `team_id` when the ref is a custom id.
    pub fn task_query(&self, task: &TaskRef) -> Result<Query> {
        if task.is_custom() {
            Ok(task.query(self.workspace_id()?))
        } else {
            Ok(Query::new())
        }
    }

    /// `GET /v2/task/{id}`
    pub async fn get_task(
        &self,
        task: &TaskRef,
        include_subtasks: bool,
        include_markdown: bool,
    ) -> Result<Task> {
        let q = self
            .task_query(task)?
            .push_if("include_subtasks", include_subtasks)
            .push_if("include_markdown_description", include_markdown);
        self.get(&format!("/v2/task/{}", task.path_id()), &q).await
    }

    /// Resolve a ref to the real task id (v3 endpoints don't take custom ids).
    pub async fn task_id(&self, task: &TaskRef) -> Result<String> {
        match task {
            TaskRef::Id(id) => Ok(id.clone()),
            TaskRef::Custom(_) => Ok(self.get_task(task, false, false).await?.id),
        }
    }

    /// Picks `GET /v2/list/{id}/task` (exactly one list) or
    /// `GET /v2/team/{id}/task`, then pages until `last_page`, a short page,
    /// `limit`, or the 500-page cap.
    pub async fn search_tasks(&self, filter: &TaskFilter) -> Result<Vec<Task>> {
        let path = match filter.single_list() {
            Some(list) => format!("/v2/list/{list}/task"),
            None => format!("/v2/team/{}/task", self.workspace_id()?),
        };
        let base = filter.query()?;
        let mut out = Vec::new();
        for page in 0..MAX_PAGES {
            let q = base.clone().push("page", page);
            let resp: TasksPage = self.get(&path, &q).await?;
            let n = resp.tasks.len();
            out.extend(resp.tasks);
            if let Some(limit) = filter.limit {
                if out.len() >= limit {
                    out.truncate(limit);
                    break;
                }
            }
            if resp.last_page == Some(true) || n < PAGE_SIZE {
                break;
            }
        }
        Ok(out)
    }

    /// `POST /v2/list/{id}/task`
    pub async fn create_task(&self, list_id: &str, task: &CreateTask) -> Result<Task> {
        if task.name.trim().is_empty() {
            return Err(ClickUpError::Invalid("task name is required".into()));
        }
        self.post(
            &format!("/v2/list/{list_id}/task"),
            &Query::new(),
            serde_json::to_value(task)?,
        )
        .await
    }

    /// `PUT /v2/task/{id}` — custom fields are not updatable here.
    pub async fn update_task(&self, task: &TaskRef, update: &UpdateTask) -> Result<Task> {
        self.put(
            &format!("/v2/task/{}", task.path_id()),
            &self.task_query(task)?,
            update.body(),
        )
        .await
    }

    pub async fn set_status(&self, task: &TaskRef, status: &str) -> Result<Task> {
        self.update_task(
            task,
            &UpdateTask {
                status: Some(status.to_string()),
                ..Default::default()
            },
        )
        .await
    }

    /// `DELETE /v2/task/{id}`
    pub async fn delete_task(&self, task: &TaskRef) -> Result<()> {
        self.delete(
            &format!("/v2/task/{}", task.path_id()),
            &self.task_query(task)?,
        )
        .await?;
        Ok(())
    }

    /// `PUT /v3/workspaces/{ws}/tasks/{id}/home_list/{list}`
    pub async fn move_task(&self, task: &TaskRef, list_id: &str, opts: &MoveTask) -> Result<Value> {
        let id = self.task_id(task).await?;
        let ws = self.workspace_id()?;
        let mut body = Map::new();
        if let Some(v) = opts.move_custom_fields {
            body.insert("move_custom_fields".into(), json!(v));
        }
        if !opts.status_mappings.is_empty() {
            body.insert(
                "status_mappings".into(),
                Value::Array(
                    opts.status_mappings
                        .iter()
                        .map(|(s, d)| json!({ "source_status_id": s, "destination_status_id": d }))
                        .collect(),
                ),
            );
        }
        self.put(
            &format!("/v3/workspaces/{ws}/tasks/{id}/home_list/{list_id}"),
            &Query::new(),
            Value::Object(body),
        )
        .await
    }

    /// `POST /v2/list/{list}/task/{id}` (Tasks in Multiple Lists)
    pub async fn add_task_to_list(&self, task: &TaskRef, list_id: &str) -> Result<()> {
        let id = self.task_id(task).await?;
        self.post::<Value>(
            &format!("/v2/list/{list_id}/task/{id}"),
            &Query::new(),
            json!({}),
        )
        .await?;
        Ok(())
    }

    /// `DELETE /v2/list/{list}/task/{id}`
    pub async fn remove_task_from_list(&self, task: &TaskRef, list_id: &str) -> Result<()> {
        let id = self.task_id(task).await?;
        self.delete(&format!("/v2/list/{list_id}/task/{id}"), &Query::new())
            .await?;
        Ok(())
    }

    /// `POST /v2/task/{target}/merge`
    pub async fn merge_tasks(&self, target: &TaskRef, sources: &[String]) -> Result<Value> {
        let id = self.task_id(target).await?;
        self.post(
            &format!("/v2/task/{id}/merge"),
            &Query::new(),
            json!({ "source_task_ids": sources }),
        )
        .await
    }

    /// `GET /v2/task/{id}/time_in_status`
    pub async fn time_in_status(&self, task: &TaskRef) -> Result<Value> {
        self.get(
            &format!("/v2/task/{}/time_in_status", task.path_id()),
            &self.task_query(task)?,
        )
        .await
    }

    /// `GET /v2/task/bulk_time_in_status/task_ids?task_ids=…` (plain ids).
    pub async fn bulk_time_in_status(&self, task_ids: &[String]) -> Result<Value> {
        let q = Query::new().push_all("task_ids", task_ids);
        self.get("/v2/task/bulk_time_in_status/task_ids", &q).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_body_sends_add_and_rem() {
        let u = UpdateTask {
            assignees_add: vec![1],
            ..Default::default()
        };
        assert_eq!(u.body(), json!({ "assignees": { "add": [1], "rem": [] } }));
        assert!(UpdateTask::default().is_empty());
    }

    #[test]
    fn update_body_clears_with_null() {
        let u = UpdateTask {
            priority: Some(None),
            due_date: Some(None),
            status: Some("done".into()),
            ..Default::default()
        };
        assert_eq!(
            u.body(),
            json!({ "priority": null, "due_date": null, "status": "done" })
        );
    }
}
