pub mod auth;
pub mod collab;
pub mod hierarchy;
pub mod mcp;
pub mod report;
pub mod task;

use std::io::Read;

use anyhow::{Context, Result};
use clickupc_core::{config::ClickUpConfig, ClickUpClient, Resolver};

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
        let mut client = ClickUpClient::new(&cfg)?;
        if let Some(w) = &self.workspace {
            let id = if !w.is_empty() && w.chars().all(|c| c.is_ascii_digit()) {
                w.clone()
            } else {
                client.find_workspace(w).await?.id
            };
            cfg.workspace_id = Some(id.clone());
            client = client.with_workspace(id);
        }
        Ok(Session {
            cfg,
            client,
            refresh: self.refresh,
        })
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
