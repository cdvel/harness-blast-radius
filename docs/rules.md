# Rules

Each finding has a severity and a confidence. The persona decides which are shown: `regular` (default) shows findings worth acting on, `pedantic` adds weaker signals and missing hardening, `auditor` shows everything.

| Rule | Severity | Persona | Reports |
|---|---|---|---|
| `bypass-default-mode` | high | regular | `defaultMode` turns permission prompts off |
| `broad-shell-allow` | high | regular | An allow rule approves every shell command |
| `mcp-plaintext-secret` | high | regular | An MCP server config holds a credential in plain text (the name is reported, never the value) |
| `hook-pipe-to-shell` | high | regular | A hook downloads code and runs it, e.g. `curl ... \| sh` or `bash -c "$(curl ...)"` |
| `repo-env-redirect` | high | regular | Repository `env` sends API traffic elsewhere: `ANTHROPIC_BASE_URL` and the other provider endpoints, `HTTPS_PROXY`, `HTTP_PROXY` |
| `repo-env-tls` | high | regular | Repository `env` changes certificate checks: `NODE_EXTRA_CA_CERTS`, `NODE_TLS_REJECT_UNAUTHORIZED`, `CLAUDE_CODE_CERT_STORE` |
| `ignored-file` | medium | regular | A config file is not loaded, so its deny rules do not apply |
| `broad-allow` | medium | regular | An allow rule approves a whole tool or every MCP server |
| `interpreter-allow` | medium | regular | An allow rule approves a program that runs arbitrary code or reaches the network, such as `python3`, `node`, `curl` or `xargs` |
| `unsupported-rule-syntax` | medium | regular | A rule uses syntax Claude Code ignores |
| `broad-additional-directory` | medium | regular | An additional directory grants the home directory or the whole filesystem |
| `mcp-unpinned-package` | medium | regular | An MCP server runs the latest published package on every start |
| `mcp-insecure-http` | medium | regular | A remote MCP server uses plain http |
| `mcp-enable-all-project-servers` | medium | regular | Every MCP server a repository defines is approved without review |
| `repo-hook` | medium | regular | A repository hook runs a command or sends hook input to a URL each time its event fires |
| `hook-unpinned-package` | medium | regular | A hook runs `npx`, `uvx` or a similar runner without a pinned version |
| `repo-command-setting` | medium | regular | Repository settings run a command, such as `statusLine`, `apiKeyHelper`, or `CLAUDE_CODE_SHELL_PREFIX` and `CLAUDE_ENV_FILE` in `env` |
| `sandbox-excluded-command` | medium | regular | A command runs outside the sandbox |
| `state-file-readable` | medium | regular | Prompt history, credentials or `~/.claude.json` are readable by other users |
| `ignored-value` | low | regular | A setting has no effect in the scope it is written in |
| `mcp-invalid-entry` | low | regular | An MCP entry has a `url` but no `type`, so Claude Code skips it |
| `state-dir-readable` | low | regular | Session data directories are readable by other users |
| `auto-default-mode` | low | pedantic | Auto mode lets a classifier approve tool calls |
| `legacy-rule-syntax` | low | pedantic | Rules use the legacy `:*` prefix form (one finding per file) |
| `secret-deny-without-sandbox` | low | pedantic | Read deny rules protect secrets from file tools, but shell commands can still read them |
| `mcp-project-unapproved` | low | pedantic | A project MCP server is not approved, so `-p` and SDK runs load it without a prompt |
| `sandbox-off` | low | pedantic | Shell commands run without filesystem or network isolation |
| `sandbox-fallback` | low | pedantic | Commands run unsandboxed if the sandbox cannot start |
| `sandbox-unsandboxed-retry` | low | pedantic | A command that fails in the sandbox can be retried outside it |

Allow and deny rules are checked with the same matcher `why` uses, not by comparing rule text: `Bash(python3 manage.py test)` is not an interpreter allow, `Bash(/usr/bin/env *)` is.

## Exit codes

| Code | Highest finding shown |
|---|---|
| 0 | none |
| 1 | error, such as a baseline file that cannot be read |
| 11 | low |
| 12 | medium |
| 13 | high |

`--no-exit-codes` exits 0 whenever the audit runs. Findings accepted in a baseline do not count.

## Output formats

`audit --format` takes `text` (default), `json` or `sarif`. All three show the same findings for the persona and use the same exit codes.

- `json`: one object with `tool`, `version`, `project`, `hidden` (findings the persona hides) and `findings`. Each finding has `rule`, `severity`, `confidence`, `detail`, `summary`, `fix`, `path`, `key` (dotted settings key) and `line`. `path`, `key` and `line` are `null` when unknown.
- `sarif`: SARIF 2.1.0. Files inside the project are relative to `%SRCROOT%`, the project directory; user and managed files have absolute `file://` URIs. `level` is `error`, `warning` or `note` for high, medium and low, and each rule has a `security-severity` of 8.0, 5.0 or 2.0.

`line` is found by following `key` through the file, then narrowing to the array element or string the finding names. A file that is not valid JSON has no lines.

## Baseline

A baseline lists findings you have reviewed and accepted, so CI fails only on new ones.

```sh
harness-blast audit --write-baseline .harness-blast-baseline.json   # accept everything found now
harness-blast audit --baseline .harness-blast-baseline.json         # report only what is not in it
```

`--write-baseline` writes the findings the persona shows, each with an empty `reason` to fill in, and exits 0. An entry matches on `rule`, `path` (`./` for the project, `~/` for home), `key` and `detail`; the "applies after workspace trust" note is left out, so a baseline written in a trusted checkout also matches in CI. Accepted findings are counted in the summary and left out of JSON and SARIF findings. Entries that match nothing are reported so they can be removed. A baseline that cannot be read or parsed exits 1.

The file is only read when passed with `--baseline`. `audit` never loads one from the project it audits: a repository could otherwise ship a baseline that hides its own hooks from someone deciding whether to trust it. In CI, keep the file in the repository and pass it from the workflow, so changes to it show up in review.
