use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Where the Claude Code adapter looks for configuration.
#[derive(Debug, Clone)]
pub struct Paths {
    pub home: PathBuf,
    /// `~/.claude`, or `$CLAUDE_CONFIG_DIR` when set.
    pub config_dir: PathBuf,
    /// App state with user- and local-scope MCP servers and project trust.
    pub claude_json: PathBuf,
    /// Directory holding `managed-settings.json` and `managed-mcp.json`.
    pub managed_dir: PathBuf,
    /// The project being audited (the session's working directory).
    pub project: PathBuf,
}

impl Paths {
    /// Paths for the current user, honoring `CLAUDE_CONFIG_DIR`.
    pub fn detect(project: &Path, managed_dir: Option<PathBuf>) -> Result<Self> {
        let home = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?);
        let mut paths = Self::for_home(&home, project, managed_dir)?;
        if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
            paths.config_dir = PathBuf::from(dir);
            // Unverified: assumes .claude.json moves into CLAUDE_CONFIG_DIR too.
            paths.claude_json = paths.config_dir.join(".claude.json");
        }
        Ok(paths)
    }

    /// Paths rooted at an explicit home directory (no environment lookups).
    pub fn for_home(home: &Path, project: &Path, managed_dir: Option<PathBuf>) -> Result<Self> {
        let project = project
            .canonicalize()
            .with_context(|| format!("project path {}", project.display()))?;
        Ok(Self {
            home: home.to_path_buf(),
            config_dir: home.join(".claude"),
            claude_json: home.join(".claude.json"),
            managed_dir: managed_dir.unwrap_or_else(default_managed_dir),
            project,
        })
    }

    /// Directory holding `.claude/settings.local.json`.
    ///
    /// Since v2.1.211 this is the git repository root. Outside a repo, or when
    /// the repo root is the home directory, it stays next to the project's
    /// `.claude/settings.json`.
    pub fn local_settings_root(&self) -> PathBuf {
        match git_root(&self.project) {
            Some(root) if root != self.home => root,
            _ => self.project.clone(),
        }
    }

    /// Shorten a path for display: `~` for home, `.` for the project.
    pub fn display(&self, path: &Path) -> String {
        if let Ok(rel) = path.strip_prefix(&self.project) {
            return format!("./{}", rel.display());
        }
        if let Ok(rel) = path.strip_prefix(&self.home) {
            return format!("~/{}", rel.display());
        }
        path.display().to_string()
    }
}

fn default_managed_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/ClaudeCode")
    } else {
        PathBuf::from("/etc/claude-code")
    }
}

fn git_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}
