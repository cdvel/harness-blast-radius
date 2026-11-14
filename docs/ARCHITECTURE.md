---
title: harness-blast-radius architecture
generator: architecture-md 0.1.0
generated: 2026-11-14
level: comprehensive
repository: https://github.com/cdvel/harness-blast-radius.git
commit: 456e9f5e8046b4e240b8ea4fb87957d32d6c5b96
commit_short: 456e9f5
release_version: 0.2.0
---

# harness-blast-radius architecture

As of commit 456e9f5 (2026-11-14), level comprehensive.

## Contents

- [Summary](#summary)
- [Diagrams](#diagrams)
- [Components](#components)
- [Infrastructure and deployment](#infrastructure-and-deployment)
- [External integrations](#external-integrations)
- [CI/CD](#cicd)
- [Data flows](#data-flows)
- [Security](#security)
- [Configuration](#configuration)
- [Observability](#observability)
- [Technology stack](#technology-stack)
- [Open work](#open-work)
- [Quick reference](#quick-reference)

Sections left out: Cloud services (no cloud provider is used), Data stores (the tool keeps no store; it reads JSON files and writes only a baseline file on request), Performance (the repo states no targets, caches or load tests), Cross-repo dependencies (a single crate with no sibling repositories), Related documents (no glossary or ADR register exists).

## Summary

`harness-blast-radius` is a static analyzer for Claude Code configuration. It reads every file Claude Code reads (managed policy, user settings, the project's `.claude/` and `.mcp.json`, `~/.claude.json`), resolves them the way Claude Code does, and reports what Claude Code may run, read, edit or fetch without asking, with the file each value came from (`README.md:9-13`, `src/lib.rs:1-4`). It only reads files; nothing is executed (`README.md:13`).

Users: a developer deciding whether to trust a cloned repository, and a CI pipeline that audits a repository's `.claude/` files on each pull request.

Core capabilities:

| Command | Purpose | Entry |
|---|---|---|
| `show` | Effective configuration and the source of every value | `src/claude/show.rs:26` |
| `why` | Allow, ask or deny for one tool call, and the deciding rule | `src/claude/why.rs:13` |
| `audit` | Risky settings, highest severity first; text, JSON or SARIF; baseline; exit codes | `src/main.rs:93` |

### Key decisions

No ADR register exists. The reasons below come from code comments and docs.

| Decision | Reason | Impact |
|---|---|---|
| Resolve configuration before judging it (merge scopes, apply workspace trust, drop ignored values) | A single settings file does not say what Claude Code will do (`README.md:65`) | Every rule runs over `Resolved`, not over raw files (`src/claude/rules.rs:15`) |
| Match permission rules with Claude Code's own semantics, not by comparing rule text | `Bash(python3 manage.py test)` is not an interpreter allow, `Bash(/usr/bin/env *)` is (`docs/rules.md:37`) | `allow_risk` probes each rule with synthetic calls (`src/claude/rules.rs:917-945`) |
| Keep provenance on every merged value | Findings must name the file they came from | `merge::Node` records `src` and `shadowed` per leaf (`src/merge.rs:16-37`) |
| Never load a baseline from the audited project | A repository could ship a baseline that hides its own hooks (`src/baseline.rs:3-5`) | `--baseline` is the only way a baseline is read (`src/main.rs:124`) |
| Strict JSON | Claude Code ignores a file with comments or trailing commas, deny rules included (`src/claude/load.rs:95-96`) | Such a file becomes an `ignored-file` finding |
| Audit the repository alone in CI | The runner's home and managed policy are not what the repository adds (`action.yml:90-91`) | The action runs with an empty `HOME` and managed directory (`action.yml:92-99`) |
| Verify the release binary before running it | Run only a binary built from the tag by this repository's release workflow (`action.yml:71`) | `gh attestation verify` gates the install (`action.yml:72-74`) |
| Adapter layout under `src/claude/` | Codex and Cursor are planned (`README.md:109`, `HARNESSES.md`) | Shared types live at the crate root (`src/finding.rs`, `src/merge.rs`); Claude Code logic stays in `src/claude/` |

### Stack at a glance

Rust 2024 edition, minimum Rust 1.85 (`Cargo.toml:4-5`); one binary `harness-blast` (`Cargo.toml:8-10`); dependencies `anyhow`, `clap`, `serde_json` (`Cargo.toml:12-15`); GitHub Actions for CI and release.

### Environments

| Name | How it is reached or deployed | Source |
|---|---|---|
| Developer machine | Release archive or `cargo install --locked --git ...` | `README.md:67-85` |
| CI (this repository) | `ci.yml` on push to `main` and on pull requests | `.github/workflows/ci.yml:3-6` |
| Release | Tag `v*` triggers builds for four targets and a GitHub release | `.github/workflows/release.yml:3-6`, `:23-32` |
| Consumer CI | `uses: cdvel/harness-blast-radius@v0.2.0` composite action | `action.yml:45-47`, `docs/github-action.md:7-22` |

## Diagrams

### Level 1: system context

```
+------------+  runs   +----------------------+  reads   +--------------------------------+
| Developer  |-------->|                      |--------->| Claude Code config files       |
+------------+         |  harness-blast       |          | (managed, user, project, ...)  |
                       |  (CLI, src/main.rs)  |          +--------------------------------+
+------------+  runs   |                      |  writes  +--------------------------------+
| CI job     |-------->|                      |--------->| stdout: text, JSON, SARIF      |
| (action)   |         +----------------------+          | baseline file (on request)     |
+------------+                                           +--------------------------------+
      |                                                       |
      | uploads SARIF                                         | read by
      v                                                       v
+------------------------+                              +----------------+
| GitHub code scanning   |                              | Claude Code    |
+------------------------+                              | (not contacted)|
                                                        +----------------+
```

The tool never talks to Claude Code; it models its behavior from documentation (`src/claude/mod.rs:1-4`).

### Level 2: containers

The repository builds one deployable unit, the binary. The action wraps it for CI.

```
+------------------------+  downloads   +--------------------------+
| action.yml (composite) |------------->| GitHub release archive   |
| shell steps            |  gh + attest | harness-blast-<target>   |
+-----------+------------+              +--------------------------+
            | execs binary, HOME=empty
            v
+--------------------------------------------------------+
| harness-blast binary                                   |
| src/main.rs -> library crate harness_blast_radius      |
+----------+------------------------------+--------------+
           | file reads (JSON)            | stdout / exit code
           v                              v
+------------------------+        +---------------------------+
| ~/.claude/, ~/.claude  |        | text, JSON, SARIF         |
| .json, .claude/, .mcp  |        | -> upload-sarif step      |
| .json, managed dir     |        +---------------------------+
+------------------------+
```

### Level 3: components inside the binary

```
src/main.rs  (clap: show | why | audit)
   |
   v
claude::Paths::detect -----> claude::load::load -----> claude::resolve::resolve
 (paths.rs)                   (load.rs)                 (resolve.rs, mcp.rs, merge.rs)
                                                              |  Resolved
        +-----------------------+-----------------------------+----------------+
        v                       v                                              v
 claude::show::render    claude::why::render                       claude::rules::check
 (show.rs)               (why.rs) -> decide.rs                     (rules.rs, uses decide.rs,
                                     -> permission.rs, shell.rs     permission.rs, shell.rs)
                                                                          |  Vec<Finding>
                                                                          v
                               Finding::visible(persona) -> baseline::apply (baseline.rs)
                                                                          |
                                         +--------------------------------+----------+
                                         v                                v          v
                                  claude::audit::render           report::json   report::sarif
                                  (text, style.rs)                (locate.rs for line numbers)
```

### Level 4: deployment

```
 tag v* pushed
      |
      v
+-----------------------------------------------+    +-------------------------------------+
| release.yml: build (matrix of 4 targets)      |--->| publish: SHA256SUMS, build          |
| ubuntu x64, ubuntu-arm, macos (x2 targets)    |    | provenance attestation, gh release  |
+-----------------------------------------------+    +------------------+------------------+
                                                                        v
                                                     +-------------------------------------+
                                                     | GitHub Releases (archives)          |
                                                     +------------------+------------------+
                                  gh release download + gh attestation verify
                                                                        v
                                        +----------------------------------------------+
                                        | consumer runner (Linux/macOS, x64/arm64)     |
                                        | $RUNNER_TEMP/harness-blast/<archive>/bin     |
                                        +----------------------------------------------+
```

No server, container, database or cloud resource exists. Runtime is a short-lived process on the user's machine or CI runner.

## Components

One Cargo package builds a library crate `harness_blast_radius` (`src/lib.rs`) and a binary `harness-blast` (`src/main.rs`). Concepts are named as the code names them; there is no glossary.

### CLI entry point

- **Path:** `src/main.rs` (148 lines). **Technology:** Rust, `clap` derive. **Purpose:** parse arguments, run the pipeline for the chosen command, set the exit code.
- **Structure:** `Cli` with a global `--color` (`src/main.rs:18-24`) and a `Command` enum with `Show`, `Why`, `Audit` (`src/main.rs:27-72`). `main` builds a `Style`, calls `Paths::detect`, then `claude::resolve` (`src/main.rs:74-92`).
- **Public surface:**

| Command | Arguments | Source |
|---|---|---|
| `show [PATH]` | `--managed-dir` | `src/main.rs:29-37` |
| `why CALL` | `--path` (default `.`), `--managed-dir` | `src/main.rs:38-48` |
| `audit [PATH]` | `--managed-dir`, `--persona regular\|pedantic\|auditor`, `--format text\|json\|sarif`, `--baseline FILE`, `--write-baseline FILE`, `--no-exit-codes` | `src/main.rs:49-71` |

- **Behavior of `audit`:** filters findings by persona, then either writes a baseline and exits 0 (`src/main.rs:107-117`) or applies a baseline, renders, and exits with the highest severity's code (`src/main.rs:118-145`). `--baseline` conflicts with `--write-baseline` (clap `conflicts_with`).
- **Exit codes:** 0 none, 11 low, 12 medium, 13 high (`src/finding.rs:25-30`); 1 on an error returned from `main` (anyhow). `docs/rules.md:41-48` lists them.
- **Depends on:** the library crate. **Tested by:** `tests/*.rs` call library functions directly; the binary itself is exercised only by the action job in CI (`.github/workflows/ci.yml:61-110`).

| File | Role |
|---|---|
| `src/main.rs` | Argument parsing, command dispatch, exit codes |
| `src/lib.rs` | Declares the seven public modules |

### Shared core

Types and helpers that are not specific to Claude Code.

| File | Role |
|---|---|
| `src/finding.rs` | `Severity`, `Confidence`, `Persona`, `Rule`, `Finding`, and `Finding::visible` (persona filter, `:91-100`); `AFTER_TRUST` note constant (`:6`) |
| `src/merge.rs` | `Node` tree (`Object`, `List`, `Scalar`) merging JSON documents lowest to highest precedence with provenance (`:16`, `:64`) |
| `src/baseline.rs` | `Baseline::load`, `apply`, and `write` for accepted findings (`:52`, `:80`, `:109`) |
| `src/report.rs` | `json` (`:47`) and `sarif` (`:97`) renderers; `Format`, `Counts` |
| `src/locate.rs` | Re-parses a JSON file with a small reader that records line numbers, so findings get a `line` (`:24`) |
| `src/style.rs` | ANSI styling; `ColorChoice` resolved from `--color` and `NO_COLOR` (`:24-34`) |

- **Merge semantics** (`src/merge.rs:1-6`): objects merge key by key; arrays are unioned with duplicates removed and every contributing source kept (`push_item`, `:135`); a scalar from a higher-precedence source replaces the lower one and records the sources it shadowed (`:64-85`).
- **Why a second JSON reader exists** (`src/locate.rs:3-4`):

  > `serde_json` drops positions, so this parses the file again with a small JSON reader that records the line each value starts on.

- **Baseline matching** (`src/baseline.rs:25-36`): an entry is `rule`, `path` as `./` or `~/`, `key`, and `detail` with the trust note removed, so a baseline written in a trusted checkout also matches in CI.
- **SARIF specifics:** paths under the repository root are relative with `uriBaseId: %SRCROOT%`; user and managed files get absolute `file://` URIs (`src/report.rs:130-136`); high, medium and low map to levels `error`, `warning`, `note` and scores 8.0, 5.0, 2.0 (`:79-94`).
- **Tested by:** unit tests in `locate.rs` (3), `merge.rs` (2), `style.rs` (1), `report.rs` (1); snapshots through `tests/audit.rs`.

### Claude Code adapter

`src/claude/` (twelve files, about 3,300 lines). It loads files, resolves them, and exposes three views (`show`, `why`, `audit`). `src/claude/mod.rs:1-5` says semantics follow the Claude Code docs as of 2026-10.

**Structure:** `Paths` (where to look) -> `load` (parse into `Loaded`) -> `resolve` (merge, drop, trust, MCP) -> `Resolved`, which every view reads.

| File | Role |
|---|---|
| `src/claude/mod.rs` | Module list; re-exports `Paths`, `Resolved`, `resolve` |
| `src/claude/paths.rs` | `Paths`: home, config dir, `.claude.json`, managed dir, project; `detect` honors `CLAUDE_CONFIG_DIR` (`:21-30`); `local_settings_root` is the git root (`:51-58`); `display` shortens to `~` and `.` (`:66`) |
| `src/claude/load.rs` | `Scope` (`User < Project < Local < Managed`, `:13`), `Kind`, `Source`, `Ignored`, `Loaded`; `load` reads eight file locations in precedence order (`:115-157`); invalid or non-object JSON goes to `ignored` (`:95-111`) |
| `src/claude/resolve.rs` | `Resolved`; `MANAGED_ONLY` (`:11`), `AFTER_TRUST` (`:22`), `COMMAND_SETTINGS` keys; `status` decides Active, AfterTrust or Ignored (`:116`); `drop_unsupported` removes values Claude Code ignores (`:162`); `trust_state` (`:217`) |
| `src/claude/mcp.rs` | `resolve_mcp` (`:64`): one winning server per name by scope Managed > Local > Project > User; statuses `Loads`, `Approved`, `Pending`, `Disabled`, `Blocked`, `Invalid`; `Approvals` for project servers |
| `src/claude/permission.rs` | `PermissionRule::parse` (`:20`), `ToolCall::parse` (`:80`), `matches_command` (`:44`), `matches_call` (`:135`); gitignore-style path matching (`:202`), domain matching (`:176`), MCP matching (`:164`) |
| `src/claude/shell.rs` | Splits commands at `&&`, `||`, `;`, `|`, `|&`, `&`, newline outside quotes; finds `$(...)` and backtick substitutions; strips wrappers (`timeout`, `nice`, ...); `wildcard` matcher (`:213`) |
| `src/claude/decide.rs` | `decide` (`:70`): deny, then ask, then allow; Bash allow needs every part covered; `Verdict` carries `after_trust` matches |
| `src/claude/rules.rs` | 29 `Rule` statics and `check` (`:15`); eight check functions; `allow_risk` probing (`:921`) |
| `src/claude/show.rs` | Text rendering of the resolved configuration, ten sections (`:26-42`) |
| `src/claude/why.rs` | Text rendering of a `Verdict`; `mode_outcome` explains what the permission mode does when no rule matches (`:87`) |
| `src/claude/audit.rs` | Text rendering of findings with a summary line (`:11`) |

**Path resolution rules** (`src/claude/permission.rs:199-201`): `//abs` is absolute, `~/p` is home-relative, `/p` is relative to the settings source, `p` and `./p` to the working directory; a pattern without a slash matches a name at any depth.

**Trust model** (`src/claude/resolve.rs:20-28`, `:132-137`): repository-controlled `permissions.allow`, `permissions.additionalDirectories`, `extraKnownMarketplaces`, `env` and `hooks` apply only after the workspace trust dialog; deny and ask rules apply immediately. `trust_state` walks the project's ancestors in `~/.claude.json` (`:217-237`).

**Reasons found in code comments:**

> `// Strict JSON, like Claude Code: comments or trailing commas make the whole file ignored.` (`src/claude/load.rs:95-96`)

> `// Since v2.1.257; earlier versions accepted bypassPermissions here.` (`src/claude/resolve.rs:186`)

> `/// Since v2.1.211 this is the git repository root. Outside a repo, or when the repo root is the home directory, it stays next to the project's .claude/settings.json.` (`src/claude/paths.rs:46-50`)

> `/// Unverified: assumes trusting a folder also trusts its subfolders.` (`src/claude/resolve.rs:216`)

> `// Unverified: assumes .claude.json moves into CLAUDE_CONFIG_DIR too.` (`src/claude/paths.rs:26`)

> `// Unverified: /path rules from user settings resolve against the project too, the same as project and local files.` (`src/claude/decide.rs:61-62`)

> `//! Not yet covered: plugin-provided servers, claude.ai connectors, the managedMcpServers setting and managed allow/deny lists.` (`src/claude/mcp.rs:8-9`)

**Detection rules.** `check` runs eight groups and sorts by severity descending, then rule id (`src/claude/rules.rs:15-31`):

| Group | Function | Rules |
|---|---|---|
| Ignored | `ignored` (`:78`) | `ignored-file`, `ignored-value` |
| Permissions | `permissions` (`:195`) | `bypass-default-mode`, `auto-default-mode`, `broad-shell-allow`, `broad-allow`, `interpreter-allow`, `unsupported-rule-syntax`, `legacy-rule-syntax`, `broad-additional-directory`, `secret-deny-without-sandbox` |
| MCP | `mcp` (`:351`) | `mcp-unpinned-package`, `mcp-plaintext-secret`, `mcp-insecure-http`, `mcp-invalid-entry`, `mcp-enable-all-project-servers`, `mcp-project-unapproved` |
| Command settings | `commands` (`:545`) | `repo-command-setting` |
| Environment | `env` (`:606`) | `repo-env-redirect`, `repo-env-tls`, `repo-command-setting` (for `CLAUDE_CODE_SHELL_PREFIX`, `CLAUDE_ENV_FILE`) |
| Hooks | `hooks` (`:731`) | `repo-hook`, `hook-unpinned-package`, `hook-pipe-to-shell` |
| Sandbox | `sandbox` (`:811`) | `sandbox-off`, `sandbox-fallback`, `sandbox-excluded-command`, `sandbox-unsandboxed-retry` |
| Local state | `local_files` (`:878`, Unix only; no-op elsewhere `:898`) | `state-file-readable`, `state-dir-readable` |

Severity, persona and the one-line report for each rule are in `docs/rules.md:5-35`. Each rule carries `severity`, `confidence`, `persona`, `summary`, `fix` (`src/finding.rs:54-61`).

**Probe-based allow analysis.** `allow_risk` does not parse rule text for dangerous patterns. It runs the rule against synthetic calls built from `PROBE = "harness-blast-radius-probe"` (`src/claude/rules.rs:917`): a rule matching both `PROBE` and `rm -rf PROBE` is `broad-shell-allow`; one matching a synthetic Edit, Write, WebFetch or MCP call is `broad-allow`; one matching `<interpreter> PROBE` or `<interpreter> -c PROBE` for any of 25 names in `INTERPRETERS` (`:177-182`) is `interpreter-allow`.

**Public surface used by tests:** `claude::resolve`, `claude::decide::decide`, `claude::permission::ToolCall`, `claude::rules::check`, `claude::audit::render`, `claude::show::render`, `claude::why::render`, `claude::Paths::for_home`.

**Depends on:** `serde_json` (parsing, `Value`), `anyhow` (errors in `Paths`), `std::fs`, `std::env`. No network access.

**Tested by:** unit tests in `rules.rs` (8), `permission.rs` (5), `shell.rs` (5); integration tests and snapshots in `tests/`.

### Tests

| File | Role |
|---|---|
| `tests/common/mod.rs` | `run(fixture, f)`: copies `tests/fixtures/<name>/` to a temp dir, replaces `$PROJECT` in `home/.claude.json`, sets home to mode 0700, builds `Paths::for_home`, and rewrites the temp path to `<tmp>` (`:24-51`) |
| `tests/audit.rs` | 10 tests: text, JSON and SARIF snapshots for fixtures `risky`, `merge`, `ignored`, `mcp`, `sandbox`, plus a Unix-only state-file permission test |
| `tests/decide.rs` | 4 tests: decision table over fixture `permissions`, untrusted-repo behavior, `why` output |
| `tests/show.rs` | 4 tests: `show` snapshots for `merge`, `mcp`, `managed`, `ignored` |
| `tests/snapshots/*.snap` | `insta` snapshots |
| `tests/fixtures/<name>/{home,project,managed}` | Config trees that stand in for a machine |

The suite is 43 `#[test]` functions by count (25 unit, 18 integration). Fixtures isolate the tests from the real home directory because `Paths::for_home` takes an explicit home (`src/claude/paths.rs:33-45`).

### GitHub Action

`action.yml` (126 lines), a composite action. See [CI/CD](#cicd) for the step-by-step description.

## Infrastructure and deployment

The repository ships no server, container image, manifest, chart or IaC. Distribution is by GitHub release archives and by `cargo install`.

**Release targets** (`.github/workflows/release.yml:23-34`):

| Target | Runner |
|---|---|
| `x86_64-unknown-linux-musl` | `ubuntu-latest` |
| `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` |
| `x86_64-apple-darwin` | `macos-latest` |
| `aarch64-apple-darwin` | `macos-latest` |

Each archive `harness-blast-<target>.tar.gz` holds the binary, `LICENSE` and `README.md` (`release.yml:54`). `action.yml:59-65` maps runner OS and architecture to the same target names; any other combination fails with `::error::`.

> **Gap:** No Windows build, and `src/claude/rules.rs:898` makes the local-state rules a no-op off Unix.
> **Question:** Is Windows support intended? `README.md` lists Linux and macOS only.

## External integrations

| Service | Purpose | Auth | Calls | Error handling |
|---|---|---|---|---|
| GitHub Releases | Source of the action's binary | `inputs.token`, default `github.token` (`action.yml:33-35`, `:53`) | `gh release download "v$VERSION" --repo ... --pattern ... --clobber` (`:70`) | `set -euo pipefail`; a failed download fails the step (`:58`) |
| GitHub artifact attestations | Prove the archive came from this repo's release workflow for the tag | same token | `gh attestation verify ... --signer-workflow ... --source-ref refs/tags/v$VERSION` (`:72-74`) | A failed verify stops before extraction (`:58`, `:75`) |
| GitHub code scanning | Show findings as alerts | Job needs `security-events: write` (`docs/github-action.md:13`) | `github/codeql-action/upload-sarif`, pinned by SHA, category `harness-blast` (`action.yml:112-117`) | Not defined in the repo |
| zizmor action | Lint workflows | none | `zizmorcore/zizmor-action` with `persona: pedantic` (`ci.yml:49-59`) | Job fails on findings |

The binary itself calls no external service. There are no timeouts or retries in this repo; the `gh` CLI and the upload action own them.

## CI/CD

### `ci.yml`

Triggers: push to `main` and every pull request (`.github/workflows/ci.yml:3-6`). Top-level permission `contents: read` (`:8-9`); concurrency cancels in-progress runs of the same ref (`:11-13`). All actions are pinned by commit SHA with a version comment (`:23`).

| Job | What it does | Lines |
|---|---|---|
| `lint` | `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings` | `:19-29` |
| `test` | `cargo test --locked` on ubuntu and macOS, toolchains `stable` and `1.85` | `:31-47` |
| `zizmor` | Workflow security lint, pedantic persona | `:49-59` |
| `action` | Builds a release binary, puts it on `PATH`, then runs the local action three times: against fixture `risky` (expects exit 13 and SARIF paths starting `tests/fixtures/risky/project/`), with a baseline written from the same fixture (expects 0), and with the released binary (expects 13) | `:61-110` |

> **To confirm:** The three local-action steps read `uses: $/` (`.github/workflows/ci.yml:76`, `:93`, `:103`) in the committed file. A local composite action is normally referenced as `./`. Whether `$/` resolves is not shown by the repository.

### `release.yml`

Triggers: tags `v*` and `workflow_dispatch` (`.github/workflows/release.yml:3-6`). The `build` job runs per target; on tags it first checks that the tag equals the `Cargo.toml` version and that `action.yml`'s default `version` equals it (`:38-43`). It builds with `cargo build --locked --release --target` (`:47`), packages, and uploads artifacts. The `publish` job runs only on tags (`:64-66`), with `contents: write`, `id-token: write`, `attestations: write` (`:69-72`); it writes `SHA256SUMS` (`:78`), attests build provenance (`:79`), and runs `gh release create ... --generate-notes` (`:82`).

Rollback is not defined: no workflow deletes or supersedes a release.

### Dependency updates

`.github/dependabot.yml:1-14`: monthly updates for `cargo` and `github-actions`, with a 7-day cooldown.

### Secrets

No repository secrets are referenced. Both workflows use `github.token` (`release.yml:84`, `action.yml:35`).

## Data flows

No HTTP service, queue or scheduled task exists. The entry points are the three CLI commands and the GitHub Action. Traces follow the code at 456e9f5.

### `audit` through the CLI

```
user -> main --> Paths::detect --> load --> resolve --> rules::check --> persona filter
                                                                              |
        exit code <-- render (text|json|sarif) <-- baseline::apply <----------+
```

| Hop | Where | What happens |
|---|---|---|
| 1 | `src/main.rs:93-104` | `Paths::detect(path, managed_dir)` reads `HOME` (error if unset), canonicalizes the project path, applies `CLAUDE_CONFIG_DIR` if set (`src/claude/paths.rs:21-30`) |
| 2 | `src/claude/load.rs:115-157` | Reads, in order: user `settings.json`; project `.claude/settings.json`; local `.claude/settings.local.json` under the git root; managed `managed-settings.json` and each `managed-settings.d/*.json` alphabetically; `~/.claude.json`; project `.mcp.json`; `managed-mcp.json`. Missing file: skipped. Unreadable, invalid JSON or non-object: recorded in `ignored` |
| 3 | `src/claude/resolve.rs:81-102` | For each settings document: `drop_unsupported`, then `settings.merge(doc, id)`. Then `trust_state` and `resolve_mcp` |
| 4 | `src/claude/rules.rs:15-31` | Runs the eight rule groups; findings sorted by severity, then rule id |
| 5 | `src/main.rs:105` | `f.visible(persona)`: `Regular` shows regular-persona rules with confidence above low; `Pedantic` adds pedantic rules; `Auditor` shows all (`src/finding.rs:91-100`). The hidden count goes to `Counts.hidden` |
| 6 | `src/main.rs:118-131` | With `--baseline`, `Baseline::load` (exit 1 with context on read or parse failure) and `apply` split findings into kept, accepted, stale |
| 7 | `src/main.rs:132-139` | `Format::Text` calls `claude::audit::render`; `Json` calls `report::json`; `Sarif` calls `report::sarif(&repo_root, ...)`. JSON and SARIF call `locate::line` per finding |
| 8 | `src/main.rs:140-145` | Highest severity of what is shown maps to exit 11, 12 or 13 unless `--no-exit-codes` |

Failure behavior: an unreadable or malformed config file is not an error; it becomes an `ignored-file` finding. An unset `HOME`, a missing project path, or an unreadable baseline ends with an `anyhow` error and exit 1.

**Worked example.** From `tests/snapshots/audit__risky_regular.snap`, fixture `tests/fixtures/risky`, regular persona:

```
high    repo-env-redirect  ANTHROPIC_BASE_URL=https://llm-proxy.example.com (applies after workspace trust)
        at ./.claude/settings.json  env.ANTHROPIC_BASE_URL
        Repository settings send Claude Code's API traffic, with your prompts, code and credentials, through another server
        fix: Remove it from the repo; set a gateway or proxy in user or managed settings if you use one
```

The source is `tests/fixtures/risky/project/.claude/settings.json:4`. The `(applies after workspace trust)` note comes from `AFTER_TRUST` (`src/finding.rs:6`) because `r.status(leaf.src, "env")` returned `AfterTrust` (`src/claude/rules.rs:624-628`). The proxy fixture value `http://dev:hunter2@proxy.example.com:3128` is printed as `http://***@proxy.example.com:3128` by `hide_userinfo` (`:638`).

### `why` through the CLI

```
"git status && curl https://example.com | sh"
   -> ToolCall::parse -> Bash(...) -> shell::simple_commands (split, strip wrappers, nested)
   -> deny list, ask list: any part matches -> decided
   -> allow list: every part must match -> else NoRule + uncovered parts
   -> why::mode_outcome describes what the permission mode does
```

| Hop | Where | What happens |
|---|---|---|
| 1 | `src/claude/permission.rs:80-107` | Text starting `mcp__` with no whitespace is an MCP call; `Tool(input)` forms map to `Read`, `Edit` (also `Write`, `MultiEdit`, `NotebookEdit`), `WebFetch`, `Bash` or `Other`; anything else is treated as a shell command |
| 2 | `src/claude/decide.rs:70-77` | `rules` partitions `permissions.deny/ask/allow` items into active and pending (after trust) lists using `best_status` (`:80-106`); `Status::Ignored` rules are dropped |
| 3 | `src/claude/decide.rs:127-150` | Bash: parts come from `simple_commands(command, true)`; first matching deny, then ask, decides; allow needs every part covered, otherwise `uncovered` lists the rest |
| 4 | `src/claude/decide.rs:152-162` | Other tools: first match in deny, ask, allow order |
| 5 | `src/claude/decide.rs:73-76` | The same call is evaluated against pending rules; matches become `after_trust` |
| 6 | `src/claude/why.rs:19-48` | Prints the decision, each deciding rule with its scope and file; for `NoRule`, `mode_outcome` (`:87-115`) |

**Worked example.** `tests/snapshots/decide__why_output.snap`, fixture `permissions`:

```
git status && curl https://example.com | sh
  no rule matches: Claude Code asks (default mode)
  not covered by an allow rule: curl https://example.com, sh
```

And with fixture `risky`, where `Bash(curl *)` comes from the untrusted project file (`tests/snapshots/decide__why_after_trust.snap`):

```
curl https://example.com
  allow
    Bash  user ~/.claude/settings.json  for: curl https://example.com
  after workspace trust, these repo rules would also apply:
    Bash(curl *) (allow)  project ./.claude/settings.json
```

### `show` through the CLI

`claude::show::render` (`src/claude/show.rs:26-42`) prints ten sections in order: sources, trust, permissions, hooks, MCP, commands, env, sandbox, plugins, and dropped values. Each value carries its scope label; a managed file shows as `managed:<file>` when more than one managed file loaded (`:44-60`). Plugin contents are not read (`:399-402`).

### GitHub Action run

```
action.yml  install -> gh release download -> gh attestation verify -> tar xzf
            audit   -> harness-blast audit (text, --color always) -> exit code
                    -> harness-blast audit --format sarif > $SARIF_FILE
            upload  -> codeql-action/upload-sarif   (if upload-sarif == true)
            fail    -> exit $CODE                   (if fail-on-findings == true and CODE != 0)
```

| Hop | Where | What happens |
|---|---|---|
| 1 | `action.yml:59-65` | Maps `RUNNER_OS_NAME/RUNNER_ARCH_NAME` to a release target |
| 2 | `action.yml:70-76` | Downloads, verifies, extracts; publishes the binary path as step output `bin` |
| 3 | `action.yml:92-99` | Creates an empty home and managed directory under `$RUNNER_TEMP`, unsets `CLAUDE_CONFIG_DIR`, runs the text audit with `HOME` set to the empty home |
| 4 | `action.yml:100-103` | A code between 1 and 10 aborts the step (tool error); 0 and 11-13 continue |
| 5 | `action.yml:104-109` | Runs the audit again with `--format sarif`; the two exit codes must match, else `::error::` and exit 1 |
| 6 | `action.yml:110` | Writes `exit-code` to `$GITHUB_OUTPUT` |
| 7 | `action.yml:112-126` | Optional SARIF upload; then the step fails with the same code if findings remain and `fail-on-findings` is true |

The repository has no HTTP calls of its own, so there are no request headers to record. Network requests are made by `gh` and `upload-sarif`; their headers are outside this repository.

## Security

### Authentication and authorization

The tool has no users, sessions or network listeners. The action authenticates to GitHub with `inputs.token` (default `github.token`) for the release download and attestation check (`action.yml:33-35`, `:53`). SARIF upload needs `security-events: write` on the calling job (`docs/github-action.md:13`). `release.yml` limits write permissions to the `publish` job (`:69-72`); the top level is `contents: read` (`:8-9`).

### Secrets handling

- Proxy URLs in `env` have the `user:pass@` part replaced with `***@` before they are printed (`src/claude/rules.rs:638-648`).
- `mcp-plaintext-secret` reports the field and variable name, never the value (`src/claude/rules.rs:413-444`; `docs/rules.md:9`).
- The tool reads `~/.claude.json`, which holds MCP configuration and trust state, and does not copy it anywhere.

### Encryption

Not applicable inside the binary. Release archives get build provenance attestations and a `SHA256SUMS` file (`release.yml:78-79`).

### Trust boundaries

| Boundary | Crossed where | Control |
|---|---|---|
| Audited repository files -> tool | `src/claude/load.rs:83-111` | Strict JSON parse; invalid files go to `ignored`; the tool executes nothing |
| Audited repository -> baseline | `src/main.rs:124` | A baseline is read only when passed on the command line (`src/baseline.rs:3-5`) |
| Release archive -> consumer runner | `action.yml:70-75` | `gh attestation verify` with signer workflow and source ref |
| PR content -> action inputs -> shell | `action.yml:81-86` | Inputs are passed through `env:` and quoted in the script, not interpolated into the script text |
| Findings -> code scanning | `action.yml:112-117` | Optional step; needs explicit permission |

### Threat table

| Threat | Where it applies | Mitigation | Status | Gap |
|---|---|---|---|---|
| Credential theft | MCP and env values read from config; proxy credentials | Names only for MCP secrets (`rules.rs:413`); userinfo masked (`rules.rs:638`) | partial | Other free-text details can echo config text, for example `short(target)` for hook commands (`rules.rs:470`, `:761-767`); no redaction is applied to them |
| Traffic interception | Release download in the action | Download over `gh`; provenance verified (`action.yml:70-74`) | mitigated | None found. The binary makes no network calls |
| Token or request replay | No tokens or sessions are issued | Not applicable: the repository issues no tokens | mitigated | None |
| Confused deputy and privilege escalation | Action `token`; release workflow | Least-privilege permissions per job (`release.yml:8-9`, `:69-72`); `persist-credentials: false` on checkouts (`ci.yml:24-25`) | mitigated | None found |
| Injection | Action inputs (`path`, `persona`, `baseline`) into bash; `why CALL` | Inputs go through `env:` (`action.yml:81-86`); `why` only parses text and never executes it (`permission.rs:80-107`) | mitigated | `AUDIT_PATH`, `PERSONA` and `BASELINE` are passed as arguments without validation beyond clap's `persona` enum; a value starting with `-` could be read as an option (inferred from `action.yml:95-98`; not tested) |
| Sensitive data in logs | Text, JSON and SARIF output, including CI logs and code scanning | See Credential theft | partial | `docs/github-action.md` does not warn that detail strings are uploaded as SARIF messages (`report.rs:128`) |
| Resource exhaustion | Matching of rules from an untrusted repository | None found | not mitigated | Inferred: `segment_matches` recurses on every `*` (`permission.rs:235-247`) and `glob_parts` on every `**` (`:224-232`), so a crafted pattern such as many `*` against a long name can take exponential time in the worst case. No input size limit on `read_to_string` (`load.rs:84`). A hostile pull request could stall an audit job |
| Abuse without rate limits | No service | Not applicable | mitigated | None |
| Dependency supply chain | Cargo crates; GitHub Actions | `Cargo.lock` committed and `--locked` in CI (`ci.yml:29`, `:47`); actions pinned by SHA; dependabot with 7-day cooldown (`dependabot.yml`); zizmor (`ci.yml:49-59`) | partial | No `cargo audit` or `cargo deny` step found in `ci.yml` |

Other observations:

- The audit's own model of Claude Code can be wrong. Three assumptions are marked `Unverified` in code (see [Components](#components)); a wrong assumption produces wrong findings, not a vulnerability in the tool.
- `local_files` checks "other" read permission along the path from home (`rules.rs:903-915`).

## Configuration

### Environment variables

| Name | Purpose | Required | Default | Read at |
|---|---|---|---|---|
| `HOME` | Home directory for user settings and `~/.claude.json`; `detect` fails when unset | yes (CLI) | none | `src/claude/paths.rs:22` |
| `CLAUDE_CONFIG_DIR` | Replaces `~/.claude`; `.claude.json` is assumed to move with it (unverified) | no | `~/.claude` | `src/claude/paths.rs:24-27` |
| `NO_COLOR` | Disables color when non-empty and `--color auto` | no | unset | `src/style.rs:29` |

Action-only variables set inside the step: `GH_TOKEN`, `VERSION`, `RUNNER_OS_NAME`, `RUNNER_ARCH_NAME` (install); `AUDIT_PATH`, `PERSONA`, `BASELINE`, `SARIF_FILE`, `BIN` (audit); `CODE` (fail step) (`action.yml:52-56`, `:81-86`, `:122-123`).

### Command-line options

| Option | Applies to | Default | Source |
|---|---|---|---|
| `--color auto\|always\|never` | all | `auto` | `src/main.rs:22-23` |
| `--managed-dir DIR` | all | `/etc/claude-code` (Linux), `/Library/Application Support/ClaudeCode` (macOS) | `src/main.rs`, `src/claude/paths.rs:77-83` |
| `--persona` | `audit` | `regular` | `src/main.rs:57-58` |
| `--format` | `audit` | `text` | `src/main.rs:60-61` |
| `--baseline FILE`, `--write-baseline FILE`, `--no-exit-codes` | `audit` | off | `src/main.rs:63-70` |

### Action inputs

Nine inputs with defaults in `action.yml:8-35`, documented in `docs/github-action.md:26-36`. The default `version` is `0.2.0` and must equal the tag and `Cargo.toml` version (`release.yml:38-43`).

### Files read

Locations and precedence are in `src/claude/load.rs:115-157` and `docs/claude-code.md:5-14`. Feature flags: none.

## Observability

The tool prints findings to stdout and a short note to stderr when it writes a baseline (`src/main.rs:110-115`). There are no logs, metrics, traces, error tracking, dashboards or alerts. Errors from `main` surface through `anyhow`'s default output and exit status 1.

## Technology stack

| Layer | Item | Version | Pinned in |
|---|---|---|---|
| Language | Rust, edition 2024 | MSRV 1.85 | `Cargo.toml:4-5`; CI tests `stable` and `1.85` (`ci.yml:37`) |
| CLI | `clap` (derive) | `4` (locked 4.6.7) | `Cargo.toml:14`, `Cargo.lock` |
| Errors | `anyhow` | `1` (locked 1.0.104) | `Cargo.toml:13`, `Cargo.lock` |
| JSON | `serde_json` | `1` (locked 1.0.154) | `Cargo.toml:15`, `Cargo.lock` |
| Snapshot tests | `insta` | `1` (locked 1.49.0) | `Cargo.toml:18`, `Cargo.lock` |
| Temp dirs in tests | `tempfile` | `3` (locked 3.27.0) | `Cargo.toml:19`, `Cargo.lock` |
| CI actions | `actions/checkout` v7.0.1, `Swatinem/rust-cache` v2.9.3, `zizmorcore/zizmor-action` v0.8.0, `actions/upload-artifact` v7.0.2, `actions/download-artifact` v8.0.2, `actions/attest-build-provenance` v4.2.2, `github/codeql-action/upload-sarif` v4.39.1 | by commit SHA | `.github/workflows/*.yml`, `action.yml:114` |
| Release tooling | `gh` CLI (releases, attestations) | runner-provided | `release.yml:82`, `action.yml:70-72` |
| Output formats | SARIF 2.1.0 | n/a | `src/report.rs:147-148` |
| License | MIT | n/a | `Cargo.toml:6`, `LICENSE` |

## Open work

`grep` finds no `TODO`, `FIXME` or `HACK` markers in `src/`. Open items are stated elsewhere:

| Area | Item | Source |
|---|---|---|
| Harness support | Codex and Cursor adapters planned | `README.md:109`, `HARNESSES.md` |
| Coverage | Plugin contents, plugin-provided MCP servers, claude.ai connectors, `managedMcpServers`, managed allow/deny lists not read | `README.md:109`, `src/claude/mcp.rs:8-9`, `src/claude/show.rs:399-402` |
| Visibility | Command-line flags such as `--settings` and `--dangerously-skip-permissions`, and server-delivered policy, are invisible to a file-based audit | `docs/claude-code.md:14` |
| Unverified assumptions | Subfolder trust inheritance; `.claude.json` moving with `CLAUDE_CONFIG_DIR`; `/path` rules from user settings resolving against the project | `src/claude/resolve.rs:216`, `src/claude/paths.rs:26`, `src/claude/decide.rs:61` |
| Platform | Local-state rules are Unix only | `src/claude/rules.rs:898` |
| CI | `uses: $/` in `ci.yml` | `.github/workflows/ci.yml:76`, `:93`, `:103` |

## Quick reference

### Key paths

| What | Path |
|---|---|
| Binary entry | `src/main.rs` |
| Pipeline root | `src/claude/resolve.rs:81` |
| Rules | `src/claude/rules.rs` |
| Permission matching | `src/claude/permission.rs`, `src/claude/shell.rs`, `src/claude/decide.rs` |
| Action | `action.yml` |
| Workflows | `.github/workflows/ci.yml`, `.github/workflows/release.yml` |
| Rule reference | `docs/rules.md` |
| Behavior modeled | `docs/claude-code.md` |
| Fixtures and snapshots | `tests/fixtures/`, `tests/snapshots/` |
| Plan and harness notes | `PLAN.md`, `HARNESSES.md` (both marked local-only drafts in their frontmatter) |

### Common commands

```sh
cargo build --locked --release          # builds target/release/harness-blast (release.yml:47)
cargo test --locked                     # ci.yml:47
cargo fmt --check                       # ci.yml:28
cargo clippy --locked --all-targets -- -D warnings   # ci.yml:29
harness-blast show                      # README.md:88-92
harness-blast why "Read(~/.ssh/id_ed25519)"
harness-blast audit --persona pedantic --format sarif
harness-blast audit --write-baseline .harness-blast-baseline.json
```

### URLs found in config

- Rules reference used as SARIF `helpUri`: `https://github.com/cdvel/harness-blast-radius/blob/main/docs/rules.md` (`src/report.rs:32`)
- SARIF schema: `https://json.schemastore.org/sarif-2.1.0.json` (`src/report.rs:147`)

### Troubleshooting notes

- A settings file with a comment or trailing comma is ignored by Claude Code and by this tool; `audit` reports `ignored-file` (`docs/rules.md:13`, `README.md:23`).
- `audit` exits 11, 12 or 13 on findings; pass `--no-exit-codes` to get 0 (`docs/rules.md:41-50`).
- Findings from a trusted checkout and from CI should match a baseline because the trust note is stripped before comparison (`src/baseline.rs:22-24`).
