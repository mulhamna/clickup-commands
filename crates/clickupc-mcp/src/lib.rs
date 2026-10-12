//! MCP server for ClickUp, linked into the `clickupc` binary (`clickupc mcp serve`).

pub mod app;
pub mod error;
pub mod models;
mod server;

pub use server::{run_stdio, run_streamable_http, ClickUpMcpServer};
