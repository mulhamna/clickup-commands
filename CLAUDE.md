# CLAUDE.md — Rust ClickUp CLI + MCP

Guide for Claude (and contributors) when working in this repo.

When in doubt about *how* something works here (error types, config loading, retry loop, rmcp wiring, `mcp install`), follow the existing code in `crates/`. When in doubt about *what* ClickUp does, read `docs/CLICKUP-API.md`, then the official reference.

---

## Claude rules — MUST follow

Claude may:
- Create and edit files on the filesystem
- Run `cargo` commands for build/test/check
- Run `git` commands for read and write operations when explicitly requested by the repo owner, including `git add`, `git commit`, and `git push`
- Do not manually bump versions, edit generated changelogs, create/push tags, or rewrite git history unless explicitly requested by the repo owner

Claude must:
- Work phase by phase from `docs/ROADMAP.md`. Do not start a phase until the previous phase's acceptance criteria are green.
- Never call the real ClickUp API from tests. Use `wiremock` with `CLICKUP_BASE_URL` pointed at the mock server.
- Never print, log, or commit a token. Tokens start with `pk_`; redact anything matching `pk_[A-Za-z0-9_]+` in debug output.
- Verify any endpoint marked **(verify)** in `docs/CLICKUP-API.md` against the official reference before implementing it, then remove the marker.

---

## Common commands

```bash
cargo build --all                                   # build workspace
cargo test --all                                    # run all tests
cargo test -p clickupc-core client::                 # tests matching a path in one crate
cargo run -p clickupc -- task list --list "Sprint 12" # run the CLI binary
cargo run -p clickupc -- mcp serve                  # run the MCP server (stdio)
cargo fmt --all                                     # format
cargo clippy --all-targets --all-features -- -D warnings  # lint
```

---

## Project overview

**One binary, `clickupc`**, that is both a CLI and an MCP server (`clickupc mcp serve`) for ClickUp, built on the ClickUp Public API v2 (+ v3 where v2 has no equivalent) with a **personal API token**.

Why it exists: ClickUp's official MCP server (`mcp.clickup.com`) is OAuth-only, has a rolling 24-hour call cap per workspace without the Everything AI add-on (100 on Free, 300 on Unlimited, 1,000 on Business), and has no delete tools. Going straight to the Public API removes the daily cap (only the per-minute Public API limit applies), allows token auth, and allows deletes.

### Names (fixed — don't rename)

Brand is **`clickupc`**. Every published name uses it, so nothing collides with existing `clickup*` crates or looks like an official ClickUp package.

| Thing | Name |
| --- | --- |
| GitHub repo | `mulhamna/clickup-commands` |
| Library crate | `clickupc-core` |
| CLI crate + binary | `clickupc` |
| MCP crate (library only, linked into `clickupc`) | `clickupc-mcp` |
| npm | `@mulham28/clickupc` (one package, one binary) |
| Config dir | `~/.config/clickupc/` |
| Cache dir | `~/.cache/clickupc/` |
| Env prefix | `CLICKUP_` (e.g. `CLICKUP_TOKEN`) — users already know this prefix |

All package descriptions say "Unofficial" (e.g. "Unofficial ClickUp CLI…") — ClickUp is a trademark.

### Workspace structure

```
crates/
├── clickupc-core/               # PUBLIC LIBRARY (crates.io: "clickupc-core")
│   └── src/
│       ├── lib.rs
│       ├── client/             # ClickUpClient — split by resource
│       │   ├── mod.rs          # http plumbing, retry, pagination helpers
│       │   ├── hierarchy.rs    # workspaces, spaces, folders, lists
│       │   ├── task.rs         # task CRUD, search, move, merge
│       │   ├── comment.rs
│       │   ├── field.rs        # custom fields
│       │   ├── time.rs         # time entries
│       │   ├── relation.rs     # tags, links, dependencies, checklists
│       │   ├── member.rs
│       │   └── doc.rs          # v3 docs
│       ├── config.rs           # ClickUpConfig loader (figment: toml + env, multi-profile)
│       ├── error.rs            # ClickUpError + Result alias
│       ├── rate_limit.rs       # X-RateLimit-* tracking + 429 backoff
│       ├── task_ref.rs         # parse task id / custom id / task URL
│       ├── filter.rs           # TaskFilter → query string (incl. custom_fields JSON)
│       ├── resolve.rs          # name → id resolution (lists, statuses, members, fields) + cache
│       ├── time.rs             # human date input → unix ms, ms → display
│       └── model/              # task, list, space, folder, status, user, comment, field, time_entry, doc
├── clickupc/                   # BINARY (crates.io: "clickupc", binary: clickupc)
│   └── src/
│       ├── main.rs
│       ├── output.rs           # table / json rendering
│       └── cli/                # clap commands: auth, workspace, space, folder, list, task, comment,
│                               #   field, time, member, doc, api, config, mcp
└── clickupc-mcp/               # MCP SERVER LIBRARY (crates.io: "clickupc-mcp", no binary)
    └── src/
        ├── lib.rs              # pub run_stdio / run_streamable_http
        ├── server.rs           # rmcp #[tool] definitions
        ├── models.rs           # tool I/O types (schemars)
        ├── error.rs            # AppError with stable codes
        └── app/                # one module per resource, thin layer over clickupc-core
```

No TUI in v1 (see ROADMAP "Later").

### Crate responsibilities

- **`clickupc-core`** — everything that talks to ClickUp. No `println!`, no clap, no rmcp. Returns typed models and `ClickUpError`.
- **`clickupc/`** — clap parsing, interactive prompts (`inquire`) for missing args and delete confirmation, table/JSON output. Binary: `clickupc`. Includes `clickupc mcp doctor` and `clickupc mcp install --client <target>` (registration via `kurir`).
- **`clickupc-mcp/`** — rmcp server exposing `clickupc-core` as tools, as a **library**. Every tool name is prefixed `clickup_`. It has no `main.rs`; the `clickupc` binary calls it from `clickupc mcp serve`.

### Single binary, single release

There is exactly one binary, one release lane and one npm package:

- One binary: `clickupc`. MCP lives under `clickupc mcp serve|doctor|install`.
- One version for the whole workspace (`/VERSION`), one tag lane `vX.Y.Z`, one GitHub Release with one archive per platform, one npm package.
- `clickupc-mcp` stays a separate crate only for code organisation and testing; it's published to crates.io solely because `clickupc` depends on it.
- MCP client configs point at `clickupc` with args `["mcp", "serve"]`.

Don't add a second `[[bin]]`, a second VERSION file, or a second release-please component.

---

## ClickUp API — implementation rules

Full notes: `docs/CLICKUP-API.md`. The rules below are the ones that bite.

- **Auth header**: `Authorization: pk_xxx` — raw token, **no `Bearer` prefix** for personal tokens. (OAuth tokens use `Bearer`; OAuth is out of scope for v1.)
- **Base URL**: `https://api.clickup.com/api`, then `/v2/...` or `/v3/...`. Overridable via `CLICKUP_BASE_URL` (tests). Keep v2 and v3 calls behind client methods; never build URLs in the CLI or MCP crates.
- **Team = Workspace**: v2 calls it `team` / `team_id`; v3 calls it `workspace_id`. Same number. Use `workspace_id` everywhere in our code and only translate at the HTTP layer.
- **Rate limit**: per token per minute (100 on Free/Unlimited/Business, 1,000 Business Plus, 10,000 Enterprise). On 429, read `X-RateLimit-Reset` (unix seconds), sleep until then (+ small jitter), retry. Max 3 retries, then `ClickUpError::RateLimit`. ClickUp documents only the `X-RateLimit-*` headers, not `Retry-After` (honor `Retry-After` too if it ever shows up).
- **Timestamps**: always Unix **milliseconds** (dates, filters, durations, `time_estimate`). Responses often return them as **strings** — deserialize with a string-or-number helper.
- **Pagination**:
  - Tasks: `page` starting at 0, max 100 per page. Stop when `last_page == true` *or* the page has fewer than 100 tasks. Hard cap 500 pages.
  - Comments: 25 per page, cursor = `start` (date of last comment) + `start_id` (id of last comment).
- **Custom task IDs** (e.g. `DEV-123`): any task endpoint must add `custom_task_ids=true&team_id={workspace_id}`. `task_ref.rs` decides this — callers never do.
- **Descriptions**: write with `markdown_content`, read with `include_markdown_description=true` → `markdown_description`. No ADF-style conversion needed.
- **Custom fields**: can be set on create, but **cannot be updated via Update Task** — use `POST /v2/task/{id}/field/{field_id}`. Resolve field names → ids at runtime per list; never hardcode field UUIDs.
- **Assignees on update**: `{ "add": [...], "rem": [...] }`, not a plain array (plain array on create).
- **Statuses** are per-list strings. Validate against the list's statuses (`GET /v2/list/{id}`) before sending, and fuzzy-suggest on mismatch.
- **Priority**: 1 Urgent, 2 High, 3 Normal, 4 Low, `null` = none. Not customizable.
- **Destructive ops** (delete task/comment/field value/time entry/link/dependency): CLI asks for confirmation unless `--yes`; MCP requires `confirm: true`.

---

## Smoke test

Claude runs this before reporting to the repo owner. Fix until all green.

```bash
cargo fmt --all -- --check && \
cargo clippy --all-targets --all-features -- -D warnings && \
cargo test --all && \
cargo build --all
```

---

## Conventional Commits

Format: `<type>(<scope>): <description>` — in English. Scopes: `core`, `cli`, `mcp`, `docs`, `ci`.

| Type                              | Bump  |
| --------------------------------- | ----- |
| `feat:`                           | MINOR |
| `fix:`, `perf:`, `refactor:`      | PATCH |
| `feat!:` / `BREAKING CHANGE:`     | MAJOR |
| `chore:`, `docs:`, `ci:`, `test:` | none  |

### Release flow (single lane)

```
feature PRs (conventional commits) ─► main
        │
        ▼  maintainer runs "release-please" (workflow_dispatch)
release PR: VERSION, Cargo.toml (x-release-please-version), packaging/npm/package.json,
            CHANGELOG.md, Cargo.lock (re-synced by the workflow)
        │
        ▼  merge release PR → release-please tags vX.Y.Z + creates the GitHub Release
release.yml (tag push):
  validate-tag ─► build ×6 (+ provenance) ─► github-release (archives + checksums.txt)
                                               ├─► publish-crates  (OIDC: core → mcp → clickupc)
                                               ├─► publish-npm     (OIDC, @mulham28/clickupc)
                                               └─► update-packages (brew tap, scoop bucket)
```

- Never bump versions, edit `CHANGELOG.md`, or push tags by hand; release-please owns all of it.
- crates.io and npm publish with **OIDC trusted publishing** from the `release` environment. There are no publish tokens in secrets. Repo secrets are PATs only: `RELEASE_PLEASE_TOKEN` (the tag push must trigger workflows) and `PACKAGES_TOKEN` (push to the Homebrew tap and Scoop bucket).
- Every publish step is idempotent, so a failed release can be re-run from the Actions tab (`workflow_dispatch` with the tag and `dry_run: false`). `dry_run: true` builds everything and publishes nothing.
- Don't add a second release-please package, a second `VERSION`, or a second tag pattern.

---

## Docs index

| File | What |
| --- | --- |
| `docs/ROADMAP.md` | Phases, tasks, acceptance criteria — **start here** |
| `docs/ARCHITECTURE.md` | Module design, key types, data flow |
| `docs/CLICKUP-API.md` | Endpoints, payloads, gotchas |
| `docs/CLI.md` | `clickupc` command spec |
| `docs/MCP.md` | MCP server (`clickupc mcp serve`) tool spec |
