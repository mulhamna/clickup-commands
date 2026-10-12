# Roadmap

Scope for v1: **core + CLI + MCP**. No TUI.

Work one phase at a time. A phase is done when its acceptance criteria pass **and** the smoke test is green:

```bash
cargo fmt --all -- --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all && cargo build --all
```

Commit per task with Conventional Commits (`feat(core): …`, `feat(cli): …`, `feat(mcp): …`). Stop and report to the repo owner at the end of each phase.

---

## Phase 0 — Scaffold

- [x] Cargo workspace with `crates/clickupc-core` (lib), `crates/clickupc-mcp` (lib, no binary), `crates/clickupc` (the only binary) (names per `CLAUDE.md`). Edition 2021, `license = "MIT"`, version `0.1.0` in `[workspace.package]`, shared deps in `[workspace.dependencies]`.
- [x] Dependencies: `reqwest` (rustls, json, multipart), `tokio`, `serde`, `serde_json`, `figment`, `toml`, `dirs`, `thiserror`, `anyhow`, `tracing`, `chrono`, `chrono-tz`, `url`, `strsim`, `mime_guess`; CLI: `clap`, `inquire`, `indicatif`, `open`, `comfy-table`; MCP: `rmcp`, `schemars`, `axum`, `tracing-subscriber`; dev: `wiremock`, `tempfile`, `serial_test`.
- [x] `LICENSE` (MIT), `.gitignore`, `README.md`, `CLAUDE.md`, `AGENTS.md`.
- [x] CI: copy `ci.yml` (fmt/clippy/test/build matrix), `security.yml`, `deny.toml`, `dependabot.yml`.
- [x] `clickupc --version` runs; `clickupc mcp serve` starts and answers `initialize` over stdio (empty tool list is fine).

**Accept:** workspace builds on CI matrix; exactly one binary is produced.

## Phase 1 — Core foundation + auth + hierarchy

Core:
- [x] `config.rs`: profiles, env overrides, chmod-600 warning, save/load round-trip. Tests ported from `config_auth_priority.rs`.
- [x] `error.rs`: `ClickUpError` per ARCHITECTURE.
- [x] `client/mod.rs`: `ClickUpClient`, v2/v3 helpers, `Query` builder (repeated `key[]=`), error-body parsing (v2 + v3), token redaction in debug logs.
- [x] `rate_limit.rs`: header tracking, proactive wait, 429 retry ×3 with jitter, shared `Arc`, bounded concurrency.
- [x] `model/`: user, workspace (+members), space, folder, list (+statuses), status. `string_or_i64` helper.
- [x] `client/hierarchy.rs` + `client/member.rs`: user, workspaces, spaces, folders, lists (foldered + folderless), list detail, list members.
- [x] `resolve.rs`: list/space/folder/member resolution, qualified paths, aliases, fuzzy suggestions, ambiguity error, disk cache with TTL + refresh-on-miss.
- [x] `time.rs`: `parse_when`, `parse_duration`, display helpers.

CLI:
- [x] `auth login|status|logout|profile`, `whoami`, `workspace list|use`, `tree`, `space list`, `folder list`, `list list|view`, `member list|find`, `alias`, `cache clear`, `config get|set`, `api`.
- [x] `--json` on every read command; exit codes per ARCHITECTURE.

MCP:
- [x] `clickupc-mcp/src/lib.rs` ported (stdio + streamable HTTP); `clickupc mcp serve` wired to it; stdout kept clean in stdio mode (test: no bytes on stdout before `initialize`).
- [x] `error.rs`, `app/shared.rs` ported.
- [x] Tools: `clickup_auth_status`, `clickup_whoami`, `clickup_workspace_list`, `clickup_hierarchy`, `clickup_list_get`, `clickup_member_list`, `clickup_member_resolve`, `clickup_api_request`.

**Accept:**
- wiremock tests: auth header is raw `pk_` (no Bearer); 429 then 200 succeeds; 429 ×4 → `RateLimit`; v2 and v3 error bodies parsed; hierarchy walk issues the expected calls and second run hits cache (zero HTTP).
- Resolver tests: exact id, alias, qualified path, unique name, ambiguous name (error lists both paths), typo (suggestion).
- `clickupc auth login` against a real account works end-to-end (manual check by owner).

## Phase 2 — Tasks

Core:
- [x] `task_ref.rs` + exhaustive unit tests (ids, custom ids, both URL forms, `#` prefix, garbage input).
- [x] `model/task.rs` (summary + full), custom field value display.
- [x] `filter.rs`: `TaskFilter` → query; endpoint selection; custom_fields JSON encoding.
- [x] `client/task.rs`: get, search (pagination: `last_page` or <100, 500-page cap, `limit`), create, update (add/rem shape, always both arrays), delete, set status, move (v3 home_list with status mappings).
- [x] `resolve.rs`: status resolution per list (case-insensitive, canonical spelling back), task type resolution.

CLI:
- [x] `task list|view|create|update|status|assign|unassign|move|delete|open` per `docs/CLI.md`, all filter flags, interactive pickers on TTY, delete confirmation.

MCP:
- [x] `clickup_task_search`, `clickup_task_get`, `clickup_task_create`, `clickup_task_update`, `clickup_task_set_status`, `clickup_task_move`, `clickup_task_delete`.

**Accept:**
- wiremock tests assert exact query strings for: single-list search (list endpoint), multi-list search (team endpoint), statuses with spaces, `include_closed`, date ranges in ms, custom_fields JSON; pagination stops on `last_page` and on short page.
- Custom-id task operations send `custom_task_ids=true&team_id=…`.
- Update with only `--assign` sends `{"assignees":{"add":[…],"rem":[]}}`.
- `task status` with a typo returns a suggestion; `task move` without needed mapping errors with target statuses.
- MCP: `clickup_task_delete` without `confirm` → `unsafe_operation`.

## Phase 3 — Collaboration: comments, attachments, custom fields, relations, time

Core + CLI + MCP for:
- [x] Comments: list (cursor pagination), add, update, delete.
- [x] Attachments: upload (multipart field `attachment`).
- [x] Custom fields: list per list, set (value conversion per type: dropdown/label option names → UUIDs, dates, numbers, money, users add/rem, checkbox, url/email/text), clear. `task create|update --field` wired in.
- [x] Tags add/remove, links add/remove, dependencies add/remove.
- [x] Time: list, add, start, stop, current, delete (negative duration = running).

**Accept:**
- Field value conversion unit-tested for every listed type; unknown option name → `Resolve` error with options.
- Comment pagination test: 60 comments across 3 pages via `start`/`start_id`.
- Every endpoint marked **(verify)** that this phase uses has been checked and the marker removed from `docs/CLICKUP-API.md`.

## Phase 4 — Bulk, checklists, reports, extras

- [x] Bulk update / bulk comment / batch file (CLI) and `clickup_task_bulk_create`, `clickup_task_bulk_update`, `clickup_task_batch` (MCP): dry-run, confirmation, bounded concurrency, progress bar, per-item results.
- [x] Checklists (create, add items, resolve/unresolve).
- [x] Threaded comment replies.
- [x] TIML add/remove list.
- [x] Time in status (single + bulk).
- [x] `list create`, `folder create`.
- [x] Reports: `standup`, `summary` (CLI) + `clickup_standup`, `clickup_list_summary` (MCP). `time report` aggregation.

**Accept:** bulk update of 250 mocked tasks completes with a simulated 429 mid-way and correct final summary; dry-run issues zero mutating requests.

## Phase 5 — Docs API + MCP install

- [x] v3 Docs: search, get, pages listing, get pages, create doc, create page, edit page (read the "Docs API limitation" guide first).
- [x] `clickupc mcp doctor` and `clickupc mcp install --client …` via kurir (all client targets).
- [ ] MCP tool descriptions reviewed end-to-end by running the server in Claude Code against a real workspace (owner).

**Accept:** `clickupc mcp install --client claude-code` produces a working server entry without embedding the token.

## Phase 6 — Release & distribution

Single lane, single binary, names per `CLAUDE.md` "Names":
- [x] release-please with **one** lane: tag `vX.Y.Z`, `/VERSION` drives all three crates (generic updater on each `Cargo.toml`). No `separate-pull-requests`, no MCP manifest.
- [x] Release workflows building binaries (linux/macos/windows, x86_64 + aarch64), crates.io publish order `clickupc-core` → `clickupc-mcp` → `clickupc`, via OIDC trusted publishing.
- [x] One npm wrapper `@mulham28/clickupc` (bin: `clickupc`); one brew formula, one scoop manifest — each installs the single binary, which covers both CLI and MCP.
- [x] install.sh / install.ps1 (checksum-verified).
- [ ] Docker (deferred).
- [ ] Claude Code plugin (`plugin/`) with ClickUp skills: list-tasks, view-task, create-task, update-task, set-status, bulk-update, comment, time, fields, standup, api.
- [x] Add a release-flow section to CLAUDE.md.

**Accept:** a dry-run release on a fork publishes nothing but builds all artifacts.

---

## Later (not v1)

- TUI (columns = statuses of a list).
- OAuth login (needed only if distributing as a hosted app).
- Webhooks-based notifications.
- Rich comment formatting + @mentions.
- Goals, Views, Chat.
