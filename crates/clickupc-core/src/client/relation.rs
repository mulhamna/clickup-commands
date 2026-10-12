//! Tags, links, dependencies, checklists, attachments.

use std::path::Path;

use serde_json::{json, Value};

use super::{Body, ClickUpClient, Query};
use crate::{
    error::{ClickUpError, Result},
    model::Checklist,
    task_ref::TaskRef,
};

/// Which side of a dependency the *other* task is on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dependency {
    /// This task is waiting on the other task (`depends_on`).
    WaitingOn,
    /// This task blocks the other task (`dependency_of`).
    Blocks,
}

impl Dependency {
    fn key(self) -> &'static str {
        match self {
            Self::WaitingOn => "depends_on",
            Self::Blocks => "dependency_of",
        }
    }
}

fn enc(segment: &str) -> String {
    url::form_urlencoded::byte_serialize(segment.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

impl ClickUpClient {
    /// `POST /v2/task/{id}/tag/{name}` — the tag must exist in the space.
    pub async fn add_tag(&self, task: &TaskRef, tag: &str) -> Result<()> {
        self.post::<Value>(
            &format!("/v2/task/{}/tag/{}", task.path_id(), enc(tag)),
            &self.task_query(task)?,
            json!({}),
        )
        .await?;
        Ok(())
    }

    /// `DELETE /v2/task/{id}/tag/{name}`
    pub async fn remove_tag(&self, task: &TaskRef, tag: &str) -> Result<()> {
        self.delete(
            &format!("/v2/task/{}/tag/{}", task.path_id(), enc(tag)),
            &self.task_query(task)?,
        )
        .await?;
        Ok(())
    }

    /// `POST /v2/task/{id}/link/{other}`
    pub async fn add_link(&self, task: &TaskRef, other: &TaskRef) -> Result<()> {
        let other_id = self.task_id(other).await?;
        self.post::<Value>(
            &format!("/v2/task/{}/link/{other_id}", task.path_id()),
            &self.task_query(task)?,
            json!({}),
        )
        .await?;
        Ok(())
    }

    /// `DELETE /v2/task/{id}/link/{other}`
    pub async fn remove_link(&self, task: &TaskRef, other: &TaskRef) -> Result<()> {
        let other_id = self.task_id(other).await?;
        self.delete(
            &format!("/v2/task/{}/link/{other_id}", task.path_id()),
            &self.task_query(task)?,
        )
        .await?;
        Ok(())
    }

    /// `POST /v2/task/{id}/dependency` body `{ depends_on | dependency_of }`
    pub async fn add_dependency(
        &self,
        task: &TaskRef,
        other: &TaskRef,
        kind: Dependency,
    ) -> Result<()> {
        let other_id = self.task_id(other).await?;
        self.post::<Value>(
            &format!("/v2/task/{}/dependency", task.path_id()),
            &self.task_query(task)?,
            json!({ kind.key(): other_id }),
        )
        .await?;
        Ok(())
    }

    /// `DELETE /v2/task/{id}/dependency?depends_on=…|dependency_of=…`
    pub async fn remove_dependency(
        &self,
        task: &TaskRef,
        other: &TaskRef,
        kind: Dependency,
    ) -> Result<()> {
        let other_id = self.task_id(other).await?;
        let q = self.task_query(task)?.push(kind.key(), other_id);
        self.delete(&format!("/v2/task/{}/dependency", task.path_id()), &q)
            .await?;
        Ok(())
    }

    /// `POST /v2/task/{id}/checklist`, then one call per item.
    pub async fn create_checklist(
        &self,
        task: &TaskRef,
        name: &str,
        items: &[String],
    ) -> Result<Checklist> {
        let v: Value = self
            .post(
                &format!("/v2/task/{}/checklist", task.path_id()),
                &self.task_query(task)?,
                json!({ "name": name }),
            )
            .await?;
        let mut checklist: Checklist =
            serde_json::from_value(v.get("checklist").cloned().unwrap_or(v))?;
        for item in items {
            checklist = self.add_checklist_item(&checklist.id, item).await?;
        }
        Ok(checklist)
    }

    /// `POST /v2/checklist/{id}/checklist_item`
    pub async fn add_checklist_item(&self, checklist_id: &str, name: &str) -> Result<Checklist> {
        let v: Value = self
            .post(
                &format!("/v2/checklist/{checklist_id}/checklist_item"),
                &Query::new(),
                json!({ "name": name }),
            )
            .await?;
        Ok(serde_json::from_value(
            v.get("checklist").cloned().unwrap_or(v),
        )?)
    }

    /// `PUT /v2/checklist/{id}/checklist_item/{item}`
    pub async fn update_checklist_item(
        &self,
        checklist_id: &str,
        item_id: &str,
        resolved: Option<bool>,
        name: Option<&str>,
    ) -> Result<Value> {
        let mut body = json!({});
        if let Some(r) = resolved {
            body["resolved"] = json!(r);
        }
        if let Some(n) = name {
            body["name"] = json!(n);
        }
        self.put(
            &format!("/v2/checklist/{checklist_id}/checklist_item/{item_id}"),
            &Query::new(),
            body,
        )
        .await
    }

    /// `POST /v2/task/{id}/attachment` (multipart field `attachment`).
    pub async fn upload_attachment(&self, task: &TaskRef, path: &Path) -> Result<Value> {
        let bytes = tokio::fs::read(path).await?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ClickUpError::Invalid(format!("bad file name: {}", path.display())))?
            .to_string();
        let mime = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();
        self.call(
            reqwest::Method::POST,
            &format!("/v2/task/{}/attachment", task.path_id()),
            &self.task_query(task)?,
            Some(Body::File {
                field: "attachment",
                name,
                bytes,
                mime,
            }),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn encodes_tag_names() {
        assert_eq!(super::enc("needs review"), "needs%20review");
        assert_eq!(super::enc("a/b"), "a%2Fb");
    }
}
