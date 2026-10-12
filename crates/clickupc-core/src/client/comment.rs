//! Task comments (cursor pagination), threaded replies.

use serde::Deserialize;
use serde_json::{json, Value};

use super::{ClickUpClient, Query};
use crate::{error::Result, model::Comment, task_ref::TaskRef};

pub const COMMENT_PAGE_SIZE: usize = 25;

#[derive(Deserialize)]
struct CommentsResp {
    #[serde(default)]
    comments: Vec<Comment>,
}

impl ClickUpClient {
    /// `GET /v2/task/{id}/comment` — newest first, 25 per page. Pass the
    /// previous page's last comment as `before` to get the next page.
    pub async fn task_comments_page(
        &self,
        task: &TaskRef,
        before: Option<&Comment>,
    ) -> Result<Vec<Comment>> {
        let mut q = self.task_query(task)?;
        if let Some(c) = before {
            q = q.push_opt("start", c.date).push("start_id", &c.id);
        }
        Ok(self
            .get::<CommentsResp>(&format!("/v2/task/{}/comment", task.path_id()), &q)
            .await?
            .comments)
    }

    /// Pages through comments until `limit` (or all when `None`).
    pub async fn task_comments(
        &self,
        task: &TaskRef,
        limit: Option<usize>,
    ) -> Result<Vec<Comment>> {
        let mut out: Vec<Comment> = Vec::new();
        loop {
            let page = self.task_comments_page(task, out.last()).await?;
            let n = page.len();
            out.extend(page);
            if limit.is_some_and(|l| out.len() >= l) {
                out.truncate(limit.unwrap_or(usize::MAX));
                break;
            }
            if n < COMMENT_PAGE_SIZE {
                break;
            }
        }
        Ok(out)
    }

    /// `POST /v2/task/{id}/comment`
    pub async fn add_comment(
        &self,
        task: &TaskRef,
        text: &str,
        assignee: Option<i64>,
        notify_all: bool,
    ) -> Result<Value> {
        let mut body = json!({ "comment_text": text, "notify_all": notify_all });
        if let Some(a) = assignee {
            body["assignee"] = json!(a);
        }
        self.post(
            &format!("/v2/task/{}/comment", task.path_id()),
            &self.task_query(task)?,
            body,
        )
        .await
    }

    /// `PUT /v2/comment/{id}`
    pub async fn update_comment(
        &self,
        comment_id: &str,
        text: &str,
        resolved: Option<bool>,
    ) -> Result<()> {
        let mut body = json!({ "comment_text": text });
        if let Some(r) = resolved {
            body["resolved"] = json!(r);
        }
        self.put::<Value>(&format!("/v2/comment/{comment_id}"), &Query::new(), body)
            .await?;
        Ok(())
    }

    /// `DELETE /v2/comment/{id}`
    pub async fn delete_comment(&self, comment_id: &str) -> Result<()> {
        self.delete(&format!("/v2/comment/{comment_id}"), &Query::new())
            .await?;
        Ok(())
    }

    /// `GET /v2/comment/{id}/reply`
    pub async fn comment_replies(&self, comment_id: &str) -> Result<Vec<Comment>> {
        Ok(self
            .get::<CommentsResp>(&format!("/v2/comment/{comment_id}/reply"), &Query::new())
            .await?
            .comments)
    }

    /// `POST /v2/comment/{id}/reply`
    pub async fn reply_comment(
        &self,
        comment_id: &str,
        text: &str,
        notify_all: bool,
    ) -> Result<Value> {
        self.post(
            &format!("/v2/comment/{comment_id}/reply"),
            &Query::new(),
            json!({ "comment_text": text, "notify_all": notify_all }),
        )
        .await
    }
}
