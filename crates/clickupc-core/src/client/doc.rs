//! v3 Docs. Note: "Search for Docs" has no free-text query parameter, so
//! name search is a client-side filter over the listing.

use serde_json::{json, Value};

use super::{ClickUpClient, Query};
use crate::error::Result;

/// Parent types accepted by Create a Doc.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DocParent {
    Space = 4,
    Folder = 5,
    List = 6,
    Everything = 7,
    Workspace = 12,
}

impl ClickUpClient {
    fn docs_path(&self, suffix: &str) -> Result<String> {
        Ok(format!(
            "/v3/workspaces/{}/docs{suffix}",
            self.workspace_id()?
        ))
    }

    /// `GET /v3/workspaces/{ws}/docs`, following `next_cursor`, optionally
    /// keeping only docs whose name contains `name_filter` (case-insensitive).
    pub async fn search_docs(&self, name_filter: Option<&str>, limit: usize) -> Result<Vec<Value>> {
        let needle = name_filter.map(str::to_lowercase);
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..50 {
            let q = Query::new()
                .push("limit", 100)
                .push_opt("cursor", cursor.as_ref());
            let v: Value = self.get(&self.docs_path("")?, &q).await?;
            for doc in v
                .get("docs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let keep = needle.as_ref().is_none_or(|n| {
                    doc.get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| name.to_lowercase().contains(n))
                });
                if keep {
                    out.push(doc.clone());
                }
            }
            if out.len() >= limit {
                out.truncate(limit);
                break;
            }
            cursor = v
                .get("next_cursor")
                .and_then(Value::as_str)
                .filter(|c| !c.is_empty())
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
    }

    /// `GET /v3/workspaces/{ws}/docs/{doc}`
    pub async fn get_doc(&self, doc_id: &str) -> Result<Value> {
        self.get(&self.docs_path(&format!("/{doc_id}"))?, &Query::new())
            .await
    }

    /// `GET …/docs/{doc}/page_listing`
    pub async fn doc_page_listing(&self, doc_id: &str) -> Result<Value> {
        self.get(
            &self.docs_path(&format!("/{doc_id}/page_listing"))?,
            &Query::new(),
        )
        .await
    }

    /// `GET …/docs/{doc}/pages` (markdown content)
    pub async fn doc_pages(&self, doc_id: &str) -> Result<Value> {
        self.get(
            &self.docs_path(&format!("/{doc_id}/pages"))?,
            &Query::new().push("content_format", "text/md"),
        )
        .await
    }

    /// `GET …/docs/{doc}/pages/{page}` (markdown content)
    pub async fn doc_page(&self, doc_id: &str, page_id: &str) -> Result<Value> {
        self.get(
            &self.docs_path(&format!("/{doc_id}/pages/{page_id}"))?,
            &Query::new().push("content_format", "text/md"),
        )
        .await
    }

    /// `POST /v3/workspaces/{ws}/docs`
    pub async fn create_doc(&self, name: &str, parent: Option<(&str, DocParent)>) -> Result<Value> {
        let mut body = json!({ "name": name, "create_page": true });
        if let Some((id, kind)) = parent {
            body["parent"] = json!({ "id": id, "type": kind as i32 });
        }
        self.post(&self.docs_path("")?, &Query::new(), body).await
    }

    /// `POST …/docs/{doc}/pages`
    pub async fn create_doc_page(
        &self,
        doc_id: &str,
        name: &str,
        content: Option<&str>,
        parent_page_id: Option<&str>,
    ) -> Result<Value> {
        let mut body = json!({ "name": name, "content_format": "text/md" });
        if let Some(c) = content {
            body["content"] = json!(c);
        }
        if let Some(p) = parent_page_id {
            body["parent_page_id"] = json!(p);
        }
        self.post(
            &self.docs_path(&format!("/{doc_id}/pages"))?,
            &Query::new(),
            body,
        )
        .await
    }

    /// `PUT …/docs/{doc}/pages/{page}`; `mode` = replace | append | prepend.
    pub async fn edit_doc_page(
        &self,
        doc_id: &str,
        page_id: &str,
        content: Option<&str>,
        name: Option<&str>,
        mode: &str,
    ) -> Result<Value> {
        let mut body = json!({ "content_format": "text/md", "content_edit_mode": mode });
        if let Some(c) = content {
            body["content"] = json!(c);
        }
        if let Some(n) = name {
            body["name"] = json!(n);
        }
        self.put(
            &self.docs_path(&format!("/{doc_id}/pages/{page_id}"))?,
            &Query::new(),
            body,
        )
        .await
    }
}
