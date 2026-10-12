//! `clickupc task …`

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use clickupc_core::{
    client::Dependency,
    model::Task,
    ops::{self, BatchOp, BulkChanges, SearchInput, TaskInput, UpdateInput},
    time::{format_duration, format_ms},
    ClickUpError, TaskRef,
};
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::json;

use super::{file_or_stdin, Ctx, Session};
use crate::output::{confirm, is_tty, opt, print_json, print_table, require_tty};

#[derive(Debug, Clone, Default, Args)]
pub struct FilterArgs {
    /// List (name, alias, Space/Folder/List path or id); repeatable
    #[arg(long = "list")]
    pub lists: Vec<String>,
    #[arg(long = "folder")]
    pub folders: Vec<String>,
    #[arg(long = "space")]
    pub spaces: Vec<String>,
    #[arg(long = "status")]
    pub statuses: Vec<String>,
    /// Include closed tasks
    #[arg(long)]
    pub closed: bool,
    /// me | name | email | id; repeatable
    #[arg(long = "assignee")]
    pub assignees: Vec<String>,
    /// Shorthand for --assignee me
    #[arg(long)]
    pub mine: bool,
    #[arg(long = "tag")]
    pub tags: Vec<String>,
    #[arg(long)]
    pub due_before: Option<String>,
    #[arg(long)]
    pub due_after: Option<String>,
    /// Past due and not closed
    #[arg(long)]
    pub overdue: bool,
    #[arg(long)]
    pub created_before: Option<String>,
    #[arg(long)]
    pub created_after: Option<String>,
    #[arg(long)]
    pub updated_before: Option<String>,
    #[arg(long)]
    pub updated_after: Option<String>,
    #[arg(long)]
    pub done_before: Option<String>,
    #[arg(long)]
    pub done_after: Option<String>,
    /// Custom field filter like 'Severity=High' or 'Points>=3'; repeatable
    #[arg(long = "field")]
    pub fields: Vec<String>,
    /// task | milestone | custom type name | id; repeatable
    #[arg(long = "type")]
    pub task_types: Vec<String>,
    #[arg(long)]
    pub parent: Option<String>,
    /// Include subtasks
    #[arg(long)]
    pub subtasks: bool,
    /// Archived tasks (single-list only)
    #[arg(long)]
    pub archived: bool,
    #[arg(long)]
    pub limit: Option<usize>,
    /// created | updated | due | id
    #[arg(long)]
    pub order_by: Option<String>,
    #[arg(long)]
    pub reverse: bool,
}

impl FilterArgs {
    fn input(&self, s: &Session) -> SearchInput {
        let mut assignees = self.assignees.clone();
        if self.mine {
            assignees.push("me".into());
        }
        SearchInput {
            lists: self.lists.clone(),
            folders: self.folders.clone(),
            spaces: self.spaces.clone(),
            statuses: self.statuses.clone(),
            include_closed: self.closed,
            assignees,
            tags: self.tags.clone(),
            due_before: self.due_before.clone(),
            due_after: self.due_after.clone(),
            overdue: self.overdue,
            created_before: self.created_before.clone(),
            created_after: self.created_after.clone(),
            updated_before: self.updated_before.clone(),
            updated_after: self.updated_after.clone(),
            done_before: self.done_before.clone(),
            done_after: self.done_after.clone(),
            fields: self.fields.clone(),
            task_types: self.task_types.clone(),
            parent: self.parent.clone(),
            subtasks: self.subtasks,
            archived: self.archived.then_some(true),
            order_by: self.order_by.clone(),
            reverse: self.reverse,
            limit: self.limit.or(s.cfg.default_task_limit),
            include_markdown: false,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AddRemove {
    Add,
    Remove,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ChecklistAction {
    Add,
    Check,
    Uncheck,
}

#[derive(Debug, Subcommand)]
pub enum TaskCmd {
    /// Search tasks
    List {
        #[command(flatten)]
        filter: FilterArgs,
    },
    /// Show one task
    View {
        task: String,
        #[arg(long)]
        comments: bool,
        #[arg(long)]
        subtasks: bool,
        /// Open in the browser instead
        #[arg(long)]
        web: bool,
    },
    /// Create a task (prompts for list and name on a terminal)
    Create(CreateArgs),
    /// Update fields; custom fields are set separately and reported per field
    Update(UpdateArgs),
    /// Change status (validated against the task's list)
    Status { task: String, status: String },
    /// Add assignees
    Assign {
        task: String,
        #[arg(required = true)]
        members: Vec<String>,
    },
    /// Remove assignees
    Unassign {
        task: String,
        #[arg(required = true)]
        members: Vec<String>,
    },
    /// Move to a new home list
    Move {
        task: String,
        #[arg(long)]
        to: String,
        /// 'old status=new status'; repeatable
        #[arg(long = "map")]
        maps: Vec<String>,
        #[arg(long)]
        move_fields: bool,
    },
    /// Also show the task in another list
    AddTo {
        task: String,
        #[arg(long)]
        list: String,
    },
    /// Remove the task from an additional list
    RemoveFrom {
        task: String,
        #[arg(long)]
        list: String,
    },
    /// Permanently delete tasks (asks first unless --yes)
    Delete {
        #[arg(required = true)]
        tasks: Vec<String>,
    },
    /// Open in the browser
    Open { task: String },
    /// Add or remove tags (tags must exist in the space)
    Tag {
        task: String,
        action: AddRemove,
        #[arg(required = true)]
        tags: Vec<String>,
    },
    /// Link or unlink two tasks
    Link {
        task: String,
        action: AddRemove,
        other: String,
    },
    /// Add or remove a dependency (--blocks / --waiting-on)
    Dep {
        task: String,
        action: AddRemove,
        /// This task blocks <task>
        #[arg(long, conflicts_with = "waiting_on")]
        blocks: Option<String>,
        /// This task waits on <task>
        #[arg(long)]
        waiting_on: Option<String>,
    },
    /// `add <name> [--item …]`, or `check|uncheck <item-id>`
    Checklist {
        task: String,
        action: ChecklistAction,
        /// Checklist name (add) or item id (check/uncheck)
        target: String,
        #[arg(long = "item")]
        items: Vec<String>,
    },
    #[command(subcommand)]
    /// Apply changes or comments to every task matching a filter
    Bulk(BulkCmd),
    /// Run mixed operations from a JSON file
    Batch {
        #[arg(long)]
        file: String,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Falls back to default_list, then a picker
    #[arg(long)]
    list: Option<String>,
    #[arg(long)]
    name: Option<String>,
    /// Markdown
    #[arg(long, conflicts_with = "description_file")]
    description: Option<String>,
    /// Markdown file, or - for stdin
    #[arg(long)]
    description_file: Option<String>,
    #[arg(long)]
    status: Option<String>,
    /// urgent | high | normal | low
    #[arg(long)]
    priority: Option<String>,
    #[arg(long = "assignee")]
    assignees: Vec<String>,
    #[arg(long = "tag")]
    tags: Vec<String>,
    #[arg(long)]
    due: Option<String>,
    #[arg(long)]
    start: Option<String>,
    /// e.g. 2h, 1h30m
    #[arg(long)]
    estimate: Option<String>,
    #[arg(long)]
    points: Option<f64>,
    #[arg(long)]
    parent: Option<String>,
    #[arg(long = "type")]
    task_type: Option<String>,
    /// 'Name=value'; repeatable
    #[arg(long = "field")]
    fields: Vec<String>,
    /// Fail if required custom fields are missing
    #[arg(long)]
    check_required: bool,
    /// Open the new task in the browser
    #[arg(long)]
    open: bool,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    task: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(long, conflicts_with = "description_file")]
    description: Option<String>,
    #[arg(long)]
    description_file: Option<String>,
    #[arg(long)]
    status: Option<String>,
    /// urgent | high | normal | low | none
    #[arg(long)]
    priority: Option<String>,
    #[arg(long = "assign")]
    assign: Vec<String>,
    #[arg(long = "unassign")]
    unassign: Vec<String>,
    #[arg(long = "watch")]
    watch: Vec<String>,
    #[arg(long = "unwatch")]
    unwatch: Vec<String>,
    /// Date or 'none'
    #[arg(long)]
    due: Option<String>,
    #[arg(long)]
    start: Option<String>,
    /// Duration or 'none'
    #[arg(long)]
    estimate: Option<String>,
    #[arg(long)]
    points: Option<f64>,
    #[arg(long)]
    parent: Option<String>,
    #[arg(long = "type")]
    task_type: Option<String>,
    #[arg(long, conflicts_with = "unarchive")]
    archive: bool,
    #[arg(long)]
    unarchive: bool,
    /// 'Name=value'; repeatable
    #[arg(long = "field")]
    fields: Vec<String>,
    #[arg(long = "clear-field")]
    clear_fields: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum BulkCmd {
    /// Apply changes to every matching task
    Update {
        #[command(flatten)]
        filter: FilterArgs,
        #[arg(long)]
        set_status: Option<String>,
        #[arg(long)]
        assign: Vec<String>,
        #[arg(long)]
        add_tag: Vec<String>,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long)]
        due: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Comment on every matching task
    Comment {
        #[command(flatten)]
        filter: FilterArgs,
        #[arg(long)]
        text: String,
        #[arg(long)]
        dry_run: bool,
    },
}

fn parse_fields(raw: &[String]) -> Result<BTreeMap<String, String>> {
    raw.iter()
        .map(|f| {
            f.split_once('=')
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                .with_context(|| format!("--field '{f}' must look like 'Name=value'"))
        })
        .collect()
}

fn task_ref(s: &str) -> Result<TaskRef> {
    Ok(s.parse::<TaskRef>()?)
}

fn tasks_table(tasks: &[Task], tz: Option<&str>) {
    print_table(
        &[
            "ID",
            "Name",
            "Status",
            "Assignees",
            "Due",
            "Priority",
            "List",
        ],
        tasks.iter().map(|t| {
            vec![
                t.display_id().to_string(),
                t.name.clone(),
                t.status.status.clone(),
                t.assignees
                    .iter()
                    .map(|u| u.display())
                    .collect::<Vec<_>>()
                    .join(", "),
                opt(t.due_date.map(|d| format_ms(d, tz))),
                opt(t.priority_name()),
                opt(t.list.as_ref().and_then(|l| l.name.clone())),
            ]
        }),
    );
}

fn progress(n: usize) -> ProgressBar {
    let pb = ProgressBar::new(n as u64);
    pb.set_style(
        ProgressStyle::with_template("{bar:30} {pos}/{len} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar()),
    );
    pb
}

pub async fn run(ctx: &Ctx, cmd: TaskCmd) -> Result<()> {
    let s = ctx.session().await?;
    let r = s.resolver()?;
    let tz = s.tz().map(str::to_string);
    let tz = tz.as_deref();
    match cmd {
        TaskCmd::List { filter } => {
            let tasks = ops::search(&r, &filter.input(&s)).await?;
            if ctx.json {
                return print_json(&tasks);
            }
            tasks_table(&tasks, tz);
            eprintln!("{} task(s)", tasks.len());
        }
        TaskCmd::View {
            task,
            comments,
            subtasks,
            web,
        } => {
            let t = task_ref(&task)?;
            let full = s.client.get_task(&t, subtasks, true).await?;
            if web {
                return open_url(&full);
            }
            let cmts = if comments {
                s.client.task_comments(&t, Some(25)).await?
            } else {
                vec![]
            };
            if ctx.json {
                return print_json(&json!({ "task": full, "comments": cmts }));
            }
            print_task(&full, tz);
            if !cmts.is_empty() {
                println!("\nComments");
                for c in cmts.iter().rev() {
                    println!(
                        "  [{}] {}: {}",
                        opt(c.date.map(|d| format_ms(d, tz))),
                        opt(c.user.as_ref().map(|u| u.display())),
                        c.comment_text.trim()
                    );
                }
            }
        }
        TaskCmd::Create(a) => {
            let list = match a.list.or_else(|| s.cfg.default_list.clone()) {
                Some(l) => l,
                None => {
                    require_tty("list", "--list")?;
                    let lists = r.hierarchy(false).await?.lists();
                    let paths: Vec<String> = lists.iter().map(|l| l.path()).collect();
                    let pick = inquire::Select::new("List:", paths.clone()).prompt()?;
                    lists[paths.iter().position(|p| *p == pick).unwrap_or(0)]
                        .id
                        .clone()
                }
            };
            let name = match a.name {
                Some(n) => n,
                None => {
                    require_tty("name", "--name")?;
                    inquire::Text::new("Task name:").prompt()?
                }
            };
            let description = match (&a.description, &a.description_file) {
                (Some(d), _) => Some(d.clone()),
                (None, Some(f)) => Some(file_or_stdin(f)?),
                _ => None,
            };
            let input = TaskInput {
                name: Some(name),
                description,
                status: a.status,
                priority: a.priority,
                assignees: a.assignees,
                tags: a.tags,
                due: a.due,
                start: a.start,
                estimate: a.estimate,
                points: a.points,
                parent: a.parent,
                task_type: a.task_type,
                fields: parse_fields(&a.fields)?,
                check_required: a.check_required,
            };
            let t = ops::create_task(&r, &list, &input).await?;
            if a.open {
                open_url(&t)?;
            }
            if ctx.json {
                return print_json(&t);
            }
            println!(
                "Created {} '{}' {}",
                t.display_id(),
                t.name,
                opt(t.url.as_ref())
            );
        }
        TaskCmd::Update(a) => {
            let description = match (&a.description, &a.description_file) {
                (Some(d), _) => Some(d.clone()),
                (None, Some(f)) => Some(file_or_stdin(f)?),
                _ => None,
            };
            let input = UpdateInput {
                task: TaskInput {
                    name: a.name,
                    description,
                    status: a.status,
                    priority: a.priority,
                    due: a.due,
                    start: a.start,
                    estimate: a.estimate,
                    points: a.points,
                    parent: a.parent,
                    task_type: a.task_type,
                    fields: parse_fields(&a.fields)?,
                    ..Default::default()
                },
                assign: a.assign,
                unassign: a.unassign,
                watch: a.watch,
                unwatch: a.unwatch,
                archived: if a.archive {
                    Some(true)
                } else if a.unarchive {
                    Some(false)
                } else {
                    None
                },
                clear_fields: a.clear_fields,
            };
            let out = ops::update_task(&r, &task_ref(&a.task)?, &input).await?;
            if ctx.json {
                return print_json(&out);
            }
            println!("Updated {} '{}'", out.task.display_id(), out.task.name);
            for f in &out.fields {
                match &f.error {
                    None => println!("  field {}: ok", f.field),
                    Some(e) => println!("  field {}: FAILED — {e}", f.field),
                }
            }
            if out.fields.iter().any(|f| !f.ok) {
                bail!("some custom fields failed to update");
            }
        }
        TaskCmd::Status { task, status } => {
            let t = ops::set_status(&r, &task_ref(&task)?, &status).await?;
            if ctx.json {
                return print_json(&t);
            }
            println!("{} → {}", t.display_id(), t.status.status);
        }
        TaskCmd::Assign { task, members } => assign(ctx, &r, &task, members, true).await?,
        TaskCmd::Unassign { task, members } => assign(ctx, &r, &task, members, false).await?,
        TaskCmd::Move {
            task,
            to,
            maps,
            move_fields,
        } => {
            let t = task_ref(&task)?;
            let mut map: BTreeMap<String, String> = maps
                .iter()
                .map(|m| {
                    m.split_once('=')
                        .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
                        .with_context(|| format!("--map '{m}' must look like 'old=new'"))
                })
                .collect::<Result<_>>()?;
            let res = match ops::move_task(&r, &t, &to, &map, move_fields).await {
                Err(ClickUpError::Resolve {
                    kind: "target status",
                    input,
                    suggestions,
                }) if is_tty() && !suggestions.is_empty() => {
                    let pick = inquire::Select::new(
                        &format!("'{input}' doesn't exist in the target list. New status:"),
                        suggestions,
                    )
                    .prompt()?;
                    map.insert(input, pick);
                    ops::move_task(&r, &t, &to, &map, move_fields).await?
                }
                other => other?,
            };
            if ctx.json {
                return print_json(&res);
            }
            println!(
                "Moved {task} to {}.",
                res["list"]["name"].as_str().unwrap_or(&to)
            );
        }
        TaskCmd::AddTo { task, list } => {
            let l = r.list(&list).await?;
            s.client.add_task_to_list(&task_ref(&task)?, &l.id).await?;
            println!("Added {task} to {}.", l.path());
        }
        TaskCmd::RemoveFrom { task, list } => {
            let l = r.list(&list).await?;
            if !confirm(ctx.yes, &format!("Remove {task} from {}?", l.path()))? {
                return Ok(());
            }
            s.client
                .remove_task_from_list(&task_ref(&task)?, &l.id)
                .await?;
            println!("Removed {task} from {}.", l.path());
        }
        TaskCmd::Delete { tasks } => {
            let refs = tasks
                .iter()
                .map(|t| task_ref(t))
                .collect::<Result<Vec<_>>>()?;
            if !confirm(
                ctx.yes,
                &format!(
                    "Permanently delete {} task(s): {}?",
                    refs.len(),
                    tasks.join(", ")
                ),
            )? {
                return Ok(());
            }
            for t in &refs {
                s.client.delete_task(t).await?;
                println!("Deleted {t}.");
            }
        }
        TaskCmd::Open { task } => {
            let t = s.client.get_task(&task_ref(&task)?, false, false).await?;
            open_url(&t)?;
        }
        TaskCmd::Tag { task, action, tags } => {
            let t = task_ref(&task)?;
            for tag in &tags {
                match action {
                    AddRemove::Add => s.client.add_tag(&t, tag).await?,
                    AddRemove::Remove => s.client.remove_tag(&t, tag).await?,
                }
            }
            println!(
                "{} tag(s) {}.",
                tags.len(),
                match action {
                    AddRemove::Add => "added",
                    AddRemove::Remove => "removed",
                }
            );
        }
        TaskCmd::Link {
            task,
            action,
            other,
        } => {
            let (a, b) = (task_ref(&task)?, task_ref(&other)?);
            match action {
                AddRemove::Add => s.client.add_link(&a, &b).await?,
                AddRemove::Remove => {
                    if !confirm(ctx.yes, &format!("Unlink {task} and {other}?"))? {
                        return Ok(());
                    }
                    s.client.remove_link(&a, &b).await?
                }
            }
            println!("Done.");
        }
        TaskCmd::Dep {
            task,
            action,
            blocks,
            waiting_on,
        } => {
            let (other, kind) = match (blocks, waiting_on) {
                (Some(o), None) => (o, Dependency::Blocks),
                (None, Some(o)) => (o, Dependency::WaitingOn),
                _ => bail!("pass --blocks <task> or --waiting-on <task>"),
            };
            let (a, b) = (task_ref(&task)?, task_ref(&other)?);
            match action {
                AddRemove::Add => s.client.add_dependency(&a, &b, kind).await?,
                AddRemove::Remove => {
                    if !confirm(ctx.yes, &format!("Remove dependency {task} ↔ {other}?"))? {
                        return Ok(());
                    }
                    s.client.remove_dependency(&a, &b, kind).await?
                }
            }
            println!("Done.");
        }
        TaskCmd::Checklist {
            task,
            action,
            target,
            items,
        } => {
            let t = task_ref(&task)?;
            match action {
                ChecklistAction::Add => {
                    let c = s.client.create_checklist(&t, &target, &items).await?;
                    if ctx.json {
                        return print_json(&c);
                    }
                    println!(
                        "Created checklist '{}' with {} item(s).",
                        c.name,
                        c.items.len()
                    );
                }
                ChecklistAction::Check | ChecklistAction::Uncheck => {
                    let full = s.client.get_task(&t, false, false).await?;
                    let checklist = full
                        .checklists
                        .iter()
                        .find(|c| c.items.iter().any(|i| i.id == target))
                        .with_context(|| format!("no checklist item '{target}' on {task}"))?;
                    let resolved = matches!(action, ChecklistAction::Check);
                    s.client
                        .update_checklist_item(&checklist.id, &target, Some(resolved), None)
                        .await?;
                    println!(
                        "Item {target} {}.",
                        if resolved { "checked" } else { "unchecked" }
                    );
                }
            }
        }
        TaskCmd::Bulk(b) => bulk(ctx, &s, b).await?,
        TaskCmd::Batch { file, dry_run } => {
            let text = file_or_stdin(&file)?;
            let ops_list: Vec<BatchOp> =
                serde_json::from_str(&text).with_context(|| format!("parsing {file}"))?;
            for (i, op) in ops_list.iter().enumerate() {
                println!("{:>3}. {}", i + 1, op.describe());
            }
            if dry_run {
                println!("Dry run: nothing changed.");
                return Ok(());
            }
            if (ops_list.len() > 1 || ops_list.iter().any(BatchOp::is_destructive))
                && !confirm(ctx.yes, &format!("Run {} operation(s)?", ops_list.len()))?
            {
                return Ok(());
            }
            let mut failed = 0;
            let mut results = Vec::new();
            for op in &ops_list {
                let res = ops::run_batch_op(&r, op).await;
                if res.is_err() {
                    failed += 1;
                }
                results.push(match res {
                    Ok(v) => json!({ "op": op.describe(), "ok": true, "result": v }),
                    Err(e) => json!({ "op": op.describe(), "ok": false, "error": e.to_string() }),
                });
            }
            if ctx.json {
                print_json(&results)?;
            } else {
                for r in &results {
                    let status = if r["ok"] == true { "ok" } else { "FAILED" };
                    println!(
                        "{status:>6}  {}  {}",
                        r["op"].as_str().unwrap_or(""),
                        r["error"].as_str().unwrap_or("")
                    );
                }
            }
            if failed > 0 {
                bail!("{failed} of {} operation(s) failed", ops_list.len());
            }
        }
    }
    Ok(())
}

async fn assign(
    ctx: &Ctx,
    r: &clickupc_core::Resolver<'_>,
    task: &str,
    members: Vec<String>,
    add: bool,
) -> Result<()> {
    let input = if add {
        UpdateInput {
            assign: members,
            ..Default::default()
        }
    } else {
        UpdateInput {
            unassign: members,
            ..Default::default()
        }
    };
    let out = ops::update_task(r, &task_ref(task)?, &input).await?;
    if ctx.json {
        return print_json(&out.task);
    }
    println!(
        "{} assignees: {}",
        out.task.display_id(),
        out.task
            .assignees
            .iter()
            .map(|u| u.display())
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}

async fn bulk(ctx: &Ctx, s: &Session, cmd: BulkCmd) -> Result<()> {
    let r = s.resolver()?;
    let (filter, dry_run) = match &cmd {
        BulkCmd::Update {
            filter, dry_run, ..
        }
        | BulkCmd::Comment {
            filter, dry_run, ..
        } => (filter, *dry_run),
    };
    let tasks = ops::search(&r, &filter.input(s)).await?;
    println!("{} task(s) matched.", tasks.len());
    if tasks.is_empty() {
        return Ok(());
    }
    if dry_run {
        tasks_table(&tasks, s.tz());
        println!("Dry run: nothing changed.");
        return Ok(());
    }
    if tasks.len() > 1 && !confirm(ctx.yes, &format!("Apply to {} tasks?", tasks.len()))? {
        return Ok(());
    }
    let refs: Vec<TaskRef> = tasks.iter().map(|t| TaskRef::Id(t.id.clone())).collect();
    let pb = progress(refs.len());
    let results: Vec<(String, std::result::Result<(), ClickUpError>)> = match cmd {
        BulkCmd::Update {
            set_status,
            assign,
            add_tag,
            priority,
            due,
            ..
        } => {
            let changes = BulkChanges {
                status: set_status,
                priority,
                assignees_add: assign,
                tags_add: add_tag,
                due,
            };
            if changes.is_empty() {
                bail!("nothing to change; pass --set-status, --assign, --add-tag, --priority or --due");
            }
            ops::bulk_update(&r, refs, &changes, || pb.inc(1)).await?
        }
        BulkCmd::Comment { text, .. } => {
            let c = &s.client;
            let text = &text;
            clickupc_core::bulk::run(
                c.limiter(),
                refs,
                |t| async move {
                    let res = c.add_comment(&t, text, None, false).await.map(|_| ());
                    (t.to_string(), res)
                },
                || pb.inc(1),
            )
            .await
        }
    };
    pb.finish_and_clear();
    let failed: Vec<_> = results.iter().filter(|(_, r)| r.is_err()).collect();
    if ctx.json {
        print_json(
            &results
                .iter()
                .map(|(t, r)| json!({ "task": t, "ok": r.is_ok(), "error": r.as_ref().err().map(|e| e.to_string()) }))
                .collect::<Vec<_>>(),
        )?;
    } else {
        println!(
            "{} ok, {} failed.",
            results.len() - failed.len(),
            failed.len()
        );
        for (t, e) in &failed {
            if let Err(e) = e {
                println!("  {t}: {e}");
            }
        }
    }
    if !failed.is_empty() {
        bail!("{} task(s) failed", failed.len());
    }
    Ok(())
}

fn open_url(t: &Task) -> Result<()> {
    let url = t
        .url
        .clone()
        .unwrap_or_else(|| format!("https://app.clickup.com/t/{}", t.id));
    open::that(&url).with_context(|| format!("opening {url}"))?;
    Ok(())
}

fn print_task(t: &Task, tz: Option<&str>) {
    println!("{}  {}", t.display_id(), t.name);
    if let Some(u) = &t.url {
        println!("{u}");
    }
    println!();
    let row = |k: &str, v: String| {
        if !v.is_empty() {
            println!("{k:<12} {v}");
        }
    };
    row("Status", t.status.status.clone());
    row("Priority", opt(t.priority_name()));
    row(
        "Assignees",
        t.assignees
            .iter()
            .map(|u| u.display())
            .collect::<Vec<_>>()
            .join(", "),
    );
    row(
        "Watchers",
        t.watchers
            .iter()
            .map(|u| u.display())
            .collect::<Vec<_>>()
            .join(", "),
    );
    row("Due", opt(t.due_date.map(|d| format_ms(d, tz))));
    row("Start", opt(t.start_date.map(|d| format_ms(d, tz))));
    row("Estimate", opt(t.time_estimate.map(format_duration)));
    row(
        "Tracked",
        opt(t.time_spent.filter(|v| *v > 0).map(format_duration)),
    );
    row("Points", opt(t.points));
    row(
        "Tags",
        t.tags
            .iter()
            .map(|x| x.name.clone())
            .collect::<Vec<_>>()
            .join(", "),
    );
    let path: Vec<String> = [&t.space, &t.folder, &t.list]
        .iter()
        .filter_map(|r| r.as_ref().and_then(|r| r.name.clone()))
        .collect();
    row("List", path.join(" / "));
    row("Created", opt(t.date_created.map(|d| format_ms(d, tz))));
    row("Updated", opt(t.date_updated.map(|d| format_ms(d, tz))));
    for f in &t.custom_fields {
        if let Some(v) = f.display_value(tz) {
            row(&f.name, v);
        }
    }
    for c in &t.checklists {
        println!("\nChecklist: {}", c.name);
        for i in &c.items {
            println!(
                "  [{}] {}  ({})",
                if i.resolved { "x" } else { " " },
                i.name,
                i.id
            );
        }
    }
    if !t.dependencies.is_empty() || !t.linked_tasks.is_empty() {
        println!(
            "\nRelations: {} dependencies, {} links",
            t.dependencies.len(),
            t.linked_tasks.len()
        );
    }
    if let Some(subs) = &t.subtasks {
        println!("\nSubtasks");
        for s in subs {
            println!("  {}  {}  [{}]", s.display_id(), s.name, s.status.status);
        }
    }
    let desc = t
        .markdown_description
        .as_deref()
        .or(t.text_content.as_deref())
        .unwrap_or("")
        .trim();
    if !desc.is_empty() {
        println!("\n{desc}");
    }
}
