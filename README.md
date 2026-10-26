<div align="center">

```
        _ _      _
    ___| (_) ___| | ___   _ _ __   ___
   / __| | |/ __| |/ / | | | '_ \ / __|
  | (__| | | (__|   <| |_| | |_) | (__
   \___|_|_|\___|_|\_\\__,_| .__/ \___|
                           |_|
```

**ClickUp from your terminal — and from your AI agent.**
One Rust binary: a fast CLI for humans and scripts, plus an MCP server with 56 tools.

[![CI](https://github.com/mulhamna/clickup-commands/actions/workflows/ci.yml/badge.svg)](https://github.com/mulhamna/clickup-commands/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/clickupc.svg?label=crates.io)](https://crates.io/crates/clickupc)
[![npm](https://img.shields.io/npm/v/@mulham28/clickupc.svg?label=npm)](https://www.npmjs.com/package/@mulham28/clickupc)
[![docs.rs](https://img.shields.io/docsrs/clickupc-core?label=docs.rs)](https://docs.rs/clickupc-core)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

[Install](#install) · [Quick start](#quick-start) · [CLI tour](#cli-tour) · [MCP server](#mcp-server) · [Configuration](#configuration) · [Docs](#docs)

</div>

---

```console
$ clickupc task list --list sprint --mine --status "in progress"
 ID        Name                        Status       Assignees  Due         Priority  List
 DEV-412   Fix login redirect loop     in progress  ana        2026-10-16  high      Sprint 12
 DEV-398   Rate-limit webhook retries  in progress  ana        2026-10-17  normal    Sprint 12

$ clickupc task status DEV-412 "in reveiw"
error: Could not resolve status 'in reveiw' (did you mean: in review?)
```

## Why clickupc

- **Names, not ids.** Lists, statuses, people (`me`, username, email), custom fields and dropdown options are resolved for you — with "did you mean" suggestions and a hard error on ambiguous names, never a silent guess.
- **Human dates.** `--due fri`, `--due "2026-10-20 14:00"`, `--updated-after -2w`, `--due-before eow`, `--estimate 1h30m`.
- **Built for the 100 req/min budget.** Hierarchy and fields are cached, 429s are retried after `X-RateLimit-Reset`, and bulk work runs with bounded concurrency.
- **Agent-ready.** `clickupc mcp serve` exposes 56 typed `clickup_*` tools to Claude, Cursor, Codex, Gemini CLI, VS Code and more; `clickupc mcp install` registers it in one command.
- **Multiple accounts.** One profile per token; switch with `--profile`, hop workspaces with `--workspace`, and let an agent pick the account per tool call.
- **Safe by default.** Deletes confirm on the CLI (`--yes` to skip) and require `confirm: true` over MCP. Your token is never printed, logged or written into client configs.
- **Scriptable.** `--json` on every read command, stable exit codes, and a raw `clickupc api` escape hatch for anything not wrapped yet.

### clickupc vs ClickUp's official MCP server

| | Official MCP (`mcp.clickup.com`) | clickupc |
| --- | --- | --- |
| Auth | OAuth only | Personal API token (`pk_…`) |
| Usage limit (without the Everything AI add-on) | Rolling 24h cap per workspace: 100 (Free), 300 (Unlimited), 1,000 (Business), … | Only the Public API per-minute limit: 100/min (Free–Business), 1,000/min (Business Plus), 10,000/min (Enterprise) |
| Delete tasks / comments / time entries | No | Yes, with explicit confirmation |
| A CLI for humans and scripts | No | Yes, same binary |
| Raw API escape hatch | No | `clickupc api` / `clickup_api_request` |

## Install

Every channel installs the same single binary (Linux, macOS, Windows — x86_64 and aarch64).

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/mulhamna/clickup-commands/main/install.sh | bash
```

```powershell
# Windows (PowerShell)
irm https://raw.githubusercontent.com/mulhamna/clickup-commands/main/install.ps1 | iex
```

| Channel | Command |
| --- | --- |
| Homebrew | `brew install mulhamna/tap/clickupc` |
| Scoop | `scoop bucket add mulhamna https://github.com/mulhamna/scoop-bucket` then `scoop install mulhamna/clickupc` |
| npm | `npm i -g @mulham28/clickupc` |
| Cargo | `cargo install clickupc` |
| Manual | archives on the [Releases](https://github.com/mulhamna/clickup-commands/releases) page |

The install scripts and the npm package verify every download against the release's `checksums.txt` (more options, upgrades and uninstalling: [INSTALL.md](INSTALL.md)); release archives also carry [build provenance attestations](https://github.com/mulhamna/clickup-commands/attestations). Pin a version with `CLICKUPC_VERSION=v1.0.1` (sh) or `-Version v1.0.1` (PowerShell).

## Quick start

```bash
clickupc auth login                 # paste your token (ClickUp → Settings → Apps → API Token), pick a workspace
clickupc tree                       # Spaces → Folders → Lists, with ids
clickupc alias set sprint "Engineering/Sprints/Sprint 12"

clickupc task list --list sprint --mine
clickupc task create --list sprint --name "Fix login redirect" --priority high --due fri --assignee me
clickupc task status DEV-123 "in review"
clickupc comment add DEV-123 "Deployed to staging"
clickupc time add DEV-123 1h30m --note "debugging"
clickupc standup
```

Wire it into your AI client:

```bash
clickupc mcp install                # interactive: checks prerequisites, then pick a client
clickupc mcp install --client claude-code
```

## CLI tour

Tasks can be referenced by id (`86b1abcde`), custom id (`DEV-123`) or URL (`https://app.clickup.com/t/…`).

**Find work**

```bash
clickupc task list --mine --status "in progress"
clickupc task list --list sprint --overdue
clickupc task list --space Engineering --tag bug --field 'Severity=High' --json
clickupc task list --list sprint --field 'Points>=3' --order-by due --limit 20
clickupc task view DEV-123 --comments --subtasks
```

**Change work**

```bash
clickupc task update DEV-123 --assign ana --due +2d --estimate 3h --field 'Severity=Low'
clickupc task move DEV-123 --to "Engineering/Done" --map 'review=closed'
clickupc task tag DEV-123 add needs-qa
clickupc task dep DEV-123 add --waiting-on DEV-120
clickupc task checklist DEV-123 add "Release" --item "Changelog" --item "Tag"
clickupc attach DEV-123 ./screenshot.png
clickupc task delete DEV-123            # asks first; --yes to skip
```

**Many tasks at once**

```bash
clickupc task bulk update --list sprint --status "in review" --set-status done --dry-run
clickupc task bulk comment --list sprint --overdue --text "Please update the ETA"
clickupc task batch --file ops.json     # mixed create / update / status / comment / delete
```

**Time, fields, reports, docs**

```bash
clickupc time start DEV-123 --note "pairing"   &&   clickupc time stop
clickupc time report --from mon --by task
clickupc field list --list sprint
clickupc field set DEV-123 "Due Window" "next fri"
clickupc summary --list sprint                 # counts by status / assignee, overdue
clickupc doc search roadmap
clickupc doc page edit <doc-id> <page-id> --content-file notes.md --mode append
```

**Escape hatch**

```bash
clickupc api GET /v2/team
clickupc api PUT "/v3/workspaces/{workspace}/tasks/abc/home_list/456" --data @body.json
```

<details>
<summary>All commands</summary>

| Command | What it does |
| --- | --- |
| `auth login \| status \| logout \| profile` | Token, workspace and profile management |
| `whoami` | Current user, workspace and plan |
| `workspace list \| use` | Switch workspaces |
| `tree`, `space`, `folder`, `list` | Browse and create hierarchy; `list view` shows statuses, fields and members |
| `alias set \| list \| remove` | Short names for lists |
| `member list \| find` | Workspace or list members |
| `task …` | `list`, `view`, `create`, `update`, `status`, `assign`, `unassign`, `move`, `add-to`, `remove-from`, `delete`, `open`, `tag`, `link`, `dep`, `checklist`, `bulk`, `batch` |
| `comment …` | `list`, `add`, `update`, `delete`, `reply`, `replies` |
| `attach` | Upload files |
| `field list \| set \| clear` | Custom fields with human values |
| `time …` | `list`, `add`, `start`, `stop`, `current`, `delete`, `report` |
| `standup`, `summary` | Reports |
| `doc …` | `search`, `view`, `create`, `page create`, `page edit` |
| `api` | Raw `/v2` or `/v3` request |
| `config get \| set` | Profile settings |
| `cache clear` | Drop cached hierarchy, members and fields |
| `mcp serve \| install \| doctor` | MCP server |
| `update [--check]` | Upgrade to the latest release |

Full spec: [docs/CLI.md](docs/CLI.md).

</details>

## MCP server

`clickupc mcp serve` speaks MCP over **stdio** (default) or **Streamable HTTP** (`--transport streamable-http --port 8787`). It reads the same config and token as the CLI.

```bash
clickupc mcp install                        # pick clients and profiles with checkboxes (logs you in first if needed)
clickupc mcp install --client claude-code --client cursor --server-profile work --server-profile personal
clickupc mcp doctor                         # binary on PATH, credentials, client config
```

Or add it by hand:

```json
{
  "mcpServers": {
    "clickupc": { "command": "clickupc", "args": ["mcp", "serve"] }
  }
}
```

| Area | Tools |
| --- | --- |
| Account & hierarchy | `profile_list`, `auth_status`, `whoami`, `workspace_list`, `hierarchy`, `list_get`, `list_create`, `folder_create`, `member_list`, `member_resolve` |
| Tasks | `task_search`, `task_get`, `task_create`, `task_update`, `task_set_status`, `task_move`, `task_delete`, `task_add_to_list`, `task_remove_from_list` |
| Bulk | `task_bulk_create`, `task_bulk_update`, `task_batch` |
| Comments | `comment_list`, `comment_add`, `comment_update`, `comment_delete`, `comment_replies`, `comment_reply` |
| Custom fields | `field_list`, `field_set`, `field_clear` |
| Relations | `tag_add`, `tag_remove`, `link_add`, `link_remove`, `dependency_add`, `dependency_remove`, `checklist_add`, `checklist_item_update`, `attachment_upload` |
| Time | `time_list`, `time_add`, `time_start`, `time_stop`, `time_current`, `time_delete`, `time_in_status` |
| Reports | `standup`, `list_summary` |
| Docs | `doc_search`, `doc_get`, `doc_pages`, `doc_create`, `doc_page_create`, `doc_page_update` |
| Escape hatch | `api_request` |

The entry is named `clickupc`, so it sits next to ClickUp's official server (usually registered as `clickup`) without clashing; pick another name with `--name`. All tool names are prefixed `clickup_`. Inputs take names or ids and human dates, plus optional `profile` / `workspace` to switch account or workspace per call; task results are compact by default (`detail: "full"` for everything) so they don't flood the model's context. Destructive tools fail with `unsafe_operation` unless `confirm: true`, and errors carry stable codes (`resolve_error` with suggestions, `clickup_api_error` with the ClickUp `ECODE`, `rate_limited` with the reset time). Details: [docs/MCP.md](docs/MCP.md).

## Configuration

`clickupc auth login` writes a profile to the config file (permissions `600`):

| OS | Config file |
| --- | --- |
| Linux | `~/.config/clickupc/config.toml` |
| macOS | `~/Library/Application Support/clickupc/config.toml` |
| Windows | `%APPDATA%\clickupc\config.toml` |

```toml
current_profile = "work"

[profiles.work]
token = "pk_..."
workspace_id = "9012345678"
default_list = "901234567890"   # used when --list is omitted
timezone = "Asia/Jakarta"

[profiles.work.aliases]
sprint = "901234567890"
```

Environment variables override the profile:

| Variable | Purpose |
| --- | --- |
| `CLICKUP_TOKEN` | Personal API token |
| `CLICKUP_WORKSPACE_ID` | Workspace (team) id |
| `CLICKUP_PROFILE` | Profile to use (or `--profile`) |
| `CLICKUP_DEFAULT_LIST` | List used when `--list` is omitted |
| `CLICKUP_TIMEZONE` | IANA timezone for date input and display |
| `CLICKUP_TIMEOUT_SECS` | HTTP timeout |
| `CLICKUP_TASK_LIMIT` | Default cap for list-like commands |

Global flags: `--profile`, `--workspace`, `--json`, `--refresh` (bypass caches), `-y/--yes`, `-v/--verbose`.

### Multiple accounts and workspaces

Each profile is one ClickUp **user** with a default workspace; use `--workspace` to switch workspaces. A token may reach only one workspace, so running `auth login` again on the same profile with another workspace's token adds it to that profile instead of replacing the first one. `--workspace` then picks the matching token on its own.

```bash
clickupc auth login                         # on a terminal: add a new profile or update an existing one
clickupc auth profile list
clickupc auth profile use                   # picker
clickupc --profile personal task list --mine
clickupc --workspace "Side Project" task list --mine
```

For agents, `clickupc mcp install` lets you tick several profiles; each becomes its own server entry (`clickupc-work`, `clickupc-personal`) pinned to that account. Every tool also accepts optional `profile` and `workspace` arguments, and `clickup_profile_list` shows what's available.
Exit codes: `0` ok · `1` error · `2` invalid input · `3` auth · `4` not found / unresolved name · `5` rate limited.

## Security

- Personal tokens are sent only in the `Authorization` header, redacted (`pk_***`) from every error and log line, and never written into MCP client configs.
- Release binaries are built in GitHub Actions with build provenance attestations; crates.io and npm are published through OIDC trusted publishing — no long-lived publish tokens exist.
- Found a vulnerability? Please report it privately — see [SECURITY.md](SECURITY.md).

## Docs

- [Install guide](INSTALL.md) · [Contributing](CONTRIBUTING.md) · [Security policy](SECURITY.md) · [Code of conduct](CODE_OF_CONDUCT.md)
- [CLI spec](docs/CLI.md) · [MCP tools](docs/MCP.md) · [Architecture](docs/ARCHITECTURE.md) · [ClickUp API notes](docs/CLICKUP-API.md) · [Roadmap](docs/ROADMAP.md)
- Rust library: [`clickupc-core`](https://docs.rs/clickupc-core) (client, resolver, models) and [`clickupc-mcp`](https://docs.rs/clickupc-mcp) (embeddable MCP server)

## License

[MIT](LICENSE). ClickUp is a trademark of its owner; this project is not affiliated with or endorsed by ClickUp.
