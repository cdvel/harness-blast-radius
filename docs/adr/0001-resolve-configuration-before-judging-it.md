---
id: ADR-0001
title: Resolve configuration before judging it
status: Accepted
status_note: ""
date: 2026-11-14
revised: []
authors: [Cesar D. Velandia]
deciders: []
supersedes: []
supersedes_in_part: []
superseded_by: []
superseded_in_part_by: []
related: []
terms:
  - {term: Resolved configuration, definition: The settings, MCP servers and trust state merged across all sources with provenance, which every command reads.}
---

# ADR-0001: Resolve configuration before judging it

**Status:** Accepted · **Date:** 2026-11-14

## Context

Claude Code takes its configuration from managed policy, user, project and local settings, `~/.claude.json` and `.mcp.json`. Lists merge across scopes, deny beats ask beats allow, and repository-controlled values apply only after the workspace trust dialog (`docs/claude-code.md`). No built-in command prints the merged result or runs in CI. A single file therefore does not say what Claude Code will do: a deny rule in one file can be undone by an allow rule in another, and a repository's allow rule means nothing until the folder is trusted.

The tool has to report what Claude Code may do without asking, and name the file each value came from.

## Options considered

### Resolve first, then run every command over the result (chosen)

Load all files, merge them in precedence order keeping the source of every value, apply trust, drop values Claude Code ignores, and hand one `Resolved` value to `show`, `why` and `audit` (`src/claude/resolve.rs:81`). Its supporters would say findings are only correct if they describe the effective state, and that sharing one resolution step keeps the three commands consistent.

### Judge each file on its own

Scan every settings file for risky entries. Its supporters would say it is simple, needs no model of Claude Code's precedence, and cannot be wrong about merging. It loses because it cannot tell a shadowed value from an effective one, rates user settings as if they were repository settings, and cannot say that an allow rule waits for trust.

### Ask Claude Code for its effective configuration

Run `claude` and read what it reports. Its supporters would say the answer is authoritative. It loses because it means starting the harness, which the project rules out (the tool "only reads files; nothing is executed", `README.md`), and no such command works unattended in CI.

## Decision

We will merge every Claude Code configuration source into one resolved configuration, with provenance and workspace trust applied, and run `show`, `why` and `audit` over that value instead of over raw files.

## Consequences

### Positive

- Every finding names the file and key it came from, through `merge::Leaf` (`src/merge.rs:30`).
- Trust-dependent values are reported with `(applies after workspace trust)` instead of being treated as active or ignored (`src/finding.rs:6`).
- The three commands cannot disagree about what the configuration is.

### Negative

- The tool carries its own model of Claude Code's behavior, taken from documentation, which can drift or be wrong. Three assumptions are marked `Unverified` in code (`src/claude/resolve.rs:216`, `src/claude/paths.rs:26`, `src/claude/decide.rs:61`), and some behavior depends on Claude Code versions (`src/claude/resolve.rs:186`, `src/claude/paths.rs:46`).
- Command-line flags and server-delivered policy are invisible to a file-based audit (`docs/claude-code.md`).

## References

- `src/claude/resolve.rs`, `src/claude/load.rs`, `src/merge.rs`
- `docs/claude-code.md`
- `docs/ARCHITECTURE.md`, Data flows
