---
id: ADR-0005
title: Keep harness-specific logic in an adapter module
status: Draft
status_note: Not ready to accept; the seam is untested until a second harness is added
date: 2026-11-14
revised: []
authors: [Cesar D. Velandia]
deciders: []
supersedes: []
supersedes_in_part: []
superseded_by: []
superseded_in_part_by: []
related:
  - {id: ADR-0001, reason: the resolve step is what an adapter implements}
terms:
  - {term: Adapter, definition: The module that reads and resolves one harness's configuration; claude is the only one so far.}
---

# ADR-0005: Keep harness-specific logic in an adapter module

**Status:** Draft (Not ready to accept; the seam is untested until a second harness is added) · **Date:** 2026-11-14

## Context

The tool is named for harnesses in general, and Codex and Cursor are planned (`README.md`, `HARNESSES.md`). Only Claude Code is implemented. The code has to decide now where harness-specific logic ends, so a second harness does not mean rewriting the first.

## Options considered

### Shared core at the crate root, one adapter module per harness (chosen)

Types every harness would use (`Finding`, `Rule`, `Persona` in `src/finding.rs`; the provenance merge in `src/merge.rs`; reporting in `src/report.rs`, `src/baseline.rs`, `src/style.rs`) stay at the root. Everything that models Claude Code lives under `src/claude/` (`src/lib.rs:4`, `src/claude/mod.rs`). Its supporters would say the split costs nothing now and shows where a second adapter plugs in.

### Put everything in one flat module

Its supporters would say there is one harness, so any abstraction is a guess. It loses because every later harness would force a rewrite and a harder untangling.

### One crate per harness in a workspace

Its supporters would say it enforces the boundary and lets consumers install only what they use. It loses for now because it adds build and release overhead for a tool with a single adapter and a single binary.

## Decision

We will keep code that models a specific harness under its own module in `src/`, starting with `src/claude/`, and keep types and helpers that do not depend on a harness at the crate root.

## Consequences

### Positive

- A new harness adds a module beside `claude`, reusing findings, merging, baselines and report formats.
- Output formats and exit codes are shared, so CI integrations need not change.

### Negative

- With one adapter, the shared types are a guess: some may carry Claude Code assumptions, such as the trust note in `src/finding.rs:6`.
- There is no adapter interface. `src/main.rs` calls `claude::` directly (`src/main.rs:78-100`), so adding a harness also means adding dispatch.

### Known gaps

- Codex and Cursor adapters do not exist; `HARNESSES.md` holds research notes only.

## References

- `src/lib.rs`, `src/claude/mod.rs`, `src/main.rs`
- `HARNESSES.md`, `PLAN.md`
