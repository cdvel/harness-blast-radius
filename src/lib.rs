//! harness-blast-radius: audit what a coding harness such as Claude Code may do
//! without asking.
//!
//! The only adapter so far is Claude Code (`claude`). It reads config files,
//! resolves them the way Claude Code does, and keeps the source of every value.

pub mod baseline;
pub mod claude;
pub mod finding;
pub mod locate;
pub mod merge;
pub mod report;
pub mod style;
