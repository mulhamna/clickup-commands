//! standup, summary, doc, api.

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use clickupc_core::{client::DocParent, model::Task, ops, time::format_ms};
use serde_json::Value;

use super::{file_or_stdin, Ctx};
use crate::output::{confirm, opt, print_json, print_table};

#[derive(Debug, Subcommand)]
pub enum DocCmd {
    /// Docs whose name contains the query (client-side filter)
    Search {
        query: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Doc and its pages, or one page's markdown
    View {
        doc_id: String,
        #[arg(long)]
        page: Option<String>,
    },
    /// Create a doc in the workspace or under a space, folder or list
    Create {
        #[arg(long)]
        name: String,
        /// space | folder | list | workspace
        #[arg(long)]
        parent_type: Option<String>,
        /// Parent name or id
        #[arg(long)]
        parent: Option<String>,
    },
    /// Create or edit doc pages
    #[command(subcommand)]
    Page(PageCmd),
}

#[derive(Debug, Subcommand)]
pub enum PageCmd {
    /// Add a markdown page to a doc
    Create {
        doc_id: String,
        #[arg(long)]
        name: String,
        /// Markdown file, or - for stdin
        #[arg(long)]
        content_file: Option<String>,
        #[arg(long)]
        parent_page: Option<String>,
    },
    /// Replace, append to or prepend to a page's content
    Edit {
        doc_id: String,
        page_id: String,
        /// Markdown file, or - for stdin
        #[arg(long)]
        content_file: Option<String>,
        #[arg(long)]
        name: Option<String>,
        /// replace | append | prepend
        #[arg(long, default_value = "replace")]
        mode: String,
    },
}

fn section(title: &str, tasks: &[Task], tz: Option<&str>) {
    if tasks.is_empty() {
        return;
    }
    println!("\n## {title}");
    for t in tasks {
        let due = t.due_date.map(|d| format!(" (due {})", format_ms(d, tz)));
        println!(
            "- {} {} [{}]{}",
            t.display_id(),
            t.name,
            t.status.status,
            opt(due)
        );
    }
}

pub async fn standup(ctx: &Ctx, since: String, user: Option<String>) -> Result<()> {
    let s = ctx.session()?;
    let r = s.resolver()?;
    let out = ops::standup(&r, &since, user.as_deref()).await?;
    if ctx.json {
        return print_json(&out);
    }
    let tz = s.tz();
    println!(
        "# Standup — {} (since {})",
        out.user,
        format_ms(out.since, tz)
    );
    section("Done", &out.closed, tz);
    section("Updated", &out.updated, tz);
    section("In progress", &out.in_progress, tz);
    section("Due soon", &out.due_soon, tz);
    section("Overdue", &out.overdue, tz);
    Ok(())
}

pub async fn summary(ctx: &Ctx, list: String) -> Result<()> {
    let s = ctx.session()?;
    let r = s.resolver()?;
    let out = ops::list_summary(&r, &list).await?;
    if ctx.json {
        return print_json(&out);
    }
    println!(
        "{} — {} task(s), {} overdue",
        out.list, out.total, out.overdue
    );
    print_table(
        &["Status", "Count"],
        out.by_status
            .iter()
            .map(|(k, v)| vec![k.clone(), v.to_string()]),
    );
    print_table(
        &["Assignee", "Count"],
        out.by_assignee
            .iter()
            .map(|(k, v)| vec![k.clone(), v.to_string()]),
    );
    Ok(())
}

pub async fn doc(ctx: &Ctx, cmd: DocCmd) -> Result<()> {
    let s = ctx.session()?;
    let r = s.resolver()?;
    let out: Value = match cmd {
        DocCmd::Search { query, limit } => {
            let docs = s.client.search_docs(query.as_deref(), limit).await?;
            if !ctx.json {
                print_table(
                    &["ID", "Name"],
                    docs.iter().map(|d| {
                        vec![
                            opt(d.get("id").and_then(Value::as_str)),
                            opt(d.get("name").and_then(Value::as_str)),
                        ]
                    }),
                );
                return Ok(());
            }
            Value::Array(docs)
        }
        DocCmd::View { doc_id, page } => match page {
            Some(p) => {
                let page = s.client.doc_page(&doc_id, &p).await?;
                if !ctx.json {
                    println!(
                        "{}",
                        page.get("content").and_then(Value::as_str).unwrap_or("")
                    );
                    return Ok(());
                }
                page
            }
            None => {
                let doc = s.client.get_doc(&doc_id).await?;
                let pages = s.client.doc_page_listing(&doc_id).await?;
                if !ctx.json {
                    println!("{}", opt(doc.get("name").and_then(Value::as_str)));
                    for p in pages.as_array().into_iter().flatten() {
                        println!(
                            "  {}  {}",
                            opt(p.get("id").and_then(Value::as_str)),
                            opt(p.get("name").and_then(Value::as_str))
                        );
                    }
                    return Ok(());
                }
                serde_json::json!({ "doc": doc, "pages": pages })
            }
        },
        DocCmd::Create {
            name,
            parent_type,
            parent,
        } => {
            let parent = match (parent_type.as_deref(), parent.as_deref()) {
                (None, None) => None,
                (Some("workspace"), _) => {
                    Some((s.client.workspace_id()?.to_string(), DocParent::Workspace))
                }
                (Some("space"), Some(p)) => Some((r.space(p).await?.id, DocParent::Space)),
                (Some("folder"), Some(p)) => Some((r.folder(p).await?.id, DocParent::Folder)),
                (Some("list"), Some(p)) => Some((r.list(p).await?.id, DocParent::List)),
                _ => bail!("use --parent-type space|folder|list|workspace with --parent <name>"),
            };
            s.client
                .create_doc(&name, parent.as_ref().map(|(id, k)| (id.as_str(), *k)))
                .await?
        }
        DocCmd::Page(PageCmd::Create {
            doc_id,
            name,
            content_file,
            parent_page,
        }) => {
            let content = content_file.as_deref().map(file_or_stdin).transpose()?;
            s.client
                .create_doc_page(&doc_id, &name, content.as_deref(), parent_page.as_deref())
                .await?
        }
        DocCmd::Page(PageCmd::Edit {
            doc_id,
            page_id,
            content_file,
            name,
            mode,
        }) => {
            if !matches!(mode.as_str(), "replace" | "append" | "prepend") {
                bail!("--mode must be replace, append or prepend");
            }
            let content = content_file.as_deref().map(file_or_stdin).transpose()?;
            s.client
                .edit_doc_page(
                    &doc_id,
                    &page_id,
                    content.as_deref(),
                    name.as_deref(),
                    &mode,
                )
                .await?
        }
    };
    if ctx.json || !out.is_null() {
        print_json(&out)?;
    }
    Ok(())
}

pub async fn api(ctx: &Ctx, method: String, path: String, data: Option<String>) -> Result<()> {
    let method = method.to_ascii_uppercase();
    let body = match data {
        Some(d) => {
            let text = match d.strip_prefix('@') {
                Some(file) => file_or_stdin(file)?,
                None => d,
            };
            Some(serde_json::from_str::<Value>(&text).context("--data must be JSON")?)
        }
        None => None,
    };
    if method != "GET" && !confirm(ctx.yes, &format!("Send {method} {path}?"))? {
        return Ok(());
    }
    let s = ctx.session()?;
    let v = s.client.raw(&method, &path, body).await?;
    print_json(&v)
}
