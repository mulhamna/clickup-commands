//! Workspaces, spaces, folders, lists, members.

use serde::Deserialize;
use serde_json::{json, Value};

use super::{ClickUpClient, Query};
use crate::{
    error::Result,
    model::{Folder, List, Space, User, Workspace},
};

#[derive(Deserialize)]
struct UserResp {
    user: User,
}
#[derive(Deserialize)]
struct TeamsResp {
    #[serde(default)]
    teams: Vec<Workspace>,
}
#[derive(Deserialize)]
struct SpacesResp {
    #[serde(default)]
    spaces: Vec<Space>,
}
#[derive(Deserialize)]
struct FoldersResp {
    #[serde(default)]
    folders: Vec<Folder>,
}
#[derive(Deserialize)]
struct ListsResp {
    #[serde(default)]
    lists: Vec<List>,
}
#[derive(Deserialize)]
struct MembersResp {
    #[serde(default)]
    members: Vec<User>,
}

impl ClickUpClient {
    /// `GET /v2/user`
    pub async fn user(&self) -> Result<User> {
        Ok(self.get::<UserResp>("/v2/user", &Query::new()).await?.user)
    }

    /// `GET /v2/team` — authorized workspaces, including members.
    pub async fn workspaces(&self) -> Result<Vec<Workspace>> {
        Ok(self
            .get::<TeamsResp>("/v2/team", &Query::new())
            .await?
            .teams)
    }

    /// Members of the current workspace (from `GET /v2/team`).
    pub async fn workspace_members(&self) -> Result<Vec<User>> {
        let id = self.workspace_id()?.to_string();
        let ws = self.workspaces().await?;
        Ok(ws
            .into_iter()
            .find(|w| w.id == id)
            .map(|w| w.members.into_iter().map(|m| m.user).collect())
            .unwrap_or_default())
    }

    /// `GET /v2/team/{id}/plan`
    pub async fn workspace_plan(&self) -> Result<Value> {
        let id = self.workspace_id()?;
        self.get(&format!("/v2/team/{id}/plan"), &Query::new())
            .await
    }

    /// `GET /v2/team/{id}/space`
    pub async fn spaces(&self, archived: bool) -> Result<Vec<Space>> {
        let id = self.workspace_id()?;
        Ok(self
            .get::<SpacesResp>(
                &format!("/v2/team/{id}/space"),
                &Query::new().push("archived", archived),
            )
            .await?
            .spaces)
    }

    /// `GET /v2/space/{id}`
    pub async fn space(&self, space_id: &str) -> Result<Space> {
        self.get(&format!("/v2/space/{space_id}"), &Query::new())
            .await
    }

    /// `GET /v2/space/{id}/folder` — each folder includes its lists.
    pub async fn folders(&self, space_id: &str, archived: bool) -> Result<Vec<Folder>> {
        Ok(self
            .get::<FoldersResp>(
                &format!("/v2/space/{space_id}/folder"),
                &Query::new().push("archived", archived),
            )
            .await?
            .folders)
    }

    /// `GET /v2/folder/{id}/list`
    pub async fn folder_lists(&self, folder_id: &str, archived: bool) -> Result<Vec<List>> {
        Ok(self
            .get::<ListsResp>(
                &format!("/v2/folder/{folder_id}/list"),
                &Query::new().push("archived", archived),
            )
            .await?
            .lists)
    }

    /// `GET /v2/space/{id}/list`
    pub async fn folderless_lists(&self, space_id: &str, archived: bool) -> Result<Vec<List>> {
        Ok(self
            .get::<ListsResp>(
                &format!("/v2/space/{space_id}/list"),
                &Query::new().push("archived", archived),
            )
            .await?
            .lists)
    }

    /// `GET /v2/list/{id}` — includes `statuses`.
    pub async fn list(&self, list_id: &str) -> Result<List> {
        self.get(&format!("/v2/list/{list_id}"), &Query::new())
            .await
    }

    /// `GET /v2/list/{id}/member`
    pub async fn list_members(&self, list_id: &str) -> Result<Vec<User>> {
        Ok(self
            .get::<MembersResp>(&format!("/v2/list/{list_id}/member"), &Query::new())
            .await?
            .members)
    }

    /// `POST /v2/folder/{id}/list` or `POST /v2/space/{id}/list` (folderless).
    pub async fn create_list(
        &self,
        folder_id: Option<&str>,
        space_id: Option<&str>,
        name: &str,
        content: Option<&str>,
    ) -> Result<List> {
        let path = match (folder_id, space_id) {
            (Some(f), _) => format!("/v2/folder/{f}/list"),
            (None, Some(s)) => format!("/v2/space/{s}/list"),
            (None, None) => {
                return Err(crate::ClickUpError::Invalid(
                    "a folder or space is required to create a list".into(),
                ))
            }
        };
        let mut body = json!({ "name": name });
        if let Some(c) = content {
            body["markdown_content"] = json!(c);
        }
        self.post(&path, &Query::new(), body).await
    }

    /// `POST /v2/space/{id}/folder`
    pub async fn create_folder(&self, space_id: &str, name: &str) -> Result<Folder> {
        self.post(
            &format!("/v2/space/{space_id}/folder"),
            &Query::new(),
            json!({ "name": name }),
        )
        .await
    }

    /// `GET /v2/team/{id}/custom_item` — custom task types.
    pub async fn custom_task_types(&self) -> Result<Vec<Value>> {
        let id = self.workspace_id()?;
        let v: Value = self
            .get(&format!("/v2/team/{id}/custom_item"), &Query::new())
            .await?;
        Ok(v.get("custom_items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    /// `GET /v2/space/{id}/tag`
    pub async fn space_tags(&self, space_id: &str) -> Result<Vec<crate::model::Tag>> {
        #[derive(Deserialize)]
        struct R {
            #[serde(default)]
            tags: Vec<crate::model::Tag>,
        }
        Ok(self
            .get::<R>(&format!("/v2/space/{space_id}/tag"), &Query::new())
            .await?
            .tags)
    }
}
