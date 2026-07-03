# harness-blast-radius

[![ci](https://github.com/cdvel/harness-blast-radius/actions/workflows/ci.yml/badge.svg)](https://github.com/cdvel/harness-blast-radius/actions/workflows/ci.yml)

**A static analyzer for Claude Code configuration: what it can do without asking, and which file allowed it.**

You clone a repository, open Claude Code, and it asks whether you trust the folder. Say yes and the repository's own settings apply: commands it approves run without asking, and its hooks run on your machine.

`harness-blast-radius` shows what that adds up to. It reads every file Claude Code reads (managed policy, your user settings, the project's `.claude/` and `.mcp.json`, `~/.claude.json`), merges them the same way, and reports what Claude Code can run, read, edit and fetch without asking. It only reads files; nothing is executed.

## Public repositories

Of 60 public GitHub repositories that commit a `.claude/settings.json`:

- 3 approve every shell command once the folder is trusted
- 12 approve an interpreter or network tool such as `python *`, `npx:*`, `node:*`, `curl:*` or `xargs:*`, which run arbitrary code or reach the network
- 14 define hooks that run commands at session start, before tool calls or when Claude Code stops
- 2 approve every MCP server their `.mcp.json` defines
- 1 has a trailing comma, so Claude Code ignores the whole file, deny rules included

It also reports credentials in MCP configs, unpinned `npx` packages, plain `http` MCP servers, settings Claude Code ignores, and prompt history readable by other users. See [the rules](docs/rules.md).

## Use cases

- Before trusting a cloned repository: `harness-blast-radius audit` shows what its settings would allow
- Reviewing a pull request that touches `.claude/` or `.mcp.json`, by hand or in CI with the exit codes
- Answering "why did that run without asking?" with `why`
- Testing a managed policy before rolling it out, with `--managed-dir`

## How it differs

A single settings file does not tell you what Claude Code will do. `harness-blast-radius` resolves the configuration Claude Code actually uses: it merges every scope, applies workspace trust, drops the values Claude Code ignores, and evaluates permission rules with Claude Code's matching (compound commands, wrappers, path anchors). A finding names the rule and the file it came from.

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

## Name

In incident response, the blast radius is everything a compromised component can reach. For a coding harness, that is whatever it may run, read, edit or fetch without asking.

## Status

Early. Claude Code only; Codex and Cursor are planned. Hooks are listed but not yet audited, plugin contents are not read, and output is text only.

## License

MIT
