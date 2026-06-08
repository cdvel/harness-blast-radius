//! MCP server resolution.
//!
//! Scopes, highest first: managed (`managed-mcp.json`, exclusive when present),
//! local (`~/.claude.json` under `projects[path].mcpServers`), project
//! (`.mcp.json`), user (`~/.claude.json` top-level `mcpServers`). Entries are
//! not merged across scopes; the winning entry is used whole.
//!
//! Not yet covered: plugin-provided servers, claude.ai connectors, the
//! `managedMcpServers` setting and managed allow/deny lists.

use serde_json::Value;

use super::Paths;
use super::load::{Kind, Loaded};
use super::resolve::{Trust, project_entry};
use crate::merge::{Node, SrcId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum McpScope {
    Managed,
    Local,
    Project,
    User,
}

impl McpScope {
    pub fn name(self) -> &'static str {
        match self {
            McpScope::Managed => "managed",
            McpScope::Local => "local",
            McpScope::Project => "project",
            McpScope::User => "user",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpStatus {
    /// User, local or managed server: loads without an approval prompt.
    Loads,
    /// Project server approved in settings or `~/.claude.json`.
    Approved,
    /// Project server with no approval: interactive sessions prompt, but
    /// `-p`, SDK and cloud sessions load it without a prompt.
    Pending,
    Disabled,
    /// Not loaded because `managed-mcp.json` is exclusive.
    Blocked,
    /// Configuration error Claude Code skips: `url` without `type`.
    Invalid,
}

#[derive(Debug, Clone)]
pub struct McpServer {
    pub name: String,
    pub scope: McpScope,
    pub src: SrcId,
    pub config: Value,
    /// Lower-precedence scopes defining the same name.
    pub shadowed: Vec<McpScope>,
    pub status: McpStatus,
}

pub fn resolve_mcp(
    paths: &Paths,
    loaded: &Loaded,
    settings: &Node,
    trust: Trust,
) -> Vec<McpServer> {
    let entry = project_entry(paths, loaded);
    let state = loaded.first(Kind::ClaudeJson);

    let mut candidates: Vec<(McpScope, SrcId, &serde_json::Map<String, Value>)> = Vec::new();
    let managed = loaded.first(Kind::ManagedMcp);
    if let Some((id, doc)) = managed {
        push_servers(&mut candidates, McpScope::Managed, id, doc);
    }
    if let (Some((id, _)), Some(entry)) = (state, entry) {
        push_servers(&mut candidates, McpScope::Local, id, entry);
    }
    if let Some((id, doc)) = loaded.first(Kind::McpJson) {
        push_servers(&mut candidates, McpScope::Project, id, doc);
    }
    if let Some((id, doc)) = state {
        push_servers(&mut candidates, McpScope::User, id, doc);
    }

    let approvals = Approvals::new(loaded, settings, entry, trust);
    let mut out: Vec<McpServer> = Vec::new();
    for (scope, src, servers) in candidates {
        for (name, config) in servers {
            if let Some(winner) = out.iter_mut().find(|s| &s.name == name) {
                winner.shadowed.push(scope);
                continue;
            }
            let status = if config.get("url").is_some() && config.get("type").is_none() {
                McpStatus::Invalid
            } else if managed.is_some() && scope != McpScope::Managed {
                McpStatus::Blocked
            } else if scope == McpScope::Project {
                approvals.project_status(name)
            } else if approvals.disabled_in_state(name) {
                McpStatus::Disabled
            } else {
                McpStatus::Loads
            };
            out.push(McpServer {
                name: name.clone(),
                scope,
                src,
                config: config.clone(),
                shadowed: Vec::new(),
                status,
            });
        }
    }
    out.sort_by(|a, b| (a.scope, &a.name).cmp(&(b.scope, &b.name)));
    out
}

fn push_servers<'a>(
    out: &mut Vec<(McpScope, SrcId, &'a serde_json::Map<String, Value>)>,
    scope: McpScope,
    src: SrcId,
    doc: &'a Value,
) {
    if let Some(Value::Object(servers)) = doc.get("mcpServers") {
        out.push((scope, src, servers));
    }
}

/// Approval lists for project (`.mcp.json`) servers.
struct Approvals<'a> {
    loaded: &'a Loaded,
    settings: &'a Node,
    entry: Option<&'a Value>,
    trust: Trust,
}

impl<'a> Approvals<'a> {
    fn new(loaded: &'a Loaded, settings: &'a Node, entry: Option<&'a Value>, trust: Trust) -> Self {
        Self {
            loaded,
            settings,
            entry,
            trust,
        }
    }

    /// Approvals from repo-controlled settings count only in a trusted folder.
    fn counts(&self, src: SrcId) -> bool {
        !self.loaded.sources[src].scope.is_repo() || self.trust == Trust::Accepted
    }

    fn in_settings(&self, key: &str, name: &str) -> bool {
        self.settings
            .items(key)
            .iter()
            .any(|i| i.value.as_str() == Some(name) && i.srcs.iter().any(|&s| self.counts(s)))
    }

    fn in_state(&self, key: &str, name: &str) -> bool {
        self.entry
            .and_then(|e| e.get(key))
            .and_then(Value::as_array)
            .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(name)))
    }

    fn disabled_in_state(&self, name: &str) -> bool {
        self.in_state("disabledMcpServers", name)
    }

    fn project_status(&self, name: &str) -> McpStatus {
        if self.in_settings("disabledMcpjsonServers", name)
            || self.in_state("disabledMcpjsonServers", name)
            || self.disabled_in_state(name)
        {
            return McpStatus::Disabled;
        }
        let all = self
            .settings
            .leaf("enableAllProjectMcpServers")
            .is_some_and(|l| l.value == Value::Bool(true) && self.counts(l.src));
        if all
            || self.in_settings("enabledMcpjsonServers", name)
            || self.in_state("enabledMcpjsonServers", name)
        {
            McpStatus::Approved
        } else {
            McpStatus::Pending
        }
    }
}
