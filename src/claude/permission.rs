//! Claude Code permission rules and tool calls.
//!
//! A rule is `Tool` or `Tool(specifier)`. A tool call is what the harness is
//! about to do, written in the same shape: `Bash(git push)`, `Read(~/.ssh/id_rsa)`,
//! `WebFetch(https://docs.rs)`, `mcp__github__create_issue`.

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
