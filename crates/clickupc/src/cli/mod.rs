pub mod auth;
pub mod collab;
pub mod hierarchy;
pub mod mcp;
pub mod report;
pub mod task;

use std::{collections::HashMap, io::Read};

use anyhow::{Context, Result};
use clickupc_core::{
    client::{select_workspace, workspaces_for},
    config::ClickUpConfig,
    ClickUpClient, Resolver,
};

/// Global flags shared by every command.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub profile: Option<String>,
    /// `--workspace`: name or id overriding the profile's workspace.
    pub workspace: Option<String>,
    pub json: bool,
    pub yes: bool,
    pub refresh: bool,
}

/// Loaded config + client; build a resolver with [`Session::resolver`].
pub struct Session {
    pub cfg: ClickUpConfig,
    pub client: ClickUpClient,
    refresh: bool,
}

impl Session {
    pub fn resolver(&self) -> Result<Resolver<'_>> {
        Ok(Resolver::new(&self.client, &self.cfg)?.with_refresh(self.refresh))
    }

    pub fn tz(&self) -> Option<&str> {
        self.cfg.timezone.as_deref()
    }
}

impl Ctx {
    pub fn config(&self) -> Result<ClickUpConfig> {
        Ok(ClickUpConfig::load_profile(self.profile.as_deref())?)
    }

    pub async fn session(&self) -> Result<Session> {
        let mut cfg = self.config()?;
        if let Some(w) = &self.workspace {
            select_workspace(&mut cfg, w).await?;
        }
        let client = ClickUpClient::new(&cfg)?;
        Ok(Session {
            cfg,
            client,
            refresh: self.refresh,
        })
    }

    /// One session per active workspace, labelled with its name, for read
    /// commands. `None` in single-workspace mode (fewer than two active
    /// workspaces, or `--workspace` given), so callers keep their usual path.
    pub async fn multi_sessions(&self) -> Result<Option<Vec<(String, Session)>>> {
        if self.workspace.is_some() {
            return Ok(None);
        }
        let cfg = self.config()?;
        let cfgs = cfg.fan_out();
        if cfgs.len() < 2 {
            return Ok(None);
        }
        // ponytail: a failed name lookup only costs the label; ids still work.
        let names: HashMap<String, String> = workspaces_for(&cfg)
            .await
            .map(|ws| ws.into_iter().map(|w| (w.id, w.name)).collect())
            .unwrap_or_default();
        cfgs.into_iter()
            .map(|cfg| {
                let id = cfg.workspace_id.clone().unwrap_or_default();
                let label = names.get(&id).cloned().unwrap_or(id);
                let session = Session {
                    client: ClickUpClient::new(&cfg)?,
                    cfg,
                    refresh: self.refresh,
                };
                Ok((label, session))
            })
            .collect::<Result<Vec<_>>>()
            .map(Some)
    }
}

/// `-` reads stdin, otherwise the value itself.
pub fn text_or_stdin(value: &str) -> Result<String> {
    if value == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        Ok(s)
    } else {
        Ok(value.to_string())
    }
}

/// `-` reads stdin, otherwise a file path.
pub fn file_or_stdin(path: &str) -> Result<String> {
    if path == "-" {
        text_or_stdin("-")
    } else {
        std::fs::read_to_string(path).with_context(|| format!("reading {path}"))
    }
}
