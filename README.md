# harness-blast-radius

See what Claude Code can do on your machine without asking, and which file allowed it.

A repository's `.claude/settings.json` and `.mcp.json` can approve shell commands, add MCP servers and run hooks. They merge with your user settings, managed policy and `~/.claude.json`, some only apply after you trust the folder, and some are silently ignored. `harness-blast-radius` resolves them the way Claude Code does and reports the result. It only reads files; it never runs Claude Code, hooks or MCP servers.

Some of the things it finds:

- Allow rules that approve any shell command, or an interpreter such as `python3 *`, `npx *` or `curl *`
- MCP servers with credentials in plain text, unpinned `npx` packages, or plain `http`
- Repositories that approve every MCP server they define, or run commands through `statusLine` or `apiKeyHelper`
- Settings Claude Code ignores, including whole files dropped for a trailing comma
- Prompt history and credentials readable by other users on the machine

## Example

```console
$ harness-blast-radius audit
high    mcp-plaintext-secret  tracker: headers.Authorization
        at ./.mcp.json  mcpServers.tracker
        MCP server config holds a credential in plain text
        fix: Reference an environment variable with ${VAR} and keep the value out of config files

medium  interpreter-allow  Bash(python3 *) (applies after workspace trust)
        at ./.claude/settings.json  permissions.allow
        Allow rule approves a program that can run arbitrary code or reach the network, which makes other rules easy to bypass
        fix: Allow specific scripts or subcommands instead of the interpreter or tool itself
...
6 findings, 1 high, 5 medium (5 hidden, --persona auditor shows all)
```

`why` answers for a single command or tool call:

```console
$ harness-blast-radius why "npm run test && rm -rf build"
npm run test && rm -rf build
  no rule matches: Claude Code asks (default mode)
  not covered by an allow rule: rm -rf build

$ harness-blast-radius why "Read(.env)"
Read(.env)
  deny
    Read(.env)  project ./.claude/settings.json
```

## Installation

```sh
cargo install --git https://github.com/cdvel/harness-blast-radius
```

Requires Rust 1.85 or later.

## Usage

```sh
harness-blast-radius show            # effective configuration and the source of every value
harness-blast-radius why "<call>"    # allow, ask or deny, and the deciding rule
harness-blast-radius audit           # risky settings, highest severity first
```

Each command takes a project directory and defaults to the current one. `why` accepts a shell command or a tool call such as `"Read(~/.ssh/id_ed25519)"`, `"WebFetch(https://docs.rs)"` or `"mcp__github__create_issue"`.

`audit --persona pedantic` or `auditor` shows weaker signals too. The exit code is 11, 12 or 13 when the highest finding is low, medium or high; `--no-exit-codes` exits 0.

## Documentation

- [Rules](docs/rules.md)
- [Claude Code behavior modeled](docs/claude-code.md)

## Status

Early. Claude Code only; Codex and Cursor are planned. Hooks are listed but not yet audited, plugin contents are not read, and output is text only.
