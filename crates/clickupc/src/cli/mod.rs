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

    pub fn session(&self) -> Result<Session> {
        let cfg = self.config()?;
        let client = ClickUpClient::new(&cfg)?;
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
