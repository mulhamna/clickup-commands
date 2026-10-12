# ClickUp API notes

Working reference for implementers. Source of truth is always <https://developer.clickup.com> (machine-readable index: <https://developer.clickup.com/llms.txt>, OpenAPI spec linked from the "OpenAPI Specification" guide).

Legend: endpoints without a marker were checked against the official reference when this doc was written (Oct 2026). **(verify)** = written from memory / not checked yet; confirm against the reference before implementing, then delete the marker.

---

## 1. Basics

| Item | Value |
| --- | --- |
| Base URL | `https://api.clickup.com/api` |
| v2 prefix | `/v2/...` — uses `team` / `team_id` for workspace |
| v3 prefix | `/v3/workspaces/{workspace_id}/...` — uses `workspace` |
| Personal token | starts with `pk_`, never expires, header `Authorization: pk_...` (no `Bearer`) |
| OAuth token | header `Authorization: Bearer ...` (out of scope for v1) |
| Content type | JSON, except attachments (`multipart/form-data`) |

`team_id` (v2) and `workspace_id` (v3) are the same number.

## 2. Rate limits

Per token, per minute, depends on the plan of the workspace that hosts the token:

| Plan | Requests/minute |
| --- | --- |
| Free Forever / Unlimited / Business | 100 |
| Business Plus | 1,000 |
| Enterprise / Enterprise Plus | 10,000 |

On exceed: HTTP `429` with headers `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset` (unix timestamp, seconds).

Client strategy (`rate_limit.rs`):
1. Record `X-RateLimit-Remaining` / `X-RateLimit-Reset` from **every** response (they're sent on success too — verify; if not, just rely on 429).
2. If `remaining == 0` and reset is in the future, sleep before sending the next request (proactive).
3. On 429: sleep until `reset + 250..750ms jitter`, retry. Max 3 retries → `ClickUpError::RateLimit { reset_at }`.
4. Bulk operations use a bounded concurrency (default 4) so they don't burn the 100/min budget instantly.

For comparison, the official MCP server without the Everything AI add-on: 100 / 300 / 1,000 / 2,500 / 5,000 / 25,000 calls per rolling 24h (Free / Unlimited / Business / Business Plus / Enterprise / Enterprise Plus), OAuth only, no delete tools.

## 3. Plan-restricted APIs

- Enterprise only: user management (invite/get/edit/remove user) and all guest endpoints.
- Business Plus+: unlimited time-entry `tags`, unlimited time entries without a task (`tid`), unlimited `assignee` on create-time-entry for other people.
- Everything else: all plans.

Surface plan-related 4xx errors with a hint ("this may require Business Plus / Enterprise").

## 4. Errors

- v2 error body: `{ "err": "...", "ECODE": "..." }` **(verify)**
- v3 error body: `{ "status": ..., "message": "...", "trace_id": "...", "timestamp": ... }`

Parse both into `ClickUpError::Api { status, code: Option<String>, message, trace_id: Option<String> }`. Include `ECODE` in user-facing messages — it's what people google.

## 5. Hierarchy

```
Workspace (team) → Space → Folder (v2: "project") → List → Task → Subtask
                         └→ Folderless List
```

A task has one **home list** but can be added to other lists (Tasks in Multiple Lists, "TIML").

| Operation | Method + path |
| --- | --- |
| Authorized user | `GET /v2/user` |
| Authorized workspaces (+ members) | `GET /v2/team` |
| Workspace plan | `GET /v2/team/{team_id}/plan` **(verify)** |
| Spaces | `GET /v2/team/{team_id}/space?archived=false` **(verify)** |
| Space | `GET /v2/space/{space_id}` **(verify)** |
| Folders in space | `GET /v2/space/{space_id}/folder` **(verify)** |
| Lists in folder | `GET /v2/folder/{folder_id}/list` **(verify)** |
| Folderless lists | `GET /v2/space/{space_id}/list` **(verify)** |
| List (incl. `statuses`) | `GET /v2/list/{list_id}` **(verify)** |
| Create list | `POST /v2/folder/{folder_id}/list`, `POST /v2/space/{space_id}/list` **(verify)** |
| List members | `GET /v2/list/{list_id}/member` **(verify)** |
| Space tags | `GET /v2/space/{space_id}/tag` **(verify)** |
| Custom task types | `GET /v2/team/{team_id}/custom_item` **(verify)** |

Hierarchy is the expensive part with a 100/min budget: one call for spaces, one per space for folders, one per folder for lists. Cache it (see ARCHITECTURE "Resolver cache").

## 6. Tasks

### 6.1 Get task — `GET /v2/task/{task_id}`

Query: `custom_task_ids`, `team_id` (required when `custom_task_ids=true`), `include_subtasks`, `include_markdown_description`.

Notable response fields: `id`, `custom_id`, `name`, `text_content`, `description`, `markdown_description` (only with the flag), `status { id, status, color, orderindex, type }`, `date_created`, `date_updated`, `date_closed`, `date_done`, `creator`, `assignees`, `watchers`, `checklists`, `tags`, `parent`, `priority`, `due_date`, `start_date`, `points`, `time_estimate`, `time_spent`, `custom_fields`, `dependencies`, `linked_tasks`, `team_id`, `url`, `list`, `folder`, `space`, `custom_item_id`, `subtasks` (only with flag), `attachments`.

`custom_fields` only contains fields applicable to the task's `custom_item_id`.

### 6.2 Search — two endpoints

**List tasks** — `GET /v2/list/{list_id}/task`. Response has `last_page`. Extra params vs team search: `include_timl`, `archived`, `watchers`, single `custom_field`.

**Filtered team tasks** — `GET /v2/team/{team_id}/task`. Use this whenever the filter isn't "exactly one list".

Shared query params:

| Param | Notes |
| --- | --- |
| `page` | 0-based, 100 tasks/page |
| `order_by` | `id`, `created` (default), `updated`, `due_date` |
| `reverse` | bool |
| `subtasks` | include subtasks (default excluded) |
| `space_ids[]`, `project_ids[]` (= folders), `list_ids[]` | team endpoint only |
| `statuses[]` | status names; closed statuses also need `include_closed=true` |
| `include_closed` | default false |
| `assignees[]` | user ids |
| `tags[]` | tag names |
| `due_date_gt/lt`, `date_created_gt/lt`, `date_updated_gt/lt`, `date_done_gt/lt` | unix ms |
| `custom_fields` | JSON array: `[{"field_id":"…","operator":"=","value":"…"}, …]`. `=` not supported for Label fields. See the "Filter tasks using Custom Fields" and "Range operator" guides for the operator list. |
| `parent` | parent task id → returns its subtasks (team endpoint) |
| `include_markdown_description` | bool |
| `custom_items[]` | task types: `0` task, `1` milestone, others workspace-defined |

Array params use repeated `key[]=` pairs. URL-encode status/tag names (spaces → `%20`).

The team endpoint's schema wasn't confirmed to include `last_page`; terminate on `last_page == true` **or** fewer than 100 tasks.

### 6.3 Create — `POST /v2/list/{list_id}/task`

Body (only `name` required): `name`, `description`, `markdown_content` (wins over `description`), `assignees: [int]`, `group_assignees: [string]`, `tags: [string]`, `status`, `priority` (1–4 or null), `due_date` + `due_date_time`, `start_date` + `start_date_time`, `time_estimate` (ms), `points`, `archived`, `notify_all`, `parent` (subtask; parent must be in same list), `links_to`, `check_required_custom_fields` (default false), `custom_fields: [{ id, value }]`, `custom_item_id`.

Custom field `value` shapes:

| Field type | Value |
| --- | --- |
| text, URL, email, phone, short/long text | string |
| dropdown | option UUID |
| label | array of option UUIDs |
| date | unix ms; add `value_options: { "time": true }` to show time |
| number, money (amount only), emoji/rating | number |
| people, task relationship | `{ "add": [...], "rem": [...] }` |
| manual progress | `{ "current": n }` |
| location | `{ "location": { "lat", "lng" }, "formatted_address" }` |
| button | `true` |

Check `applied_objects` (from Get List Custom Fields) — fields not applicable to the task type are silently dropped.

### 6.4 Update — `PUT /v2/task/{task_id}`

Query: `custom_task_ids`, `team_id`. Body: `name`, `description` (send `" "` to clear), `markdown_content`, `status`, `priority`, `due_date(_time)`, `start_date(_time)`, `time_estimate`, `points`, `custom_item_id`, `parent` (re-parent; `null` does **not** promote to task), `archived`, `assignees { add, rem }`, `watchers { add, rem }`, `group_assignees { add, rem }`.

Schema marks both `add` and `rem` required → always send both (empty array if unused).

**Custom fields are not updatable here.**

### 6.5 Other task operations

| Operation | Method + path |
| --- | --- |
| Delete task | `DELETE /v2/task/{task_id}` **(verify)** |
| Move to new home list (v3) | `PUT /v3/workspaces/{workspace_id}/tasks/{task_id}/home_list/{list_id}` body `{ move_custom_fields?, custom_fields_to_move?, status_mappings?: [{ source_status_id, destination_status_id }] }` — `status_mappings` required if the current status doesn't exist in the target list |
| Add to another list (TIML) | `POST /v2/list/{list_id}/task/{task_id}` **(verify)** |
| Remove from list | `DELETE /v2/list/{list_id}/task/{task_id}` **(verify)** |
| Merge tasks | see "Merge Tasks" reference **(verify)** |
| Time in status | `GET /v2/task/{task_id}/time_in_status` **(verify)** |
| Bulk time in status | `GET /v2/task/bulk_time_in_status/task_ids?task_ids=…` **(verify)** |
| Create from template | see "Create Task From Template" **(verify)** |

There's no bulk create/update endpoint in v2 → bulk = client-side loop with bounded concurrency.

### 6.6 Gotchas

- Escape `"` as `\"` inside description strings — serde does this already; don't double-escape.
- Huge lists with many subtasks can time out. Default `subtasks=false`, and keep `timeout_secs` configurable.
- Status names are case-insensitive in the UI but compare case-insensitively on our side and send the list's canonical spelling.

## 7. Custom fields

| Operation | Method + path |
| --- | --- |
| List fields | `GET /v2/list/{list_id}/field` **(verify)** |
| Folder / space / workspace fields | `GET /v2/folder/{id}/field`, `/v2/space/{id}/field`, `/v2/team/{id}/field` **(verify)** |
| Set value | `POST /v2/task/{task_id}/field/{field_id}` body `{ "value": … }` (+ `value_options` for dates) **(verify)** |
| Remove value | `DELETE /v2/task/{task_id}/field/{field_id}` **(verify)** |

Both set/remove accept `custom_task_ids` + `team_id`.

Field object: `id` (UUID), `name`, `type` (`drop_down`, `labels`, `text`, `short_text`, `number`, `currency`, `date`, `users`, `tasks`, `checkbox`, `url`, `email`, `phone`, `emoji`, `progress`, `location`, `formula`, …), `type_config.options[] { id, name, orderindex }`. The CLI accepts option **names** and resolves them to UUIDs.

## 8. Comments

| Operation | Method + path |
| --- | --- |
| Task comments | `GET /v2/task/{task_id}/comment` |
| Create | `POST /v2/task/{task_id}/comment` body `{ comment_text, assignee?, notify_all }` **(verify body)** |
| Update | `PUT /v2/comment/{comment_id}` **(verify)** |
| Delete | `DELETE /v2/comment/{comment_id}` **(verify)** |
| Threaded replies | `GET/POST /v2/comment/{comment_id}/reply` **(verify)** |
| List comments | `GET/POST /v2/list/{list_id}/comment` **(verify)** |

Pagination: 25 per page newest-first. Next page: `?start={date of last comment}&start_id={id of last comment}`.

Rich formatting uses a `comment: [ {text, attributes}, … ]` array (see "Comment formatting" guide). v1: plain `comment_text` only; mentions/rich text in a later phase.

## 9. Attachments

`POST /v2/task/{task_id}/attachment` — `multipart/form-data`, file in field `attachment`. Accepts `custom_task_ids` + `team_id`. Size limit not documented; surface the API error.

v3 also has generic entity attachments (Get Attachments / Create an Attachment) — not needed for v1.

## 10. Tags, links, dependencies, checklists

| Operation | Method + path |
| --- | --- |
| Add tag to task | `POST /v2/task/{task_id}/tag/{tag_name}` **(verify)** |
| Remove tag | `DELETE /v2/task/{task_id}/tag/{tag_name}` **(verify)** |
| Add link | `POST /v2/task/{task_id}/link/{links_to}` **(verify)** |
| Delete link | `DELETE /v2/task/{task_id}/link/{links_to}` **(verify)** |
| Add dependency | `POST /v2/task/{task_id}/dependency` body `{ depends_on }` or `{ dependency_of }` **(verify)** |
| Delete dependency | `DELETE /v2/task/{task_id}/dependency?depends_on=…` or `?dependency_of=…` **(verify)** |
| Create checklist | `POST /v2/task/{task_id}/checklist` body `{ name }` **(verify)** |
| Checklist item | `POST /v2/checklist/{checklist_id}/checklist_item`, `PUT/DELETE …/checklist_item/{item_id}` **(verify)** |

A tag must exist in the space before adding it to a task (create via Space Tags) **(verify)**.

## 11. Time tracking

| Operation | Method + path |
| --- | --- |
| Entries in range | `GET /v2/team/{team_id}/time_entries` |
| Create entry | `POST /v2/team/{team_id}/time_entries` body `{ tid?, start, duration, description?, billable?, tags?, assignee? }` **(verify)** |
| Start timer | `POST /v2/team/{team_id}/time_entries/start` **(verify)** |
| Stop timer | `POST /v2/team/{team_id}/time_entries/stop` **(verify)** |
| Running entry | `GET /v2/team/{team_id}/time_entries/current` **(verify)** |
| Get / update / delete entry | `GET/PUT/DELETE /v2/team/{team_id}/time_entries/{timer_id}` **(verify)** |

`GET time_entries` params: `start_date`, `end_date` (unix ms), `assignee` (comma-separated ids; owner/admin only), one of `space_id` / `folder_id` / `list_id` / `task_id`, `custom_task_ids` + `team_id`, `is_billable`, `include_task_tags`, `include_location_names`. Default (no dates): last 30 days, authenticated user only.

`duration` is a string of milliseconds; **negative = timer running**.

The legacy "tracked time" endpoints (`/v2/task/{id}/time`) exist but are deprecated-style — don't use them.

## 12. Docs (v3)

Endpoints under `/v3/workspaces/{workspace_id}/docs…`: Search Docs, Create a Doc, Fetch a Doc, Fetch PageListing, Fetch Pages, Create a Page, Get Page, Edit a Page. Read the "Docs API limitation" guide before implementing (import/export limits). All paths **(verify)** — implemented in Phase 5.

## 13. Not used in v1

Webhooks, Goals, Guests/Users (Enterprise), Views, Chat (v3), audit logs, ACL, OAuth app flow.
