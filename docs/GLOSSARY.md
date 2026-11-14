# Glossary

This file is the project's current vocabulary: each term has one definition and the source it comes from. The Source column is a citation, not a history. A replaced term keeps a row naming its replacement while the old name still appears in the repo; other rows that no longer apply are removed.

| Term | Definition | Source |
|---|---|---|
| Resolved configuration | The settings, MCP servers and trust state merged across all sources with provenance, which every command reads. | [ADR-0001](adr/0001-resolve-configuration-before-judging-it.md) |
| Probe | A synthetic tool call used to test whether an allow rule is broad, instead of reading its text. | [ADR-0002](adr/0002-judge-allow-rules-with-probes.md) |
| Baseline | A file of reviewed findings passed with `--baseline` so only new findings fail; it is never read from the audited project. | [ADR-0003](adr/0003-never-load-a-baseline-from-the-audited-project.md) |
| Adapter | The module that reads and resolves one harness's configuration; `claude` is the only one so far. | Convention |
| After trust | The status of a repository value that is present but not yet applied because the folder is not trusted. | Convention |
| Audit rule | A check with an id, severity, confidence, persona, summary and fix that produces findings. | Convention |
| Blast radius | What a harness may run, read, edit or fetch without asking, as its configuration allows. | Convention |
| Finding | One reported instance of an audit rule, with its detail and the file and key it came from. | Convention |
| Harness | The program that runs a model's tool calls on the user's machine; Claude Code is the only one supported so far. | Convention |
| Ignored file | A config file Claude Code would not load, such as one with invalid JSON, so its deny rules do not apply. | Convention |
| Ignored value | A value Claude Code does not apply in the scope it is written in; the code also calls it a dropped value. | Convention |
| Managed policy | Settings and MCP files in the system location that override every other scope. | Convention |
| Permission rule | An allow, ask or deny entry such as `Bash(npm run *)` in `permissions`. | Convention |
| Persona | The audience that sets how much `audit` shows: regular, pedantic or auditor. | Convention |
| Scope | Where a settings file sits in Claude Code's precedence order: user, project, local or managed (MCP servers use their own order: managed, local, project, user). | Convention |
| Shadowed | A value from a lower-precedence source that a higher one replaced, which the merge keeps a record of. | Convention |
| Source | One config file that was loaded, with its scope, kind and path, which every merged value points back to. | Convention |
| Tool call | An action the harness is about to take, written like a rule: `Bash(git push)`, `Read(~/.ssh/id_rsa)` or `mcp__github__create_issue`. | Convention |
| Verdict | The allow, ask, deny or no-rule outcome for a tool call, with the rules that decided it. | Convention |
| Workspace trust | The folder-trust dialog whose acceptance lets repository-controlled allow rules, `env` and hooks take effect. | Convention |
| Wrapper | A command such as `timeout` or `nice` that is stripped before a Bash rule is matched. | Convention |
