//! Discover and parse Claude Code config files.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::Paths;
use crate::merge::SrcId;

/// Settings scopes in ascending precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    User,
    Project,
    Local,
    Managed,
}

impl Scope {
    pub fn name(self) -> &'static str {
        match self {
            Scope::User => "user",
            Scope::Project => "project",
            Scope::Local => "local",
            Scope::Managed => "managed",
        }
    }

    /// Project and local files are controlled by the repository (local files
    /// are untracked, but live in the repo and load only for that folder).
    pub fn is_repo(self) -> bool {
        matches!(self, Scope::Project | Scope::Local)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Settings,
    /// `~/.claude.json`: app state, MCP servers, per-project trust.
    ClaudeJson,
    /// Project `.mcp.json`.
    McpJson,
    /// `managed-mcp.json`: exclusive MCP server list.
    ManagedMcp,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub scope: Scope,
    pub kind: Kind,
    pub path: PathBuf,
}

/// A file that exists but that Claude Code would not load.
#[derive(Debug, Clone)]
pub struct Ignored {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct Loaded {
    pub sources: Vec<Source>,
    /// Parsed documents, indexed in parallel with `sources`.
    pub docs: Vec<Value>,
    pub ignored: Vec<Ignored>,
}

impl Loaded {
    pub fn of_kind(&self, kind: Kind) -> impl Iterator<Item = (SrcId, &Source, &Value)> {
        self.sources
            .iter()
            .enumerate()
            .filter(move |(_, s)| s.kind == kind)
            .map(|(i, s)| (i, s, &self.docs[i]))
    }

    pub fn first(&self, kind: Kind) -> Option<(SrcId, &Value)> {
        self.of_kind(kind).next().map(|(i, _, v)| (i, v))
    }

    fn add(&mut self, scope: Scope, kind: Kind, path: PathBuf) {
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                self.ignored.push(Ignored {
                    path,
                    reason: format!("unreadable: {e}"),
                });
                return;
            }
        };
        // Strict JSON, like Claude Code: comments or trailing commas make the
        // whole file ignored.
        match serde_json::from_str::<Value>(&text) {
            Ok(v @ Value::Object(_)) => {
                self.sources.push(Source { scope, kind, path });
                self.docs.push(v);
            }
            Ok(_) => self.ignored.push(Ignored {
                path,
                reason: "top level is not a JSON object".into(),
            }),
            Err(e) => self.ignored.push(Ignored {
                path,
                reason: format!("invalid JSON: {e}"),
            }),
        }
    }
}

/// Load every config source, settings files in ascending precedence.
pub fn load(paths: &Paths) -> Loaded {
    let mut l = Loaded::default();

    l.add(
        Scope::User,
        Kind::Settings,
        paths.config_dir.join("settings.json"),
    );
    l.add(
        Scope::Project,
        Kind::Settings,
        paths.project.join(".claude/settings.json"),
    );
    l.add(
        Scope::Local,
        Kind::Settings,
        paths
            .local_settings_root()
            .join(".claude/settings.local.json"),
    );
    // managed-settings.json first, then the drop-in directory alphabetically.
    l.add(
        Scope::Managed,
        Kind::Settings,
        paths.managed_dir.join("managed-settings.json"),
    );
    for path in sorted_json_files(&paths.managed_dir.join("managed-settings.d")) {
        l.add(Scope::Managed, Kind::Settings, path);
    }

    l.add(Scope::User, Kind::ClaudeJson, paths.claude_json.clone());
    l.add(
        Scope::Project,
        Kind::McpJson,
        paths.project.join(".mcp.json"),
    );
    l.add(
        Scope::Managed,
        Kind::ManagedMcp,
        paths.managed_dir.join("managed-mcp.json"),
    );
    l
}

fn sorted_json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    files
}
