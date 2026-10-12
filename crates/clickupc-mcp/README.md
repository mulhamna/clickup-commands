# clickupc-mcp

[![crates.io](https://img.shields.io/crates/v/clickupc-mcp.svg)](https://crates.io/crates/clickupc-mcp)
[![docs.rs](https://img.shields.io/docsrs/clickupc-mcp)](https://docs.rs/clickupc-mcp)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/mulhamna/clickup-commands/blob/main/LICENSE)

The ClickUp [Model Context Protocol](https://modelcontextprotocol.io) server behind `clickupc mcp serve`, as an embeddable library built on [`rmcp`](https://crates.io/crates/rmcp).

> **Just want to use it?** Install the [`clickupc`](https://crates.io/crates/clickupc) binary and run `clickupc mcp install` — this crate is for embedding the server in your own Rust program.

- **55 `clickup_*` tools** — tasks, bulk & batch, comments, custom fields, tags / links / dependencies, checklists, attachments, time tracking, reports, Docs, and a raw API escape hatch.
- **Model-friendly inputs** — names or ids for lists, statuses, members, fields and options; dates like `tomorrow`, `+3d`, `eow`.
- **Compact outputs** — task summaries by default (`detail: "full"` on request), responses bounded to stay under stdio line limits.
- **Safe** — delete / remove / clear tools return `unsafe_operation` unless `confirm: true`.
- **Stable errors** — `data.code` is one of `validation_error`, `unsafe_operation`, `auth_missing`, `config_error`, `resolve_error` (+ `suggestions`), `clickup_api_error` (+ `status`, `ecode`, `trace_id`), `rate_limited` (+ `reset_at`).
- **Two transports** — stdio and Streamable HTTP.

```toml
[dependencies]
clickupc-mcp = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
anyhow = "1"
```

```rust,no_run
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Reads the clickupc profile / CLICKUP_* env vars on every tool call.
    clickupc_mcp::run_stdio().await
    // or: clickupc_mcp::run_streamable_http("127.0.0.1", 8787, "/mcp").await
}
```

`ClickUpMcpServer` implements `rmcp::ServerHandler` if you need to serve it over your own transport.

Full tool reference: [docs/MCP.md](https://github.com/mulhamna/clickup-commands/blob/main/docs/MCP.md). Part of [clickup-commands](https://github.com/mulhamna/clickup-commands). MIT licensed. Not affiliated with or endorsed by ClickUp.
