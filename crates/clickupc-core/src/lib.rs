//! Unofficial ClickUp API client library powering `clickupc`.
//!
//! Everything that talks to ClickUp lives here: config, HTTP client with
//! rate-limit handling, typed models, task references, filters, and the
//! name → id resolver. No printing, no CLI parsing.

pub mod bulk;
pub mod client;
pub mod config;
mod de;
pub mod error;
pub mod filter;
pub mod model;
pub mod rate_limit;
pub mod resolve;
pub mod task_ref;
pub mod time;

pub use client::{ClickUpClient, Query};
pub use config::ClickUpConfig;
pub use error::{ClickUpError, Result};
pub use resolve::Resolver;
pub use task_ref::TaskRef;
