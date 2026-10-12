//! auth, whoami, workspace, config.

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use clickupc_core::{
    config::{mask_token, ClickUpConfig, ConfigFile},
    ClickUpClient,
};
use serde_json::json;

use super::Ctx;
use crate::output::{is_tty, print_json, print_table};

#[derive(Debug, Subcommand)]
pub enum AuthCmd {
    /// Store a personal API token (ClickUp → Settings → Apps → API Token)
    #[command(long_about = crate::help_text::AUTH_LOGIN_LONG_ABOUT)]
    Login {
        /// pk_… token; prompted when omitted
        #[arg(long)]
        token: Option<String>,
        /// Workspace name or id; prompted when there are several
        #[arg(long)]
        workspace: Option<String>,
    },
    /// Show profile, workspace, user and token presence
    #[command(long_about = crate::help_text::AUTH_STATUS_LONG_ABOUT)]
    Status,
    /// Remove the stored token from the profile
    Logout,
    /// Manage profiles
    #[command(subcommand)]
    Profile(ProfileCmd),
}

#[derive(Debug, Subcommand)]
pub enum ProfileCmd {
    /// List saved profiles
    List,
    /// Make a profile the default
    Use { name: String },
    /// Delete a profile
    Remove { name: String },
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceCmd {
    /// List workspaces the token can access
    List,
    /// Set the workspace for the current profile
    Use { workspace: String },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCmd {
    /// Print a setting (or all settings of the profile)
    Get { key: Option<String> },
    /// Set default_list, timezone, timeout_secs, default_task_limit or workspace_id
    Set { key: String, value: String },
}

fn profile_name(ctx: &Ctx, file: &ConfigFile) -> String {
    ctx.profile
        .clone()
        .unwrap_or_else(|| file.selected_profile_name())
}

pub async fn auth(ctx: &Ctx, cmd: AuthCmd) -> Result<()> {
    match cmd {
        AuthCmd::Login { token, workspace } => login(ctx, token, workspace).await,
        AuthCmd::Status => status(ctx).await,
        AuthCmd::Logout => {
            let mut file = ConfigFile::load()?;
            let name = profile_name(ctx, &file);
            file.profile_mut(&name).token = None;
            file.save()?;
            println!("Removed token from profile '{name}'.");
            Ok(())
        }
        AuthCmd::Profile(p) => profile(ctx, p),
    }
}

async fn login(ctx: &Ctx, token: Option<String>, workspace: Option<String>) -> Result<()> {
    let token = match token {
        Some(t) => t,
        None => {
            if !is_tty() {
                bail!("missing token; pass --token pk_…");
            }
            inquire::Password::new("ClickUp API token (pk_…):")
                .without_confirmation()
                .prompt()?
        }
    };
    let token = token.trim().to_string();
    if !token.starts_with("pk_") {
        bail!("personal API tokens start with pk_ (OAuth tokens are not supported)");
    }
    let base = ctx.config().map(|c| c.base_url).unwrap_or_default();
    let probe = ClickUpConfig::with_token(token.clone(), base);
    let client = ClickUpClient::new(&probe)?;
    let user = client.user().await.context("token rejected by ClickUp")?;
    let workspaces = client.workspaces().await?;
    let chosen = match workspace {
        Some(w) => workspaces
            .iter()
            .find(|x| x.id == w || x.name.eq_ignore_ascii_case(&w))
            .with_context(|| format!("workspace '{w}' not found for this token"))?
            .clone(),
        None => match workspaces.as_slice() {
            [] => bail!("this token has no workspaces"),
            [one] => one.clone(),
            many => {
                if !is_tty() {
                    bail!("several workspaces available; pass --workspace <name|id>");
                }
                let names: Vec<String> = many
                    .iter()
                    .map(|w| format!("{} ({})", w.name, w.id))
                    .collect();
                let pick = inquire::Select::new("Workspace:", names.clone()).prompt()?;
                let idx = names.iter().position(|n| *n == pick).unwrap_or(0);
                many[idx].clone()
            }
        },
    };
    let mut file = ConfigFile::load()?;
    let name = profile_name(ctx, &file);
    let p = file.profile_mut(&name);
    p.token = Some(token);
    p.workspace_id = Some(chosen.id.clone());
    if p.timezone.is_none() {
        p.timezone = user.timezone.clone();
    }
    file.current_profile = Some(name.clone());
    let path = file.save()?;
    println!(
        "Logged in as {} — workspace '{}' saved to profile '{name}' ({}).",
        user.display(),
        chosen.name,
        path.display()
    );
    Ok(())
}

async fn status(ctx: &Ctx) -> Result<()> {
    let cfg = ctx.config()?;
    let user = match ClickUpClient::new(&cfg) {
        Ok(c) => c.user().await.ok().map(|u| u.display()),
        Err(_) => None,
    };
    let out = json!({
        "profile": cfg.profile,
        "workspace_id": cfg.workspace_id,
        "user": user,
        "token": cfg.masked_token(),
        "config_path": cfg.config_path.display().to_string(),
        "timezone": cfg.timezone,
    });
    if ctx.json {
        return print_json(&out);
    }
    println!("Profile:   {}", cfg.profile);
    println!(
        "Workspace: {}",
        cfg.workspace_id.as_deref().unwrap_or("(none)")
    );
    println!("User:      {}", user.as_deref().unwrap_or("(not verified)"));
    println!(
        "Token:     {}",
        cfg.masked_token().as_deref().unwrap_or("(missing)")
    );
    println!("Config:    {}", cfg.config_path.display());
    Ok(())
}

fn profile(ctx: &Ctx, cmd: ProfileCmd) -> Result<()> {
    let mut file = ConfigFile::load()?;
    match cmd {
        ProfileCmd::List => {
            let current = file.selected_profile_name();
            if ctx.json {
                return print_json(&file.profiles.keys().collect::<Vec<_>>());
            }
            print_table(
                &["", "Profile", "Workspace", "Token"],
                file.profiles.iter().map(|(name, p)| {
                    vec![
                        if *name == current { "*" } else { "" }.to_string(),
                        name.clone(),
                        p.workspace_id.clone().unwrap_or_default(),
                        p.token.as_deref().map(mask_token).unwrap_or_default(),
                    ]
                }),
            );
        }
        ProfileCmd::Use { name } => {
            if !file.profiles.contains_key(&name) {
                bail!("no profile named '{name}'");
            }
            file.current_profile = Some(name.clone());
            file.save()?;
            println!("Using profile '{name}'.");
        }
        ProfileCmd::Remove { name } => {
            if file.profiles.remove(&name).is_none() {
                bail!("no profile named '{name}'");
            }
            if file.current_profile.as_deref() == Some(name.as_str()) {
                file.current_profile = None;
            }
            file.save()?;
            println!("Removed profile '{name}'.");
        }
    }
    Ok(())
}

pub async fn whoami(ctx: &Ctx) -> Result<()> {
    let s = ctx.session()?;
    let user = s.client.user().await?;
    let ws = match &s.cfg.workspace_id {
        Some(id) => s
            .client
            .workspaces()
            .await?
            .into_iter()
            .find(|w| &w.id == id),
        None => None,
    };
    let plan = if ws.is_some() {
        s.client.workspace_plan().await.ok()
    } else {
        None
    };
    if ctx.json {
        return print_json(&json!({
            "user": user,
            "workspace": ws.as_ref().map(|w| json!({ "id": w.id, "name": w.name })),
            "plan": plan,
        }));
    }
    println!("User:      {} ({})", user.display(), user.id);
    if let Some(e) = &user.email {
        println!("Email:     {e}");
    }
    if let Some(tz) = &user.timezone {
        println!("Timezone:  {tz}");
    }
    if let Some(w) = ws {
        println!("Workspace: {} ({})", w.name, w.id);
    }
    if let Some(name) = plan
        .as_ref()
        .and_then(|p| p.get("plan_name"))
        .and_then(|v| v.as_str())
    {
        println!("Plan:      {name}");
    }
    Ok(())
}

pub async fn workspace(ctx: &Ctx, cmd: WorkspaceCmd) -> Result<()> {
    let s = ctx.session()?;
    let list = s.client.workspaces().await?;
    match cmd {
        WorkspaceCmd::List => {
            if ctx.json {
                return print_json(&list);
            }
            let current = s.cfg.workspace_id.clone().unwrap_or_default();
            print_table(
                &["", "ID", "Name", "Members"],
                list.iter().map(|w| {
                    vec![
                        if w.id == current { "*" } else { "" }.to_string(),
                        w.id.clone(),
                        w.name.clone(),
                        w.members.len().to_string(),
                    ]
                }),
            );
        }
        WorkspaceCmd::Use { workspace } => {
            let w = list
                .iter()
                .find(|x| x.id == workspace || x.name.eq_ignore_ascii_case(&workspace))
                .with_context(|| format!("workspace '{workspace}' not found"))?;
            let mut file = ConfigFile::load()?;
            let name = profile_name(ctx, &file);
            file.profile_mut(&name).workspace_id = Some(w.id.clone());
            file.save()?;
            println!(
                "Profile '{name}' now uses workspace '{}' ({}).",
                w.name, w.id
            );
        }
    }
    Ok(())
}

const KEYS: [&str; 5] = [
    "default_list",
    "timezone",
    "timeout_secs",
    "default_task_limit",
    "workspace_id",
];

pub fn config(ctx: &Ctx, cmd: ConfigCmd) -> Result<()> {
    let mut file = ConfigFile::load()?;
    let name = profile_name(ctx, &file);
    match cmd {
        ConfigCmd::Get { key } => {
            let p = file.profiles.get(&name).cloned().unwrap_or_default();
            let all = json!({
                "default_list": p.default_list,
                "timezone": p.timezone,
                "timeout_secs": p.timeout_secs,
                "default_task_limit": p.default_task_limit,
                "workspace_id": p.workspace_id,
            });
            match key {
                Some(k) => {
                    let v = all.get(&k).with_context(|| {
                        format!("unknown key '{k}' (one of {})", KEYS.join(", "))
                    })?;
                    if ctx.json {
                        print_json(v)?;
                    } else if !v.is_null() {
                        println!(
                            "{}",
                            v.as_str().map(str::to_string).unwrap_or(v.to_string())
                        );
                    }
                }
                None => print_json(&all)?,
            }
        }
        ConfigCmd::Set { key, value } => {
            let p = file.profile_mut(&name);
            let clear = value.eq_ignore_ascii_case("none") || value.is_empty();
            match key.as_str() {
                "default_list" => p.default_list = (!clear).then(|| value.clone()),
                "timezone" => {
                    if !clear && !clickupc_core::time::is_valid_tz(&value) {
                        bail!("unknown timezone '{value}' (use an IANA name like Asia/Jakarta)");
                    }
                    p.timezone = (!clear).then(|| value.clone())
                }
                "timeout_secs" => p.timeout_secs = if clear { None } else { Some(value.parse()?) },
                "default_task_limit" => {
                    p.default_task_limit = if clear { None } else { Some(value.parse()?) }
                }
                "workspace_id" => p.workspace_id = (!clear).then(|| value.clone()),
                other => bail!("unknown key '{other}' (one of {})", KEYS.join(", ")),
            }
            file.save()?;
            println!("Set {key} for profile '{name}'.");
        }
    }
    Ok(())
}
