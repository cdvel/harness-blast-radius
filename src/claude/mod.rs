//! Claude Code adapter.
//!
//! Semantics follow the Claude Code docs as of 2026-10 (settings, permissions,
//! hooks, MCP pages at code.claude.com/docs). Behavior that changed between
//! versions is noted where it is modeled.

pub mod load;
pub mod paths;

pub use paths::Paths;
