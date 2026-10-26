mod cli;
mod help_text;
mod output;

use std::{path::PathBuf, process::ExitCode};

use anyhow::Result;
use clap::{Parser, Subcommand};
use clickupc_core::ClickUpError;
use tracing_subscriber::{fmt, EnvFilter};

use cli::{
    auth::{AuthCmd, ConfigCmd, WorkspaceCmd},
    collab::{CommentCmd, FieldCmd, TimeCmd},
    hierarchy::{AliasCmd, CacheCmd, FolderCmd, ListCmd, MemberCmd, SpaceCmd},
    mcp::McpCmd,
    report::DocCmd,
    task::TaskCmd,
};

#[derive(Debug, Parser)]
#[command(
    name = "clickupc",
    about = "Unofficial ClickUp CLI and MCP server",
    long_about = help_text::ROOT_LONG_ABOUT,
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Config profile to use (default: CLICKUP_PROFILE or current_profile)
    #[arg(long, global = true)]
    profile: Option<String>,

    /// Workspace name or id for this command (overrides the profile)
    #[arg(long, global = true)]
    workspace: Option<String>,

    /// Machine-readable JSON output
    #[arg(long, global = true)]
    json: bool,

    /// Debug logging to stderr
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Ignore cached hierarchy / members / fields
    #[arg(long, global = true)]
    refresh: bool,

    /// Skip confirmations
    #[arg(short = 'y', long, global = true)]
    yes: bool,
}

// ponytail: parsed once per process, so the size difference is irrelevant.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Subcommand)]
enum Commands {
    /// Log in, check status, manage profiles
    #[command(subcommand, long_about = help_text::AUTH_LONG_ABOUT)]
    Auth(AuthCmd),
    /// Current user, workspace and plan
    Whoami,
    /// List workspaces or switch the active one
    #[command(subcommand)]
    Workspace(WorkspaceCmd),
    /// Space → Folder → List tree
    Tree {
        #[arg(long)]
        space: Option<String>,
        #[arg(long)]
        archived: bool,
    },
    /// List spaces
    #[command(subcommand)]
    Space(SpaceCmd),
    /// List or create folders
    #[command(subcommand)]
    Folder(FolderCmd),
    /// List, inspect (statuses, fields, members) or create lists
    #[command(subcommand)]
    List(ListCmd),
    /// Short names for lists
    #[command(subcommand)]
    Alias(AliasCmd),
    /// Clear cached hierarchy, members and fields
    #[command(subcommand)]
    Cache(CacheCmd),
    /// List or find workspace members
    #[command(subcommand)]
    Member(MemberCmd),
    /// Search, create, update, move, bulk-edit and delete tasks
    #[command(subcommand)]
    Task(TaskCmd),
    /// List, add, edit, reply to and delete comments
    #[command(subcommand)]
    Comment(CommentCmd),
    /// Upload files to a task
    Attach {
        task: String,
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// List, set and clear custom fields
    #[command(subcommand)]
    Field(FieldCmd),
    /// Time tracking: timers, entries and reports
    #[command(subcommand)]
    Time(TimeCmd),
    /// My tasks closed/updated since, in progress, due soon, overdue
    Standup {
        #[arg(long, default_value = "yesterday")]
        since: String,
        #[arg(long)]
        user: Option<String>,
    },
    /// Task counts for a list by status and assignee
    Summary {
        #[arg(long)]
        list: String,
    },
    /// Search, read, create and edit ClickUp Docs
    #[command(subcommand)]
    Doc(DocCmd),
    /// Raw API request, e.g. `clickupc api GET /v2/team`
    Api {
        method: String,
        /// /v2/... or /v3/...; {workspace} is substituted
        path: String,
        /// JSON body, or @file.json
        #[arg(long)]
        data: Option<String>,
    },
    /// Read or change profile settings
    #[command(subcommand)]
    Config(ConfigCmd),
    /// MCP server: serve, install, doctor
    #[command(subcommand, long_about = help_text::MCP_LONG_ABOUT)]
    Mcp(McpCmd),
    /// Upgrade clickupc to the latest release
    Update {
        /// Only report whether a newer release exists
        #[arg(long)]
        check: bool,
    },
}

async fn run(cli: Cli) -> Result<()> {
    let ctx = Ctx {
        profile: cli.profile,
        workspace: cli.workspace,
        json: cli.json,
        yes: cli.yes,
        refresh: cli.refresh,
    };
    if let Some(p) = &ctx.profile {
        // The MCP server path loads config itself; make the flag visible there too.
        std::env::set_var("CLICKUP_PROFILE", p);
    }
    use cli::*;
    match cli.command {
        Commands::Auth(c) => auth::auth(&ctx, c).await,
        Commands::Whoami => auth::whoami(&ctx).await,
        Commands::Workspace(c) => auth::workspace(&ctx, c).await,
        Commands::Config(c) => auth::config(&ctx, c),
        Commands::Tree { space, archived } => hierarchy::tree(&ctx, space, archived).await,
        Commands::Space(c) => hierarchy::space(&ctx, c).await,
        Commands::Folder(c) => hierarchy::folder(&ctx, c).await,
        Commands::List(c) => hierarchy::list(&ctx, c).await,
        Commands::Alias(c) => hierarchy::alias(&ctx, c).await,
        Commands::Cache(c) => hierarchy::cache(&ctx, c).await,
        Commands::Member(c) => hierarchy::member(&ctx, c).await,
        Commands::Task(c) => task::run(&ctx, c).await,
        Commands::Comment(c) => collab::comment(&ctx, c).await,
        Commands::Attach { task, files } => collab::attach(&ctx, task, files).await,
        Commands::Field(c) => collab::field(&ctx, c).await,
        Commands::Time(c) => collab::time(&ctx, c).await,
        Commands::Standup { since, user } => report::standup(&ctx, since, user).await,
        Commands::Summary { list } => report::summary(&ctx, list).await,
        Commands::Doc(c) => report::doc(&ctx, c).await,
        Commands::Api { method, path, data } => report::api(&ctx, method, path, data).await,
        Commands::Mcp(c) => mcp::run(&ctx, c).await,
        Commands::Update { check } => update::update(&ctx, check).await,
    }
}

/// 0 ok, 1 generic, 2 usage, 3 auth, 4 not found, 5 rate limited.
fn exit_code(err: &anyhow::Error) -> u8 {
    match err.downcast_ref::<ClickUpError>() {
        Some(ClickUpError::Auth(_)) => 3,
        Some(ClickUpError::NotFound(_)) | Some(ClickUpError::Resolve { .. }) => 4,
        Some(ClickUpError::RateLimit { .. }) => 5,
        Some(ClickUpError::Invalid(_)) | Some(ClickUpError::Config(_)) => 2,
        _ => 1,
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let filter = if cli.verbose {
        EnvFilter::new("clickupc=debug,clickupc_core=debug,clickupc_mcp=debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"))
    };
    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", clickupc_core::error::redact(&format!("{e:#}")));
            ExitCode::from(exit_code(&e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn exit_codes() {
        let e = |c: ClickUpError| exit_code(&anyhow::Error::new(c));
        assert_eq!(e(ClickUpError::Auth("x".into())), 3);
        assert_eq!(e(ClickUpError::NotFound("x".into())), 4);
        assert_eq!(e(ClickUpError::resolve("list", "x", vec![])), 4);
        assert_eq!(e(ClickUpError::RateLimit { reset_at: 0 }), 5);
        assert_eq!(e(ClickUpError::Invalid("x".into())), 2);
        assert_eq!(exit_code(&anyhow::anyhow!("other")), 1);
    }
}
