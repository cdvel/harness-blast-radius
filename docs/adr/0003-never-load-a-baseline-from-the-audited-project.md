---
id: ADR-0003
title: Never load a baseline from the audited project
status: Draft
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
  - {term: Baseline, definition: A file of reviewed findings passed with --baseline so only new findings fail; it is never read from the audited project.}
---

# ADR-0003: Never load a baseline from the audited project

**Status:** Draft · **Date:** 2026-11-14

## Context

CI should fail only on new findings, so teams need a way to accept the ones they have reviewed. The tool's main use is also deciding whether to trust a repository someone else wrote. If the audited repository could supply the file that hides its findings, the audit would show what the repository chose to show.

## Options considered

### Read a baseline only when the caller passes its path (chosen)

`--baseline FILE` is the only way a baseline is loaded; `audit` never looks in the project (`src/baseline.rs:1-5`, `src/main.rs:124`). Its supporters would say the person or workflow running the audit chooses what to accept, so a repository cannot suppress its own hooks.

### Auto-discover a baseline file in the project

Look for a conventional file such as `.harness-blast-baseline.json` in the audited directory. Its supporters would say it needs no flag and behaves like other linters' ignore files. It loses because a cloned repository could ship a baseline accepting its own hook or `env` redirect, and the audit run before the trust decision would report nothing.

### Suppress findings inside the settings files

Let settings carry a marker for accepted entries. Its supporters would say acceptance sits next to the value. It loses for the same reason as discovery, since the repository controls the file, and because Claude Code reads settings as strict JSON with no comments (`src/claude/load.rs:95`).

## Decision

We will read a baseline only from the path given with `--baseline`, never from the audited project, and in CI keep the file in the repository and pass it from the workflow so changes show up in review.

## Consequences

### Positive

- A repository cannot hide its own findings from someone deciding whether to trust it.
- Baseline changes in CI are reviewable diffs (`docs/github-action.md`).
- Entries match on rule, path, key and detail with the trust note removed, so a baseline written in a trusted checkout also matches in CI (`src/baseline.rs:22-36`).

### Negative

- Users must pass the flag every time; there is no default location.
- A workflow that passes a baseline from the same pull request still honors what that pull request changed. Review of the baseline file is the control.

## References

- `src/baseline.rs`, `src/main.rs`
- `docs/rules.md` (Baseline), `docs/github-action.md`
