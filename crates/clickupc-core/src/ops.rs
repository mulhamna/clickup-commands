//! Operations that take human input (names, dates like `tomorrow`, option
//! names) and turn them into API calls. Shared by the CLI and MCP server so
//! both behave identically.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    client::{CreateTask, FieldValue, MoveTask, UpdateTask},
    error::{ClickUpError, Result},
    filter::{parse_field_expr, FieldFilter, OrderBy, Range, TaskFilter},
    model::{parse_priority, CustomField, Task, TimeEntry},
    resolve::Resolver,
    task_ref::TaskRef,
    time::{format_ms, now_ms, parse_duration, parse_when},
};

/// Task search in human terms. Every name may also be an id.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SearchInput {
    pub lists: Vec<String>,
    pub folders: Vec<String>,
    pub spaces: Vec<String>,
    pub statuses: Vec<String>,
    pub include_closed: bool,
    pub assignees: Vec<String>,
    pub tags: Vec<String>,
    pub due_before: Option<String>,
    pub due_after: Option<String>,
    pub overdue: bool,
    pub created_before: Option<String>,
    pub created_after: Option<String>,
    pub updated_before: Option<String>,
    pub updated_after: Option<String>,
    pub done_before: Option<String>,
    pub done_after: Option<String>,
    /// `Name<op>value` expressions, e.g. `Severity=High`, `Points>=3`.
    pub fields: Vec<String>,
    pub task_types: Vec<String>,
    pub parent: Option<String>,
    pub subtasks: bool,
    pub archived: Option<bool>,
    pub order_by: Option<String>,
    pub reverse: bool,
    pub limit: Option<usize>,
    pub include_markdown: bool,
}

fn when(input: &Option<String>, tz: Option<&str>) -> Result<Option<i64>> {
    input.as_deref().map(|s| parse_when(s, tz)).transpose()
}

fn looks_like_uuid(s: &str) -> bool {
    s.len() == 36 && s.chars().filter(|c| *c == '-').count() == 4
}

fn split_list(raw: &str) -> Vec<&str> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

async fn filter_scalar(r: &Resolver<'_>, field: &CustomField, raw: &str) -> Result<Value> {
    let tz = r.client().timezone();
    Ok(match field.kind.as_str() {
        "users" => json!(r.member_id(raw).await?),
        "drop_down" | "labels" | "date" | "number" | "currency" | "emoji" | "checkbox" => {
            let probe = CustomField {
                kind: if field.kind == "labels" {
                    "drop_down".into()
                } else {
                    field.kind.clone()
                },
                ..field.clone()
            };
            crate::client::convert_field_value(&probe, raw, tz)?.value
        }
        _ => json!(raw),
    })
}

async fn filter_value(
    r: &Resolver<'_>,
    field: &CustomField,
    op: &str,
    raw: Option<&str>,
) -> Result<Value> {
    let Some(raw) = raw else {
        return Ok(Value::Null);
    };
    match op {
        "RANGE" | "ANY" | "ALL" | "NOT ANY" | "NOT ALL" => {
            let parts = split_list(raw);
            if op == "RANGE" && parts.len() != 2 {
                return Err(ClickUpError::Invalid(format!(
                    "RANGE needs two values 'low,high', got '{raw}'"
                )));
            }
            let mut out = Vec::new();
            for p in parts {
                out.push(filter_scalar(r, field, p).await?);
            }
            Ok(Value::Array(out))
        }
        _ => filter_scalar(r, field, raw).await,
    }
}

/// Resolve names in `input` into a [`TaskFilter`].
pub async fn build_filter(r: &Resolver<'_>, input: &SearchInput) -> Result<TaskFilter> {
    let tz = r.client().timezone().map(str::to_string);
    let tz = tz.as_deref();
    let mut f = TaskFilter::default();
    for l in &input.lists {
        f.list_ids.push(r.list(l).await?.id);
    }
    for x in &input.folders {
        f.folder_ids.push(r.folder(x).await?.id);
    }
    for x in &input.spaces {
        f.space_ids.push(r.space(x).await?.id);
    }
    let single = f.single_list().map(str::to_string);
    for s in &input.statuses {
        f.statuses.push(match &single {
            Some(list) => r.status(list, s).await?,
            None => s.clone(),
        });
    }
    f.include_closed = input.include_closed;
    for a in &input.assignees {
        f.assignees.push(r.member(a).await?.id);
    }
    f.tags = input.tags.clone();
    f.due = Range {
        gt: when(&input.due_after, tz)?,
        lt: when(&input.due_before, tz)?,
    };
    if input.overdue {
        f.due.lt = Some(f.due.lt.map_or(now_ms(), |lt| lt.min(now_ms())));
    }
    f.created = Range {
        gt: when(&input.created_after, tz)?,
        lt: when(&input.created_before, tz)?,
    };
    f.updated = Range {
        gt: when(&input.updated_after, tz)?,
        lt: when(&input.updated_before, tz)?,
    };
    f.done = Range {
        gt: when(&input.done_after, tz)?,
        lt: when(&input.done_before, tz)?,
    };
    for expr in &input.fields {
        let (name, op, raw) = parse_field_expr(expr)?;
        let field = if looks_like_uuid(&name) {
            CustomField {
                id: name.clone(),
                name: name.clone(),
                ..Default::default()
            }
        } else {
            if f.list_ids.is_empty() {
                return Err(ClickUpError::Invalid(format!(
                    "field filter '{expr}' needs a list to resolve the field name (or pass the field id)"
                )));
            }
            let mut found = None;
            let mut last_err = None;
            for list in &f.list_ids {
                match r.field(list, &name).await {
                    Ok(field) => {
                        found = Some(field);
                        break;
                    }
                    Err(e) => last_err = Some(e),
                }
            }
            match (found, last_err) {
                (Some(field), _) => field,
                (None, Some(e)) => return Err(e),
                (None, None) => unreachable!("list_ids is not empty"),
            }
        };
        let value = filter_value(r, &field, &op, raw.as_deref()).await?;
        f.custom_fields.push(FieldFilter {
            field_id: field.id,
            operator: op,
            value,
        });
    }
    for t in &input.task_types {
        f.task_types.push(r.task_type(t).await?);
    }
    if let Some(p) = &input.parent {
        let p: TaskRef = p.parse()?;
        f.parent = Some(r.client().task_id(&p).await?);
    }
    f.subtasks = input.subtasks;
    f.archived = input.archived;
    f.order_by = input
        .order_by
        .as_deref()
        .map(str::parse::<OrderBy>)
        .transpose()?;
    f.reverse = input.reverse;
    f.limit = input.limit;
    f.include_markdown = input.include_markdown;
    Ok(f)
}

pub async fn search(r: &Resolver<'_>, input: &SearchInput) -> Result<Vec<Task>> {
    let filter = build_filter(r, input).await?;
    r.client().search_tasks(&filter).await
}

/// Fields shared by create and update. Strings are human input.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct TaskInput {
    pub name: Option<String>,
    /// Markdown.
    pub description: Option<String>,
    pub status: Option<String>,
    /// urgent | high | normal | low | none
    pub priority: Option<String>,
    pub assignees: Vec<String>,
    pub tags: Vec<String>,
    /// Date input, or `none` to clear on update.
    pub due: Option<String>,
    pub start: Option<String>,
    /// Duration like `2h`, or `none` to clear on update.
    pub estimate: Option<String>,
    pub points: Option<f64>,
    pub parent: Option<String>,
    pub task_type: Option<String>,
    /// Custom field name or id → human value.
    pub fields: BTreeMap<String, String>,
    pub check_required: bool,
}

fn has_time(ms: i64, tz: Option<&str>) -> bool {
    format_ms(ms, tz).contains(':')
}

fn is_none(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "none" | "null" | "clear"
    )
}

pub async fn build_create(
    r: &Resolver<'_>,
    list_id: &str,
    input: &TaskInput,
) -> Result<CreateTask> {
    let tz = r.client().timezone().map(str::to_string);
    let tz = tz.as_deref();
    let name = input
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .ok_or_else(|| ClickUpError::Invalid("task name is required".into()))?;
    let mut t = CreateTask {
        name,
        markdown_content: input.description.clone(),
        tags: input.tags.clone(),
        points: input.points,
        check_required_custom_fields: input.check_required.then_some(true),
        ..Default::default()
    };
    if let Some(s) = &input.status {
        t.status = Some(r.status(list_id, s).await?);
    }
    if let Some(p) = &input.priority {
        t.priority = parse_priority(p)?;
    }
    for a in &input.assignees {
        t.assignees.push(r.member_id(a).await?);
    }
    if let Some(d) = &input.due {
        let ms = parse_when(d, tz)?;
        t.due_date = Some(ms);
        t.due_date_time = Some(has_time(ms, tz));
    }
    if let Some(d) = &input.start {
        let ms = parse_when(d, tz)?;
        t.start_date = Some(ms);
        t.start_date_time = Some(has_time(ms, tz));
    }
    if let Some(e) = &input.estimate {
        t.time_estimate = Some(parse_duration(e)?);
    }
    if let Some(p) = &input.parent {
        let p: TaskRef = p.parse()?;
        t.parent = Some(r.client().task_id(&p).await?);
    }
    if let Some(tt) = &input.task_type {
        t.custom_item_id = Some(r.task_type(tt).await?);
    }
    for (name, raw) in &input.fields {
        let field = r.field(list_id, name).await?;
        let v = r.field_value(&field, raw).await?;
        t.custom_fields
            .push(json!({ "id": field.id, "value": v.value }));
    }
    Ok(t)
}

pub async fn create_task(r: &Resolver<'_>, list: &str, input: &TaskInput) -> Result<Task> {
    let list = r.list(list).await?;
    let body = build_create(r, &list.id, input).await?;
    r.client().create_task(&list.id, &body).await
}

/// Update in human terms; `assignees` in [`TaskInput`] is ignored here —
/// use `assign` / `unassign`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct UpdateInput {
    #[serde(flatten)]
    pub task: TaskInput,
    pub assign: Vec<String>,
    pub unassign: Vec<String>,
    pub watch: Vec<String>,
    pub unwatch: Vec<String>,
    pub archived: Option<bool>,
    pub clear_fields: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct UpdatePlan {
    pub update: UpdateTask,
    pub set_fields: Vec<(CustomField, FieldValue)>,
    pub clear_fields: Vec<CustomField>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldResult {
    pub field: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateOutcome {
    pub task: Task,
    pub fields: Vec<FieldResult>,
}

pub async fn build_update(
    r: &Resolver<'_>,
    task: &TaskRef,
    input: &UpdateInput,
) -> Result<UpdatePlan> {
    let tz = r.client().timezone().map(str::to_string);
    let tz = tz.as_deref();
    let t = &input.task;
    let needs_list = t.status.is_some() || !t.fields.is_empty() || !input.clear_fields.is_empty();
    let list_id = if needs_list {
        let current = r.client().get_task(task, false, false).await?;
        Some(
            current
                .list
                .map(|l| l.id)
                .ok_or_else(|| ClickUpError::Invalid("task has no home list".into()))?,
        )
    } else {
        None
    };
    let mut u = UpdateTask {
        name: t.name.clone(),
        markdown_content: t.description.clone(),
        points: t.points,
        archived: input.archived,
        ..Default::default()
    };
    if let (Some(s), Some(list)) = (&t.status, &list_id) {
        u.status = Some(r.status(list, s).await?);
    }
    if let Some(p) = &t.priority {
        u.priority = Some(parse_priority(p)?);
    }
    if let Some(d) = &t.due {
        if is_none(d) {
            u.due_date = Some(None);
        } else {
            let ms = parse_when(d, tz)?;
            u.due_date = Some(Some(ms));
            u.due_date_time = Some(has_time(ms, tz));
        }
    }
    if let Some(d) = &t.start {
        if is_none(d) {
            u.start_date = Some(None);
        } else {
            let ms = parse_when(d, tz)?;
            u.start_date = Some(Some(ms));
            u.start_date_time = Some(has_time(ms, tz));
        }
    }
    if let Some(e) = &t.estimate {
        u.time_estimate = Some(if is_none(e) {
            None
        } else {
            Some(parse_duration(e)?)
        });
    }
    if let Some(p) = &t.parent {
        let p: TaskRef = p.parse()?;
        u.parent = Some(r.client().task_id(&p).await?);
    }
    if let Some(tt) = &t.task_type {
        u.custom_item_id = Some(r.task_type(tt).await?);
    }
    for m in &input.assign {
        u.assignees_add.push(r.member_id(m).await?);
    }
    for m in &input.unassign {
        u.assignees_rem.push(r.member_id(m).await?);
    }
    for m in &input.watch {
        u.watchers_add.push(r.member_id(m).await?);
    }
    for m in &input.unwatch {
        u.watchers_rem.push(r.member_id(m).await?);
    }
    let mut set_fields = Vec::new();
    let mut clear_fields = Vec::new();
    if let Some(list) = &list_id {
        for (name, raw) in &t.fields {
            let field = r.field(list, name).await?;
            let v = r.field_value(&field, raw).await?;
            set_fields.push((field, v));
        }
        for name in &input.clear_fields {
            clear_fields.push(r.field(list, name).await?);
        }
    }
    Ok(UpdatePlan {
        update: u,
        set_fields,
        clear_fields,
    })
}

/// PUT the task (if anything changed), then set / clear each custom field
/// separately and report per-field results.
pub async fn apply_update(
    r: &Resolver<'_>,
    task: &TaskRef,
    plan: &UpdatePlan,
) -> Result<UpdateOutcome> {
    let c = r.client();
    let mut updated = if plan.update.is_empty() {
        None
    } else {
        Some(c.update_task(task, &plan.update).await?)
    };
    let mut fields = Vec::new();
    for (field, value) in &plan.set_fields {
        let res = c.set_field(task, &field.id, value).await;
        fields.push(FieldResult {
            field: field.name.clone(),
            ok: res.is_ok(),
            error: res.err().map(|e| e.to_string()),
        });
    }
    for field in &plan.clear_fields {
        let res = c.clear_field(task, &field.id).await;
        fields.push(FieldResult {
            field: field.name.clone(),
            ok: res.is_ok(),
            error: res.err().map(|e| e.to_string()),
        });
    }
    if updated.is_none() || !fields.is_empty() {
        updated = Some(c.get_task(task, false, false).await?);
    }
    Ok(UpdateOutcome {
        task: updated.expect("set above"),
        fields,
    })
}

pub async fn update_task(
    r: &Resolver<'_>,
    task: &TaskRef,
    input: &UpdateInput,
) -> Result<UpdateOutcome> {
    let plan = build_update(r, task, input).await?;
    apply_update(r, task, &plan).await
}

/// Validate against the task's list, then set.
pub async fn set_status(r: &Resolver<'_>, task: &TaskRef, status: &str) -> Result<Task> {
    let current = r.client().get_task(task, false, false).await?;
    let list = current
        .list
        .ok_or_else(|| ClickUpError::Invalid("task has no home list".into()))?;
    let canonical = r.status(&list.id, status).await?;
    r.client().set_status(task, &canonical).await
}

/// Move to a new home list. `status_map` maps old status name → new status
/// name. When the current status doesn't exist in the target list and no
/// mapping covers it, returns `Resolve { kind: "target status", .. }` whose
/// suggestions are the target list's statuses.
pub async fn move_task(
    r: &Resolver<'_>,
    task: &TaskRef,
    list: &str,
    status_map: &BTreeMap<String, String>,
    move_fields: bool,
) -> Result<Value> {
    let c = r.client();
    let current = c.get_task(task, false, false).await?;
    let target = r.list(list).await?;
    let target_list = r.list_detail(&target.id).await?;
    let current_status = current.status.status.clone();
    let exists = target_list
        .statuses
        .iter()
        .any(|s| s.status.eq_ignore_ascii_case(&current_status));
    let mut opts = MoveTask {
        move_custom_fields: move_fields.then_some(true),
        status_mappings: vec![],
    };
    let mapped = status_map
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(&current_status))
        .map(|(_, v)| v.clone());
    match (exists, mapped) {
        (_, Some(new_name)) => {
            let dest = target_list
                .statuses
                .iter()
                .find(|s| s.status.eq_ignore_ascii_case(&new_name))
                .ok_or_else(|| {
                    ClickUpError::resolve(
                        "target status",
                        new_name.clone(),
                        target_list
                            .statuses
                            .iter()
                            .map(|s| s.status.clone())
                            .collect(),
                    )
                })?;
            let (Some(src_id), Some(dst_id)) = (current.status.id.clone(), dest.id.clone()) else {
                return Err(ClickUpError::Invalid(
                    "status ids missing in API response; cannot build a status mapping".into(),
                ));
            };
            opts.status_mappings.push((src_id, dst_id));
        }
        (true, None) => {}
        (false, None) => {
            return Err(ClickUpError::resolve(
                "target status",
                current_status,
                target_list
                    .statuses
                    .iter()
                    .map(|s| s.status.clone())
                    .collect(),
            ))
        }
    }
    c.move_task(task, &target.id, &opts).await?;
    Ok(json!({ "task": current.id, "list": { "id": target.id, "name": target.name } }))
}

/// Compact task view used for LLM output and batch results.
pub fn task_summary(t: &Task, tz: Option<&str>) -> Value {
    json!({
        "id": t.id,
        "custom_id": t.custom_id,
        "name": t.name,
        "status": t.status.status,
        "priority": t.priority_name(),
        "assignees": t.assignees.iter().map(|u| json!({ "id": u.id, "username": u.username })).collect::<Vec<_>>(),
        "due": t.due_date.map(|ms| json!({ "ms": ms, "iso": crate::time::to_iso(ms, tz) })),
        "list": t.list.as_ref().map(|l| json!({ "id": l.id, "name": l.name })),
        "url": t.url,
    })
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Standup {
    pub user: String,
    pub since: i64,
    pub closed: Vec<Task>,
    pub updated: Vec<Task>,
    pub in_progress: Vec<Task>,
    pub due_soon: Vec<Task>,
    pub overdue: Vec<Task>,
}

const DAY_MS: i64 = 86_400_000;

/// My tasks closed / updated since, in progress, due within 3 days, overdue.
pub async fn standup(r: &Resolver<'_>, since: &str, user: Option<&str>) -> Result<Standup> {
    let tz = r.client().timezone().map(str::to_string);
    let since_ms = parse_when(since, tz.as_deref())?;
    let me = r.member(user.unwrap_or("me")).await?;
    let c = r.client();
    let recent = c
        .search_tasks(&TaskFilter {
            assignees: vec![me.id.clone()],
            include_closed: true,
            updated: Range {
                gt: Some(since_ms),
                lt: None,
            },
            subtasks: true,
            limit: Some(500),
            ..Default::default()
        })
        .await?;
    let open = c
        .search_tasks(&TaskFilter {
            assignees: vec![me.id.clone()],
            subtasks: true,
            limit: Some(500),
            ..Default::default()
        })
        .await?;
    let now = now_ms();
    let mut out = Standup {
        user: me.display(),
        since: since_ms,
        ..Default::default()
    };
    for t in recent {
        if t.status.is_closed() || t.date_closed.is_some_and(|d| d >= since_ms) {
            out.closed.push(t);
        } else {
            out.updated.push(t);
        }
    }
    for t in open {
        if t.status.is_closed() {
            continue;
        }
        if t.due_date.is_some_and(|d| d < now) {
            out.overdue.push(t.clone());
        } else if t.due_date.is_some_and(|d| d <= now + 3 * DAY_MS) {
            out.due_soon.push(t.clone());
        }
        if t.status.kind.as_deref() != Some("open") {
            out.in_progress.push(t);
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ListSummary {
    pub list: String,
    pub total: usize,
    pub overdue: usize,
    pub by_status: BTreeMap<String, usize>,
    pub by_assignee: BTreeMap<String, usize>,
}

pub async fn list_summary(r: &Resolver<'_>, list: &str) -> Result<ListSummary> {
    let list = r.list(list).await?;
    let tasks = r
        .client()
        .search_tasks(&TaskFilter {
            list_ids: vec![list.id.clone()],
            include_closed: true,
            subtasks: true,
            ..Default::default()
        })
        .await?;
    let now = now_ms();
    let mut s = ListSummary {
        list: list.path(),
        total: tasks.len(),
        ..Default::default()
    };
    for t in &tasks {
        *s.by_status.entry(t.status.status.clone()).or_default() += 1;
        if t.assignees.is_empty() {
            *s.by_assignee.entry("(unassigned)".into()).or_default() += 1;
        }
        for a in &t.assignees {
            *s.by_assignee.entry(a.display()).or_default() += 1;
        }
        if !t.status.is_closed() && t.due_date.is_some_and(|d| d < now) {
            s.overdue += 1;
        }
    }
    Ok(s)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReportBy {
    Task,
    List,
    Day,
}

impl std::str::FromStr for ReportBy {
    type Err = ClickUpError;
    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "task" => Ok(Self::Task),
            "list" => Ok(Self::List),
            "day" => Ok(Self::Day),
            other => Err(ClickUpError::Invalid(format!(
                "unknown grouping '{other}' (use task, list or day)"
            ))),
        }
    }
}

/// Client-side aggregation of time entries → (group, total ms), largest first.
pub fn time_report(entries: &[TimeEntry], by: ReportBy, tz: Option<&str>) -> Vec<(String, i64)> {
    let now = now_ms();
    let mut totals: BTreeMap<String, i64> = BTreeMap::new();
    for e in entries {
        let key = match by {
            ReportBy::Task => match (e.task_id(), e.task_name()) {
                (Some(id), Some(name)) => format!("{id} {name}"),
                (Some(id), None) => id,
                _ => "(no task)".into(),
            },
            ReportBy::List => e
                .task_location
                .as_ref()
                .and_then(|l| l.get("list_name"))
                .and_then(Value::as_str)
                .unwrap_or("(no list)")
                .to_string(),
            ReportBy::Day => e
                .start
                .map(|ms| format_ms(ms, tz).chars().take(10).collect())
                .unwrap_or_else(|| "(unknown)".into()),
        };
        *totals.entry(key).or_default() += e.effective_ms(now);
    }
    let mut out: Vec<(String, i64)> = totals.into_iter().collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.1));
    out
}

/// One entry of a batch file / `clickup_task_batch` call.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BatchOp {
    Create {
        list: String,
        #[serde(flatten)]
        task: TaskInput,
    },
    Update {
        task: String,
        #[serde(flatten)]
        update: UpdateInput,
    },
    Status {
        task: String,
        status: String,
    },
    Comment {
        task: String,
        text: String,
    },
    Delete {
        task: String,
    },
}

impl BatchOp {
    pub fn describe(&self) -> String {
        match self {
            Self::Create { list, task } => format!(
                "create '{}' in {list}",
                task.name.as_deref().unwrap_or_default()
            ),
            Self::Update { task, .. } => format!("update {task}"),
            Self::Status { task, status } => format!("set {task} status to '{status}'"),
            Self::Comment { task, .. } => format!("comment on {task}"),
            Self::Delete { task } => format!("delete {task}"),
        }
    }

    pub fn is_destructive(&self) -> bool {
        matches!(self, Self::Delete { .. })
    }
}

pub async fn run_batch_op(r: &Resolver<'_>, op: &BatchOp) -> Result<Value> {
    let tz = r.client().timezone().map(str::to_string);
    let tz = tz.as_deref();
    match op {
        BatchOp::Create { list, task } => {
            let t = create_task(r, list, task).await?;
            Ok(task_summary(&t, tz))
        }
        BatchOp::Update { task, update } => {
            let out = update_task(r, &task.parse()?, update).await?;
            Ok(json!({ "task": task_summary(&out.task, tz), "fields": out.fields }))
        }
        BatchOp::Status { task, status } => {
            let t = set_status(r, &task.parse()?, status).await?;
            Ok(task_summary(&t, tz))
        }
        BatchOp::Comment { task, text } => {
            r.client()
                .add_comment(&task.parse()?, text, None, false)
                .await
        }
        BatchOp::Delete { task } => {
            r.client().delete_task(&task.parse()?).await?;
            Ok(json!({ "deleted": task }))
        }
    }
}

/// Changes applied to every task in a bulk update.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct BulkChanges {
    pub status: Option<String>,
    pub priority: Option<String>,
    pub assignees_add: Vec<String>,
    pub tags_add: Vec<String>,
    pub due: Option<String>,
}

impl BulkChanges {
    pub fn is_empty(&self) -> bool {
        self.status.is_none()
            && self.priority.is_none()
            && self.assignees_add.is_empty()
            && self.tags_add.is_empty()
            && self.due.is_none()
    }
}

/// Resolve names once, then apply to each task with bounded concurrency.
/// Returns `(task, result)` in input order.
pub async fn bulk_update(
    r: &Resolver<'_>,
    tasks: Vec<TaskRef>,
    changes: &BulkChanges,
    on_done: impl Fn() + Send + Sync,
) -> Result<Vec<(String, Result<()>)>> {
    let tz = r.client().timezone().map(str::to_string);
    let mut u = UpdateTask {
        // ponytail: status is sent as typed; per-list validation would cost a
        // GET per task. ClickUp's error is reported per item instead.
        status: changes.status.clone(),
        ..Default::default()
    };
    if let Some(p) = &changes.priority {
        u.priority = Some(parse_priority(p)?);
    }
    for m in &changes.assignees_add {
        u.assignees_add.push(r.member_id(m).await?);
    }
    if let Some(d) = &changes.due {
        if is_none(d) {
            u.due_date = Some(None);
        } else {
            let ms = parse_when(d, tz.as_deref())?;
            u.due_date = Some(Some(ms));
            u.due_date_time = Some(has_time(ms, tz.as_deref()));
        }
    }
    let c = r.client();
    let u = &u;
    let tags = &changes.tags_add;
    let results = crate::bulk::run(
        c.limiter(),
        tasks,
        |t| async move {
            let res = async {
                if !u.is_empty() {
                    c.update_task(&t, u).await?;
                }
                for tag in tags {
                    c.add_tag(&t, tag).await?;
                }
                Ok(())
            }
            .await;
            (t.to_string(), res)
        },
        on_done,
    )
    .await;
    Ok(results)
}

/// Create many tasks in one list with bounded concurrency.
pub async fn bulk_create(
    r: &Resolver<'_>,
    list: &str,
    tasks: Vec<TaskInput>,
    on_done: impl Fn() + Send + Sync,
) -> Result<Vec<Result<Task>>> {
    let list = r.list(list).await?;
    let mut bodies = Vec::new();
    for t in &tasks {
        bodies.push(build_create(r, &list.id, t).await);
    }
    let c = r.client();
    let id = &list.id;
    Ok(crate::bulk::run(
        c.limiter(),
        bodies,
        |b| async move {
            match b {
                Ok(body) => c.create_task(id, &body).await,
                Err(e) => Err(e),
            }
        },
        on_done,
    )
    .await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_ops_parse() {
        let ops: Vec<BatchOp> = serde_json::from_value(json!([
            { "op": "create", "list": "sprint", "name": "A", "priority": "high", "points": 3 },
            { "op": "update", "task": "DEV-1", "assign": ["me"], "due": "none" },
            { "op": "status", "task": "abc", "status": "done" },
            { "op": "comment", "task": "abc", "text": "hi" },
            { "op": "delete", "task": "abc" }
        ]))
        .unwrap();
        assert_eq!(ops.len(), 5);
        assert!(ops[4].is_destructive());
        assert_eq!(ops[0].describe(), "create 'A' in sprint");
        match &ops[1] {
            BatchOp::Update { update, .. } => {
                assert_eq!(update.assign, vec!["me"]);
                assert_eq!(update.task.due.as_deref(), Some("none"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn report_groups() {
        let e = |task: &str, list: &str, start: i64, dur: i64| TimeEntry {
            id: "x".into(),
            task: Some(json!({ "id": task, "name": "T" })),
            start: Some(start),
            duration: Some(dur),
            task_location: Some(json!({ "list_name": list })),
            ..Default::default()
        };
        let entries = vec![
            e("a", "L1", 0, 1000),
            e("a", "L1", 86_400_000, 2000),
            e("b", "L2", 0, 5000),
        ];
        assert_eq!(
            time_report(&entries, ReportBy::Task, Some("UTC")),
            vec![("b T".into(), 5000), ("a T".into(), 3000)]
        );
        assert_eq!(
            time_report(&entries, ReportBy::List, Some("UTC"))[0],
            ("L2".into(), 5000)
        );
        assert_eq!(time_report(&entries, ReportBy::Day, Some("UTC")).len(), 2);
    }
}
