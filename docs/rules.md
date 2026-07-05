# Rules

Each finding has a severity and a confidence. The persona decides which are shown: `regular` (default) shows findings worth acting on, `pedantic` adds weaker signals and missing hardening, `auditor` shows everything.

| Rule | Severity | Persona | Reports |
|---|---|---|---|
| `bypass-default-mode` | high | regular | `defaultMode` turns permission prompts off |
| `broad-shell-allow` | high | regular | An allow rule approves every shell command |
| `mcp-plaintext-secret` | high | regular | An MCP server config holds a credential in plain text (the name is reported, never the value) |
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
| `repo-command-setting` | medium | regular | Repository settings run a command, such as `statusLine` or `apiKeyHelper` |
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
| 11 | low |
| 12 | medium |
| 13 | high |

`--no-exit-codes` exits 0 whenever the audit runs.
