# Porting map: jira-commands → clickup-commands

Source repo: <https://github.com/mulhamna/jira-commands> (v2.11.0 at time of writing). Clone it next to this repo so it can be read while implementing:

```bash
git clone --depth 1 https://github.com/mulhamna/jira-commands ../jira-commands
```

Legend: **Copy** = port with renames only · **Adapt** = same idea, different ClickUp details · **New** = no Jira equivalent · **Drop** = not needed.

## Concepts

| Jira | ClickUp | Notes |
| --- | --- | --- |
| Site / base URL | fixed `https://api.clickup.com/api` | no per-user base URL; keep `CLICKUP_BASE_URL` for tests only |
| Email + API token (Cloud), PAT (DC) | personal token `pk_…` | single auth type, raw header, no email |
| — | Workspace (`team_id`) | chosen at `auth login`, stored in profile |
| Project | Space (+ Folder) | |
| Board | List (or a view) | lists are where tasks live |
| Issue / issue key `PROJ-123` | Task / task id `86b1abcde`, optional custom id `DEV-123` | `TaskRef` handles both |
| Issue type | Custom task type (`custom_item_id`) | `0` = Task, `1` = Milestone |
| Sub-task | Subtask (`parent`) | parent must be in the same list on create |
| Workflow + transitions | Per-list statuses, set directly | no transition ids; validate status name |
| JQL | Query params on two search endpoints | `TaskFilter` + CLI filter flags |
| Custom field `customfield_10553` | Custom field UUID per list | resolved by name at runtime (same principle) |
| Labels / components | Tags (space-scoped) | |
| Fix versions | — | Drop (closest: folders/lists per release; not modeled) |
| Sprint | Sprint folders (Sprints ClickApp) | treat as lists inside a sprint folder; no special API in v1 |
| Worklog | Time entry | team-scoped endpoints, ms durations, running timers |
| Watchers | Watchers (`add`/`rem` on update) | |
| Issue links (typed) | Task links (untyped) + dependencies (blocks / waiting on) | |
| Remote links | — | Drop |
| ADF description | Markdown (`markdown_content`) | big simplification |
| Mentions / notifications scan | — | Drop for v1 (no notifications API); revisit with webhooks |
| Plans (premium) | — | Drop |
| Archive issue (async task) | `archived: true` on update | synchronous |

## Files

### jira-core → clickupc-core

| jira-core | clickupc-core | Verdict |
| --- | --- | --- |
| `config.rs` (figment, profiles, env, chmod warning) | `config.rs` | **Adapt** — drop deployment/auth_type/api_version/ca_bundle/email; add workspace_id, default_list, timezone, aliases |
| `auth.rs` | folded into `config.rs` / `client` | **Adapt** |
| `error.rs` | `error.rs` | **Adapt** — add `code`/`trace_id` to `Api`, `RateLimit { reset_at }`, new `Resolve` variant |
| `client.rs` `execute_with_retry` / `request*` | `client/mod.rs` | **Adapt** — retry on 429 reads `X-RateLimit-Reset`, not `Retry-After`; add proactive limiter |
| `client.rs` (4k lines, everything) | `client/*.rs` split by resource | **Adapt** — split from day one |
| `client.rs` pagination (`next_page_token`) | page + `last_page`; comment cursor | **Adapt** — keep the 500-iteration safeguard |
| `client.rs` multipart attachment upload | `client/task.rs` attach | **Copy** (field name `attachment`) |
| `field_cache.rs` | `resolve.rs` | **Adapt** — generalised to lists/statuses/members/fields, on-disk cache |
| `jql.rs` (JQL builder) | `filter.rs` | **Adapt** — builds query params instead of a query string |
| `adf.rs` | — | **Drop** (+ drop `fuzz/fuzz_targets/markdown_adf.rs`; add a `task_ref` fuzz target instead if fuzzing is kept) |
| `model/*` | `model/*` | **Adapt** — new shapes; drop board/component/version/remote_link/sprint models |
| `tests/config_auth_priority.rs` | `tests/config_priority.rs` | **Copy** idea |
| — | `task_ref.rs`, `rate_limit.rs`, `time.rs` | **New** |

### jira (jirac) → clickupc

| jira | clickupc | Verdict |
| --- | --- | --- |
| `cli/auth.rs` | `cli/auth.rs` | **Adapt** — token + workspace picker |
| `cli/issue.rs` + `issue/{browse,manage,collaboration,bulk}.rs` | `cli/task.rs` + `task/{list,manage,relations,bulk}.rs` | **Adapt** |
| `cli/api.rs` | `cli/api.rs` | **Copy** (+ `{workspace}` substitution) |
| `cli/config.rs` | `cli/config.rs` | **Copy** |
| `cli/mcp.rs` (`doctor`, `install --client …`) | `cli/mcp.rs` | **Copy** — rename binary/server key; keep all client targets |
| `cli/board.rs` | `cli/hierarchy.rs` (`tree`, `space`, `folder`, `list`) | **Adapt** |
| `cli/plan.rs` | — | **Drop** |
| `cli/interactive.rs`, `cli/progress.rs` | same | **Copy** |
| `datetime.rs` | moves to `clickupc-core/src/time.rs` | **Adapt** (MCP needs it too) |
| `notifications.rs` | — | **Drop** |
| `version_check.rs`, `version_insights.rs` | same | **Copy** (point at the new repo's releases) |
| `help_text.rs` | same | **Adapt** |
| `tui/*` | — | **Drop for v1** (see ROADMAP "Later") |

### jira-mcp → clickupc-mcp

| jira-mcp | clickupc-mcp | Verdict |
| --- | --- | --- |
| `main.rs` (serve, transports) | `clickupc/src/cli/mcp.rs` (`mcp serve`) | **Adapt** — moves into the CLI binary; no MCP binary |
| `lib.rs` (`run_stdio`, `run_streamable_http`) | `lib.rs` | **Copy** |
| `server.rs` (`#[tool]` list) | `server.rs` | **Adapt** — tool list from `docs/MCP.md` |
| `models.rs` | `models.rs` | **Adapt** |
| `error.rs` (`AppError`, stable codes, status → rpc code) | `error.rs` | **Copy** + `resolve_error`, `rate_limited`; rename `jira_api_error` → `clickup_api_error` |
| `app/shared.rs` (`require_confirm`, helpers) | `app/shared.rs` | **Copy** |
| `app/auth.rs`, `app/api.rs`, `app/comment.rs`, `app/attachment.rs`, `app/worklog.rs` | `app/{auth,api,comment,attachment,time}.rs` | **Adapt** |
| `app/issue.rs` | `app/task.rs` | **Adapt** |
| `app/jql.rs` | — (folded into `task_search`) | **Drop** |
| `app/board.rs`, `app/project.rs`, `app/sprint.rs` | `app/hierarchy.rs` | **Adapt** |
| `app/link.rs`, `app/watcher.rs` | `app/relation.rs` | **Adapt** |
| `app/notify.rs`, `app/plan.rs`, `app/meta.rs` | — / `app/meta.rs` | **Drop** / **Adapt** |
| `app/tests.rs`, `tests/mcp_error_mapping.rs` | same | **Copy** pattern |

### Repo plumbing (Phase 6)

| jira-commands | clickup-commands | Verdict |
| --- | --- | --- |
| `.github/workflows/ci.yml`, `security.yml`, `deny.toml`, `dependabot.yml` | same | **Copy** in Phase 1 (CI from day one) |
| `release-please*`, `release-tag.yml`, `release-recover.yml`, `pr-automerge.yml`, `VERSION` | same | **Adapt** in Phase 6 — **single lane** `vX.Y.Z` for the whole workspace |
| `release-tag-mcp.yml`, `winget-submit-mcp.yml`, `.release-please-manifest-mcp.json`, `release-please-config-mcp.json`, `crates/jira-mcp/VERSION` | — | **Drop** (no separate MCP release) |
| `packaging/npm` | `@mulham28/clickupc` | **Copy** in Phase 6 |
| `packaging/npm-mcp`, `packaging/winget-mcp` | — | **Drop** |
| `packaging/winget*`, `scoop`, `chocolatey`, `install.sh`, `install.ps1`, `Dockerfile` | same | **Copy** in Phase 6 |
| `plugin/` (Claude Code plugin, skills), `.claude-plugin/` | same, skills renamed to ClickUp verbs | **Adapt** in Phase 6 |
| `clawhub/` | same | **Optional**, Phase 6 |
| `docs/` (GitHub Pages site) | `site/` or `docs/site/` | **Optional**; keep `docs/` for these design docs |
| `.clusterfuzzlite/`, `fuzz/` | keep only if a fuzz target is added | **Optional** |
