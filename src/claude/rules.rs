//! Rules over the resolved Claude Code configuration.

use std::path::Path;

use serde_json::Value;

use super::decide::path_context;
use super::mcp::{McpScope, McpServer, McpStatus};
use super::permission::{PathContext, PermissionRule, ToolCall};
use super::resolve::{COMMAND_SETTINGS, Resolved, Status};
use crate::finding::{Confidence, Finding, Persona, Rule, Severity};
use crate::merge::{Item, Node, SrcId};

pub fn check(r: &Resolved) -> Vec<Finding> {
    let mut out = Vec::new();
    ignored(r, &mut out);
    permissions(r, &mut out);
    mcp(r, &mut out);
    commands(r, &mut out);
    hooks(r, &mut out);
    sandbox(r, &mut out);
    local_files(r, &mut out);
    out.sort_by(|a, b| {
        b.rule
            .severity
            .cmp(&a.rule.severity)
            .then(a.rule.id.cmp(b.rule.id))
    });
    out
}

fn path(r: &Resolved, src: SrcId) -> &Path {
    &r.loaded.sources[src].path
}

/// Sources of an item that take effect, with a note when they wait for trust.
fn effective_srcs(r: &Resolved, item: &Item, key: &str) -> Option<(SrcId, &'static str)> {
    let mut after_trust = None;
    for &src in &item.srcs {
        match r.status(src, key) {
            Status::Active => return Some((src, "")),
            Status::AfterTrust => after_trust = after_trust.or(Some(src)),
            Status::Ignored(_) => {}
        }
    }
    after_trust.map(|src| (src, " (applies after workspace trust)"))
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// Files and values Claude Code does not apply.

pub static IGNORED_FILE: Rule = Rule {
    id: "ignored-file",
    severity: Severity::Medium,
    confidence: Confidence::High,
    persona: Persona::Regular,
    summary: "Config file is not loaded, so none of its settings apply, including deny rules",
    fix: "Fix the JSON (no comments or trailing commas) or remove the file",
};

pub static IGNORED_VALUE: Rule = Rule {
    id: "ignored-value",
    severity: Severity::Low,
    confidence: Confidence::High,
    persona: Persona::Regular,
    summary: "Setting has no effect in this scope",
    fix: "Move the setting to a scope that honors it, or remove it",
};

fn ignored(r: &Resolved, out: &mut Vec<Finding>) {
    for ig in &r.loaded.ignored {
        out.push(Finding::new(&IGNORED_FILE, ig.reason.clone()).at(&ig.path, ""));
    }
    for d in &r.dropped {
        out.push(
            Finding::new(
                &IGNORED_VALUE,
                format!("{} = {}: {}", d.key, text(&d.value), d.reason),
            )
            .at(path(r, d.src), d.key),
        );
    }
}

// Permissions.

pub static BYPASS_MODE: Rule = Rule {
    id: "bypass-default-mode",
    severity: Severity::High,
    confidence: Confidence::High,
    persona: Persona::Regular,
    summary: "Permission prompts are off: every tool call runs without asking",
    fix: "Use default or acceptEdits, run bypass mode only inside a sandbox or container, or set permissions.disableBypassPermissionsMode to \"disable\"",
};

pub static AUTO_MODE: Rule = Rule {
    id: "auto-default-mode",
    severity: Severity::Low,
    confidence: Confidence::Medium,
    persona: Persona::Pedantic,
    summary: "Auto mode lets a classifier approve tool calls without asking",
    fix: "Use default or acceptEdits where prompts matter",
};

pub static BROAD_SHELL_ALLOW: Rule = Rule {
    id: "broad-shell-allow",
    severity: Severity::High,
    confidence: Confidence::High,
    persona: Persona::Regular,
    summary: "Allow rule approves every shell command",
    fix: "Replace with specific rules such as Bash(npm run test *)",
};

pub static BROAD_ALLOW: Rule = Rule {
    id: "broad-allow",
    severity: Severity::Medium,
    confidence: Confidence::High,
    persona: Persona::Regular,
    summary: "Allow rule approves a whole tool or every MCP server without asking",
    fix: "Scope the rule to paths, domains or specific MCP tools",
};

pub static INTERPRETER_ALLOW: Rule = Rule {
    id: "interpreter-allow",
    severity: Severity::Medium,
    confidence: Confidence::Medium,
    persona: Persona::Regular,
    summary: "Allow rule approves a program that can run arbitrary code or reach the network, which makes other rules easy to bypass",
    fix: "Allow specific scripts or subcommands instead of the interpreter or tool itself",
};

pub static UNSUPPORTED_RULE_SYNTAX: Rule = Rule {
    id: "unsupported-rule-syntax",
    severity: Severity::Medium,
    confidence: Confidence::High,
    persona: Persona::Regular,
    summary: "Permission rule uses syntax Claude Code ignores, so it does not allow, ask or deny what it appears to",
    fix: "Rewrite as Bash(prefix *), e.g. Bash(rm *)",
};

pub static LEGACY_RULE_SYNTAX: Rule = Rule {
    id: "legacy-rule-syntax",
    severity: Severity::Low,
    confidence: Confidence::Medium,
    persona: Persona::Pedantic,
    summary: "Permission rule uses the legacy :* prefix syntax",
    fix: "Use the space wildcard form, e.g. Bash(git log *)",
};

pub static BROAD_ADDITIONAL_DIR: Rule = Rule {
    id: "broad-additional-directory",
    severity: Severity::Medium,
    confidence: Confidence::High,
    persona: Persona::Regular,
    summary: "Additional directory grants file access to the home directory or the whole filesystem",
    fix: "List the specific directories the project needs",
};

pub static SECRET_DENY_WITHOUT_SANDBOX: Rule = Rule {
    id: "secret-deny-without-sandbox",
    severity: Severity::Low,
    confidence: Confidence::Medium,
    persona: Persona::Pedantic,
    summary: "Read deny rules protect secret files from file tools only; shell commands can still read them without the sandbox",
    fix: "Enable the sandbox with sandbox.filesystem.denyRead for the same paths",
};

/// Programs that run arbitrary code, fetch from the network, or change privileges.
const INTERPRETERS: &[&str] = &[
    "bash", "bun", "bunx", "curl", "deno", "docker", "env", "eval", "fish", "node", "npx", "perl",
    "php", "pnpx", "podman", "python", "python3", "ruby", "sh", "ssh", "sudo", "uvx", "wget",
    "xargs", "zsh",
];

/// Secret files a Read deny rule would usually be written to protect.
const SECRET_FILES: &[&str] = &[
    "~/.ssh/id_ed25519",
    "~/.aws/credentials",
    "~/.npmrc",
    "~/.netrc",
    "~/.gnupg/private-keys-v1.d/key",
    "~/.config/gcloud/credentials.db",
    ".env",
    "config/.env.production",
];

fn permissions(r: &Resolved, out: &mut Vec<Finding>) {
    if let Some(leaf) = r.settings.leaf("permissions.defaultMode") {
        let bypass_disabled = r
            .settings
            .leaf("permissions.disableBypassPermissionsMode")
            .is_some_and(|l| l.value == "disable");
        let rule = match leaf.value.as_str() {
            Some("bypassPermissions") if !bypass_disabled => Some(&BYPASS_MODE),
            Some("auto") => Some(&AUTO_MODE),
            _ => None,
        };
        if let Some(rule) = rule {
            out.push(
                Finding::new(rule, format!("defaultMode = {}", text(&leaf.value)))
                    .at(path(r, leaf.src), "permissions.defaultMode"),
            );
        }
    }

    let ctx = path_context(r);
    for item in r.settings.items("permissions.allow") {
        let Some((src, note)) = effective_srcs(r, item, "permissions.allow") else {
            continue;
        };
        let rule_text = text(&item.value);
        let Some(parsed) = PermissionRule::parse(&rule_text) else {
            continue;
        };
        if let Some(rule) = allow_risk(&parsed, &ctx) {
            out.push(
                Finding::new(rule, format!("{rule_text}{note}"))
                    .at(path(r, src), "permissions.allow"),
            );
        }
    }

    for list in ["allow", "ask", "deny"] {
        let key = format!("permissions.{list}");
        // Legacy rules are grouped per file: configs often have dozens.
        let mut legacy: Vec<(SrcId, Vec<String>)> = Vec::new();
        for item in r.settings.items(&key) {
            let rule_text = text(&item.value);
            let src = item.srcs[0];
            if rule_text.starts_with("Bash(command:") {
                out.push(
                    Finding::new(&UNSUPPORTED_RULE_SYNTAX, rule_text).at(path(r, src), key.clone()),
                );
            } else if rule_text.contains(":*)") {
                match legacy.iter_mut().find(|(s, _)| *s == src) {
                    Some((_, rules)) => rules.push(rule_text),
                    None => legacy.push((src, vec![rule_text])),
                }
            }
        }
        for (src, rules) in legacy {
            let detail = match rules.len() {
                1 => rules[0].clone(),
                n => format!("{} and {} more", rules[0], n - 1),
            };
            out.push(Finding::new(&LEGACY_RULE_SYNTAX, detail).at(path(r, src), key.clone()));
        }
    }

    let home = r.paths.home.to_string_lossy().to_string();
    for item in r.settings.items("permissions.additionalDirectories") {
        let dir = text(&item.value);
        let trimmed = dir.trim_end_matches('/');
        if matches!(trimmed, "" | "~" | "//") || trimmed == home {
            out.push(
                Finding::new(&BROAD_ADDITIONAL_DIR, dir)
                    .at(path(r, item.srcs[0]), "permissions.additionalDirectories"),
            );
        }
    }

    let sandbox_on = r
        .settings
        .leaf("sandbox.enabled")
        .is_some_and(|l| l.value == Value::Bool(true));
    if !sandbox_on {
        for item in r.settings.items("permissions.deny") {
            let rule_text = text(&item.value);
            let protects_secret = PermissionRule::parse(&rule_text).is_some_and(|rule| {
                rule.tool == "Read"
                    && SECRET_FILES
                        .iter()
                        .any(|f| rule.matches_call(&ToolCall::Read(f.to_string()), &ctx))
            });
            if protects_secret {
                out.push(
                    Finding::new(&SECRET_DENY_WITHOUT_SANDBOX, rule_text)
                        .at(path(r, item.srcs[0]), "permissions.deny"),
                );
            }
        }
    }
}

// MCP servers.

pub static MCP_UNPINNED_PACKAGE: Rule = Rule {
    id: "mcp-unpinned-package",
    severity: Severity::Medium,
    confidence: Confidence::Medium,
    persona: Persona::Regular,
    summary: "MCP server runs the latest published package on every start, so a compromised release runs without review",
    fix: "Pin an exact version, e.g. npx -y pkg@1.2.3 or uvx pkg==1.2.3",
};

pub static MCP_PLAINTEXT_SECRET: Rule = Rule {
    id: "mcp-plaintext-secret",
    severity: Severity::High,
    confidence: Confidence::Medium,
    persona: Persona::Regular,
    summary: "MCP server config holds a credential in plain text",
    fix: "Reference an environment variable with ${VAR} and keep the value out of config files",
};

pub static MCP_INSECURE_HTTP: Rule = Rule {
    id: "mcp-insecure-http",
    severity: Severity::Medium,
    confidence: Confidence::High,
    persona: Persona::Regular,
    summary: "Remote MCP server uses plain http, so requests and tokens travel unencrypted",
    fix: "Use an https URL",
};

pub static MCP_INVALID_ENTRY: Rule = Rule {
    id: "mcp-invalid-entry",
    severity: Severity::Low,
    confidence: Confidence::High,
    persona: Persona::Regular,
    summary: "MCP entry has a url but no type; Claude Code reads it as stdio and skips it",
    fix: "Add \"type\": \"http\" (or \"ws\")",
};

pub static MCP_ENABLE_ALL_PROJECT: Rule = Rule {
    id: "mcp-enable-all-project-servers",
    severity: Severity::Medium,
    confidence: Confidence::High,
    persona: Persona::Regular,
    summary: "Every MCP server a repository's .mcp.json defines is approved without review",
    fix: "Remove enableAllProjectMcpServers and approve servers by name with enabledMcpjsonServers",
};

pub static MCP_PROJECT_UNAPPROVED: Rule = Rule {
    id: "mcp-project-unapproved",
    severity: Severity::Low,
    confidence: Confidence::Medium,
    persona: Persona::Pedantic,
    summary: "Project MCP server is not approved: interactive sessions prompt, but -p, SDK and cloud sessions load it without a prompt",
    fix: "Approve it with enabledMcpjsonServers or block it with disabledMcpjsonServers",
};

const PACKAGE_RUNNERS: &[&str] = &["npx", "bunx", "pnpx", "uvx", "pipx"];

fn mcp(r: &Resolved, out: &mut Vec<Finding>) {
    if let Some(leaf) = r.settings.leaf("enableAllProjectMcpServers") {
        if leaf.value == Value::Bool(true) {
            let note = if r.status(leaf.src, "permissions.allow") == Status::AfterTrust {
                " (applies after workspace trust)"
            } else {
                ""
            };
            out.push(
                Finding::new(
                    &MCP_ENABLE_ALL_PROJECT,
                    format!("enableAllProjectMcpServers = true{note}"),
                )
                .at(path(r, leaf.src), "enableAllProjectMcpServers"),
            );
        }
    }

    for s in &r.mcp {
        let at = |f: Finding| f.at(path(r, s.src), mcp_key(s));
        for (field, name) in plaintext_secrets(&s.config) {
            out.push(at(Finding::new(
                &MCP_PLAINTEXT_SECRET,
                format!("{}: {field}.{name}", s.name),
            )));
        }
        if s.status == McpStatus::Invalid {
            out.push(at(Finding::new(&MCP_INVALID_ENTRY, s.name.clone())));
            continue;
        }
        if matches!(s.status, McpStatus::Blocked | McpStatus::Disabled) {
            continue;
        }
        if s.status == McpStatus::Pending {
            out.push(at(Finding::new(&MCP_PROJECT_UNAPPROVED, s.name.clone())));
        }
        if let Some(pkg) = unpinned_package(&s.config) {
            out.push(at(Finding::new(
                &MCP_UNPINNED_PACKAGE,
                format!("{}: {pkg}", s.name),
            )));
        }
        if let Some(url) = s.config.get("url").and_then(Value::as_str) {
            if insecure_url(url) {
                out.push(at(Finding::new(
                    &MCP_INSECURE_HTTP,
                    format!("{}: {url}", s.name),
                )));
            }
        }
    }
}

fn mcp_key(s: &McpServer) -> String {
    match s.scope {
        McpScope::Local => format!("projects.<path>.mcpServers.{}", s.name),
        _ => format!("mcpServers.{}", s.name),
    }
}

/// `env` and `headers` entries that look like credentials and are not
/// `${VAR}` references. Returns field and key names only, never values.
fn plaintext_secrets(config: &Value) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for field in ["env", "headers"] {
        let Some(Value::Object(map)) = config.get(field) else {
            continue;
        };
        for (name, value) in map {
            let Some(value) = value.as_str() else {
                continue;
            };
            let literal = value.trim_start_matches("Bearer ").trim();
            if literal.is_empty() || literal.starts_with("${") {
                continue;
            }
            let upper = name.to_ascii_uppercase();
            let secret_name = [
                "TOKEN",
                "KEY",
                "SECRET",
                "PASSWORD",
                "AUTHORIZATION",
                "CREDENTIAL",
            ]
            .iter()
            .any(|w| upper.contains(w));
            if secret_name || value.starts_with("Bearer ") {
                out.push((field, name.clone()));
            }
        }
    }
    out
}

/// Package argument of an `npx`/`uvx`-style server without a pinned version.
fn unpinned_package(config: &Value) -> Option<String> {
    let command = config.get("command")?.as_str()?;
    let runner = command.rsplit('/').next().unwrap_or(command);
    if !PACKAGE_RUNNERS.contains(&runner) {
        return None;
    }
    let args = config.get("args")?.as_array()?;
    let pkg = args
        .iter()
        .filter_map(Value::as_str)
        .find(|a| !a.starts_with('-'))?;
    let pinned = match runner {
        "uvx" | "pipx" => pkg.contains("==") || pkg.contains('@'),
        // Scoped npm packages start with @; the version follows a second @.
        _ => pkg
            .trim_start_matches('@')
            .split_once('@')
            .is_some_and(|(_, v)| !v.is_empty() && v != "latest"),
    };
    (!pinned).then(|| pkg.to_string())
}

fn insecure_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    if rest.starts_with("[::1]") {
        return false;
    }
    let host = rest.split(['/', ':']).next().unwrap_or("");
    !matches!(host, "localhost" | "127.0.0.1")
}

// Settings that run commands.

pub static REPO_COMMAND_SETTING: Rule = Rule {
    id: "repo-command-setting",
    severity: Severity::Medium,
    confidence: Confidence::High,
    persona: Persona::Regular,
    summary: "Repository settings run a command on your machine",
    fix: "Review the command; move it to user settings if you want it, or remove it from the repo",
};

fn commands(r: &Resolved, out: &mut Vec<Finding>) {
    for &key in COMMAND_SETTINGS {
        let Some(leaf) = r.command_setting(key) else {
            continue;
        };
        if r.scope(leaf.src).is_repo() {
            out.push(
                Finding::new(
                    &REPO_COMMAND_SETTING,
                    format!("{key}: {}", text(&leaf.value)),
                )
                .at(path(r, leaf.src), key),
            );
        }
    }
}

// Hooks.

pub static REPO_HOOK: Rule = Rule {
    id: "repo-hook",
    severity: Severity::Medium,
    confidence: Confidence::High,
    persona: Persona::Regular,
    summary: "Repository hook runs a command or sends hook input to a URL each time its event fires",
    fix: "Review what the hook runs; move it to user settings if you want it, or remove it from the repo",
};

/// One hook handler and the group that defines it.
struct Hook<'a> {
    event: &'a str,
    matcher: String,
    handler: &'a Value,
    group: &'a Item,
}

impl Hook<'_> {
    /// `command` (the default) or `http`; prompt and agent hooks run no code.
    fn target(&self) -> Option<(&'static str, &str)> {
        let kind = self
            .handler
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("command");
        let key = match kind {
            "command" => "command",
            "http" => "url",
            _ => return None,
        };
        Some((key, self.handler.get(key)?.as_str()?))
    }
}

fn hook_handlers(r: &Resolved) -> Vec<Hook<'_>> {
    let Some(Node::Object(events)) = r.settings.get("hooks") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (event, node) in events {
        let Node::List(groups) = node else { continue };
        for group in groups {
            let matcher = group.value.get("matcher").map(text).unwrap_or_default();
            let matcher = if matcher.is_empty() {
                "*".into()
            } else {
                matcher
            };
            let handlers = group.value.get("hooks").and_then(Value::as_array);
            for handler in handlers.into_iter().flatten() {
                out.push(Hook {
                    event,
                    matcher: matcher.clone(),
                    handler,
                    group,
                });
            }
        }
    }
    out
}

fn hooks(r: &Resolved, out: &mut Vec<Finding>) {
    for hook in hook_handlers(r) {
        let Some((src, note)) = effective_srcs(r, hook.group, "hooks") else {
            continue;
        };
        let Some((key, target)) = hook.target() else {
            continue;
        };
        if r.scope(src).is_repo() {
            out.push(
                Finding::new(
                    &REPO_HOOK,
                    format!("{} {}: {key} {target}{note}", hook.event, hook.matcher),
                )
                .at(path(r, src), format!("hooks.{}", hook.event)),
            );
        }
    }
}

// Sandbox.

pub static SANDBOX_OFF: Rule = Rule {
    id: "sandbox-off",
    severity: Severity::Low,
    confidence: Confidence::High,
    persona: Persona::Pedantic,
    summary: "Shell commands run without filesystem or network isolation",
    fix: "Enable the sandbox with /sandbox or sandbox.enabled",
};

pub static SANDBOX_FALLBACK: Rule = Rule {
    id: "sandbox-fallback",
    severity: Severity::Low,
    confidence: Confidence::Medium,
    persona: Persona::Pedantic,
    summary: "If the sandbox cannot start, commands run unsandboxed without stopping",
    fix: "Set sandbox.failIfUnavailable to true",
};

pub static SANDBOX_EXCLUDED_COMMAND: Rule = Rule {
    id: "sandbox-excluded-command",
    severity: Severity::Medium,
    confidence: Confidence::High,
    persona: Persona::Regular,
    summary: "Command runs outside the sandbox with full filesystem and network access",
    fix: "Remove it from sandbox.excludedCommands, or narrow the pattern",
};

pub static SANDBOX_UNSANDBOXED_RETRY: Rule = Rule {
    id: "sandbox-unsandboxed-retry",
    severity: Severity::Low,
    confidence: Confidence::Medium,
    persona: Persona::Pedantic,
    summary: "A command that fails in the sandbox can be retried outside it",
    fix: "Set sandbox.allowUnsandboxedCommands to false",
};

fn sandbox(r: &Resolved, out: &mut Vec<Finding>) {
    let flag = |key: &str| {
        r.settings
            .leaf(key)
            .map(|l| (l.value == Value::Bool(true), l.src))
    };
    let Some((true, src)) = flag("sandbox.enabled") else {
        out.push(Finding::new(&SANDBOX_OFF, "sandbox.enabled is not true"));
        return;
    };
    if !matches!(flag("sandbox.failIfUnavailable"), Some((true, _))) {
        out.push(
            Finding::new(&SANDBOX_FALLBACK, "sandbox.failIfUnavailable is not true")
                .at(path(r, src), "sandbox.enabled"),
        );
    }
    match r.settings.leaf("sandbox.allowUnsandboxedCommands") {
        Some(l) if l.value == Value::Bool(false) => {}
        _ => out.push(
            Finding::new(
                &SANDBOX_UNSANDBOXED_RETRY,
                "sandbox.allowUnsandboxedCommands is not false",
            )
            .at(path(r, src), "sandbox.enabled"),
        ),
    }
    for item in r.settings.items("sandbox.excludedCommands") {
        out.push(
            Finding::new(&SANDBOX_EXCLUDED_COMMAND, text(&item.value))
                .at(path(r, item.srcs[0]), "sandbox.excludedCommands"),
        );
    }
}

// Local state files.

pub static STATE_FILE_READABLE: Rule = Rule {
    id: "state-file-readable",
    severity: Severity::Medium,
    confidence: Confidence::Medium,
    persona: Persona::Regular,
    summary: "File with prompt history, credentials or app state is readable by other users on this machine",
    fix: "chmod 600 the file",
};

pub static STATE_DIR_READABLE: Rule = Rule {
    id: "state-dir-readable",
    severity: Severity::Low,
    confidence: Confidence::Medium,
    persona: Persona::Regular,
    summary: "Directories with session data, file snapshots or pasted text are readable by other users on this machine",
    fix: "chmod 700 the directories",
};

const STATE_FILES: &[&str] = &["history.jsonl", ".credentials.json"];
const STATE_DIRS: &[&str] = &[
    "file-history",
    "paste-cache",
    "plans",
    "projects",
    "session-env",
    "sessions",
    "shell-snapshots",
    "todos",
];

#[cfg(unix)]
fn local_files(r: &Resolved, out: &mut Vec<Finding>) {
    let config = &r.paths.config_dir;
    let mut files: Vec<std::path::PathBuf> = STATE_FILES.iter().map(|f| config.join(f)).collect();
    files.push(r.paths.claude_json.clone());
    for file in files {
        if others_can_read(&r.paths.home, &file) {
            out.push(Finding::new(&STATE_FILE_READABLE, r.paths.display(&file)).at(&file, ""));
        }
    }
    let dirs: Vec<&str> = STATE_DIRS
        .iter()
        .copied()
        .filter(|d| others_can_read(&r.paths.home, &config.join(d)))
        .collect();
    if !dirs.is_empty() {
        out.push(Finding::new(&STATE_DIR_READABLE, dirs.join(", ")).at(config, ""));
    }
}

#[cfg(not(unix))]
fn local_files(_: &Resolved, _: &mut Vec<Finding>) {}

/// Readable by "other" and reachable: every directory from `home` down to
/// the path's parent lets others traverse it.
#[cfg(unix)]
fn others_can_read(home: &Path, path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let mode = |p: &Path| std::fs::metadata(p).map(|m| m.permissions().mode()).ok();
    let Some(m) = mode(path) else { return false };
    if m & 0o004 == 0 {
        return false;
    }
    path.ancestors()
        .skip(1)
        .take_while(|dir| dir.starts_with(home))
        .all(|dir| mode(dir).is_some_and(|m| m & 0o001 != 0))
}

/// Probe input no real config names, standing in for "anything".
const PROBE: &str = "harness-blast-radius-probe";

/// Which allow rule finding, if any, applies: checked by running the rule
/// against probe tool calls it would only match if it were broad.
fn allow_risk(rule: &PermissionRule, ctx: &PathContext) -> Option<&'static Rule> {
    let any_command = [PROBE.to_string(), format!("rm -rf {PROBE}")];
    if any_command.iter().all(|c| rule.matches_command(c)) {
        return Some(&BROAD_SHELL_ALLOW);
    }
    let whole_tool = [
        ToolCall::Edit(format!("/etc/{PROBE}")),
        ToolCall::Other {
            tool: "Write".into(),
            input: None,
        },
        ToolCall::WebFetch(format!("https://{PROBE}.invalid/")),
        ToolCall::Mcp(format!("mcp__{PROBE}__tool")),
    ];
    if whole_tool.iter().any(|c| rule.matches_call(c, ctx)) {
        return Some(&BROAD_ALLOW);
    }
    let runs_interpreter = interpreter_names(rule).iter().any(|p| {
        [format!("{p} {PROBE}"), format!("{p} -c {PROBE}")]
            .iter()
            .any(|c| rule.matches_command(c))
    });
    runs_interpreter.then_some(&INTERPRETER_ALLOW)
}

/// Interpreter names to probe a Bash rule with: every bare name, plus the
/// rule's own program when it is an interpreter given by path.
fn interpreter_names(rule: &PermissionRule) -> Vec<String> {
    let mut names: Vec<String> = INTERPRETERS.iter().map(|p| p.to_string()).collect();
    let first = rule
        .specifier
        .as_deref()
        .and_then(|s| s.split_whitespace().next())
        .map(|w| w.trim_end_matches(":*").trim_end_matches('*'));
    if let Some(first) = first.filter(|w| w.contains('/')) {
        let base = first.rsplit('/').next().unwrap_or(first);
        if INTERPRETERS.contains(&base) {
            names.push(first.to_string());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn risk(rule: &str) -> Option<&'static str> {
        let ctx = PathContext {
            home: "/home/u".into(),
            cwd: "/home/u/proj".into(),
            source_root: "/home/u/proj".into(),
        };
        allow_risk(&PermissionRule::parse(rule).unwrap(), &ctx).map(|r| r.id)
    }

    #[test]
    fn secret_files_resolve_like_tool_calls() {
        let ctx = PathContext {
            home: "/home/u".into(),
            cwd: "/home/u/proj".into(),
            source_root: "/home/u/proj".into(),
        };
        let protects = |rule: &str| {
            let rule = PermissionRule::parse(rule).unwrap();
            SECRET_FILES
                .iter()
                .any(|f| rule.matches_call(&ToolCall::Read(f.to_string()), &ctx))
        };
        assert!(protects("Read(~/.ssh/**)"));
        assert!(protects("Read(.env*)"));
        assert!(protects("Read(//**/credentials*)"));
        assert!(!protects("Read(**/credentials*)"));
        assert!(!protects("Read(./docs/.ssh-notes.md)"));
        assert!(!protects("Read(~/.ssh-backup/**)"));
    }

    #[test]
    fn allow_risk_matches_rules_exactly() {
        for broad_shell in ["Bash", "Bash(*)", "*", "Bash(**)"] {
            assert_eq!(
                risk(broad_shell),
                Some("broad-shell-allow"),
                "{broad_shell}"
            );
        }
        for broad in [
            "Edit",
            "Edit(//**)",
            "Write",
            "WebFetch",
            "WebFetch(domain:*)",
            "mcp__*",
        ] {
            assert_eq!(risk(broad), Some("broad-allow"), "{broad}");
        }
        for interpreter in [
            "Bash(python3 *)",
            "Bash(curl:*)",
            "Bash(sh -c *)",
            "Bash(/usr/bin/env *)",
        ] {
            assert_eq!(
                risk(interpreter),
                Some("interpreter-allow"),
                "{interpreter}"
            );
        }
        for narrow in [
            "Bash(python3 manage.py test)",
            "Bash(/usr/bin/node script.js)",
            "Bash(npm run *)",
            "Bash(rm *)",
            "Edit(src/**)",
            "WebFetch(domain:docs.rs)",
            "mcp__github",
            "Read",
        ] {
            assert_eq!(risk(narrow), None, "{narrow}");
        }
    }

    #[test]
    fn unpinned_package_detects_missing_versions() {
        let npx = |pkg: &str| json!({"command": "npx", "args": ["-y", pkg]});
        assert_eq!(unpinned_package(&npx("server")), Some("server".into()));
        assert_eq!(
            unpinned_package(&npx("@scope/server")),
            Some("@scope/server".into())
        );
        assert_eq!(
            unpinned_package(&npx("@scope/server@latest")),
            Some("@scope/server@latest".into())
        );
        assert_eq!(unpinned_package(&npx("@scope/server@1.2.3")), None);
        assert_eq!(
            unpinned_package(&json!({"command": "uvx", "args": ["tool==0.4"]})),
            None
        );
        assert_eq!(unpinned_package(&json!({"command": "./bin/server"})), None);
    }

    #[test]
    fn insecure_url_allows_localhost() {
        assert!(insecure_url("http://mcp.example.com/mcp"));
        assert!(!insecure_url("http://localhost:3000/mcp"));
        assert!(!insecure_url("https://mcp.example.com/mcp"));
    }
}
