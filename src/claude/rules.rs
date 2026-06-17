//! Rules over the resolved Claude Code configuration.

use std::path::Path;

use serde_json::Value;

use super::mcp::{McpScope, McpServer, McpStatus};
use super::resolve::{COMMAND_SETTINGS, Resolved, Status};
use crate::finding::{Confidence, Finding, Persona, Rule, Severity};
use crate::merge::{Item, SrcId};

pub fn check(r: &Resolved) -> Vec<Finding> {
    let mut out = Vec::new();
    ignored(r, &mut out);
    permissions(r, &mut out);
    mcp(r, &mut out);
    commands(r, &mut out);
    sandbox(r, &mut out);
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

const SECRET_PATHS: &[&str] = &[
    ".ssh",
    ".aws",
    ".env",
    ".npmrc",
    ".gnupg",
    "credentials",
    ".netrc",
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

    for item in r.settings.items("permissions.allow") {
        let Some((src, note)) = effective_srcs(r, item, "permissions.allow") else {
            continue;
        };
        let rule_text = text(&item.value);
        let rule = match rule_text.as_str() {
            "Bash" | "Bash(*)" | "*" => Some(&BROAD_SHELL_ALLOW),
            "Edit" | "Write" | "WebFetch" | "WebFetch(domain:*)" | "mcp__*" => Some(&BROAD_ALLOW),
            other => bash_program(other)
                .filter(|p| INTERPRETERS.contains(p))
                .map(|_| &INTERPRETER_ALLOW),
        };
        if let Some(rule) = rule {
            out.push(
                Finding::new(rule, format!("{rule_text}{note}"))
                    .at(path(r, src), "permissions.allow"),
            );
        }
    }

    for list in ["allow", "ask", "deny"] {
        let key = format!("permissions.{list}");
        for item in r.settings.items(&key) {
            let rule_text = text(&item.value);
            let rule = if rule_text.starts_with("Bash(command:") {
                &UNSUPPORTED_RULE_SYNTAX
            } else if rule_text.contains(":*)") {
                &LEGACY_RULE_SYNTAX
            } else {
                continue;
            };
            out.push(Finding::new(rule, rule_text).at(path(r, item.srcs[0]), key.clone()));
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
            if rule_text.starts_with("Read(") && SECRET_PATHS.iter().any(|p| rule_text.contains(p))
            {
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

/// First program in a `Bash(...)` rule, ignoring wildcards and the legacy `:*`.
fn bash_program(rule: &str) -> Option<&str> {
    let inner = rule.strip_prefix("Bash(")?.strip_suffix(')')?;
    let first = inner.split_whitespace().next()?;
    let first = first.trim_end_matches(":*").trim_end_matches('*');
    let first = first.rsplit('/').next().unwrap_or(first);
    (!first.is_empty()).then_some(first)
}

#[cfg(test)]
mod tests {
    use super::{bash_program, insecure_url, unpinned_package};
    use serde_json::json;

    #[test]
    fn bash_program_strips_wildcards_and_paths() {
        assert_eq!(bash_program("Bash(python3 *)"), Some("python3"));
        assert_eq!(bash_program("Bash(curl:*)"), Some("curl"));
        assert_eq!(bash_program("Bash(/usr/bin/node script.js)"), Some("node"));
        assert_eq!(bash_program("Bash(*)"), None);
        assert_eq!(bash_program("Read(**)"), None);
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
