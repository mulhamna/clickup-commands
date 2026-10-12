//! MCP server for ClickUp, linked into the `clickupc` binary (`clickupc mcp serve`).

mod server;

pub use server::{run_stdio, run_streamable_http, ClickUpMcpServer};
