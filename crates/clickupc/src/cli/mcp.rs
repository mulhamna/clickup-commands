//! `clickupc mcp serve|install|doctor`. Registration goes through kurir; the
//! registered entry runs this same binary with `mcp serve` and never
//! carries the token.

use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use clap::{Subcommand, ValueEnum};
use clickupc_core::config::ClickUpConfig;
use kurir::{
    doctor as kurir_doctor, register, registration::snippet_for, Harness, RegistrationOptions,
    Scope, ServerSpec,
};

#[derive(Debug, Subcommand)]
pub enum McpCmd {
    /// Run the MCP server over stdio (default) or Streamable HTTP
    Serve {
        #[arg(long, value_enum, default_value = "stdio")]
        transport: Transport,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8787)]
        port: u16,
        #[arg(long, default_value = "/mcp")]
        path: String,
    },
    /// Register `clickupc mcp serve` in an MCP client's config.
    ///
    /// Omit `--client` to pick interactively.
    Install {
        #[arg(long, value_enum)]
        client: Option<McpClient>,
        /// Server entry name. Defaults to `clickupc` so it never collides
        /// with ClickUp's official server, which clients usually call `clickup`.
        #[arg(long, default_value = "clickupc")]
        name: String,
        /// Binary to register (default: clickupc on PATH)
        #[arg(long, default_value = "clickupc")]
        command: String,
        /// Pin a clickupc profile via CLICKUP_PROFILE in the entry
        #[arg(long = "server-profile")]
        server_profile: Option<String>,
        /// Print what would be written
        #[arg(long)]
        print: bool,
        #[arg(long)]
        dry_run: bool,
        /// Overwrite an existing entry
        #[arg(long)]
        force: bool,
    },
    /// Check binary, credentials and client config readiness
    Doctor {
        #[arg(long, value_enum)]
        client: Option<McpClient>,
        #[arg(long, default_value = "clickupc")]
        command: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Transport {
    Stdio,
    StreamableHttp,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
pub enum McpClient {
    ClaudeCode,
    #[value(name = "claude-code-cli")]
    ClaudeCodeCli,
    ClaudeDesktop,
    Cursor,
    GeminiCli,
    Codex,
    #[value(name = "vscode")]
    Vscode,
    #[value(name = "copilot-cli")]
    CopilotCli,
    #[value(name = "opencode")]
    OpenCode,
    Windsurf,
    Zed,
    #[value(name = "openclaw")]
    OpenClaw,
    Hermes,
    #[value(name = "antigravity-cli", alias = "antigravity")]
    AntigravityCli,
    AntigravityDesktop,
    #[value(name = "generic-json", alias = "omp")]
    GenericJson,
    #[value(name = "arsy-code")]
    ArsyCode,
}

const ALL_CLIENTS: [McpClient; 17] = [
    McpClient::ClaudeCode,
    McpClient::ClaudeCodeCli,
    McpClient::ClaudeDesktop,
    McpClient::Cursor,
    McpClient::GeminiCli,
    McpClient::Codex,
    McpClient::Vscode,
    McpClient::CopilotCli,
    McpClient::OpenCode,
    McpClient::Windsurf,
    McpClient::Zed,
    McpClient::OpenClaw,
    McpClient::Hermes,
    McpClient::AntigravityCli,
    McpClient::AntigravityDesktop,
    McpClient::GenericJson,
    McpClient::ArsyCode,
];

impl McpClient {
    fn harness(self) -> Harness {
        const HARNESSES: [Harness; 17] = [
            Harness::ClaudeCode,
            Harness::ClaudeCodeCli,
            Harness::ClaudeDesktop,
            Harness::Cursor,
            Harness::GeminiCli,
            Harness::Codex,
            Harness::Vscode,
            Harness::CopilotCli,
            Harness::OpenCode,
            Harness::Windsurf,
            Harness::Zed,
            Harness::OpenClaw,
            Harness::Hermes,
            Harness::AntigravityCli,
            Harness::AntigravityDesktop,
            Harness::Omp,
            Harness::ArsyCode,
        ];
        HARNESSES[self as usize]
    }
}

impl std::fmt::Display for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const LABELS: [&str; 17] = [
            "claude-code         (~/.claude.json)",
            "claude-code-cli     (delegates to `claude mcp add`)",
            "claude-desktop      (Claude Desktop config)",
            "cursor              (Cursor mcp.json)",
            "gemini-cli          (delegates to `gemini mcp add`)",
            "codex               (delegates to `codex mcp add`)",
            "vscode              (delegates to `code --add-mcp`)",
            "copilot-cli         (~/.copilot/mcp-config.json)",
            "opencode            (opencode.json)",
            "windsurf            (Windsurf config)",
            "zed                 (Zed settings)",
            "openclaw            (openclaw.json)",
            "hermes              (delegates to `hermes mcp add`)",
            "antigravity-cli     (Antigravity CLI config)",
            "antigravity-desktop (Antigravity Desktop config)",
            "generic-json        (print snippet only)",
            "arsy-code           (delegates to `arsy mcp add`)",
        ];
        f.write_str(LABELS[*self as usize])
    }
}

pub async fn run(cmd: McpCmd) -> Result<()> {
    match cmd {
        McpCmd::Serve {
            transport,
            host,
            port,
            path,
        } => match transport {
            Transport::Stdio => clickupc_mcp::run_stdio().await,
            Transport::StreamableHttp => {
                let path = if path.starts_with('/') {
                    path
                } else {
                    format!("/{path}")
                };
                clickupc_mcp::run_streamable_http(&host, port, &path).await
            }
        },
        McpCmd::Install {
            client,
            name,
            command,
            server_profile,
            print,
            dry_run,
            force,
        } => {
            let client = match client {
                Some(c) => c,
                None => pick_client(&command)?,
            };
            install(
                client,
                &name,
                &command,
                server_profile.as_deref(),
                print,
                dry_run,
                force,
            )
        }
        McpCmd::Doctor { client, command } => doctor(client, &command),
    }
}

fn pick_client(command: &str) -> Result<McpClient> {
    crate::output::require_tty("MCP client", "--client <target>")?;
    println!("{}", crate::help_text::BANNER.trim_end());
    println!();
    println!("clickupc mcp install — interactive setup");
    println!("────────────────────────────────────────");
    match resolve_command_path(command) {
        Some(p) => println!("[ok]   clickupc binary:  {}", p.display()),
        None => println!("[warn] '{command}' is not on PATH (pass --command /path/to/clickupc)"),
    }
    let cfg = ClickUpConfig::load().ok();
    match &cfg {
        Some(c) if c.token.is_some() && c.workspace_id.is_some() => println!(
            "[ok]   ClickUp auth:     profile '{}', workspace {}",
            c.profile,
            c.workspace_id.as_deref().unwrap_or_default()
        ),
        Some(c) if c.token.is_some() => {
            println!("[fail] ClickUp auth:     token present, no workspace selected")
        }
        _ => println!("[fail] ClickUp auth:     no token configured"),
    }
    if !cfg
        .as_ref()
        .is_some_and(|c| c.token.is_some() && c.workspace_id.is_some())
    {
        bail!(
            "ClickUp credentials are not set up yet. Run:\n\n  clickupc auth login\n\nthen re-run `clickupc mcp install`."
        );
    }
    println!();
    inquire::Select::new("Pick the MCP client to install into:", ALL_CLIENTS.to_vec())
        .with_page_size(17)
        .prompt()
        .context("MCP client selection cancelled")
}

pub fn server_spec(name: &str, command: String, profile: Option<&str>) -> ServerSpec {
    let mut spec = ServerSpec::stdio(name, command, vec!["mcp".into(), "serve".into()]);
    if let Some(p) = profile {
        spec.env.insert("CLICKUP_PROFILE".into(), p.to_string());
    }
    spec
}

fn install(
    client: McpClient,
    name: &str,
    command: &str,
    profile: Option<&str>,
    print: bool,
    dry_run: bool,
    force: bool,
) -> Result<()> {
    let harness = client.harness();
    let resolved = resolve_command_for_client(client, command, dry_run)?;
    let spec = server_spec(name, resolved, profile);

    if harness.is_snippet_only() {
        println!("{}", serde_json::to_string_pretty(&snippet_for(&spec))?);
        return Ok(());
    }
    let options = RegistrationOptions {
        scope: Scope::User,
        force,
        dry_run,
        print,
        ..RegistrationOptions::default()
    };
    let result = register(harness, &spec, &options).context("MCP registration failed")?;
    if result.changed {
        println!("Installed MCP entry '{name}' for {harness}.");
    } else if result.action == "already-configured" {
        println!("MCP entry '{name}' is already configured (use --force to overwrite).");
    } else if dry_run {
        println!("Dry run: no client config written.");
    }
    Ok(())
}

fn doctor(client: Option<McpClient>, command: &str) -> Result<()> {
    println!("clickupc mcp doctor");
    match resolve_command_path(command) {
        Some(p) => println!("[ok]   binary on PATH: {}", p.display()),
        None => println!("[warn] '{command}' not on PATH; MCP clients won't be able to start it"),
    }
    match ClickUpConfig::load() {
        Ok(c) if c.token.is_none() => println!("[warn] no token; run `clickupc auth login`"),
        Ok(c) if c.workspace_id.is_none() => {
            println!("[warn] no workspace; run `clickupc workspace use <name>`")
        }
        Ok(c) => println!(
            "[ok]   profile '{}' with workspace {}",
            c.profile,
            c.workspace_id.unwrap_or_default()
        ),
        Err(e) => println!("[fail] config: {e}"),
    }
    let reports = kurir_doctor(
        client.map(McpClient::harness),
        &RegistrationOptions::default(),
    )
    .context("MCP client check failed")?;
    let mut blocking = 0;
    for report in reports {
        let status = if report.available {
            "ok  "
        } else {
            blocking += 1;
            "warn"
        };
        let target = report
            .target
            .map(|p| format!(" ({})", p.display()))
            .unwrap_or_default();
        println!("[{status}] {}{target}: {}", report.harness, report.detail);
    }
    if client.is_some() && blocking > 0 {
        bail!("{blocking} blocking issue(s) found");
    }
    Ok(())
}

fn resolve_command_for_client(client: McpClient, command: &str, dry_run: bool) -> Result<String> {
    if client.harness().is_snippet_only() {
        return Ok(command.to_string());
    }
    if let Some(p) = resolve_command_path(command) {
        return Ok(p.display().to_string());
    }
    if dry_run {
        eprintln!("[warn] '{command}' was not found on PATH.");
        return Ok(command.to_string());
    }
    bail!(
        "'{command}' was not found on PATH. Install clickupc first (npm i -g @mulham28/clickupc, cargo install clickupc) or pass --command /path/to/clickupc"
    )
}

fn resolve_command_path(command: &str) -> Option<PathBuf> {
    let path = PathBuf::from(command);
    if path.components().count() > 1 || path.is_absolute() {
        return executable_candidates(&path)
            .into_iter()
            .find(|c| c.is_file());
    }
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths).find_map(|dir| {
            executable_candidates(&dir.join(command))
                .into_iter()
                .find(|c| c.is_file())
        })
    })
}

#[cfg(target_os = "windows")]
fn executable_candidates(path: &Path) -> Vec<PathBuf> {
    let mut out = vec![path.to_path_buf()];
    if path.extension().is_some() {
        return out;
    }
    let exts = env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    for ext in exts.split(';').map(|e| e.trim().trim_start_matches('.')) {
        if !ext.is_empty() {
            out.push(path.with_extension(ext));
        }
    }
    out
}

#[cfg(not(target_os = "windows"))]
fn executable_candidates(path: &Path) -> Vec<PathBuf> {
    vec![path.to_path_buf()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_runs_mcp_serve_without_token() {
        let spec = server_spec("clickupc", "/usr/local/bin/clickupc".into(), Some("work"));
        assert_eq!(spec.command.as_deref(), Some("/usr/local/bin/clickupc"));
        assert_eq!(spec.args, vec!["mcp", "serve"]);
        assert_eq!(spec.transport, kurir::Transport::Stdio);
        assert_eq!(
            spec.env.get("CLICKUP_PROFILE").map(String::as_str),
            Some("work")
        );
        let json = serde_json::to_string(&snippet_for(&spec)).unwrap();
        assert!(!json.contains("pk_"));
        assert!(!json.contains("CLICKUP_TOKEN"));
    }

    #[test]
    fn default_entry_name_does_not_clash_with_official_server() {
        use clap::Parser;
        #[derive(Parser)]
        struct T {
            #[command(subcommand)]
            cmd: McpCmd,
        }
        let t = T::try_parse_from(["t", "install", "--client", "generic-json"]).unwrap();
        match t.cmd {
            McpCmd::Install { name, .. } => assert_eq!(name, "clickupc"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn clients_map_to_harnesses() {
        assert_eq!(McpClient::ClaudeCode.harness(), Harness::ClaudeCode);
        assert_eq!(McpClient::Codex.harness(), Harness::Codex);
        assert_eq!(McpClient::GenericJson.harness(), Harness::Omp);
        assert_eq!(McpClient::ArsyCode.harness(), Harness::ArsyCode);
        for (i, c) in ALL_CLIENTS.iter().enumerate() {
            assert_eq!(*c as usize, i);
        }
    }

    #[test]
    fn missing_binary_is_an_error_unless_dry_run() {
        let err = resolve_command_for_client(McpClient::Cursor, "definitely-not-clickupc", false)
            .unwrap_err();
        assert!(err.to_string().contains("not found on PATH"));
        assert!(
            resolve_command_for_client(McpClient::Cursor, "definitely-not-clickupc", true).is_ok()
        );
    }
}
