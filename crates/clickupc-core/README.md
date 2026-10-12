# clickupc-core

[![crates.io](https://img.shields.io/crates/v/clickupc-core.svg)](https://crates.io/crates/clickupc-core)
[![docs.rs](https://img.shields.io/docsrs/clickupc-core)](https://docs.rs/clickupc-core)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/mulhamna/clickup-commands/blob/main/LICENSE)

Async Rust client for the ClickUp Public API (v2 + v3) — the library behind the
[`clickupc`](https://crates.io/crates/clickupc) CLI and MCP server.

- **Typed client** — tasks, comments, custom fields, tags, links, dependencies, checklists, attachments, time entries, hierarchy, members and Docs.
- **Rate limits handled** — tracks `X-RateLimit-*`, waits for `X-RateLimit-Reset` on 429 and retries, with a shared limiter for bounded concurrency.
- **Task references** — parse ids, custom ids (`DEV-123`) and task URLs; custom-id query parameters are added for you.
- **Resolver** — names → ids for lists (`"Space/Folder/List"`, aliases), spaces, folders, statuses, members (`me`, username, email), custom fields and options, with a disk cache and "did you mean" suggestions.
- **Human input** — `parse_when("fri")`, `parse_duration("1h30m")`, custom-field values from option names, dates and numbers.
- **High-level operations** — search, create, update (with per-field results), move with status mapping, bulk, batch, standup and list summaries.
- **Tolerant models** — ids and timestamps that arrive as strings or numbers, `null` arrays, unknown fields.

```toml
[dependencies]
clickupc-core = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Search with human input

```rust,no_run
use clickupc_core::{
    ops::{self, SearchInput},
    ClickUpClient, ClickUpConfig, Resolver,
};

#[tokio::main]
async fn main() -> clickupc_core::Result<()> {
    // Same profile file and CLICKUP_* env vars as the clickupc CLI.
    let cfg = ClickUpConfig::load()?;
    let client = ClickUpClient::new(&cfg)?;
    let resolver = Resolver::new(&client, &cfg)?;

    let tasks = ops::search(
        &resolver,
        &SearchInput {
            lists: vec!["Engineering/Sprints/Sprint 12".into()],
            assignees: vec!["me".into()],
            due_before: Some("eow".into()),
            ..Default::default()
        },
    )
    .await?;

    for t in &tasks {
        println!("{} {} [{}]", t.display_id(), t.name, t.status.status);
    }
    Ok(())
}
```

## Low-level client

```rust,no_run
use clickupc_core::{ClickUpClient, ClickUpConfig, TaskRef};

async fn mark_in_review() -> clickupc_core::Result<()> {
    let mut cfg = ClickUpConfig::with_token("pk_...", "https://api.clickup.com/api");
    cfg.workspace_id = Some("9012345678".into());
    let client = ClickUpClient::new(&cfg)?;

    let task: TaskRef = "DEV-123".parse()?; // custom id → custom_task_ids=true&team_id=…
    let t = client.get_task(&task, false, true).await?;
    client.set_status(&task, "in review").await?;
    println!("{}", t.markdown_description.unwrap_or_default());
    Ok(())
}
```

Errors are a single [`ClickUpError`](https://docs.rs/clickupc-core/latest/clickupc_core/error/enum.ClickUpError.html) enum (`Auth`, `NotFound`, `Api { status, code, .. }`, `RateLimit { reset_at }`, `Resolve { suggestions, .. }`, …); personal tokens are redacted from every message.

Part of [clickup-commands](https://github.com/mulhamna/clickup-commands). MIT licensed. Not affiliated with or endorsed by ClickUp.
