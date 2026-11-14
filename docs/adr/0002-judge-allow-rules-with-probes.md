---
id: ADR-0002
title: Judge allow rules with probe calls and the shared matcher
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
related:
  - {id: ADR-0001, reason: probes run against the resolved configuration's rules}
terms:
  - {term: Probe, definition: A synthetic tool call used to test whether an allow rule is broad, instead of reading its text.}
---

# ADR-0002: Judge allow rules with probe calls and the shared matcher

**Status:** Accepted · **Date:** 2026-11-14

## Context

`audit` has to say when an allow rule is too broad: it approves every shell command, a whole tool, or an interpreter that runs arbitrary code. Rule text varies widely: `Bash`, `Bash(*)`, `Bash(python3 *)`, `Bash(python3:*)`, `Bash(/usr/bin/env *)`, `Edit(//**)`. The same matcher already decides `why` answers (`src/claude/permission.rs`, `src/claude/decide.rs`), including compound commands, wrappers and path anchors (`src/claude/shell.rs`).

## Options considered

### Run each rule against synthetic probe calls (chosen)

Build calls no real config names (`harness-blast-radius-probe`) and see whether the rule matches them: a rule that matches both the probe and `rm -rf <probe>` approves every command, one that matches a synthetic Edit, Write, WebFetch or MCP call approves a whole tool, and one that matches `<interpreter> <probe>` approves an interpreter (`src/claude/rules.rs:917-945`). Its supporters would say it reuses the exact semantics Claude Code is modeled with, so `Bash(python3 manage.py test)` is not flagged while `Bash(/usr/bin/env *)` is (`docs/rules.md`).

### Pattern-match the rule text

Flag rules whose text contains `*`, a known interpreter name or similar. Its supporters would say it is trivial to write, fast, and easy to explain. It loses because the same text means different things under different anchors and wrappers, producing false positives for narrow rules and misses for equivalent spellings.

### Enumerate dangerous commands and test each

Keep a list of dangerous commands and check which the rule allows. Its supporters would say findings could name the exact command. It loses because the list has to be complete to be right, which is the problem probes avoid by testing for breadth instead of listing harm.

## Decision

We will classify an allow rule by matching it with the same matcher `why` uses, against probe calls that only a broad rule would accept, and keep only the interpreter list as hand-maintained data.

## Consequences

### Positive

- Findings follow Claude Code's matching, not rule spelling, and `audit` and `why` agree.
- Rule spellings such as the legacy `prefix:*` form are covered without separate cases (`src/claude/permission.rs:55`).

### Negative

- The interpreter list (`src/claude/rules.rs:177`, 25 names) is maintained by hand; a missing name is a missed finding.
- A rule that is broad only for inputs the probes do not exercise is not detected.
- Probing relies on the matcher's cost. Inferred: its recursion on `*` and `**` (`src/claude/permission.rs:224-247`) can be exponential for crafted patterns, and probes run every allow rule from an untrusted repository through it.

## References

- `src/claude/rules.rs`, `src/claude/permission.rs`, `src/claude/shell.rs`
- `docs/rules.md`
- ADR-0001
