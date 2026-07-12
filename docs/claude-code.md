# Claude Code behavior modeled

`harness-blast-radius` follows the Claude Code documentation. Assumptions not yet confirmed against Claude Code itself are marked `Unverified` in the source.

## Sources

- Managed policy: `managed-settings.json`, `managed-settings.d/` drop-ins and `managed-mcp.json` in the system location, or the directory given with `--managed-dir`
- User: `~/.claude/settings.json` (or `CLAUDE_CONFIG_DIR`)
- Project: `.claude/settings.json`
- Local: `.claude/settings.local.json` at the git root
- `~/.claude.json`: user and local MCP servers, and per-folder workspace trust
- `.mcp.json`: project MCP servers

Command-line flags such as `--settings` and `--dangerously-skip-permissions`, and policy delivered from a server, are not visible to a file-based audit.

## Merging

Permission lists union across scopes. Other values take the highest-precedence scope: managed, then local, project, user. For a tool call, deny beats ask beats allow.

Files must be strict JSON. A file with comments or a trailing comma is not loaded, and every rule in it is dropped.

## Workspace trust

Repository allow rules, env, hooks and similar settings apply only after the trust dialog, recorded in `~/.claude.json`. Trust of a parent folder is treated as covering its subfolders (unverified).

## Environment variables

`env` values are written into Claude Code's own process and the processes it starts. Values from project and local settings wait for workspace trust, apart from variables Claude Code classifies as safe, and apply at once in `-p` mode, which never shows the trust dialog. Project and local settings cannot set some variables, including the OpenTelemetry exporter variables, so those are not reported. In Claude Desktop sessions where the app manages the connection, repository proxy and TLS variables are ignored; the audit reports them because terminal sessions apply them.

## Ignored values

- `bypassPermissions` and `auto` default modes set from project or local files
- Managed-only keys outside managed policy

## Permission rules

- **Bash:** commands split at `&&`, `||`, `;`, `|` and `&` outside quotes. Deny and ask rules also match commands inside `$(...)` and backticks. Wrappers such as `timeout`, `nice` and `nohup` are stripped first. `*` matches any text including spaces; the legacy `prefix:*` form matches as a prefix. An allow rule must cover every part of a compound command.
- **Read and Edit:** gitignore-style patterns. `//path` is absolute, `~/path` is home-relative, `/path` is relative to the settings file's project, and other paths are relative to the working directory. A pattern without a slash matches at any depth.
- **WebFetch:** `domain:example.com` matches that host only, `domain:*.example.com` matches subdomains but not the apex, `domain:*` matches any host.
- **MCP:** `mcp__server` and `mcp__server__*` match every tool of a server, `mcp__server__tool` matches one tool, other globs match by wildcard.

## MCP servers

One server wins per name, by scope. Project servers need approval, which only counts in trusted folders; unapproved servers prompt in interactive sessions but load without a prompt in `-p` and SDK runs. `managed-mcp.json` replaces every other source when present. An entry with a `url` but no `type` is read as stdio and skipped.
