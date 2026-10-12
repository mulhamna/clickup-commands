//! Long help texts with the clickupc banner, mirroring jirac's help_text.rs.

macro_rules! banner {
    () => {
        concat!(
            "        _ _      _                     \n",
            "    ___| (_) ___| | ___   _ _ __   ___ \n",
            "   / __| | |/ __| |/ / | | | '_ \\ / __|\n",
            "  | (__| | | (__|   <| |_| | |_) | (__ \n",
            "   \\___|_|_|\\___|_|\\_\\\\__,_| .__/ \\___|\n",
            "                           |_|         \n",
            "\n",
        )
    };
}

pub const ROOT_LONG_ABOUT: &str = concat!(
    banner!(),
    "Unofficial ClickUp CLI and MCP server in one binary.\n\n",
    "Quick start:\n",
    "  clickupc auth login                         Store your personal API token\n",
    "  clickupc tree                               Spaces → Folders → Lists\n",
    "  clickupc task list --mine                   Your open tasks\n",
    "  clickupc task create --list sprint --name \"Fix login\" --due fri\n",
    "  clickupc standup                            Daily standup summary\n",
    "  clickupc mcp install                        Register the MCP server (interactive picker)\n",
    "  clickupc mcp install --client claude-code   Register with Claude Code directly\n",
    "  clickupc mcp doctor                         Check MCP prerequisites\n\n",
    "Install options:\n",
    "  Script:    curl -fsSL https://raw.githubusercontent.com/mulhamna/clickup-commands/main/install.sh | bash\n",
    "  npm:       npm i -g @mulham28/clickupc\n",
    "  Homebrew:  brew install mulhamna/tap/clickupc\n",
    "  Scoop:     scoop bucket add mulhamna https://github.com/mulhamna/scoop-bucket\n",
    "             scoop install mulhamna/clickupc\n",
    "  Cargo:     cargo install clickupc\n\n",
    "Env vars:    CLICKUP_TOKEN, CLICKUP_WORKSPACE_ID, CLICKUP_PROFILE, CLICKUP_DEFAULT_LIST\n",
    "Version:     use --version or -V (note: -v enables verbose logging)\n",
    "Not affiliated with or endorsed by ClickUp."
);

pub const AUTH_LONG_ABOUT: &str = concat!(
    banner!(),
    "Manage ClickUp auth profiles and the active workspace.\n\n",
    "Typical flow:\n",
    "  clickupc auth login                  Paste a token, pick a workspace\n",
    "  clickupc auth status                 Show profile, workspace and token\n",
    "  clickupc auth profile list           List saved profiles\n",
    "  clickupc auth profile use <name>     Switch the active profile\n",
    "  clickupc auth logout                 Remove the stored token"
);

pub const AUTH_LOGIN_LONG_ABOUT: &str = concat!(
    banner!(),
    "Store a personal API token (ClickUp → Settings → Apps → API Token).\n\n",
    "The token is validated against ClickUp, then you pick a workspace.\n",
    "Saved to the clickupc config file (chmod 600 on Unix).\n",
    "Override at runtime with CLICKUP_TOKEN / CLICKUP_WORKSPACE_ID / CLICKUP_PROFILE.\n\n",
    "Examples:\n",
    "  clickupc auth login\n",
    "  clickupc auth login --profile work --token pk_… --workspace \"Acme\""
);

pub const AUTH_STATUS_LONG_ABOUT: &str = concat!(
    banner!(),
    "Show the active profile, workspace, user and token presence.\n\n",
    "  clickupc auth status\n",
    "  clickupc auth status --profile work --json"
);

pub const MCP_LONG_ABOUT: &str = concat!(
    banner!(),
    "Run clickupc as an MCP server and register it with AI clients.\n\n",
    "  clickupc mcp install                        Interactive: checks prerequisites, then a client picker\n",
    "  clickupc mcp install --client cursor        Non-interactive\n",
    "  clickupc mcp doctor                         Binary, credentials and client config checks\n",
    "  clickupc mcp serve                          stdio transport (what clients launch)\n",
    "  clickupc mcp serve --transport streamable-http --port 8787"
);

/// Plain banner for interactive flows.
pub const BANNER: &str = banner!();
