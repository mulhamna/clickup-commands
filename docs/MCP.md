# MCP server tool spec

Run: `clickupc mcp serve [--transport stdio|streamable-http] [--host 127.0.0.1] [--port 8787] [--path /mcp]`. There is no separate MCP binary; the code lives in the `clickupc-mcp` library crate and ships inside `clickupc`.

Auth: reads the same config/profile as `clickupc` (`~/.config/clickupc/config.toml`, `CLICKUP_TOKEN`, `CLICKUP_PROFILE`, …). No OAuth.

## Accounts

Every tool takes two optional inputs: `profile` (a clickupc profile, i.e. a ClickUp user's token) and `workspace` (name or id). Omitted, the server's profile (`CLICKUP_PROFILE` in the client entry, else the active profile) and that profile's workspace are used. `clickup_profile_list` lists the profiles; an unknown profile fails with `config_error`. Rate limiting is tracked per token.

## Design rules

1. **Names in, ids out.** Every tool accepts human names (list, status, member, field, option) *or* ids, resolved with `Resolver`. Outputs always include ids so the model can chain calls without re-resolving.
2. **Compact by default.** Task outputs return a summary (`id`, `custom_id`, `name`, `status`, `priority`, `assignees[] {id, username}`, `due`, `list {id, name}`, `url`). Pass `detail: "full"` for description, custom fields, checklists, relations.
3. **Human dates.** Date params accept ISO dates and the relative forms from `time.rs` (`tomorrow`, `+3d`, `eow`); outputs return both ISO-8601 (in the configured timezone) and unix ms.
4. **Destructive = `confirm: true`.** Any delete/remove/clear tool fails with `unsafe_operation` unless `confirm` is true. Bulk tools also require `confirm: true` when they would touch more than 1 task; with `dry_run: true` they return the plan instead.
5. **Stable errors.** `AppError` codes: `validation_error`, `unsafe_operation`, `auth_missing`, `config_error`, `resolve_error` (`data.suggestions`), `clickup_api_error` (`data.status`, `data.ecode`, `data.trace_id`), `rate_limited` (`data.reset_at`). 4xx → `INVALID_PARAMS`, 5xx → `INTERNAL_ERROR`.
6. **Descriptions are prompts.** Each tool's description tells the model *when* to use it and the gotchas (e.g. "use clickup_field_set to change custom fields; clickup_task_update cannot").
7. Rate limiting is invisible to the model: the core retries 429s. Only after retries are exhausted does a tool return `rate_limited`.

## Tools

Phase tags refer to `docs/ROADMAP.md`. "Official" = equivalent exists in ClickUp's official MCP server.

### Auth & meta (P1)

| Tool | Input | Notes | Official |
| --- | --- | --- | --- |
| `clickup_profile_list` | — | configured profiles (name, workspace, masked token, active) | — |
| `clickup_auth_status` | — | profile, workspace, user, token present (masked), config path | — |
| `clickup_whoami` | — | user id, username, email, timezone, workspace id/name, plan | — |
| `clickup_workspace_list` | — | authorized workspaces | — |

### Hierarchy (P1)

| Tool | Input | Notes | Official |
| --- | --- | --- | --- |
| `clickup_hierarchy` | `space?`, `refresh?` | Space → Folder → List tree (ids + names). Cached. | ✓ |
| `clickup_list_get` | `list` | statuses (name, type, order), custom fields (name, type, options), members | ✓ |
| `clickup_list_create` | `name`, `folder?` \| `space?`, `content?` | (P4) | ✓ |
| `clickup_folder_create` | `name`, `space` | (P4) | ✓ |

### Members (P1)

| Tool | Input | Notes | Official |
| --- | --- | --- | --- |
| `clickup_member_list` | `list?` | workspace or list members | ✓ |
| `clickup_member_resolve` | `queries: [string]` | names/emails/"me" → user ids, with match confidence | ✓ |

### Tasks (P2)

| Tool | Input | Notes | Official |
| --- | --- | --- | --- |
| `clickup_task_search` | `lists?`, `folders?`, `spaces?`, `statuses?`, `include_closed?`, `assignees?`, `tags?`, `due_before?`, `due_after?`, `created_*?`, `updated_*?`, `done_*?`, `custom_fields?: [{field, op, value}]`, `task_types?`, `parent?`, `subtasks?`, `order_by?`, `reverse?`, `limit?` (default 50, max 500), `detail?` | Picks list vs team endpoint; paginates | ✓ |
| `clickup_task_get` | `task`, `detail?`, `include_subtasks?`, `include_comments?` | `task` = id, custom id, or URL | ✓ |
| `clickup_task_create` | `list`, `name`, `description?` (markdown), `status?`, `priority?` (`urgent\|high\|normal\|low`), `assignees?`, `tags?`, `due?`, `start?`, `estimate?` (`"2h"`), `points?`, `parent?`, `task_type?`, `custom_fields?: {name_or_id: value}` | | ✓ |
| `clickup_task_update` | `task`, any of create's fields (except list), `assignees_add?`, `assignees_remove?`, `watchers_add?`, `watchers_remove?`, `archived?` | `custom_fields` here are applied via Set Custom Field after the update, results reported per field | ✓ |
| `clickup_task_set_status` | `task`, `status` | validates against the task's list; returns suggestions on mismatch | — |
| `clickup_task_move` | `task`, `list`, `status_map?: {old: new}`, `move_custom_fields?` | v3 home list; errors with target statuses if mapping needed | ✓ |
| `clickup_task_delete` | `task`, `confirm` | | ✗ (official has none) |

### Bulk (P4)

| Tool | Input | Notes | Official |
| --- | --- | --- | --- |
| `clickup_task_bulk_create` | `list`, `tasks: [create input]`, `dry_run?` | bounded concurrency, per-item results | ✓ |
| `clickup_task_bulk_update` | `tasks: [string]` \| `filter: {search input}`, `changes: {status?, priority?, assignees_add?, tags_add?, due?}`, `dry_run?`, `confirm?` | | ✓ |
| `clickup_task_batch` | `ops: [{op, …}]`, `dry_run?`, `confirm?` | mixed create/update/status/comment/delete | — |

### Comments (P3)

| Tool | Input | Notes | Official |
| --- | --- | --- | --- |
| `clickup_comment_list` | `task`, `limit?` (default 25) | handles `start`/`start_id` cursor | ✓ |
| `clickup_comment_add` | `task`, `text`, `assignee?`, `notify_all?` | | ✓ |
| `clickup_comment_update` | `comment_id`, `text`, `resolved?` | | — |
| `clickup_comment_delete` | `comment_id`, `confirm` | | ✗ |
| `clickup_comment_replies` | `comment_id` | (P4) | ✓ |
| `clickup_comment_reply` | `comment_id`, `text` | (P4) | — |

### Custom fields (P3)

| Tool | Input | Notes | Official |
| --- | --- | --- | --- |
| `clickup_field_list` | `list` | name, id, type, options | — |
| `clickup_field_set` | `task`, `field`, `value` | value given as human input (option name, date, number, member) and converted per type | ✓ |
| `clickup_field_clear` | `task`, `field`, `confirm` | | ✗ |

### Relations (P3)

| Tool | Input | Notes | Official |
| --- | --- | --- | --- |
| `clickup_tag_add` / `clickup_tag_remove` | `task`, `tags` | | ✓ |
| `clickup_link_add` / `clickup_link_remove` | `task`, `other_task` (+ `confirm` on remove) | | ✓ |
| `clickup_dependency_add` / `clickup_dependency_remove` | `task`, `blocks?` \| `waiting_on?` (+ `confirm` on remove) | | ✓ |
| `clickup_checklist_add` | `task`, `name`, `items?` | (P4) | — |
| `clickup_checklist_item_update` | `checklist_id`, `item_id`, `resolved?`, `name?` | (P4) | — |
| `clickup_attachment_upload` | `task`, `path` | local file path | ✓ |
| `clickup_task_add_to_list` / `clickup_task_remove_from_list` | `task`, `list` | TIML (P4) | ✓ (add) |

### Time tracking (P3)

| Tool | Input | Notes | Official |
| --- | --- | --- | --- |
| `clickup_time_list` | `task?` \| `list?` \| `space?`, `from?`, `to?`, `users?` | durations rendered as `1h 30m` + ms | ✓ |
| `clickup_time_add` | `task`, `duration`, `at?`, `note?`, `billable?` | | ✓ |
| `clickup_time_start` | `task?`, `note?` | | ✓ |
| `clickup_time_stop` | — | | ✓ |
| `clickup_time_current` | — | running entry or null | ✓ |
| `clickup_time_delete` | `entry_id`, `confirm` | | ✗ |
| `clickup_time_in_status` | `task` \| `tasks` | (P4) | ✓ |

### Reports (P4)

| Tool | Input | Notes | Official |
| --- | --- | --- | --- |
| `clickup_standup` | `since?` (default yesterday), `user?` | my tasks closed/updated since, in progress, due soon, overdue | — |
| `clickup_list_summary` | `list` | counts by status / assignee, overdue count | — |

### Docs (P5)

`clickup_doc_search`, `clickup_doc_get`, `clickup_doc_pages`, `clickup_doc_create`, `clickup_doc_page_create`, `clickup_doc_page_update` (official: ✓ for create/list/get/create page/update page).

### Escape hatch (P1)

| Tool | Input | Notes |
| --- | --- | --- |
| `clickup_api_request` | `method`, `path` (`/v2/...` or `/v3/...`, `{workspace}` substituted), `query?`, `body?`, `confirm?` | non-GET requires `confirm: true`. Lets the model reach any endpoint we haven't wrapped. |

## Not mirrored from the official server

- Workspace-wide full-text search ("Search Workspace") — the Public API has no equivalent endpoint; `clickup_task_search` covers structured search. Revisit if ClickUp adds one.
- Chat channels / messages (v3 Chat) — out of scope for v1.
- Connected search — not available via API.

## Client registration

`clickupc mcp install --client <target>` writes the server entry. Example for Claude Code:

```json
{
  "mcpServers": {
    "clickupc": {
      "command": "clickupc",
      "args": ["mcp", "serve"],
      "env": { "CLICKUP_PROFILE": "work" }
    }
  }
}
```

The default entry name is `clickupc` (override with `--name`) so it can coexist with ClickUp's official server, which clients typically register as `clickup`. Claude Code namespaces tools per server (`mcp__clickupc__clickup_task_get`), so the shared `clickup_` tool prefix does not collide.

Never write the token into client config files; the server reads it from the clickupc config (or the user's env).
