//! Claude Code adapter.
//!
//! Semantics follow the Claude Code docs as of 2026-10 (settings, permissions,
//! hooks, MCP pages at code.claude.com/docs). Behavior that changed between
//! versions is noted where it is modeled.

pub mod audit;
pub mod decide;
pub mod load;
pub mod mcp;
pub mod paths;
pub mod permission;
pub mod resolve;
pub mod rules;
pub mod shell;
pub mod show;
pub mod why;

pub use paths::Paths;
pub use resolve::{Resolved, resolve};
