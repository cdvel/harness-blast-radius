//! Terminal rendering of the effective configuration (`show`).

use std::fmt::Write;

use serde_json::Value;

use super::load::{Kind, Scope};
use super::mcp::{McpServer, McpStatus};
use super::resolve::{Resolved, Status, Trust};
use crate::merge::{Item, Node, SrcId};

/// Settings that run a command or script.
const COMMAND_SETTINGS: &[&str] = &[
    "apiKeyHelper",
    "statusLine",
    "fileSuggestion",
    "subagentStatusLine",
    "awsAuthRefresh",
    "awsCredentialExport",
    "gcpAuthRefresh",
    "otelHeadersHelper",
    "processWrapper",
    "policyHelper",
];

pub fn render(r: &Resolved) -> String {
    let mut out = String::new();
    sources(r, &mut out);
    trust(r, &mut out);
    permissions(r, &mut out);
    hooks(r, &mut out);
    mcp(r, &mut out);
    commands(r, &mut out);
    env(r, &mut out);
    sandbox(r, &mut out);
    plugins(r, &mut out);
    dropped(r, &mut out);
    out
}

fn label(r: &Resolved, src: SrcId) -> String {
    let source = &r.loaded.sources[src];
    let managed_files = r
        .loaded
        .of_kind(Kind::Settings)
        .filter(|(_, s, _)| s.scope == Scope::Managed)
        .count();
    if source.kind == Kind::Settings && source.scope == Scope::Managed && managed_files > 1 {
        let file = source
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        return format!("managed:{file}");
    }
    source.scope.name().to_string()
}

fn labels(r: &Resolved, srcs: &[SrcId]) -> String {
    srcs.iter()
        .map(|&s| label(r, s))
        .collect::<Vec<_>>()
        .join(", ")
}

fn overrides(r: &Resolved, shadowed: &[SrcId]) -> String {
    if shadowed.is_empty() {
        String::new()
    } else {
        format!("  (overrides {})", labels(r, shadowed))
    }
}

fn status_note(status: Status) -> String {
    match status {
        Status::Active => String::new(),
        Status::AfterTrust => "  (after trust)".into(),
        Status::Ignored(why) => format!("  (ignored: {why})"),
    }
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn heading(out: &mut String, title: &str) {
    let _ = writeln!(out, "\n{title}");
}

/// Print rows as two aligned columns plus a trailing note.
fn rows(out: &mut String, indent: &str, rows: &[(String, String, String)]) {
    let width = rows
        .iter()
        .map(|(a, _, _)| a.chars().count())
        .max()
        .unwrap_or(0);
    for (a, b, note) in rows {
        let _ = writeln!(out, "{indent}{a:<width$}  {b}{note}");
    }
}

fn sources(r: &Resolved, out: &mut String) {
    let _ = writeln!(
        out,
        "Claude Code configuration for {}",
        r.paths.project.display()
    );
    heading(out, "Sources");
    let list: Vec<_> = r
        .loaded
        .sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let what = match s.kind {
                Kind::Settings => label(r, i),
                Kind::ClaudeJson => "state".into(),
                Kind::McpJson => "project mcp".into(),
                Kind::ManagedMcp => "managed mcp".into(),
            };
            (what, r.paths.display(&s.path), String::new())
        })
        .collect();
    if list.is_empty() {
        let _ = writeln!(out, "  none found");
    }
    rows(out, "  ", &list);
    for ig in &r.loaded.ignored {
        let _ = writeln!(
            out,
            "  ignored  {}: {}",
            r.paths.display(&ig.path),
            ig.reason
        );
    }
}

fn trust(r: &Resolved, out: &mut String) {
    let line = match r.trust {
        Trust::Accepted => "accepted",
        Trust::NotAccepted => {
            "not accepted: repo allow rules, env and hooks wait for the trust dialog"
        }
        Trust::Unknown => {
            "unknown (no entry in ~/.claude.json): repo allow rules, env and hooks wait for the trust dialog"
        }
    };
    let _ = writeln!(out, "\nWorkspace trust: {line}");
}

fn permissions(r: &Resolved, out: &mut String) {
    heading(out, "Permissions");
    match r.settings.leaf("permissions.defaultMode") {
        Some(leaf) => {
            let _ = writeln!(
                out,
                "  mode: {}  {}{}",
                text(&leaf.value),
                label(r, leaf.src),
                overrides(r, &leaf.shadowed)
            );
        }
        None => {
            let _ = writeln!(out, "  mode: default (not set)");
        }
    }
    if let Some(leaf) = r.settings.leaf("permissions.disableBypassPermissionsMode") {
        let _ = writeln!(
            out,
            "  disableBypassPermissionsMode: {}  {}",
            text(&leaf.value),
            label(r, leaf.src)
        );
    }
    // Evaluation order is deny, then ask, then allow; first match wins.
    for list in ["deny", "ask", "allow", "additionalDirectories"] {
        let key = format!("permissions.{list}");
        let items = r.settings.items(&key);
        let _ = writeln!(
            out,
            "  {list}{}",
            if items.is_empty() { ": none" } else { "" }
        );
        rows(out, "    ", &item_rows(r, items, &key));
    }
}

fn item_rows(r: &Resolved, items: &[Item], key: &str) -> Vec<(String, String, String)> {
    items
        .iter()
        .map(|i| {
            (
                text(&i.value),
                labels(r, &i.srcs),
                status_note(r.best_status(&i.srcs, key)),
            )
        })
        .collect()
}

fn hooks(r: &Resolved, out: &mut String) {
    heading(out, "Hooks");
    let Some(Node::Object(events)) = r.settings.get("hooks") else {
        let _ = writeln!(out, "  none");
        return;
    };
    for (event, node) in events {
        let Node::List(groups) = node else { continue };
        let _ = writeln!(out, "  {event}");
        let mut list = Vec::new();
        for group in groups {
            let matcher = group.value.get("matcher").map(text).unwrap_or_default();
            let matcher = if matcher.is_empty() {
                "*".into()
            } else {
                matcher
            };
            let handlers = group.value.get("hooks").and_then(Value::as_array);
            for h in handlers.into_iter().flatten() {
                list.push((
                    matcher.clone(),
                    format!("{}  {}", handler(h), labels(r, &group.srcs)),
                    status_note(r.best_status(&group.srcs, "hooks")),
                ));
            }
        }
        rows(out, "    ", &list);
    }
}

fn handler(h: &Value) -> String {
    let kind = h.get("type").map(text).unwrap_or_else(|| "command".into());
    let target = ["command", "url", "tool", "prompt"]
        .iter()
        .find_map(|k| h.get(*k))
        .map(text)
        .unwrap_or_default();
    format!("{kind}: {target}")
}

fn mcp(r: &Resolved, out: &mut String) {
    heading(out, "MCP servers");
    if r.mcp.is_empty() {
        let _ = writeln!(out, "  none");
        return;
    }
    let list: Vec<_> = r
        .mcp
        .iter()
        .map(|s| {
            (
                s.name.clone(),
                format!("{:<7}  {}", s.scope.name(), transport(s)),
                mcp_note(s),
            )
        })
        .collect();
    rows(out, "  ", &list);
}

fn transport(s: &McpServer) -> String {
    let c = &s.config;
    let mut line = match (c.get("url"), c.get("command")) {
        (Some(url), _) => {
            let kind = c
                .get("type")
                .map(text)
                .unwrap_or_else(|| "missing type".into());
            format!("{kind}: {}", text(url))
        }
        (None, Some(cmd)) => {
            let args: Vec<String> = c
                .get("args")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(text)
                .collect();
            format!("stdio: {} {}", text(cmd), args.join(" "))
                .trim_end()
                .to_string()
        }
        _ => "unknown transport".into(),
    };
    for field in ["env", "headers"] {
        if let Some(Value::Object(m)) = c.get(field) {
            let keys: Vec<&str> = m.keys().map(String::as_str).collect();
            let _ = write!(line, "  {field}: {}", keys.join(", "));
        }
    }
    line
}

fn mcp_note(s: &McpServer) -> String {
    let status = match s.status {
        McpStatus::Loads => "",
        McpStatus::Approved => "  (approved)",
        McpStatus::Pending => {
            "  (not approved: prompts interactively, loads without a prompt in -p and SDK runs)"
        }
        McpStatus::Disabled => "  (disabled)",
        McpStatus::Blocked => "  (blocked: managed-mcp.json is exclusive)",
        McpStatus::Invalid => "  (skipped: url without type is read as stdio and skipped)",
    };
    if s.shadowed.is_empty() {
        return status.into();
    }
    let names: Vec<&str> = s.shadowed.iter().map(|sc| sc.name()).collect();
    format!("{status}  (overrides {})", names.join(", "))
}

fn commands(r: &Resolved, out: &mut String) {
    heading(out, "Settings that run commands");
    let mut list = Vec::new();
    for &key in COMMAND_SETTINGS {
        let leaf = r
            .settings
            .leaf(key)
            .or_else(|| r.settings.leaf(&format!("{key}.command")));
        if let Some(leaf) = leaf {
            list.push((
                key.to_string(),
                format!("{}  {}", text(&leaf.value), label(r, leaf.src)),
                String::new(),
            ));
        }
    }
    if list.is_empty() {
        let _ = writeln!(out, "  none");
    }
    rows(out, "  ", &list);
}

fn env(r: &Resolved, out: &mut String) {
    heading(out, "Environment (values hidden)");
    let Some(Node::Object(vars)) = r.settings.get("env") else {
        let _ = writeln!(out, "  none");
        return;
    };
    let list: Vec<_> = vars
        .iter()
        .filter_map(|(name, node)| match node {
            Node::Scalar(leaf) => Some((
                name.clone(),
                label(r, leaf.src),
                format!(
                    "{}{}",
                    status_note(r.status(leaf.src, "env")),
                    overrides(r, &leaf.shadowed)
                ),
            )),
            _ => None,
        })
        .collect();
    rows(out, "  ", &list);
}

fn sandbox(r: &Resolved, out: &mut String) {
    heading(out, "Sandbox");
    let Some(node) = r.settings.get("sandbox") else {
        let _ = writeln!(out, "  not configured (off by default)");
        return;
    };
    let mut list = Vec::new();
    flatten(r, "sandbox", node, &mut list);
    rows(out, "  ", &list);
}

fn flatten(r: &Resolved, path: &str, node: &Node, out: &mut Vec<(String, String, String)>) {
    match node {
        Node::Object(m) => {
            for (k, n) in m {
                flatten(r, &format!("{path}.{k}"), n, out);
            }
        }
        Node::List(items) => {
            for i in items {
                out.push((
                    path.to_string(),
                    format!("{}  {}", text(&i.value), labels(r, &i.srcs)),
                    String::new(),
                ));
            }
        }
        Node::Scalar(leaf) => {
            out.push((
                path.to_string(),
                format!("{}  {}", text(&leaf.value), label(r, leaf.src)),
                String::new(),
            ));
        }
    }
}

fn plugins(r: &Resolved, out: &mut String) {
    heading(
        out,
        "Plugins (hooks and MCP servers from plugins are not read yet)",
    );
    let Some(Node::Object(plugins)) = r.settings.get("enabledPlugins") else {
        let _ = writeln!(out, "  none");
        return;
    };
    let list: Vec<_> = plugins
        .iter()
        .filter_map(|(name, node)| match node {
            Node::Scalar(leaf) => Some((
                name.clone(),
                format!("{}  {}", text(&leaf.value), label(r, leaf.src)),
                String::new(),
            )),
            _ => None,
        })
        .collect();
    rows(out, "  ", &list);
}

fn dropped(r: &Resolved, out: &mut String) {
    if r.dropped.is_empty() {
        return;
    }
    heading(out, "Values Claude Code ignores");
    let list: Vec<_> = r
        .dropped
        .iter()
        .map(|d| {
            (
                format!("{} = {}", d.key, text(&d.value)),
                label(r, d.src),
                format!("  ({})", d.reason),
            )
        })
        .collect();
    rows(out, "  ", &list);
}
