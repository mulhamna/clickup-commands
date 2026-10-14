# Architecture

```
            ┌──────────────── clickupc (one binary) ───────────────┐
            │ ┌──────────────┐          ┌───────────────────────┐ │
 user ────► │ │  CLI cmds    │          │ `clickupc mcp serve`  │ │ ◄── MCP client (Claude, Cursor, …)
            │ │  (clap)      │          │ clickupc-mcp (rmcp)   │ │
            │ └──────┬───────┘          └──────────┬────────────┘ │
            └────────┼─────────────────────────────┼──────────────┘
                   │  typed calls            │
                   ▼                         ▼
            ┌─────────────────────────────────────────┐
            │              clickupc-core               │
            │  Resolver ─► ClickUpClient ─► RateLimit │
            │  (names→ids,  (v2 + v3 HTTP)  (429 /    │
            │   cache)                      reset)    │
            └────────────────────┬────────────────────┘
                                 ▼
                    https://api.clickup.com/api
```

Rule of thumb: **the CLI and MCP crates never see a URL or a raw HTTP status.** They call `clickupc-core` with typed inputs and get typed models or `ClickUpError` back.

---

## clickupc-core

### Config — `config.rs`

Profiles, env overrides, chmod-600 on save and a warning when the file is readable by others.

File: `~/.config/clickupc/config.toml` (via `dirs::config_dir()`).

```toml
current_profile = "work"

[profiles.work]
token = "pk_..."            # optional if CLICKUP_TOKEN is set
workspace_id = "9012345678" # chosen during `auth login`
default_list = "901234567890" # optional; used when --list omitted on create
timezone = "Asia/Jakarta"   # optional; default: user's ClickUp timezone, then system
timeout_secs = 30
default_task_limit = 200    # optional cap for list-like commands

[profiles.work.aliases]     # optional short names → ids
sprint = "901234567890"
inbox  = "901234567891"

[profiles.work.tokens]      # optional workspace id → token, written by `auth login`
"9012345678" = "pk_..."     # used when that workspace is selected; `token` is the fallback
```

Env overrides (highest priority): `CLICKUP_TOKEN`, `CLICKUP_WORKSPACE_ID`, `CLICKUP_PROFILE`, `CLICKUP_DEFAULT_LIST`, `CLICKUP_TIMEOUT_SECS`, `CLICKUP_TASK_LIMIT`, `CLICKUP_BASE_URL` (tests / proxies), `CLICKUP_CACHE_DIR`.

Priority: env > selected profile > defaults. `CLICKUP_PROFILE` selects the profile. Token: `CLICKUP_TOKEN` > `tokens[workspace_id]` > `token`.

### Errors — `error.rs`

```rust
#[derive(Debug, thiserror::Error)]
pub enum ClickUpError {
    #[error("HTTP error: {0}")]            Http(#[from] reqwest::Error),
    #[error("Authentication error: {0}")]  Auth(String),          // 401, missing token
    #[error("API error {status}{}: {message}", code.as_deref().map(|c| format!(" [{c}]")).unwrap_or_default())]
    Api { status: u16, code: Option<String>, message: String, trace_id: Option<String> },
    #[error("Not found: {0}")]             NotFound(String),
    #[error("Rate limited until {reset_at}")] RateLimit { reset_at: i64 },
    #[error("Configuration error: {0}")]   Config(String),
    #[error("Could not resolve {kind} '{input}'{}", suggestion_hint(.suggestions))]
    Resolve { kind: &'static str, input: String, suggestions: Vec<String> },
    #[error("Serialization error: {0}")]   Serialization(#[from] serde_json::Error),
    #[error("IO error: {0}")]              Io(#[from] std::io::Error),
}
pub type Result<T> = std::result::Result<T, ClickUpError>;
```

`Resolve` is what you get for a typo'd list/status/member/field name, with "did you mean" suggestions (use `strsim` Jaro-Winkler, top 3 above 0.8).

### HTTP client — `client/mod.rs`

```rust
pub struct ClickUpClient {
    http: reqwest::Client,
    base: Url,                 // https://api.clickup.com/api
    workspace_id: Option<String>,
    limiter: Arc<RateLimiter>,
}

impl ClickUpClient {
    pub fn new(cfg: &ClickUpConfig) -> Result<Self>;
    // private plumbing
    async fn v2<T: DeserializeOwned>(&self, method: Method, path: &str, q: &Query, body: Option<&Value>) -> Result<T>;
    async fn v3<T: DeserializeOwned>(&self, method: Method, path: &str, q: &Query, body: Option<&Value>) -> Result<T>;
    async fn send_with_retry(&self, build: impl Fn() -> RequestBuilder) -> Result<Response>;
    pub async fn raw(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value>; // for `clickupc api`
}
```

- `Authorization` header set once as a default header (raw `pk_` token).
- `Query` is a small builder that supports repeated `key[]=v` pairs and JSON-encoded `custom_fields`.
- `send_with_retry` asks `RateLimiter` for a permit, sends, updates the limiter from response headers, and loops on 429 (see CLICKUP-API §2).
- Error bodies parsed for v2 (`err`/`ECODE`) and v3 (`message`/`trace_id`).
- `tracing::debug!` logs method + path + status + remaining budget. Never the token.

Resource methods live in `client/{hierarchy,task,comment,field,time,relation,member,doc}.rs` as `impl ClickUpClient` blocks so no single file grows into a monolith.

### Rate limiter — `rate_limit.rs`

```rust
pub struct RateLimiter {
    state: Mutex<State>,   // remaining: Option<u32>, reset_at: Option<i64>
    permits: Semaphore,    // bounded concurrency for bulk ops (default 4)
}
```

`acquire()` → if `remaining == Some(0)` and `reset_at > now`, sleep until reset. `observe(&HeaderMap)` updates state. Shared via `Arc` between CLI bulk commands and MCP concurrent tool calls, so parallel callers don't stampede.

### Task references — `task_ref.rs`

```rust
pub enum TaskRef { Id(String), Custom(String) }
impl FromStr for TaskRef { … }
impl TaskRef { pub fn query(&self, workspace_id: &str) -> Query; pub fn path_id(&self) -> &str; }
```

Accepts:
- `86b1abcde` → `Id`
- `DEV-123`, `ENG-42` (prefix + dash + digits) → `Custom` (adds `custom_task_ids=true&team_id=…`)
- `https://app.clickup.com/t/86b1abcde` → `Id`
- `https://app.clickup.com/t/9012345678/DEV-123` → `Custom`
- `#86b1abcde` (as pasted from the UI) → strip `#`

Every task-scoped client method takes `&TaskRef`, not `&str`. Unit-test the parser heavily (it's a fuzz target candidate).

### Resolver — `resolve.rs`

The CLI and MCP accept human names; the API wants ids. The resolver bridges that and caches aggressively because hierarchy walks are expensive at 100 req/min.

```rust
pub struct Resolver<'a> { client: &'a ClickUpClient, cache: Cache }

impl Resolver<'_> {
    pub async fn list(&self, input: &str) -> Result<ListRef>;        // id | alias | "Space/Folder/List" | unique list name
    pub async fn space(&self, input: &str) -> Result<SpaceRef>;
    pub async fn folder(&self, input: &str) -> Result<FolderRef>;
    pub async fn status(&self, list: &ListRef, input: &str) -> Result<String>;   // canonical status name
    pub async fn member(&self, input: &str) -> Result<UserId>;      // "me" | id | email | username (fuzzy)
    pub async fn field(&self, list: &ListRef, input: &str) -> Result<CustomField>;
    pub async fn field_value(&self, field: &CustomField, raw: &str) -> Result<Value>; // "High" → option uuid, "2026-10-20" → ms
    pub async fn hierarchy(&self, refresh: bool) -> Result<Hierarchy>;
}
```

Cache: JSON file at `~/.cache/clickupc/{workspace_id}/hierarchy.json` (+ `members.json`, `fields/{list_id}.json`), TTL 1 hour, `--refresh` flag / `clickupc cache clear` to bust. On a resolve miss, refresh once before failing (handles newly created lists).

Ambiguity (two lists named "Backlog" in different spaces) → `Resolve` error listing the qualified paths `Space/Folder/List`, never a silent pick.

### Filters — `filter.rs`

```rust
#[derive(Default)]
pub struct TaskFilter {
    pub space_ids: Vec<String>, pub folder_ids: Vec<String>, pub list_ids: Vec<String>,
    pub statuses: Vec<String>, pub include_closed: bool,
    pub assignees: Vec<String>, pub tags: Vec<String>,
    pub due: Range, pub created: Range, pub updated: Range, pub done: Range, // Range { gt: Option<i64>, lt: Option<i64> }
    pub custom_fields: Vec<FieldFilter>,   // { field_id, operator, value }
    pub task_types: Vec<i64>, pub parent: Option<String>,
    pub subtasks: bool, pub archived: Option<bool>,
    pub order_by: Option<OrderBy>, pub reverse: bool,
    pub limit: Option<usize>,
}
```

`client.search_tasks(&filter)` picks the endpoint: exactly one list and no space/folder filters → `GET /v2/list/{id}/task`; otherwise `GET /v2/team/{id}/task`. Then paginates until done or `limit` reached. Filters are already resolved ids — name resolution happens in the CLI/MCP layer via `Resolver`.

### Time input — `time.rs`

`parse_when(input, tz) -> i64 (unix ms)` accepts `2026-10-20`, `2026-10-20 14:00`, `today`, `tomorrow`, `yesterday`, `mon`..`sun` (next occurrence), `+3d`, `-2w`, `eod`, `eow`. `parse_duration("1h30m") -> ms`. Display helpers render ms in the configured timezone.

### Models — `model/`

Serde structs mirroring API responses, tolerant by default (`#[serde(default)]`, `Option<…>`, unknown fields ignored). Helper `de::string_or_i64` for the many ms timestamps returned as strings. Keep raw `serde_json::Value` for `custom_fields[].value` (shape varies by type) plus a `display_value()` that renders it readably.

---

## clickupc (CLI)

- `clap` derive, one module per noun under `cli/`.
- Global flags: `--profile`, `--json`, `--verbose`, `--refresh` (bust resolver cache), `--yes` (skip confirmations).
- Output: `output.rs` renders tables with `comfy-table` and `--json` prints the raw model. JSON is stable output for scripting; table layout can change.
- Interactive: when a required arg is missing and stdin is a TTY, prompt with `inquire` (list picker, status picker, assignee picker). Non-TTY → error with the flag name.
- Destructive commands confirm unless `--yes`.
- Exit codes: 0 ok, 1 generic error, 2 usage, 3 auth, 4 not found, 5 rate limited.

Full spec: `docs/CLI.md`.

## clickupc-mcp

A **library crate** (no `main.rs`):

- `lib.rs`: `pub async fn run_stdio(cfg) ` and `pub async fn run_streamable_http(cfg, host, port, path)`. Invoked by `clickupc mcp serve --transport stdio|streamable-http --host --port --path`.
- In stdio mode, **nothing may write to stdout except the MCP transport**: tracing goes to stderr, and the CLI's version-check notice, progress bars and color output must be disabled on this code path.
- `server.rs`: rmcp `#[tool]` methods, names prefixed `clickup_`.
- `models.rs`: request/response structs with `schemars` doc comments — these become the tool schemas the LLM sees, so write descriptions for the model, not for humans.
- `error.rs`: `AppError` with stable string codes: `validation_error`, `unsafe_operation`, `auth_missing`, `config_error`, `resolve_error` (carry suggestions in `data`), `clickup_api_error` (carry `status`, `ecode`, `trace_id` in `data`), `rate_limited` (carry `reset_at`).
- `app/`: one module per resource, each a thin adapter: validate input → resolve names → call core → shape output.
- Destructive tools require `confirm: true` (`require_confirm` helper in `app.rs`).
- Outputs are compact by default (id, custom_id, name, status, assignees, due, url) with `detail: "full"` to get everything — keeps LLM context small.

Full spec: `docs/MCP.md`.

## Testing

- Unit tests next to code (`task_ref`, `filter` query building, `time` parsing, resolver fuzzy matching).
- HTTP tests with `wiremock`: point `CLICKUP_BASE_URL` at the mock, assert method/path/query/headers/body. Required coverage per client method: happy path, 404 → `NotFound`, 429 → retry then success, 429 ×4 → `RateLimit`.
- Fixtures: store sample responses under `crates/clickupc-core/tests/fixtures/*.json`, trimmed from the official reference examples.
- MCP: in-process tests calling tool handlers with a mocked core.
- Optional live smoke test behind `CLICKUP_LIVE_TEST=1` against a throwaway workspace, never in CI.
