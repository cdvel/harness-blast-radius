//! harness-blast-radius: audit what an AI coding harness configuration allows.
//!
//! The only adapter so far is Claude Code (`claude`). It reads config files,
//! resolves them the way Claude Code does, and keeps the source of every value.

pub mod claude;
pub mod merge;
