# harness-blast-radius

Shows what Claude Code is allowed to touch and run on your machine, and which config file allowed it. Codex and Cursor are planned.

Claude Code reads settings from up to five scopes, plus `~/.claude.json` and `.mcp.json`. Lists merge across scopes, some values only apply after you trust a folder, and some are silently ignored in the file they are written in. `harness-blast-radius` resolves all of it the way Claude Code does and answers three questions:

- **`show`**: what configuration is in effect, and where each value comes from
- **`why`**: would this command or tool call be allowed, asked or denied, and by which rule
- **`audit`**: which settings widen what the harness can do without asking

It only reads files. It never runs Claude Code, its hooks, or the MCP servers it finds.

## Example

A repository ships this `.claude/settings.json` and `.mcp.json`:

```json
{
  "permissions": {
    "allow": ["Bash(npm run *)", "Bash(python3 *)", "Bash(git log:*)", "WebFetch"],
    "deny": ["Read(.env)"]
  },
  "enableAllProjectMcpServers": true
}
```

```json
{
  "mcpServers": {
    "browser": { "command": "npx", "args": ["-y", "@example/browser-mcp"] },
    "tracker": { "type": "http", "url": "http://tracker.internal.example/mcp",
                 "headers": { "Authorization": "Bearer sk-live-1234567890abcdef" } }
  }
}
```

```console
$ harness-blast-radius audit
high    mcp-plaintext-secret  tracker: headers.Authorization
        at ./.mcp.json  mcpServers.tracker
        MCP server config holds a credential in plain text
        fix: Reference an environment variable with ${VAR} and keep the value out of config files

medium  broad-allow  WebFetch (applies after workspace trust)
        at ./.claude/settings.json  permissions.allow
        Allow rule approves a whole tool or every MCP server without asking
        fix: Scope the rule to paths, domains or specific MCP tools

medium  interpreter-allow  Bash(python3 *) (applies after workspace trust)
        at ./.claude/settings.json  permissions.allow
        Allow rule approves a program that can run arbitrary code or reach the network, which makes other rules easy to bypass
        fix: Allow specific scripts or subcommands instead of the interpreter or tool itself

medium  mcp-enable-all-project-servers  enableAllProjectMcpServers = true (applies after workspace trust)
        at ./.claude/settings.json  enableAllProjectMcpServers
        Every MCP server a repository's .mcp.json defines is approved without review
        fix: Remove enableAllProjectMcpServers and approve servers by name with enabledMcpjsonServers

medium  mcp-insecure-http  tracker: http://tracker.internal.example/mcp
        at ./.mcp.json  mcpServers.tracker
        Remote MCP server uses plain http, so requests and tokens travel unencrypted
        fix: Use an https URL

medium  mcp-unpinned-package  browser: @example/browser-mcp
        at ./.mcp.json  mcpServers.browser
        MCP server runs the latest published package on every start, so a compromised release runs without review
        fix: Pin an exact version, e.g. npx -y pkg@1.2.3 or uvx pkg==1.2.3

6 findings, 1 high, 5 medium (5 hidden, --persona auditor shows all)
```

Credentials are reported by name. Their values are never printed.

Once the folder is trusted, `why` shows what those rules mean for a specific command:

```console
$ harness-blast-radius why "npm run test && rm -rf build"
npm run test && rm -rf build
  no rule matches: Claude Code asks (default mode)
  not covered by an allow rule: rm -rf build

$ harness-blast-radius why "python3 -c 'import os; os.system(\"id\")'"
python3 -c 'import os; os.system("id")'
  allow
    Bash(python3 *)  project ./.claude/settings.json  for: python3 -c 'import os; os.system("id")'

$ harness-blast-radius why "Read(.env)"
Read(.env)
  deny
    Read(.env)  project ./.claude/settings.json
```

## Installation

Requires Rust 1.85 or later.

```sh
cargo install --git https://github.com/cdvel/harness-blast-radius
```

Or from a clone:

```sh
cargo install --path .
```

## Usage

All commands default to the current directory and read your real home directory, so user settings and `~/.claude.json` are included.

### show

```sh
harness-blast-radius show [PATH]
```

Prints the effective configuration: config sources, workspace trust, permission rules, hooks, MCP servers, settings that run commands, environment variable names (values hidden), sandbox and plugins. Every value is labeled with the scope it came from, rules waiting for workspace trust are marked `(after trust)`, and a final section lists values Claude Code ignores, with the reason.

### why

```sh
harness-blast-radius why <CALL> [--path PATH]
```

`CALL` is a shell command or a tool call in permission rule form: `"git push --force"`, `"Read(~/.ssh/id_ed25519)"`, `"WebFetch(https://docs.rs)"`, `"mcp__github__create_issue"`.

The answer is `deny`, `ask` or `allow` with the rule and file that decided it. When no rule matches, it says what the permission mode does instead. Repo rules that would only apply after the trust dialog are listed separately.

### audit

```sh
harness-blast-radius audit [PATH] [--persona regular|pedantic|auditor] [--no-exit-codes]
```

| Persona | Shows |
|---|---|
| `regular` (default) | Findings worth acting on |
| `pedantic` | Adds weaker signals and missing hardening |
| `auditor` | Everything, including low-confidence findings |

### Options

`--managed-dir DIR` reads managed policy (`managed-settings.json`, `managed-settings.d/`, `managed-mcp.json`) from `DIR` instead of the system location. Useful for testing a policy before rolling it out.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | No findings shown |
| 11 | Highest finding shown is low |
| 12 | Highest finding shown is medium |
| 13 | Highest finding shown is high |

`--no-exit-codes` always exits 0 when the audit runs, for advisory use in CI.

## Claude Code behavior modeled

- **Scopes and precedence:** managed policy (including `managed-settings.d/` drop-ins), user `~/.claude/settings.json`, project `.claude/settings.json`, local `.claude/settings.local.json` at the git root, `~/.claude.json`, `.mcp.json` and `managed-mcp.json`.
- **Merging:** permission lists union across scopes; other values take the highest-precedence scope; deny beats ask beats allow.
- **Workspace trust:** repo allow rules, env, hooks and similar settings apply only after the trust dialog, read from `~/.claude.json`, including parent folders.
- **Ignored values:** `bypassPermissions` and `auto` modes set from project or local files, managed-only keys outside managed policy, and files that are not strict JSON (comments or trailing commas), which drop every rule they contain.
- **Bash rules:** commands split at `&&`, `||`, `;`, `|` and `&` outside quotes; commands inside `$(...)` and backticks checked by deny and ask rules; wrappers such as `timeout`, `nice` and `nohup` stripped; `*` matching across spaces; the legacy `prefix:*` form.
- **Path, domain and MCP rules:** gitignore-style path patterns with `//`, `~/`, `/` and relative anchors; `WebFetch(domain:...)` exact host and subdomain matching; MCP rules for a server, a tool or a glob.
- **MCP servers:** scope winner per name, project server approvals, `managed-mcp.json` exclusivity, and entries with a `url` but no `type`, which Claude Code skips.

## Rules

| Rule | Severity | Persona | Reports |
|---|---|---|---|
| `bypass-default-mode` | high | regular | `defaultMode` turns permission prompts off |
| `broad-shell-allow` | high | regular | An allow rule approves every shell command |
| `mcp-plaintext-secret` | high | regular | An MCP server config holds a credential in plain text |
| `ignored-file` | medium | regular | A config file is not loaded, so its deny rules do not apply |
| `broad-allow` | medium | regular | An allow rule approves a whole tool or every MCP server |
| `interpreter-allow` | medium | regular | An allow rule approves a program that runs arbitrary code or reaches the network, such as `python3`, `node`, `curl` or `xargs` |
| `unsupported-rule-syntax` | medium | regular | A rule uses syntax Claude Code ignores |
| `broad-additional-directory` | medium | regular | An additional directory grants the home directory or the whole filesystem |
| `mcp-unpinned-package` | medium | regular | An MCP server runs the latest published package on every start |
| `mcp-insecure-http` | medium | regular | A remote MCP server uses plain http |
| `mcp-enable-all-project-servers` | medium | regular | Every MCP server a repository defines is approved without review |
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

Allow and deny rules are checked with the same matcher `why` uses, not by comparing rule text. `Bash(python3 manage.py test)` is not an interpreter allow; `Bash(/usr/bin/env *)` is.

## Limitations

- Claude Code only. Codex and Cursor adapters are planned.
- Hook commands are listed by `show` but not yet audited.
- Plugins are listed, but the hooks and MCP servers they bring are not read yet.
- Output is text only. JSON and SARIF are planned.
- The audit works from config files. It does not see command-line flags such as `--dangerously-skip-permissions` or `--settings`, or policy delivered from a server.
- Claude Code's behavior changes between releases. The model follows the Claude Code documentation; assumptions not yet confirmed against Claude Code itself are marked `Unverified` in the source.

## Related tools

- [zizmor](https://github.com/zizmorcore/zizmor): static analysis for GitHub Actions. The severity and confidence model and the personas here follow its design.
