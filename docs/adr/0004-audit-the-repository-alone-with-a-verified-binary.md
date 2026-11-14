---
id: ADR-0004
title: Audit the repository alone in CI with an attested binary
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
related:
  - {id: ADR-0003, reason: the action passes the baseline explicitly}
terms: []
---

# ADR-0004: Audit the repository alone in CI with an attested binary

**Status:** Draft · **Date:** 2026-11-14

## Context

The GitHub Action runs the tool on pull requests (`action.yml`). A runner has its own home directory and may have a managed policy directory; neither belongs to the repository under review, and findings about them would be noise or leak runner details into code scanning. The action also has to get a binary onto the runner, and that binary runs with the job's token in a pipeline that audits untrusted contributions.

## Options considered

### Empty home and managed directory, with a downloaded binary verified by attestation (chosen)

The audit step creates an empty home and managed directory under `$RUNNER_TEMP`, unsets `CLAUDE_CONFIG_DIR`, and runs with `HOME` pointed at the empty home (`action.yml:90-99`). The install step downloads the release archive and runs `gh attestation verify` requiring this repository's release workflow and the tag `v<version>` (`action.yml:70-74`). Its supporters would say findings then describe only what the repository adds, and only a binary this repository built from that tag is executed.

### Audit with the runner's real environment

Run in the default environment. Its supporters would say it is the simplest and reflects what a developer would see locally. It loses because the runner's files change what is reported and would differ between runners.

### Build from source in the action

Check out and compile with `cargo`. Its supporters would say there is no download to trust. It loses because it needs a Rust toolchain on every consumer runner and adds minutes to each audit.

### Download the archive and check `SHA256SUMS` only

Its supporters would say a checksum is simple. It loses because the checksum file and the archive come from the same release, so it proves integrity but not that this repository's workflow built it; attestation does.

## Decision

We will run the audit in CI with an empty home and managed directory, and install the binary only from a release archive that passes `gh attestation verify` for this repository's release workflow and the requested tag.

## Consequences

### Positive

- Findings are about the repository, so results match between runners and local checkouts of the same files.
- An archive that fails verification is not run.
- The release workflow ties tag, `Cargo.toml` and the action's default version together (`.github/workflows/release.yml:38-43`).

### Negative

- Needs the `gh` CLI and an attested release for every version the action installs; there are no binaries for Windows (`action.yml:59-65`).
- A repository's trust state in `~/.claude.json` is not seen in CI, so values that wait for workspace trust are reported as waiting.
- The default `version` in `action.yml` must be updated with each release.

## References

- `action.yml`, `.github/workflows/release.yml`, `.github/workflows/ci.yml`
- `docs/github-action.md`
