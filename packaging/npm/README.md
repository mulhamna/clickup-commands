# @mulham28/clickupc

[![npm](https://img.shields.io/npm/v/@mulham28/clickupc.svg)](https://www.npmjs.com/package/@mulham28/clickupc)
[![CI](https://github.com/mulhamna/clickup-commands/actions/workflows/ci.yml/badge.svg)](https://github.com/mulhamna/clickup-commands/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/mulhamna/clickup-commands/blob/main/LICENSE)

**ClickUp from your terminal — and from your AI agent.** A fast native CLI plus an MCP server with 56 tools, in one binary.

```bash
npm i -g @mulham28/clickupc
clickupc auth login                      # personal API token: ClickUp → Settings → Apps → API Token
clickupc mcp install --client claude-code
```

## What you get

```bash
clickupc task list --mine --status "in progress"
clickupc task create --list sprint --name "Fix login redirect" --priority high --due fri
clickupc task update DEV-123 --assign ana --field 'Severity=Low'
clickupc task bulk update --list sprint --status review --set-status done --dry-run
clickupc time add DEV-123 1h30m --note "debugging"
clickupc standup
```

- **Names, not ids** — lists, statuses, people (`me`, username, email) and custom fields resolved for you, with typo suggestions.
- **Human dates** — `fri`, `tomorrow`, `+3d`, `eow`, `2026-10-20 14:00`.
- **MCP server** — `clickupc mcp serve` gives Claude, Cursor, Codex, Gemini CLI, VS Code and others 56 `clickup_*` tools: tasks, bulk, comments, custom fields, relations, time tracking, reports and Docs. Deletes require explicit confirmation.
- **No daily cap** — talks to the ClickUp Public API with your personal token, so only the per-minute rate limit applies; 429s are retried automatically.

## Use it from an MCP client

```json
{
  "mcpServers": {
    "clickupc": { "command": "clickupc", "args": ["mcp", "serve"] }
  }
}
```

Or skip `-g` and let the client fetch it:

```json
{
  "mcpServers": {
    "clickupc": { "command": "npx", "args": ["-y", "@mulham28/clickupc", "mcp", "serve"] }
  }
}
```

The token stays in the clickupc config (or `CLICKUP_TOKEN`) — it is never written into client configs.

## How the package works

On install, a small script downloads the prebuilt `clickupc` binary for your platform (Linux, macOS, Windows · x86_64 / arm64) from the matching [GitHub Release](https://github.com/mulhamna/clickup-commands/releases) and verifies its SHA-256 against the release's `checksums.txt`. Releases are built in GitHub Actions and published to npm with provenance via trusted publishing. If the binary is missing (e.g. installed with `--ignore-scripts`), run `npm rebuild -g @mulham28/clickupc`.

## More

[Full README & CLI tour](https://github.com/mulhamna/clickup-commands#readme) · [MCP tools](https://github.com/mulhamna/clickup-commands/blob/main/docs/MCP.md) · [Changelog](https://github.com/mulhamna/clickup-commands/blob/main/CHANGELOG.md) · [Issues](https://github.com/mulhamna/clickup-commands/issues)

Other installs: Homebrew `brew install mulhamna/tap/clickupc` · Scoop `scoop install mulhamna/clickupc` · Cargo `cargo install clickupc`.

MIT licensed. Not affiliated with or endorsed by ClickUp.
