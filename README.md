# clickupc

**Unofficial** ClickUp CLI and MCP server in one Rust binary.

> Status: pre-1.0. See [`docs/ROADMAP.md`](docs/ROADMAP.md).

One download gives you both:

- **`clickupc …`** — a fast CLI for tasks, comments, custom fields, time tracking and bulk operations.
- **`clickupc mcp serve`** — an MCP server so Claude, Cursor, Codex, Gemini CLI and friends can work in your ClickUp.

## Why not ClickUp's official MCP server?

| | Official MCP (`mcp.clickup.com`) | clickupc |
| --- | --- | --- |
| Auth | OAuth only | Personal API token (`pk_…`) |
| Limit (without Everything AI add-on) | Rolling 24h cap per workspace: 100 (Free), 300 (Unlimited), 1,000 (Business), … | Public API per-minute limit only: 100/min (Free–Business), 1,000/min (Business Plus), 10,000/min (Enterprise) |
| Delete tasks / comments / time entries | No | Yes (with explicit confirmation) |
| CLI for humans and scripts | No | Yes, same binary |
| Raw API escape hatch | No | `clickupc api` / `clickup_api_request` |

Rate limits are handled automatically: on HTTP 429 the client waits until `X-RateLimit-Reset` and retries.

## Install

Every channel installs the same single binary.

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/mulhamna/clickup-commands/main/install.sh | bash

# Windows (PowerShell)
irm https://raw.githubusercontent.com/mulhamna/clickup-commands/main/install.ps1 | iex
```

Both scripts verify the download against the release's `checksums.txt`. Pin a version with `CLICKUPC_VERSION=v0.1.0` (sh) or `-Version v0.1.0` (PowerShell).

```bash
npm i -g @mulham28/clickupc              # npm
cargo install clickupc                   # crates.io
brew install mulhamna/tap/clickupc       # Homebrew (macOS / Linux)
scoop bucket add mulhamna https://github.com/mulhamna/scoop-bucket
scoop install mulhamna/clickupc          # Scoop (Windows)
winget install mulhamna.clickupc         # winget (once accepted into winget-pkgs)
```

Prebuilt archives for Linux, macOS and Windows (x86_64 + aarch64) are on the [Releases](https://github.com/mulhamna/clickup-commands/releases) page with a `checksums.txt` and build provenance attestations.

## Quick start

```bash
clickupc auth login                       # paste your token: ClickUp → Settings → Apps → API Token
clickupc tree                             # see Spaces → Folders → Lists
clickupc alias set sprint "Engineering/Sprints/Sprint 12"

clickupc task list --list sprint --mine
clickupc task create --list sprint --name "Fix login redirect" --priority high --due fri
clickupc task status DEV-123 "in review"
clickupc comment add DEV-123 "Deployed to staging"
clickupc time add DEV-123 1h30m --note "debugging"
```

Names work everywhere ids do: lists, statuses, people (`me`, name, email) and custom fields are resolved for you, with "did you mean" suggestions on typos.

## Use with an AI client

```bash
clickupc mcp install --client claude-code   # or claude-desktop, cursor, codex, gemini-cli, vscode, …
clickupc mcp doctor
```

Or by hand:

```json
{
  "mcpServers": {
    "clickup": { "command": "clickupc", "args": ["mcp", "serve"] }
  }
}
```

Your token stays in `~/.config/clickupc/config.toml` (or `CLICKUP_TOKEN`); it is never written into client configs.

## Configuration

| Env var | Purpose |
| --- | --- |
| `CLICKUP_TOKEN` | Personal API token |
| `CLICKUP_WORKSPACE_ID` | Workspace (team) id |
| `CLICKUP_PROFILE` | Profile name in `config.toml` |
| `CLICKUP_DEFAULT_LIST` | List used when `--list` is omitted |
| `CLICKUP_TIMEOUT_SECS` | HTTP timeout |
| `CLICKUP_TASK_LIMIT` | Default cap for list-like commands |
| `CLICKUP_TIMEZONE` | IANA timezone for date input and display |

## Docs

- [Roadmap](docs/ROADMAP.md) · [Architecture](docs/ARCHITECTURE.md) · [CLI spec](docs/CLI.md) · [MCP tools](docs/MCP.md)
- [ClickUp API notes](docs/CLICKUP-API.md)

## License

MIT. ClickUp is a trademark of its owner; this project is not affiliated with or endorsed by ClickUp.
