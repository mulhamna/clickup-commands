#![doc = include_str!("../README.md")]

pub mod bulk;
pub mod client;
pub mod config;
mod de;
pub mod error;
pub mod filter;
pub mod model;
pub mod ops;
pub mod rate_limit;
pub mod resolve;
pub mod task_ref;
pub mod time;

pub use client::{ClickUpClient, Query};
pub use config::ClickUpConfig;
pub use error::{ClickUpError, Result};
pub use resolve::Resolver;
pub use task_ref::TaskRef;
