# clickupc

[![crates.io](https://img.shields.io/crates/v/clickupc.svg)](https://crates.io/crates/clickupc)
[![npm](https://img.shields.io/npm/v/@mulham28/clickupc.svg?label=npm)](https://www.npmjs.com/package/@mulham28/clickupc)
[![CI](https://github.com/mulhamna/clickup-commands/actions/workflows/ci.yml/badge.svg)](https://github.com/mulhamna/clickup-commands/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/mulhamna/clickup-commands/blob/main/LICENSE)

**ClickUp from your terminal — and from your AI agent.** One binary: a fast CLI for humans and scripts, plus an MCP server with 55 tools (`clickupc mcp serve`).

```bash
cargo install clickupc
clickupc auth login            # personal API token: ClickUp → Settings → Apps → API Token
clickupc task list --mine --status "in progress"
clickupc mcp install           # register the MCP server with Claude, Cursor, Codex, …
```

## Highlights

- **Names, not ids** — lists, statuses, people (`me`, username, email), custom fields and dropdown options are resolved for you, with "did you mean" suggestions.
- **Human dates** — `--due fri`, `--updated-after -2w`, `--due-before eow`, `--estimate 1h30m`.
- **Tasks end to end** — search with filters (status, assignee, tag, dates, custom fields), create, update, move between lists, tags, links, dependencies, checklists, attachments.
- **Bulk & batch** — `task bulk update|comment` with `--dry-run`, mixed operations from a JSON file, bounded concurrency with a progress bar.
- **Time tracking & reports** — timers, manual entries, `time report --by task|list|day`, `standup`, `summary`.
- **Docs** — search, read, create and edit ClickUp Docs pages in markdown.
- **Rate-limit aware** — cached hierarchy, automatic retry after `X-RateLimit-Reset`.
- **Safe & scriptable** — deletes confirm unless `--yes`, `--json` on every read, stable exit codes, `clickupc api` for raw `/v2` / `/v3` calls.

## MCP server

```bash
clickupc mcp install --client claude-code   # or cursor, codex, gemini-cli, vscode, claude-desktop, … (17 targets)
```

```json
{ "mcpServers": { "clickup": { "command": "clickupc", "args": ["mcp", "serve"] } } }
```

55 `clickup_*` tools across tasks, bulk, comments, custom fields, relations, time, reports and docs. Inputs accept names and human dates; destructive tools require `confirm: true`; the token is read from the clickupc config and never written into client configs. Stdio by default, Streamable HTTP with `--transport streamable-http`.

## Other ways to install

Homebrew `brew install mulhamna/tap/clickupc` · Scoop `scoop install mulhamna/clickupc` · npm `npm i -g @mulham28/clickupc` · [install script and release archives](https://github.com/mulhamna/clickup-commands#install).

## Links

[README & CLI tour](https://github.com/mulhamna/clickup-commands#readme) · [CLI spec](https://github.com/mulhamna/clickup-commands/blob/main/docs/CLI.md) · [MCP tools](https://github.com/mulhamna/clickup-commands/blob/main/docs/MCP.md) · [Changelog](https://github.com/mulhamna/clickup-commands/blob/main/CHANGELOG.md)

MIT licensed. Not affiliated with or endorsed by ClickUp.
