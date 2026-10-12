//! Tool implementations: validate → resolve names → call core → shape output.
//! Config is reloaded per call so `clickupc auth login` takes effect without
//! restarting the server; the rate limiter is shared across calls.

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};

use clickupc_core::{
    client::{DocParent, NewTimeEntry, TimeQuery},
    config::{mask_token, ClickUpConfig, ConfigFile},
    model::{Comment, Task, TimeEntry},
    ops::{self, BatchOp, BulkChanges, SearchInput, TaskInput, UpdateInput},
    rate_limit::RateLimiter,
    time::{format_duration, now_ms, parse_duration, parse_when, to_iso},
    ClickUpClient, Resolver, TaskRef,
};
use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    models::*,
};

pub const DEFAULT_SEARCH_LIMIT: usize = 50;
pub const MAX_SEARCH_LIMIT: usize = 500;

/// One limiter per token: ClickUp's budget is per token, so two accounts in
/// one server must not throttle each other.
fn limiter_for(token: &str) -> Arc<RateLimiter> {
    static LIMITERS: OnceLock<Mutex<HashMap<String, Arc<RateLimiter>>>> = OnceLock::new();
    LIMITERS
        .get_or_init(Default::default)
        .lock()
        .expect("limiter map lock")
        .entry(token.to_string())
        .or_insert_with(|| Arc::new(RateLimiter::default()))
        .clone()
}

pub struct Ctx {
    pub cfg: ClickUpConfig,
    pub client: ClickUpClient,
}

impl Ctx {
    /// Config for the requested profile (or the server's), with an optional
    /// per-call workspace override.
    pub async fn load(acc: &Account) -> AppResult<Self> {
        let mut cfg = load_config(acc.profile.as_deref())?;
        let token = cfg.token()?.to_string();
        let mut client = ClickUpClient::new(&cfg)?.with_limiter(limiter_for(&token));
        if let Some(w) = acc.workspace.as_deref().filter(|w| !w.trim().is_empty()) {
            let id = if w.chars().all(|c| c.is_ascii_digit()) {
                w.to_string()
            } else {
                client.find_workspace(w).await?.id
            };
            cfg.workspace_id = Some(id.clone());
            client = client.with_workspace(id);
        }
        Ok(Self { cfg, client })
    }

    pub fn resolver(&self) -> AppResult<Resolver<'_>> {
        Ok(Resolver::new(&self.client, &self.cfg)?)
    }

    fn tz(&self) -> Option<&str> {
        self.cfg.timezone.as_deref()
    }
}

fn load_config(profile: Option<&str>) -> AppResult<ClickUpConfig> {
    if let Some(p) = profile {
        let file = ConfigFile::load()?;
        if !file.profiles.contains_key(p) {
            let names: Vec<&String> = file.profiles.keys().collect();
            return Err(AppError::config_error(format!(
                "unknown profile '{p}'; available: {names:?} (see clickup_profile_list)"
            )));
        }
    }
    Ok(ClickUpConfig::load_profile(profile)?)
}

pub fn require_confirm(confirm: Option<bool>) -> AppResult<()> {
    if confirm == Some(true) {
        Ok(())
    } else {
        Err(AppError::unsafe_operation(
            "this operation requires confirm=true",
        ))
    }
}

fn task_ref(s: &str) -> AppResult<TaskRef> {
    Ok(s.parse::<TaskRef>()?)
}

/// JSON value → the human string the core parsers expect.
pub fn value_to_input(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(value_to_input)
            .collect::<Vec<_>>()
            .join(","),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn fields_to_input(fields: &BTreeMap<String, Value>) -> BTreeMap<String, String> {
    fields
        .iter()
        .map(|(k, v)| (k.clone(), value_to_input(v)))
        .collect()
}

fn is_full(detail: &Option<String>) -> bool {
    detail
        .as_deref()
        .is_some_and(|d| d.eq_ignore_ascii_case("full"))
}

fn task_out(t: &Task, full: bool, tz: Option<&str>) -> AppResult<Value> {
    if !full {
        return Ok(ops::task_summary(t, tz));
    }
    let mut v = serde_json::to_value(t)?;
    v["custom_fields"] = Value::Array(
        t.custom_fields
            .iter()
            .map(|f| {
                json!({ "id": f.id, "name": f.name, "type": f.kind, "value": f.display_value(tz) })
            })
            .collect(),
    );
    Ok(v)
}

fn comment_out(c: &Comment, tz: Option<&str>) -> Value {
    json!({
        "id": c.id,
        "text": c.comment_text,
        "user": c.user.as_ref().map(|u| u.display()),
        "date": c.date.and_then(|ms| to_iso(ms, tz)),
        "resolved": c.resolved,
        "reply_count": c.reply_count,
    })
}

fn time_out(e: &TimeEntry, tz: Option<&str>) -> Value {
    let ms = e.effective_ms(now_ms());
    json!({
        "id": e.id,
        "task": e.task_id(),
        "task_name": e.task_name(),
        "user": e.user.as_ref().map(|u| u.display()),
        "start": e.start.and_then(|s| to_iso(s, tz)),
        "duration": format_duration(ms),
        "duration_ms": ms,
        "running": e.is_running(),
        "billable": e.billable,
        "description": e.description,
    })
}

pub fn search_input(a: &TaskSearchArgs) -> SearchInput {
    SearchInput {
        lists: a.lists.clone(),
        folders: a.folders.clone(),
        spaces: a.spaces.clone(),
        statuses: a.statuses.clone(),
        include_closed: a.include_closed.unwrap_or(false),
        assignees: a.assignees.clone(),
        tags: a.tags.clone(),
        due_before: a.due_before.clone(),
        due_after: a.due_after.clone(),
        overdue: a.overdue.unwrap_or(false),
        created_before: a.created_before.clone(),
        created_after: a.created_after.clone(),
        updated_before: a.updated_before.clone(),
        updated_after: a.updated_after.clone(),
        done_before: a.done_before.clone(),
        done_after: a.done_after.clone(),
        fields: a
            .custom_fields
            .iter()
            .map(|f| match &f.value {
                Some(v) => format!("{} {} {}", f.field, f.op, v),
                None => format!("{} {}", f.field, f.op),
            })
            .collect(),
        task_types: a.task_types.clone(),
        parent: a.parent.clone(),
        subtasks: a.subtasks.unwrap_or(false),
        archived: None,
        order_by: a.order_by.clone(),
        reverse: a.reverse.unwrap_or(false),
        limit: Some(
            a.limit
                .unwrap_or(DEFAULT_SEARCH_LIMIT)
                .clamp(1, MAX_SEARCH_LIMIT),
        ),
        include_markdown: is_full(&a.detail),
    }
}

fn task_input(f: &TaskFields) -> TaskInput {
    TaskInput {
        name: Some(f.name.clone()),
        description: f.description.clone(),
        status: f.status.clone(),
        priority: f.priority.clone(),
        assignees: f.assignees.clone(),
        tags: f.tags.clone(),
        due: f.due.clone(),
        start: f.start.clone(),
        estimate: f.estimate.clone(),
        points: f.points,
        parent: f.parent.clone(),
        task_type: f.task_type.clone(),
        fields: fields_to_input(&f.custom_fields),
        check_required: false,
    }
}

// ── auth & meta ──────────────────────────────────────────────────────────

pub fn auth_status(a: Account) -> AppResult<Value> {
    let cfg = load_config(a.profile.as_deref())?;
    Ok(json!({
        "profile": cfg.profile,
        "workspace_id": cfg.workspace_id,
        "token_present": cfg.token.is_some(),
        "token": cfg.masked_token(),
        "config_path": cfg.config_path.display().to_string(),
        "timezone": cfg.timezone,
    }))
}

pub async fn whoami(a: Account) -> AppResult<Value> {
    let ctx = Ctx::load(&a).await?;
    let user = ctx.client.user().await?;
    let ws = match &ctx.cfg.workspace_id {
        Some(id) => ctx
            .client
            .workspaces()
            .await?
            .into_iter()
            .find(|w| &w.id == id)
            .map(|w| json!({ "id": w.id, "name": w.name })),
        None => None,
    };
    let plan = if ws.is_some() {
        ctx.client.workspace_plan().await.ok()
    } else {
        None
    };
    Ok(json!({
        "id": user.id, "username": user.username, "email": user.email,
        "timezone": user.timezone, "workspace": ws, "plan": plan,
    }))
}

pub async fn workspace_list(a: Account) -> AppResult<Value> {
    let ctx = Ctx::load(&a).await?;
    let current = ctx.cfg.workspace_id.clone();
    let ws = ctx.client.workspaces().await?;
    Ok(json!(ws
        .iter()
        .map(|w| json!({
            "id": w.id, "name": w.name, "members": w.members.len(),
            "current": current.as_deref() == Some(w.id.as_str()),
        }))
        .collect::<Vec<_>>()))
}

pub async fn hierarchy(a: HierarchyArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let mut tree = r.hierarchy(a.refresh.unwrap_or(false)).await?;
    if let Some(space) = &a.space {
        let s = r.space(space).await?;
        tree.spaces.retain(|x| x.id == s.id);
    }
    Ok(serde_json::to_value(tree)?)
}

pub async fn list_get(a: ListArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let list = r.list(&a.list).await?;
    let detail = ctx.client.list(&list.id).await?;
    let fields = r.fields(&list.id).await?;
    let members = ctx.client.list_members(&list.id).await?;
    Ok(json!({
        "id": detail.id,
        "name": detail.name,
        "path": list.path(),
        "statuses": detail.statuses.iter().map(|s| json!({ "name": s.status, "type": s.kind, "order": s.orderindex })).collect::<Vec<_>>(),
        "custom_fields": fields.iter().map(|f| json!({
            "id": f.id, "name": f.name, "type": f.kind,
            "options": f.options().iter().map(|o| o.name.clone()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "members": members.iter().map(|u| json!({ "id": u.id, "username": u.username, "email": u.email })).collect::<Vec<_>>(),
    }))
}

pub async fn member_list(a: MemberListArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let members = match &a.list {
        Some(l) => ctx.client.list_members(&r.list(l).await?.id).await?,
        None => r.members().await?,
    };
    Ok(json!(members
        .iter()
        .map(|u| json!({ "id": u.id, "username": u.username, "email": u.email }))
        .collect::<Vec<_>>()))
}

pub async fn member_resolve(a: MemberResolveArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let mut out = Vec::new();
    for q in &a.queries {
        out.push(match r.member(q).await {
            Ok(u) => json!({ "query": q, "id": u.id, "username": u.username, "email": u.email }),
            Err(e) => {
                let err = AppError::from(e);
                json!({ "query": q, "error": err.code(), "data": err.data() })
            }
        });
    }
    Ok(Value::Array(out))
}

pub async fn api_request(a: ApiRequestArgs) -> AppResult<Value> {
    let method = a.method.trim().to_ascii_uppercase();
    if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
        return Err(AppError::validation(
            "method must be GET, POST, PUT, PATCH or DELETE",
        ));
    }
    if method != "GET" {
        require_confirm(a.confirm)?;
    }
    if !a.path.starts_with("/v2/") && !a.path.starts_with("/v3/") {
        return Err(AppError::validation("path must start with /v2/ or /v3/"));
    }
    let mut path = a.path.clone();
    if let Some(q) = a.query.filter(|q| !q.is_empty()) {
        if path.contains('?') {
            return Err(AppError::validation(
                "path already has a query string; omit `query`",
            ));
        }
        let mut ser = clickupc_core::client::Query::new();
        for (k, v) in q {
            if v.is_null() || v.is_array() || v.is_object() {
                return Err(AppError::validation(format!(
                    "query parameter '{k}' must be a string, number or boolean"
                )));
            }
            ser = ser.push(&k, value_to_input(&v));
        }
        let encoded: String = ser
            .pairs()
            .iter()
            .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
            .collect::<Vec<_>>()
            .join("&");
        path = format!("{path}?{encoded}");
    }
    let ctx = Ctx::load(&a.account).await?;
    Ok(ctx.client.raw(&method, &path, a.body).await?)
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn profile_list() -> AppResult<Value> {
    let file = ConfigFile::load()?;
    let active = file.selected_profile_name();
    Ok(json!({
        "active": active,
        "profiles": file.profiles.iter().map(|(name, p)| json!({
            "name": name,
            "workspace_id": p.workspace_id,
            "token": p.token.as_deref().map(mask_token),
            "active": *name == active,
        })).collect::<Vec<_>>(),
        "hint": "Pass `profile` (and optionally `workspace`) to any clickup_* tool to run it as that account.",
    }))
}

// ── tasks ────────────────────────────────────────────────────────────────

pub async fn task_search(a: TaskSearchArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let tasks = ops::search(&r, &search_input(&a)).await?;
    let full = is_full(&a.detail);
    let items = tasks
        .iter()
        .map(|t| task_out(t, full, ctx.tz()))
        .collect::<AppResult<Vec<_>>>()?;
    Ok(json!({ "count": items.len(), "tasks": items }))
}

pub async fn task_get(a: TaskGetArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let t = task_ref(&a.task)?;
    let full = is_full(&a.detail);
    let task = ctx
        .client
        .get_task(&t, a.include_subtasks.unwrap_or(false), full)
        .await?;
    let mut out = task_out(&task, full, ctx.tz())?;
    if a.include_subtasks.unwrap_or(false) {
        out["subtasks"] = json!(task
            .subtasks
            .iter()
            .flatten()
            .map(|s| ops::task_summary(s, ctx.tz()))
            .collect::<Vec<_>>());
    }
    if a.include_comments.unwrap_or(false) {
        let comments = ctx.client.task_comments(&t, Some(25)).await?;
        out["comments"] = json!(comments
            .iter()
            .map(|c| comment_out(c, ctx.tz()))
            .collect::<Vec<_>>());
    }
    Ok(out)
}

pub async fn task_create(a: TaskCreateArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let t = ops::create_task(&r, &a.list, &task_input(&a.fields)).await?;
    Ok(ops::task_summary(&t, ctx.tz()))
}

pub async fn task_update(a: TaskUpdateArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let input = UpdateInput {
        task: TaskInput {
            name: a.name,
            description: a.description,
            status: a.status,
            priority: a.priority,
            due: a.due,
            start: a.start,
            estimate: a.estimate,
            points: a.points,
            parent: a.parent,
            task_type: a.task_type,
            fields: fields_to_input(&a.custom_fields),
            ..Default::default()
        },
        assign: a.assignees_add,
        unassign: a.assignees_remove,
        watch: a.watchers_add,
        unwatch: a.watchers_remove,
        archived: a.archived,
        clear_fields: vec![],
    };
    let out = ops::update_task(&r, &task_ref(&a.task)?, &input).await?;
    Ok(json!({ "task": ops::task_summary(&out.task, ctx.tz()), "custom_fields": out.fields }))
}

pub async fn task_set_status(a: TaskStatusArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let t = ops::set_status(&r, &task_ref(&a.task)?, &a.status).await?;
    Ok(ops::task_summary(&t, ctx.tz()))
}

pub async fn task_move(a: TaskMoveArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    Ok(ops::move_task(
        &r,
        &task_ref(&a.task)?,
        &a.list,
        &a.status_map.unwrap_or_default(),
        a.move_custom_fields.unwrap_or(false),
    )
    .await?)
}

pub async fn task_delete(a: TaskConfirmArgs) -> AppResult<Value> {
    require_confirm(a.confirm)?;
    let ctx = Ctx::load(&a.account).await?;
    ctx.client.delete_task(&task_ref(&a.task)?).await?;
    Ok(json!({ "deleted": a.task }))
}

// ── bulk ─────────────────────────────────────────────────────────────────

pub async fn task_bulk_create(a: BulkCreateArgs) -> AppResult<Value> {
    if a.tasks.is_empty() {
        return Err(AppError::validation("tasks must not be empty"));
    }
    let inputs: Vec<TaskInput> = a.tasks.iter().map(task_input).collect();
    if a.dry_run.unwrap_or(false) {
        return Ok(json!({
            "dry_run": true,
            "list": a.list,
            "would_create": inputs.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
        }));
    }
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let results = ops::bulk_create(&r, &a.list, inputs, || {}).await?;
    let items: Vec<Value> = results
        .iter()
        .zip(&a.tasks)
        .map(|(res, input)| match res {
            Ok(t) => {
                json!({ "name": input.name, "ok": true, "task": ops::task_summary(t, ctx.tz()) })
            }
            Err(e) => json!({ "name": input.name, "ok": false, "error": e.to_string() }),
        })
        .collect();
    let ok = items.iter().filter(|i| i["ok"] == true).count();
    Ok(json!({ "created": ok, "failed": items.len() - ok, "results": items }))
}

pub async fn task_bulk_update(a: BulkUpdateArgs) -> AppResult<Value> {
    let changes = BulkChanges {
        status: a.changes.status.clone(),
        priority: a.changes.priority.clone(),
        assignees_add: a.changes.assignees_add.clone(),
        tags_add: a.changes.tags_add.clone(),
        due: a.changes.due.clone(),
    };
    if changes.is_empty() {
        return Err(AppError::validation("changes must set at least one field"));
    }
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let tasks: Vec<TaskRef> = match (&a.tasks, &a.filter) {
        (Some(ids), None) => ids.iter().map(|s| task_ref(s)).collect::<AppResult<_>>()?,
        (None, Some(f)) => {
            let mut input = search_input(f);
            input.limit = Some(
                f.limit
                    .unwrap_or(MAX_SEARCH_LIMIT)
                    .clamp(1, MAX_SEARCH_LIMIT),
            );
            ops::search(&r, &input)
                .await?
                .into_iter()
                .map(|t| TaskRef::Id(t.id))
                .collect()
        }
        _ => {
            return Err(AppError::validation(
                "provide exactly one of `tasks` or `filter`",
            ))
        }
    };
    if a.dry_run.unwrap_or(false) {
        return Ok(json!({
            "dry_run": true,
            "matched": tasks.len(),
            "tasks": tasks.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "changes": changes,
        }));
    }
    if tasks.len() > 1 {
        require_confirm(a.confirm)?;
    }
    let results = ops::bulk_update(&r, tasks, &changes, || {}).await?;
    let items: Vec<Value> = results
        .iter()
        .map(|(t, res)| match res {
            Ok(()) => json!({ "task": t, "ok": true }),
            Err(e) => json!({ "task": t, "ok": false, "error": e.to_string() }),
        })
        .collect();
    let ok = items.iter().filter(|i| i["ok"] == true).count();
    Ok(json!({ "updated": ok, "failed": items.len() - ok, "results": items }))
}

pub async fn task_batch(a: BatchArgs) -> AppResult<Value> {
    let ops_list: Vec<BatchOp> = a
        .ops
        .iter()
        .map(|v| serde_json::from_value(v.clone()))
        .collect::<Result<_, _>>()
        .map_err(|e| AppError::validation(format!("invalid op: {e}")))?;
    if ops_list.is_empty() {
        return Err(AppError::validation("ops must not be empty"));
    }
    if a.dry_run.unwrap_or(false) {
        return Ok(json!({
            "dry_run": true,
            "plan": ops_list.iter().map(BatchOp::describe).collect::<Vec<_>>(),
        }));
    }
    if ops_list.len() > 1 || ops_list.iter().any(BatchOp::is_destructive) {
        require_confirm(a.confirm)?;
    }
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let mut results = Vec::new();
    for op in &ops_list {
        results.push(match ops::run_batch_op(&r, op).await {
            Ok(v) => json!({ "op": op.describe(), "ok": true, "result": v }),
            Err(e) => json!({ "op": op.describe(), "ok": false, "error": e.to_string() }),
        });
    }
    Ok(json!({ "results": results }))
}

// ── comments ─────────────────────────────────────────────────────────────

pub async fn comment_list(a: CommentListArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let comments = ctx
        .client
        .task_comments(&task_ref(&a.task)?, Some(a.limit.unwrap_or(25).max(1)))
        .await?;
    Ok(json!(comments
        .iter()
        .map(|c| comment_out(c, ctx.tz()))
        .collect::<Vec<_>>()))
}

pub async fn comment_add(a: CommentAddArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let assignee = match &a.assignee {
        Some(m) => Some(ctx.resolver()?.member_id(m).await?),
        None => None,
    };
    Ok(ctx
        .client
        .add_comment(
            &task_ref(&a.task)?,
            &a.text,
            assignee,
            a.notify_all.unwrap_or(false),
        )
        .await?)
}

pub async fn comment_update(a: CommentUpdateArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    ctx.client
        .update_comment(&a.comment_id, &a.text, a.resolved)
        .await?;
    Ok(json!({ "updated": a.comment_id }))
}

pub async fn comment_delete(a: CommentDeleteArgs) -> AppResult<Value> {
    require_confirm(a.confirm)?;
    let ctx = Ctx::load(&a.account).await?;
    ctx.client.delete_comment(&a.comment_id).await?;
    Ok(json!({ "deleted": a.comment_id }))
}

pub async fn comment_replies(a: CommentIdArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let replies = ctx.client.comment_replies(&a.comment_id).await?;
    Ok(json!(replies
        .iter()
        .map(|c| comment_out(c, ctx.tz()))
        .collect::<Vec<_>>()))
}

pub async fn comment_reply(a: CommentReplyArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    Ok(ctx
        .client
        .reply_comment(&a.comment_id, &a.text, false)
        .await?)
}

// ── custom fields ────────────────────────────────────────────────────────

pub async fn field_list(a: ListArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let list = r.list(&a.list).await?;
    let fields = ctx.client.list_fields(&list.id).await?;
    Ok(json!(fields
        .iter()
        .map(|f| json!({
            "id": f.id, "name": f.name, "type": f.kind, "required": f.required,
            "options": f.options().iter().map(|o| o.name.clone()).collect::<Vec<_>>(),
        }))
        .collect::<Vec<_>>()))
}

async fn task_list_id(ctx: &Ctx, t: &TaskRef) -> AppResult<String> {
    ctx.client
        .get_task(t, false, false)
        .await?
        .list
        .map(|l| l.id)
        .ok_or_else(|| AppError::validation("task has no home list"))
}

pub async fn field_set(a: FieldSetArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let t = task_ref(&a.task)?;
    let list = task_list_id(&ctx, &t).await?;
    let field = r.field(&list, &a.field).await?;
    let value = r.field_value(&field, &value_to_input(&a.value)).await?;
    ctx.client.set_field(&t, &field.id, &value).await?;
    Ok(json!({ "task": a.task, "field": field.name, "value": value.value }))
}

pub async fn field_clear(a: FieldClearArgs) -> AppResult<Value> {
    require_confirm(a.confirm)?;
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let t = task_ref(&a.task)?;
    let list = task_list_id(&ctx, &t).await?;
    let field = r.field(&list, &a.field).await?;
    ctx.client.clear_field(&t, &field.id).await?;
    Ok(json!({ "task": a.task, "cleared": field.name }))
}

// ── relations ────────────────────────────────────────────────────────────

pub async fn tag_add(a: TagArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let t = task_ref(&a.task)?;
    for tag in &a.tags {
        ctx.client.add_tag(&t, tag).await?;
    }
    Ok(json!({ "task": a.task, "added": a.tags }))
}

pub async fn tag_remove(a: TagArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let t = task_ref(&a.task)?;
    for tag in &a.tags {
        ctx.client.remove_tag(&t, tag).await?;
    }
    Ok(json!({ "task": a.task, "removed": a.tags }))
}

pub async fn link_add(a: LinkArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    ctx.client
        .add_link(&task_ref(&a.task)?, &task_ref(&a.other_task)?)
        .await?;
    Ok(json!({ "linked": [a.task, a.other_task] }))
}

pub async fn link_remove(a: LinkArgs) -> AppResult<Value> {
    require_confirm(a.confirm)?;
    let ctx = Ctx::load(&a.account).await?;
    ctx.client
        .remove_link(&task_ref(&a.task)?, &task_ref(&a.other_task)?)
        .await?;
    Ok(json!({ "unlinked": [a.task, a.other_task] }))
}

fn dependency(a: &DependencyArgs) -> AppResult<(TaskRef, clickupc_core::client::Dependency)> {
    use clickupc_core::client::Dependency;
    match (&a.blocks, &a.waiting_on) {
        (Some(o), None) => Ok((task_ref(o)?, Dependency::Blocks)),
        (None, Some(o)) => Ok((task_ref(o)?, Dependency::WaitingOn)),
        _ => Err(AppError::validation(
            "provide exactly one of `blocks` or `waiting_on`",
        )),
    }
}

pub async fn dependency_add(a: DependencyArgs) -> AppResult<Value> {
    let (other, kind) = dependency(&a)?;
    let ctx = Ctx::load(&a.account).await?;
    ctx.client
        .add_dependency(&task_ref(&a.task)?, &other, kind)
        .await?;
    Ok(json!({ "task": a.task, "other": other.to_string(), "kind": format!("{kind:?}") }))
}

pub async fn dependency_remove(a: DependencyArgs) -> AppResult<Value> {
    require_confirm(a.confirm)?;
    let (other, kind) = dependency(&a)?;
    let ctx = Ctx::load(&a.account).await?;
    ctx.client
        .remove_dependency(&task_ref(&a.task)?, &other, kind)
        .await?;
    Ok(json!({ "task": a.task, "removed": other.to_string() }))
}

pub async fn checklist_add(a: ChecklistAddArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let c = ctx
        .client
        .create_checklist(&task_ref(&a.task)?, &a.name, &a.items)
        .await?;
    Ok(serde_json::to_value(c)?)
}

pub async fn checklist_item_update(a: ChecklistItemUpdateArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    Ok(ctx
        .client
        .update_checklist_item(&a.checklist_id, &a.item_id, a.resolved, a.name.as_deref())
        .await?)
}

pub async fn attachment_upload(a: AttachmentArgs) -> AppResult<Value> {
    let path = PathBuf::from(&a.path);
    if !path.is_file() {
        return Err(AppError::validation(format!("no such file: {}", a.path)));
    }
    let ctx = Ctx::load(&a.account).await?;
    Ok(ctx
        .client
        .upload_attachment(&task_ref(&a.task)?, &path)
        .await?)
}

pub async fn task_add_to_list(a: TaskListArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let list = ctx.resolver()?.list(&a.list).await?;
    ctx.client
        .add_task_to_list(&task_ref(&a.task)?, &list.id)
        .await?;
    Ok(json!({ "task": a.task, "added_to": list.path() }))
}

pub async fn task_remove_from_list(a: TaskListArgs) -> AppResult<Value> {
    require_confirm(a.confirm)?;
    let ctx = Ctx::load(&a.account).await?;
    let list = ctx.resolver()?.list(&a.list).await?;
    ctx.client
        .remove_task_from_list(&task_ref(&a.task)?, &list.id)
        .await?;
    Ok(json!({ "task": a.task, "removed_from": list.path() }))
}

// ── time ─────────────────────────────────────────────────────────────────

pub async fn time_list(a: TimeListArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let mut q = TimeQuery {
        start: a
            .from
            .as_deref()
            .map(|s| parse_when(s, ctx.tz()))
            .transpose()?,
        end: a
            .to
            .as_deref()
            .map(|s| parse_when(s, ctx.tz()))
            .transpose()?,
        task: a.task.as_deref().map(task_ref).transpose()?,
        ..Default::default()
    };
    if let Some(l) = &a.list {
        q.list_id = Some(r.list(l).await?.id);
    }
    if let Some(s) = &a.space {
        q.space_id = Some(r.space(s).await?.id);
    }
    for u in &a.users {
        q.assignees.push(r.member(u).await?.id);
    }
    let entries = ctx.client.time_entries(&q).await?;
    let total: i64 = entries.iter().map(|e| e.effective_ms(now_ms())).sum();
    Ok(json!({
        "total": format_duration(total),
        "total_ms": total,
        "entries": entries.iter().map(|e| time_out(e, ctx.tz())).collect::<Vec<_>>(),
    }))
}

pub async fn time_add(a: TimeAddArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let duration = parse_duration(&a.duration)?;
    let start = match &a.at {
        Some(s) => parse_when(s, ctx.tz())?,
        None => now_ms() - duration,
    };
    let e = ctx
        .client
        .add_time_entry(&NewTimeEntry {
            task: Some(task_ref(&a.task)?),
            start,
            duration,
            description: a.note,
            billable: a.billable,
        })
        .await?;
    Ok(time_out(&e, ctx.tz()))
}

pub async fn time_start(a: TimeStartArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let t = a.task.as_deref().map(task_ref).transpose()?;
    let e = ctx
        .client
        .start_timer(t.as_ref(), a.note.as_deref())
        .await?;
    Ok(time_out(&e, ctx.tz()))
}

pub async fn time_stop(a: Account) -> AppResult<Value> {
    let ctx = Ctx::load(&a).await?;
    let e = ctx.client.stop_timer().await?;
    Ok(time_out(&e, ctx.tz()))
}

pub async fn time_current(a: Account) -> AppResult<Value> {
    let ctx = Ctx::load(&a).await?;
    Ok(match ctx.client.current_timer().await? {
        Some(e) => time_out(&e, ctx.tz()),
        None => Value::Null,
    })
}

pub async fn time_delete(a: TimeDeleteArgs) -> AppResult<Value> {
    require_confirm(a.confirm)?;
    let ctx = Ctx::load(&a.account).await?;
    ctx.client.delete_time_entry(&a.entry_id).await?;
    Ok(json!({ "deleted": a.entry_id }))
}

pub async fn time_in_status(a: TimeInStatusArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    match (&a.task, &a.tasks) {
        (Some(t), None) => Ok(ctx.client.time_in_status(&task_ref(t)?).await?),
        (None, Some(ids)) if !ids.is_empty() => Ok(ctx.client.bulk_time_in_status(ids).await?),
        _ => Err(AppError::validation(
            "provide exactly one of `task` or `tasks`",
        )),
    }
}

// ── reports ──────────────────────────────────────────────────────────────

pub async fn standup(a: StandupArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let s = ops::standup(
        &r,
        a.since.as_deref().unwrap_or("yesterday"),
        a.user.as_deref(),
    )
    .await?;
    let tz = ctx.tz();
    let list = |ts: &[Task]| {
        ts.iter()
            .map(|t| ops::task_summary(t, tz))
            .collect::<Vec<_>>()
    };
    Ok(json!({
        "user": s.user,
        "since": to_iso(s.since, tz),
        "closed": list(&s.closed),
        "updated": list(&s.updated),
        "in_progress": list(&s.in_progress),
        "due_soon": list(&s.due_soon),
        "overdue": list(&s.overdue),
    }))
}

pub async fn list_summary(a: ListArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    Ok(serde_json::to_value(ops::list_summary(&r, &a.list).await?)?)
}

pub async fn list_create(a: ListCreateArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let (folder, space) = match (&a.folder, &a.space) {
        (Some(f), _) => (Some(r.folder(f).await?.id), None),
        (None, Some(s)) => (None, Some(r.space(s).await?.id)),
        (None, None) => return Err(AppError::validation("provide `folder` or `space`")),
    };
    let list = ctx
        .client
        .create_list(
            folder.as_deref(),
            space.as_deref(),
            &a.name,
            a.content.as_deref(),
        )
        .await?;
    let _ = r.cache().clear();
    Ok(json!({ "id": list.id, "name": list.name }))
}

pub async fn folder_create(a: FolderCreateArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let space = r.space(&a.space).await?;
    let folder = ctx.client.create_folder(&space.id, &a.name).await?;
    let _ = r.cache().clear();
    Ok(json!({ "id": folder.id, "name": folder.name, "space": space.name }))
}

// ── docs ─────────────────────────────────────────────────────────────────

pub async fn doc_search(a: DocSearchArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let docs = ctx
        .client
        .search_docs(a.query.as_deref(), a.limit.unwrap_or(50).max(1))
        .await?;
    Ok(json!(docs
        .iter()
        .map(|d| json!({ "id": d.get("id"), "name": d.get("name"), "parent": d.get("parent") }))
        .collect::<Vec<_>>()))
}

pub async fn doc_get(a: DocGetArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    match &a.page_id {
        Some(p) => Ok(ctx.client.doc_page(&a.doc_id, p).await?),
        None => {
            let doc = ctx.client.get_doc(&a.doc_id).await?;
            let pages = ctx.client.doc_page_listing(&a.doc_id).await?;
            Ok(json!({ "doc": doc, "pages": pages }))
        }
    }
}

pub async fn doc_pages(a: DocIdArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    Ok(ctx.client.doc_pages(&a.doc_id).await?)
}

pub async fn doc_create(a: DocCreateArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    let r = ctx.resolver()?;
    let parent = match (
        a.parent_type.as_deref().map(str::to_ascii_lowercase),
        &a.parent,
    ) {
        (None, None) => None,
        (Some(t), _) if t == "workspace" => {
            Some((ctx.client.workspace_id()?.to_string(), DocParent::Workspace))
        }
        (Some(t), Some(p)) if t == "space" => Some((r.space(p).await?.id, DocParent::Space)),
        (Some(t), Some(p)) if t == "folder" => Some((r.folder(p).await?.id, DocParent::Folder)),
        (Some(t), Some(p)) if t == "list" => Some((r.list(p).await?.id, DocParent::List)),
        _ => return Err(AppError::validation(
            "parent_type must be space, folder, list or workspace (with `parent` unless workspace)",
        )),
    };
    Ok(ctx
        .client
        .create_doc(&a.name, parent.as_ref().map(|(id, k)| (id.as_str(), *k)))
        .await?)
}

pub async fn doc_page_create(a: DocPageCreateArgs) -> AppResult<Value> {
    let ctx = Ctx::load(&a.account).await?;
    Ok(ctx
        .client
        .create_doc_page(
            &a.doc_id,
            &a.name,
            a.content.as_deref(),
            a.parent_page_id.as_deref(),
        )
        .await?)
}

pub async fn doc_page_update(a: DocPageUpdateArgs) -> AppResult<Value> {
    let mode = a.mode.as_deref().unwrap_or("replace").to_ascii_lowercase();
    if !matches!(mode.as_str(), "replace" | "append" | "prepend") {
        return Err(AppError::validation(
            "mode must be replace, append or prepend",
        ));
    }
    let ctx = Ctx::load(&a.account).await?;
    Ok(ctx
        .client
        .edit_doc_page(
            &a.doc_id,
            &a.page_id,
            a.content.as_deref(),
            a.name.as_deref(),
            &mode,
        )
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_inputs() {
        assert_eq!(value_to_input(&json!("High")), "High");
        assert_eq!(value_to_input(&json!(3)), "3");
        assert_eq!(value_to_input(&json!(true)), "true");
        assert_eq!(value_to_input(&json!(["a", "b"])), "a,b");
    }

    #[test]
    fn confirm_required() {
        assert_eq!(
            require_confirm(None).unwrap_err().code(),
            "unsafe_operation"
        );
        assert!(require_confirm(Some(true)).is_ok());
    }

    #[test]
    fn search_limit_is_clamped() {
        let a = TaskSearchArgs {
            limit: Some(10_000),
            ..Default::default()
        };
        assert_eq!(search_input(&a).limit, Some(MAX_SEARCH_LIMIT));
        assert_eq!(
            search_input(&TaskSearchArgs::default()).limit,
            Some(DEFAULT_SEARCH_LIMIT)
        );
    }

    #[test]
    fn query_encoding() {
        assert_eq!(enc("in progress"), "in%20progress");
        assert_eq!(enc("a[]"), "a%5B%5D");
    }
}
