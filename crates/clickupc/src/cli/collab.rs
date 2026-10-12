//! comment, attach, field, time.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use clickupc_core::{
    client::{NewTimeEntry, TimeQuery},
    model::TimeEntry,
    ops::{self, ReportBy},
    time::{format_duration, format_ms, now_ms, parse_duration, parse_when},
    TaskRef,
};
use serde_json::json;

use super::{text_or_stdin, Ctx, Session};
use crate::output::{confirm, opt, print_json, print_table};

#[derive(Debug, Subcommand)]
pub enum CommentCmd {
    /// Newest 25 comments (--all for everything)
    List {
        task: String,
        #[arg(long)]
        all: bool,
    },
    /// Add a comment; text "-" reads stdin
    Add {
        task: String,
        text: String,
        #[arg(long)]
        assign: Option<String>,
        #[arg(long)]
        notify_all: bool,
    },
    /// Edit a comment's text or mark it resolved
    Update {
        comment_id: String,
        text: String,
        #[arg(long)]
        resolved: Option<bool>,
    },
    /// Delete a comment (asks first unless --yes)
    Delete { comment_id: String },
    /// Reply in a comment thread
    Reply { comment_id: String, text: String },
    /// Show replies to a comment
    Replies { comment_id: String },
}

#[derive(Debug, Subcommand)]
pub enum FieldCmd {
    /// Custom fields of a list
    List {
        #[arg(long)]
        list: String,
    },
    /// Set a field from human input (option name, date, number, member…)
    Set {
        task: String,
        field: String,
        value: String,
    },
    /// Remove a custom field value from a task
    Clear { task: String, field: String },
}

#[derive(Debug, Subcommand)]
pub enum TimeCmd {
    /// Time entries (default: last 30 days, you)
    List {
        #[arg(long)]
        task: Option<String>,
        #[arg(long)]
        list: Option<String>,
        #[arg(long)]
        space: Option<String>,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
        #[arg(long = "user")]
        users: Vec<String>,
        #[arg(long)]
        billable: bool,
    },
    /// Log time, e.g. `time add DEV-1 1h30m`
    Add {
        task: String,
        duration: String,
        /// Start time; default now minus duration
        #[arg(long)]
        at: Option<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        billable: bool,
    },
    /// Start a timer, optionally on a task
    Start {
        task: Option<String>,
        #[arg(long)]
        note: Option<String>,
    },
    /// Stop the running timer
    Stop,
    /// Show the running timer
    Current,
    /// Delete a time entry (asks first unless --yes)
    Delete { entry_id: String },
    /// Totals grouped by task, list or day
    Report {
        #[arg(long, default_value = "mon")]
        from: String,
        #[arg(long, default_value = "now")]
        to: String,
        #[arg(long, default_value = "task")]
        by: String,
    },
}

fn task_ref(s: &str) -> Result<TaskRef> {
    Ok(s.parse::<TaskRef>()?)
}

pub async fn comment(ctx: &Ctx, cmd: CommentCmd) -> Result<()> {
    let s = ctx.session()?;
    let tz = s.tz();
    match cmd {
        CommentCmd::List { task, all } => {
            let comments = s
                .client
                .task_comments(&task_ref(&task)?, (!all).then_some(25))
                .await?;
            if ctx.json {
                return print_json(&comments);
            }
            print_table(
                &["ID", "Date", "User", "Comment"],
                comments.iter().map(|c| {
                    vec![
                        c.id.clone(),
                        opt(c.date.map(|d| format_ms(d, tz))),
                        opt(c.user.as_ref().map(|u| u.display())),
                        c.comment_text.trim().to_string(),
                    ]
                }),
            );
        }
        CommentCmd::Add {
            task,
            text,
            assign,
            notify_all,
        } => {
            let assignee = match assign {
                Some(m) => Some(s.resolver()?.member_id(&m).await?),
                None => None,
            };
            let text = text_or_stdin(&text)?;
            let v = s
                .client
                .add_comment(&task_ref(&task)?, &text, assignee, notify_all)
                .await?;
            if ctx.json {
                return print_json(&v);
            }
            println!("Comment {} added.", opt(v.get("id").map(|x| x.to_string())));
        }
        CommentCmd::Update {
            comment_id,
            text,
            resolved,
        } => {
            s.client
                .update_comment(&comment_id, &text_or_stdin(&text)?, resolved)
                .await?;
            println!("Comment {comment_id} updated.");
        }
        CommentCmd::Delete { comment_id } => {
            if !confirm(ctx.yes, &format!("Delete comment {comment_id}?"))? {
                return Ok(());
            }
            s.client.delete_comment(&comment_id).await?;
            println!("Comment {comment_id} deleted.");
        }
        CommentCmd::Reply { comment_id, text } => {
            let v = s
                .client
                .reply_comment(&comment_id, &text_or_stdin(&text)?, false)
                .await?;
            if ctx.json {
                return print_json(&v);
            }
            println!("Reply added.");
        }
        CommentCmd::Replies { comment_id } => {
            let replies = s.client.comment_replies(&comment_id).await?;
            if ctx.json {
                return print_json(&replies);
            }
            for c in &replies {
                println!(
                    "[{}] {}: {}",
                    opt(c.date.map(|d| format_ms(d, tz))),
                    opt(c.user.as_ref().map(|u| u.display())),
                    c.comment_text.trim()
                );
            }
        }
    }
    Ok(())
}

pub async fn attach(ctx: &Ctx, task: String, files: Vec<PathBuf>) -> Result<()> {
    let s = ctx.session()?;
    let t = task_ref(&task)?;
    for f in &files {
        if !f.is_file() {
            bail!("no such file: {}", f.display());
        }
        let v = s.client.upload_attachment(&t, f).await?;
        if ctx.json {
            print_json(&v)?;
        } else {
            println!(
                "Attached {} {}",
                f.display(),
                opt(v.get("url").and_then(|u| u.as_str()))
            );
        }
    }
    Ok(())
}

async fn task_list(s: &Session, t: &TaskRef) -> Result<String> {
    s.client
        .get_task(t, false, false)
        .await?
        .list
        .map(|l| l.id)
        .context("task has no home list")
}

pub async fn field(ctx: &Ctx, cmd: FieldCmd) -> Result<()> {
    let s = ctx.session()?;
    let r = s.resolver()?;
    match cmd {
        FieldCmd::List { list } => {
            let l = r.list(&list).await?;
            let fields = s.client.list_fields(&l.id).await?;
            if ctx.json {
                return print_json(&fields);
            }
            print_table(
                &["Name", "Type", "ID", "Options"],
                fields.iter().map(|f| {
                    vec![
                        f.name.clone(),
                        f.kind.clone(),
                        f.id.clone(),
                        f.options()
                            .iter()
                            .map(|o| o.name.clone())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ]
                }),
            );
        }
        FieldCmd::Set { task, field, value } => {
            let t = task_ref(&task)?;
            let list = task_list(&s, &t).await?;
            let f = r.field(&list, &field).await?;
            let v = r.field_value(&f, &value).await?;
            s.client.set_field(&t, &f.id, &v).await?;
            println!("{task}: {} set.", f.name);
        }
        FieldCmd::Clear { task, field } => {
            let t = task_ref(&task)?;
            let list = task_list(&s, &t).await?;
            let f = r.field(&list, &field).await?;
            if !confirm(ctx.yes, &format!("Clear '{}' on {task}?", f.name))? {
                return Ok(());
            }
            s.client.clear_field(&t, &f.id).await?;
            println!("{task}: {} cleared.", f.name);
        }
    }
    Ok(())
}

fn entry_row(e: &TimeEntry, tz: Option<&str>) -> Vec<String> {
    vec![
        e.id.clone(),
        opt(e.start.map(|d| format_ms(d, tz))),
        format!(
            "{}{}",
            format_duration(e.effective_ms(now_ms())),
            if e.is_running() { " (running)" } else { "" }
        ),
        opt(e.task_id()),
        opt(e.task_name()),
        opt(e.description.clone()),
    ]
}

const ENTRY_HEADERS: [&str; 6] = ["ID", "Start", "Duration", "Task", "Name", "Note"];

pub async fn time(ctx: &Ctx, cmd: TimeCmd) -> Result<()> {
    let s = ctx.session()?;
    let r = s.resolver()?;
    let tz = s.tz().map(str::to_string);
    let tz = tz.as_deref();
    match cmd {
        TimeCmd::List {
            task,
            list,
            space,
            from,
            to,
            users,
            billable,
        } => {
            let mut q = TimeQuery {
                start: from.as_deref().map(|d| parse_when(d, tz)).transpose()?,
                end: to.as_deref().map(|d| parse_when(d, tz)).transpose()?,
                task: task.as_deref().map(task_ref).transpose()?,
                billable: billable.then_some(true),
                ..Default::default()
            };
            if let Some(l) = list {
                q.list_id = Some(r.list(&l).await?.id);
            }
            if let Some(sp) = space {
                q.space_id = Some(r.space(&sp).await?.id);
            }
            for u in &users {
                q.assignees.push(r.member(u).await?.id);
            }
            let entries = s.client.time_entries(&q).await?;
            if ctx.json {
                return print_json(&entries);
            }
            print_table(&ENTRY_HEADERS, entries.iter().map(|e| entry_row(e, tz)));
            let total: i64 = entries.iter().map(|e| e.effective_ms(now_ms())).sum();
            println!("Total: {}", format_duration(total));
        }
        TimeCmd::Add {
            task,
            duration,
            at,
            note,
            billable,
        } => {
            let ms = parse_duration(&duration)?;
            let start = match at {
                Some(a) => parse_when(&a, tz)?,
                None => now_ms() - ms,
            };
            let e = s
                .client
                .add_time_entry(&NewTimeEntry {
                    task: Some(task_ref(&task)?),
                    start,
                    duration: ms,
                    description: note,
                    billable: billable.then_some(true),
                })
                .await?;
            if ctx.json {
                return print_json(&e);
            }
            println!("Logged {} on {task}.", format_duration(ms));
        }
        TimeCmd::Start { task, note } => {
            let t = task.as_deref().map(task_ref).transpose()?;
            let e = s.client.start_timer(t.as_ref(), note.as_deref()).await?;
            if ctx.json {
                return print_json(&e);
            }
            println!("Timer started{}.", opt(task.map(|t| format!(" on {t}"))));
        }
        TimeCmd::Stop => {
            let e = s.client.stop_timer().await?;
            if ctx.json {
                return print_json(&e);
            }
            println!(
                "Timer stopped: {}.",
                format_duration(e.effective_ms(now_ms()))
            );
        }
        TimeCmd::Current => {
            let e = s.client.current_timer().await?;
            if ctx.json {
                return print_json(&e);
            }
            match e {
                Some(e) => print_table(&ENTRY_HEADERS, [entry_row(&e, tz)]),
                None => println!("No timer running."),
            }
        }
        TimeCmd::Delete { entry_id } => {
            if !confirm(ctx.yes, &format!("Delete time entry {entry_id}?"))? {
                return Ok(());
            }
            s.client.delete_time_entry(&entry_id).await?;
            println!("Deleted time entry {entry_id}.");
        }
        TimeCmd::Report { from, to, by } => {
            let by: ReportBy = by.parse()?;
            let q = TimeQuery {
                start: Some(parse_when(&from, tz)?),
                end: Some(parse_when(&to, tz)?),
                ..Default::default()
            };
            let entries = s.client.time_entries(&q).await?;
            let rows = ops::time_report(&entries, by, tz);
            let total: i64 = rows.iter().map(|(_, ms)| ms).sum();
            if ctx.json {
                return print_json(&json!({
                    "total_ms": total,
                    "groups": rows.iter().map(|(k, ms)| json!({ "group": k, "ms": ms, "duration": format_duration(*ms) })).collect::<Vec<_>>(),
                }));
            }
            print_table(
                &["Group", "Time"],
                rows.iter()
                    .map(|(k, ms)| vec![k.clone(), format_duration(*ms)]),
            );
            println!("Total: {}", format_duration(total));
        }
    }
    Ok(())
}
