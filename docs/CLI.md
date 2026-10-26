# `clickupc` CLI spec

Conventions:
- Anything that takes a list/space/folder/status/member/field accepts a **name or id** (resolved via `Resolver`). Lists also accept an alias from config or a qualified path `"Space/Folder/List"`.
- Anything that takes a task accepts a task id, custom id (`DEV-123`), or task URL.
- Dates accept `2026-10-20`, `2026-10-20 14:00`, `today`, `tomorrow`, `fri`, `+3d`, `eow`, … (see ARCHITECTURE `time.rs`).
- Durations accept `1h30m`, `45m`, `2h`.
- Global flags: `--profile <name>`, `--workspace <name|id>`, `--json`, `-v/--verbose`, `--refresh`, `-y/--yes`.
- Phase tags (P1…P5) refer to `docs/ROADMAP.md`.

---

## auth (P1)

```
clickupc auth login [--token pk_…|-] [--profile work] [--workspace <w>]
    # Terminal: if profiles exist, pick "+ New profile" or one to update (never overwritten silently);
    # token input is masked; workspace picker; new profile name suggested from the workspace;
    # asks whether to make it active, then offers to run `mcp install`.
    # Non-TTY: --token, --token - (stdin) or CLICKUP_TOKEN; updates --profile or the active profile.
    # Updating a profile adds the token for the chosen workspace (others are kept) and makes it the default.
clickupc auth status                                  # profile, workspace, user, token present (masked), config path
clickupc auth logout [--profile work]                 # remove stored tokens (all workspaces)
clickupc auth profile list | use [<name>] | remove <name>   # `use` without a name shows a picker
clickupc whoami                                       # user id, username, email, timezone, workspace + plan
```

## workspace / hierarchy (P1)

```
clickupc workspace list
clickupc workspace use [<name|id>]                   # set workspace_id on current profile (picker when omitted)
clickupc workspace use <ws> <ws>...                  # multi-workspace: task list + standup query all; first = default for writes
clickupc tree [--space <s>] [--archived]             # Space → Folder → List tree with ids
clickupc space list
clickupc folder list --space <s>
clickupc list list [--space <s>] [--folder <f>]
clickupc list view <list>                            # statuses (with type: open/custom/closed), custom fields, members
clickupc list create --folder <f>|--space <s> --name <n>     (P4)
clickupc alias set <name> <list> | list | remove <name>      # writes [profiles.x.aliases]
clickupc cache clear
```

## task (P2)

### list / search

```
clickupc task list [filters…] [--limit N] [--order-by created|updated|due|id] [--reverse]
```

| Flag | Repeatable | Maps to |
| --- | --- | --- |
| `--list <l>` / `--folder <f>` / `--space <s>` | yes | `list_ids[]` / `project_ids[]` / `space_ids[]` |
| `--status <name>` | yes | `statuses[]` (validated per list when exactly one list is given) |
| `--closed` | | `include_closed=true` |
| `--assignee <me\|name\|email\|id>` | yes | `assignees[]` |
| `--mine` | | shorthand for `--assignee me` |
| `--tag <name>` | yes | `tags[]` |
| `--due-before <d>` / `--due-after <d>` | | `due_date_lt` / `due_date_gt` |
| `--overdue` | | `due_date_lt=now`, excludes closed |
| `--created-before/after`, `--updated-before/after`, `--done-before/after` | | `date_*_lt/gt` |
| `--field '<name><op><value>'` | yes | `custom_fields` JSON; ops: `=`, `!=`, `<`, `<=`, `>`, `>=`, `IS NULL`, `IS NOT NULL`, `RANGE`, `ANY`, `ALL`, `NOT ANY`, `NOT ALL` (check "Filter tasks using Custom Fields" guide for the exact supported set) |
| `--type <task\|milestone\|name\|id>` | yes | `custom_items[]` |
| `--parent <task>` | | `parent` |
| `--subtasks` | | `subtasks=true` |
| `--archived` | | `archived=true` (single-list endpoint only) |

Default columns: `ID` (custom id if present), `Name`, `Status`, `Assignees`, `Due`, `Priority`, `List`.

Examples:
```
clickupc task list --mine --status "in progress"
clickupc task list --list sprint --overdue
clickupc task list --space Engineering --tag bug --field 'Severity=High' --json
```

### view

```
clickupc task view <task> [--comments] [--subtasks] [--web]
```
Shows name, ids, url, status, priority, assignees, watchers, dates, estimate/time spent, tags, list path, custom fields (rendered), checklists, dependencies/links, markdown description. `--web` opens the URL.

### create / update

```
clickupc task create [--list <l>] --name <n> [--description <md> | --description-file <path|->]
                     [--status <s>] [--priority urgent|high|normal|low] [--assignee …]… [--tag …]…
                     [--due <d>] [--start <d>] [--estimate 2h] [--points 3]
                     [--parent <task>] [--type <task type>] [--field 'Name=value']… [--check-required]
                     [--open]

clickupc task update <task> [--name] [--description | --description-file] [--status] [--priority none|…]
                     [--assign …]… [--unassign …]… [--watch …]… [--unwatch …]…
                     [--due <d>|none] [--start <d>|none] [--estimate|none] [--points]
                     [--parent <task>] [--type …] [--archive | --unarchive]
                     [--field 'Name=value']… [--clear-field <name>]…
```

`--list` falls back to `default_list`; if neither and TTY → picker. `--field` on update is applied as separate Set Custom Field calls after the task update (Update Task can't set fields) — report each result.

### status / assign / move / delete

```
clickupc task status <task> <status>              # validates against the task's list; suggests close matches
clickupc task assign <task> <member>… | unassign <task> <member>…
clickupc task move <task> --to <list> [--map 'old status=new status']… [--move-fields]   # v3 home_list
clickupc task add-to <task> --list <l> | remove-from <task> --list <l>                  # TIML (P4)
clickupc task delete <task>… [--yes]
clickupc task open <task>                          # open in browser
```

For `move`: if the current status doesn't exist in the target list and no `--map` given, TTY → prompt for target status; non-TTY → error listing target statuses.

### relations

```
clickupc task tag <task> add|remove <tag>…
clickupc task link <task> add|remove <other-task>
clickupc task dep <task> add|remove (--blocks <task> | --waiting-on <task>)
clickupc task checklist <task> add <name> [--item <text>]…          (P4)
clickupc task checklist <task> check|uncheck <item-id>               (P4)
```

### bulk (P4)

```
clickupc task bulk update [filters… same as task list] [--set-status s] [--assign m] [--add-tag t] [--priority p] [--dry-run] [--yes]
clickupc task bulk comment [filters…] --text <t> [--dry-run] [--yes]
clickupc task batch --file ops.json [--dry-run]     # [{ "op": "create"|"update"|"status"|"comment"|"delete", … }]
```

Always prints the matched task count and asks before touching more than 1 task (unless `--yes`). Uses the shared rate limiter with bounded concurrency; prints a progress bar (`indicatif`) and a per-task result summary. `--dry-run` shows what would change.

## comment (P3)

```
clickupc comment list <task> [--all]               # default newest 25; --all paginates
clickupc comment add <task> <text | -> [--assign <member>] [--notify-all]
clickupc comment update <comment-id> <text>
clickupc comment delete <comment-id> [--yes]
clickupc comment reply <comment-id> <text>          (P4)
```

## attachment (P3)

```
clickupc attach <task> <file>…
```

## field (P3)

```
clickupc field list --list <l>                      # name, type, id, options
clickupc field set <task> <field> <value>           # value parsed per field type (option name, date, number, member, …)
clickupc field clear <task> <field> [--yes]
```

## time (P3)

```
clickupc time list [--task <t>|--list <l>|--space <s>] [--from <d>] [--to <d>] [--user <m>] [--billable]
clickupc time add <task> <duration> [--at <d>] [--note <text>] [--billable]
clickupc time start [<task>] [--note <text>]
clickupc time stop
clickupc time current
clickupc time delete <entry-id> [--yes]
clickupc time report [--from mon] [--to today] [--by task|list|day]   # client-side aggregation  (P4)
```

## member (P1)

```
clickupc member list [--list <l>]
clickupc member find <query>                        # fuzzy username/email → id
```

## reports (P4)

```
clickupc standup [--since yesterday]                # my tasks updated/closed since, in progress, due soon
clickupc summary --list <l>                         # counts by status and assignee
```

## doc (P5)

```
clickupc doc search <query>
clickupc doc view <doc-id> [--page <page-id>]
clickupc doc create --name <n> [--parent …]
clickupc doc page create <doc-id> --name <n> [--content-file md]
clickupc doc page edit <doc-id> <page-id> --content-file md [--mode replace|append]
```

## escape hatch (P1)

```
clickupc api GET /v2/team
clickupc api POST /v2/list/123/task --data '{"name":"x"}'
clickupc api PUT /v3/workspaces/{workspace}/tasks/abc/home_list/456 --data @body.json
```
`{workspace}` is substituted from config. Non-GET asks for confirmation unless `--yes`.

## config / mcp (P1 / P5)

```
clickupc config get|set <key> [value]               # default_task_limit, timezone, default_list, timeout_secs
clickupc mcp serve [--transport stdio|streamable-http] [--host] [--port] [--path]   # run the MCP server
clickupc mcp doctor                                 # check binary on PATH, token, workspace, a test call
clickupc update [--check]                           # upgrade via brew/scoop/npm/cargo or the install script
clickupc mcp install [--client <c>]… [--server-profile <p>]… [--name clickupc] [--dry-run] [--force]
    # No --client on a terminal: prerequisite checklist (runs `auth login` if needed), then checkbox
    # pickers for clients and, with several profiles, for profiles. One entry per client × profile:
    # `clickupc` for a single profile, `clickupc-<profile>` for several, each pinned via CLICKUP_PROFILE.
```
Registration goes through `kurir`; the written entry is `command: "clickupc", args: ["mcp", "serve"]` (no separate MCP binary). `--server-profile` pins `CLICKUP_PROFILE` in the entry; the token is never written.
