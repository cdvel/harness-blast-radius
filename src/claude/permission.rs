//! Claude Code permission rules and tool calls.
//!
//! A rule is `Tool` or `Tool(specifier)`. A tool call is what the harness is
//! about to do, written in the same shape: `Bash(git push)`, `Read(~/.ssh/id_rsa)`,
//! `WebFetch(https://docs.rs)`, `mcp__github__create_issue`.

use std::path::{Component, Path, PathBuf};

use super::shell;

/// A parsed permission rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRule {
    pub tool: String,
    /// Text inside the parentheses; `None` for a bare tool name.
    pub specifier: Option<String>,
}

impl PermissionRule {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        match text.split_once('(') {
            Some((tool, rest)) => {
                let specifier = rest.strip_suffix(')')?;
                Some(Self {
                    tool: tool.to_string(),
                    specifier: Some(specifier.to_string()),
                })
            }
            None => Some(Self {
                tool: text.to_string(),
                specifier: None,
            }),
        }
    }
}

impl PermissionRule {
    /// Whether a Bash rule matches one simple command (already split and
    /// stripped of wrappers).
    pub fn matches_command(&self, command: &str) -> bool {
        if self.tool != "Bash" {
            return false;
        }
        let Some(spec) = self.specifier.as_deref() else {
            return true;
        };
        // Legacy prefix syntax: `Bash(npm run test:*)`.
        if let Some(prefix) = spec.strip_suffix(":*") {
            return command == prefix || command.starts_with(&format!("{prefix} "));
        }
        shell::wildcard(spec, command.trim())
    }
}

/// A tool call to evaluate against the rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCall {
    Bash(String),
    Read(String),
    /// Edit, Write, MultiEdit and NotebookEdit share Edit rules.
    Edit(String),
    WebFetch(String),
    /// Full MCP tool name, `mcp__<server>__<tool>`.
    Mcp(String),
    Other {
        tool: String,
        input: Option<String>,
    },
}

impl ToolCall {
    /// Parse `Tool(input)`, a bare MCP tool name, or a plain shell command.
    pub fn parse(text: &str) -> Self {
        let text = text.trim();
        if text.starts_with("mcp__") && !text.contains(char::is_whitespace) {
            return ToolCall::Mcp(text.to_string());
        }
        let call = text
            .split_once('(')
            .filter(|(tool, rest)| {
                rest.ends_with(')')
                    && !tool.is_empty()
                    && tool.chars().all(|c| c.is_ascii_alphanumeric())
            })
            .map(|(tool, rest)| (tool, &rest[..rest.len() - 1]));
        let Some((tool, input)) = call else {
            return ToolCall::Bash(text.to_string());
        };
        let input = input.to_string();
        match tool {
            "Bash" => ToolCall::Bash(input),
            "Read" => ToolCall::Read(input),
            "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => ToolCall::Edit(input),
            "WebFetch" => ToolCall::WebFetch(input),
            _ => ToolCall::Other {
                tool: tool.to_string(),
                input: Some(input),
            },
        }
    }

    /// Tool name that rules are written against.
    pub fn tool(&self) -> &str {
        match self {
            ToolCall::Bash(_) => "Bash",
            ToolCall::Read(_) => "Read",
            ToolCall::Edit(_) => "Edit",
            ToolCall::WebFetch(_) => "WebFetch",
            ToolCall::Mcp(name) => name,
            ToolCall::Other { tool, .. } => tool,
        }
    }
}

/// Directories that relative paths in rules and tool calls resolve against.
#[derive(Debug, Clone)]
pub struct PathContext {
    pub home: PathBuf,
    /// Working directory: `path` and `./path` resolve here.
    pub cwd: PathBuf,
    /// Directory of the settings source: `/path` resolves here. For project
    /// and local files this is the project root.
    pub source_root: PathBuf,
}

impl PermissionRule {
    /// Whether a non-Bash rule matches a tool call.
    pub fn matches_call(&self, call: &ToolCall, ctx: &PathContext) -> bool {
        // `*` in deny and ask lists matches every tool.
        if self.tool == "*" {
            return true;
        }
        if let ToolCall::Mcp(name) = call {
            return mcp_matches(&self.tool, name);
        }
        if self.tool != call.tool() {
            return false;
        }
        let Some(spec) = self.specifier.as_deref() else {
            return true;
        };
        match call {
            ToolCall::Read(path) | ToolCall::Edit(path) => {
                let path = resolve_call_path(path, ctx);
                path_rule_matches(spec, &path, ctx)
            }
            ToolCall::WebFetch(url) => domain_matches(spec, url),
            ToolCall::Other { input, .. } => input.as_deref() == Some(spec),
            ToolCall::Bash(_) | ToolCall::Mcp(_) => false,
        }
    }
}

/// `mcp__server` and `mcp__server__*` match every tool of the server;
/// `mcp__server__tool` matches one tool; other globs such as `mcp__*` match
/// by wildcard.
fn mcp_matches(rule: &str, name: &str) -> bool {
    if !rule.starts_with("mcp__") {
        return false;
    }
    if rule.matches("__").count() == 1 && !rule.contains('*') {
        return name.starts_with(&format!("{rule}__"));
    }
    shell::wildcard(rule, name)
}

/// `domain:example.com` matches that host only, `domain:*.example.com`
/// matches subdomains but not the apex, and `domain:*` matches every host.
fn domain_matches(spec: &str, url: &str) -> bool {
    let Some(domain) = spec.strip_prefix("domain:") else {
        return false;
    };
    let host = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = host.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    match domain.strip_prefix("*.") {
        _ if domain == "*" => true,
        Some(apex) => host.ends_with(&format!(".{apex}")),
        None => host == domain.to_ascii_lowercase(),
    }
}

fn resolve_call_path(path: &str, ctx: &PathContext) -> PathBuf {
    let p = match path.strip_prefix("~/") {
        Some(rest) => ctx.home.join(rest),
        None => ctx.cwd.join(path),
    };
    normalize(&p)
}

/// Gitignore-style path rule. `//abs` is absolute, `~/p` is home-relative,
/// `/p` is relative to the settings source, `p` and `./p` to the working
/// directory. A pattern without a slash matches a name at any depth.
fn path_rule_matches(spec: &str, path: &Path, ctx: &PathContext) -> bool {
    let (base, pattern) = if let Some(rest) = spec.strip_prefix("//") {
        (PathBuf::from("/"), rest)
    } else if let Some(rest) = spec.strip_prefix("~/") {
        (ctx.home.clone(), rest)
    } else if let Some(rest) = spec.strip_prefix('/') {
        (ctx.source_root.clone(), rest)
    } else {
        (ctx.cwd.clone(), spec.strip_prefix("./").unwrap_or(spec))
    };
    let Ok(rel) = path.strip_prefix(normalize(&base)) else {
        return false;
    };
    let path_parts: Vec<&str> = rel.iter().filter_map(|c| c.to_str()).collect();
    let pattern = pattern.trim_end_matches('/');
    let mut parts: Vec<&str> = pattern.split('/').filter(|p| !p.is_empty()).collect();
    if !pattern.contains('/') {
        parts.insert(0, "**");
    }
    glob_parts(&parts, &path_parts)
}

fn glob_parts(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|i| glob_parts(rest, &path[i..])),
        Some((seg, rest)) => {
            !path.is_empty() && segment_matches(seg, path[0]) && glob_parts(rest, &path[1..])
        }
    }
}

/// `*` matches within one path segment, `?` matches one character.
fn segment_matches(pattern: &str, name: &str) -> bool {
    fn go(p: &[char], n: &[char]) -> bool {
        match p.split_first() {
            None => n.is_empty(),
            Some(('*', rest)) => (0..=n.len()).any(|i| go(rest, &n[i..])),
            Some(('?', rest)) => !n.is_empty() && go(rest, &n[1..]),
            Some((c, rest)) => n.first() == Some(c) && go(rest, &n[1..]),
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    go(&p, &n)
}

/// Lexical normalization of `.` and `..` (no filesystem access).
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rules() {
        let r = PermissionRule::parse("Bash(npm run *)").unwrap();
        assert_eq!(
            (r.tool.as_str(), r.specifier.as_deref()),
            ("Bash", Some("npm run *"))
        );
        let r = PermissionRule::parse("WebFetch").unwrap();
        assert_eq!((r.tool.as_str(), r.specifier), ("WebFetch", None));
        assert_eq!(PermissionRule::parse("Bash(unclosed"), None);
    }

    #[test]
    fn matches_bash_commands() {
        let rule = |t: &str| PermissionRule::parse(t).unwrap();
        assert!(rule("Bash").matches_command("rm -rf /"));
        assert!(rule("Bash(*)").matches_command("anything"));
        assert!(rule("Bash(npm run *)").matches_command("npm run build"));
        assert!(!rule("Bash(npm run *)").matches_command("npm install"));
        assert!(rule("Bash(git log:*)").matches_command("git log --oneline"));
        assert!(!rule("Bash(git log:*)").matches_command("git logx"));
        assert!(!rule("Read").matches_command("cat x"));
    }

    fn ctx() -> PathContext {
        PathContext {
            home: "/home/u".into(),
            cwd: "/home/u/proj/sub".into(),
            source_root: "/home/u/proj".into(),
        }
    }

    fn matches(rule: &str, call: &str) -> bool {
        PermissionRule::parse(rule)
            .unwrap()
            .matches_call(&ToolCall::parse(call), &ctx())
    }

    #[test]
    fn matches_paths() {
        assert!(matches("Read(~/.ssh/**)", "Read(~/.ssh/id_ed25519)"));
        assert!(matches("Read(//etc/passwd)", "Read(/etc/passwd)"));
        assert!(matches(
            "Read(/secrets/**)",
            "Read(/home/u/proj/secrets/a/b.pem)"
        ));
        assert!(matches("Read(./notes.md)", "Read(notes.md)"));
        assert!(matches("Read(*.env)", "Read(config/prod.env)"));
        assert!(!matches("Read(*.env)", "Read(config/prod.env.example)"));
        assert!(matches("Edit(src/**)", "Write(src/a/b.rs)"));
        assert!(!matches("Edit(src/**)", "Write(other/src/b.rs)"));
        assert!(matches("Edit(**/src/**)", "Write(other/src/b.rs)"));
        assert!(matches("Read", "Read(anything)"));
        assert!(!matches("Read(~/.ssh/**)", "Read(../../.ssh/../x)"));
    }

    #[test]
    fn matches_domains_and_mcp() {
        assert!(matches(
            "WebFetch(domain:docs.rs)",
            "WebFetch(https://docs.rs/serde)"
        ));
        assert!(!matches(
            "WebFetch(domain:docs.rs)",
            "WebFetch(https://evil.docs.rs.example)"
        ));
        assert!(matches(
            "WebFetch(domain:*.github.com)",
            "WebFetch(https://api.github.com/x)"
        ));
        assert!(!matches(
            "WebFetch(domain:*.github.com)",
            "WebFetch(https://github.com/x)"
        ));
        assert!(matches(
            "WebFetch(domain:*)",
            "WebFetch(http://localhost:8080)"
        ));
        assert!(matches("mcp__github", "mcp__github__create_issue"));
        assert!(matches("mcp__github__*", "mcp__github__create_issue"));
        assert!(matches(
            "mcp__github__create_issue",
            "mcp__github__create_issue"
        ));
        assert!(!matches(
            "mcp__github__create_issue",
            "mcp__github__delete_repo"
        ));
        assert!(!matches("mcp__git", "mcp__github__create_issue"));
        assert!(matches("mcp__*", "mcp__anything__x"));
    }

    #[test]
    fn parses_tool_calls() {
        assert_eq!(
            ToolCall::parse("git push --force"),
            ToolCall::Bash("git push --force".into())
        );
        assert_eq!(
            ToolCall::parse("Bash(ls -la)"),
            ToolCall::Bash("ls -la".into())
        );
        assert_eq!(
            ToolCall::parse("Write(src/main.rs)"),
            ToolCall::Edit("src/main.rs".into())
        );
        assert_eq!(
            ToolCall::parse("mcp__github__create_issue"),
            ToolCall::Mcp("mcp__github__create_issue".into())
        );
        // A shell command with parentheses is not a tool call.
        assert_eq!(
            ToolCall::parse("echo $(date)"),
            ToolCall::Bash("echo $(date)".into())
        );
    }
}
