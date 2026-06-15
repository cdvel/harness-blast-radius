//! Rules over the resolved Claude Code configuration.

use std::path::Path;

use serde_json::Value;

use super::resolve::{Resolved, Status};
use crate::finding::{Confidence, Finding, Persona, Rule, Severity};
use crate::merge::{Item, SrcId};

pub fn check(r: &Resolved) -> Vec<Finding> {
    let mut out = Vec::new();
    ignored(r, &mut out);
    permissions(r, &mut out);
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
    use super::bash_program;

    #[test]
    fn bash_program_strips_wildcards_and_paths() {
        assert_eq!(bash_program("Bash(python3 *)"), Some("python3"));
        assert_eq!(bash_program("Bash(curl:*)"), Some("curl"));
        assert_eq!(bash_program("Bash(/usr/bin/node script.js)"), Some("node"));
        assert_eq!(bash_program("Bash(*)"), None);
        assert_eq!(bash_program("Read(**)"), None);
    }
}
