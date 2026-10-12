//! auth, whoami, workspace, config.

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use clickupc_core::{
    config::{mask_token, profile_slug, ClickUpConfig, ConfigFile},
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
        /// pk_… token, or `-` to read it from stdin. Prompted when omitted
        /// (falls back to CLICKUP_TOKEN without a terminal). Pick the
        /// workspace with the global --workspace and the profile with --profile.
        #[arg(long)]
        token: Option<String>,
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
    /// Make a profile the default (picker when omitted)
    Use { name: Option<String> },
    /// Delete a profile
    Remove { name: String },
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceCmd {
    /// List workspaces the token can access
    List,
    /// Set the workspace for the current profile (picker when omitted)
    Use {
        #[arg(value_name = "WORKSPACE")]
        target: Option<String>,
    },
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
        AuthCmd::Login { token } => login(ctx, token, true).await,
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

/// Where `auth login` writes the token.
enum Target {
    Existing(String),
    /// New profile; `None` = ask for a name (suggested from the workspace).
    New(Option<String>),
}

fn profile_label(name: &str, file: &ConfigFile) -> String {
    let p = file.profiles.get(name).cloned().unwrap_or_default();
    format!(
        "{name}  (workspace {} · {})",
        p.workspace_id.as_deref().unwrap_or("none"),
        p.token
            .as_deref()
            .map(mask_token)
            .unwrap_or_else(|| "no token".into())
    )
}

/// Interactive-aware login. `offer_mcp` asks to continue into
/// `mcp install` afterwards (off when called from `mcp install` itself).
pub async fn login(ctx: &Ctx, token: Option<String>, offer_mcp: bool) -> Result<()> {
    let mut file = ConfigFile::load()?;
    let had_profiles = !file.profiles.is_empty();

    // 1. Which profile? Never overwrite one silently on a terminal.
    let target = match &ctx.profile {
        Some(p) if file.profiles.contains_key(p) => Target::Existing(p.clone()),
        Some(p) => Target::New(Some(p.clone())),
        // First login: CLICKUP_PROFILE, else `default` (unchanged behaviour).
        None if !had_profiles => Target::New(Some(file.selected_profile_name())),
        None if is_tty() => {
            const NEW: &str = "+ New profile";
            let mut options = vec![NEW.to_string()];
            let names: Vec<String> = file.profiles.keys().cloned().collect();
            options.extend(
                names
                    .iter()
                    .map(|n| format!("Update {}", profile_label(n, &file))),
            );
            let pick = inquire::Select::new("Which profile?", options.clone()).prompt()?;
            match options.iter().position(|o| *o == pick) {
                Some(0) | None => Target::New(None),
                Some(i) => Target::Existing(names[i - 1].clone()),
            }
        }
        // Scripts: keep updating the selected profile, as before.
        None => Target::Existing(file.selected_profile_name()),
    };

    // 2. Token. Masked (not hidden) so a paste visibly lands.
    let token = match token.as_deref() {
        Some("-") => {
            let mut buf = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
            buf
        }
        Some(t) => t.to_string(),
        None if is_tty() => inquire::Password::new("ClickUp API token (pk_…):")
            .with_display_mode(inquire::PasswordDisplayMode::Masked)
            .without_confirmation()
            .with_help_message(
                "ClickUp → Settings → Apps → API Token. Paste with Cmd+V / Ctrl+Shift+V; input shows as ****",
            )
            .prompt()?,
        None => match std::env::var("CLICKUP_TOKEN") {
            Ok(t) if !t.trim().is_empty() => t,
            _ => bail!("missing token; pass --token pk_…, --token - (stdin) or set CLICKUP_TOKEN"),
        },
    };
    let token = token.trim().to_string();
    if !token.starts_with("pk_") {
        bail!("personal API tokens start with pk_ (OAuth tokens are not supported)");
    }
    let base = ctx.config().map(|c| c.base_url).unwrap_or_default();
    let client = ClickUpClient::new(&ClickUpConfig::with_token(token.clone(), base))?;
    let spinner = crate::output::spinner("Verifying token with ClickUp…");
    let user = client.user().await;
    spinner.finish_and_clear();
    let user = user.context("token rejected by ClickUp")?;

    // 3. Workspace.
    let chosen = match &ctx.workspace {
        Some(w) => client.find_workspace(w).await?,
        None => {
            let workspaces = client.workspaces().await?;
            match workspaces.as_slice() {
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
                    many[names.iter().position(|n| *n == pick).unwrap_or(0)].clone()
                }
            }
        }
    };

    // 4. Profile name for a new profile.
    let name = match target {
        Target::Existing(n) | Target::New(Some(n)) => n,
        Target::New(None) => {
            let mut suggestion = profile_slug(&chosen.name);
            let mut i = 2;
            while file.profiles.contains_key(&suggestion) {
                suggestion = format!("{}-{i}", profile_slug(&chosen.name));
                i += 1;
            }
            let taken: Vec<String> = file.profiles.keys().cloned().collect();
            inquire::Text::new("Profile name:")
                .with_default(&suggestion)
                .with_validator(move |v: &str| {
                    let v = v.trim();
                    Ok(if v.is_empty() {
                        inquire::validator::Validation::Invalid("name can't be empty".into())
                    } else if taken.iter().any(|t| t == v) {
                        inquire::validator::Validation::Invalid(
                            "a profile with that name exists; pick it from the list to update it"
                                .into(),
                        )
                    } else {
                        inquire::validator::Validation::Valid
                    })
                })
                .prompt()?
                .trim()
                .to_string()
        }
    };

    let p = file.profile_mut(&name);
    p.token = Some(token);
    p.workspace_id = Some(chosen.id.clone());
    if p.timezone.is_none() {
        p.timezone = user.timezone.clone();
    }

    // 5. Active profile.
    let current = file.current_profile.clone();
    let make_active = match current.as_deref() {
        None => true,
        Some(c) if c == name => true,
        Some(c) if is_tty() && had_profiles => inquire::Confirm::new(&format!(
            "Make '{name}' the active profile? (currently '{c}')"
        ))
        .with_default(true)
        .prompt()?,
        Some(_) => true,
    };
    if make_active {
        file.current_profile = Some(name.clone());
    }
    let path = file.save()?;
    println!(
        "Logged in as {} — workspace '{}' saved to profile '{name}' ({}).",
        user.display(),
        chosen.name,
        path.display()
    );
    if !make_active {
        println!(
            "Active profile is still '{}'. Use it with --profile {name} or `clickupc auth profile use {name}`.",
            current.unwrap_or_default()
        );
    }
    if offer_mcp
        && is_tty()
        && inquire::Confirm::new("Set up the MCP server for your AI clients now?")
            .with_default(true)
            .with_help_message("Registers `clickupc mcp serve` in Claude Code, Cursor, Codex, …")
            .prompt()?
    {
        println!();
        let plan = Box::pin(super::mcp::interactive_setup(ctx, "clickupc")).await?;
        super::mcp::install_plan(&plan)?;
    }
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
            let name = match name {
                Some(n) => n,
                None => {
                    crate::output::require_tty("profile name", "<name>")?;
                    if file.profiles.is_empty() {
                        bail!("no profiles yet; run `clickupc auth login`");
                    }
                    let names: Vec<String> = file.profiles.keys().cloned().collect();
                    let labels: Vec<String> =
                        names.iter().map(|n| profile_label(n, &file)).collect();
                    let pick = inquire::Select::new("Use which profile?", labels.clone())
                        .with_starting_cursor(
                            names
                                .iter()
                                .position(|n| *n == file.selected_profile_name())
                                .unwrap_or(0),
                        )
                        .prompt()?;
                    names[labels.iter().position(|l| *l == pick).unwrap_or(0)].clone()
                }
            };
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
    let s = ctx.session().await?;
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
    let s = ctx.session().await?;
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
        WorkspaceCmd::Use { target } => {
            let w = match target {
                Some(t) => list
                    .iter()
                    .find(|x| x.id == t || x.name.eq_ignore_ascii_case(&t))
                    .with_context(|| format!("workspace '{t}' not found"))?,
                None => {
                    crate::output::require_tty("workspace", "<workspace>")?;
                    let labels: Vec<String> = list
                        .iter()
                        .map(|w| format!("{} ({})", w.name, w.id))
                        .collect();
                    let pick = inquire::Select::new("Use which workspace?", labels.clone())
                        .with_starting_cursor(
                            list.iter()
                                .position(|w| Some(&w.id) == s.cfg.workspace_id.as_ref())
                                .unwrap_or(0),
                        )
                        .prompt()?;
                    &list[labels.iter().position(|l| *l == pick).unwrap_or(0)]
                }
            };
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
