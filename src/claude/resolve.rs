//! Resolve loaded files into the effective Claude Code configuration.

use serde_json::Value;

use super::Paths;
use super::load::{Kind, Loaded, Scope, load};
use crate::merge::{Node, SrcId};

/// Settings only honored in managed policy.
const MANAGED_ONLY: &[&str] = &[
    "allowManagedHooksOnly",
    "allowManagedPermissionRulesOnly",
    "allowManagedMcpServersOnly",
    "managedMcpServers",
    "policyHelper",
    "strictKnownMarketplaces",
];

/// Repo-controlled settings that apply only after the workspace trust dialog.
/// Deny and ask rules apply immediately.
const AFTER_TRUST: &[&str] = &[
    "permissions.allow",
    "permissions.additionalDirectories",
    "extraKnownMarketplaces",
    "env",
    "hooks",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    Accepted,
    NotAccepted,
    /// No entry for this project in `~/.claude.json`.
    Unknown,
}

/// A value present in a file that Claude Code does not apply.
#[derive(Debug, Clone)]
pub struct Dropped {
    pub key: &'static str,
    pub value: Value,
    pub src: SrcId,
    pub reason: &'static str,
}

/// Whether a value from a given source takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Active,
    /// Repo-controlled value waiting for workspace trust.
    AfterTrust,
    Ignored(&'static str),
}

#[derive(Debug)]
pub struct Resolved {
    pub paths: Paths,
    pub loaded: Loaded,
    /// Merged settings across all scopes, with provenance.
    pub settings: Node,
    pub dropped: Vec<Dropped>,
    pub trust: Trust,
}

pub fn resolve(paths: Paths) -> Resolved {
    let loaded = load(&paths);
    let mut settings = Node::empty();
    let mut dropped = Vec::new();

    for (id, source, doc) in loaded.of_kind(Kind::Settings) {
        let mut doc = doc.clone();
        drop_unsupported(source.scope, id, &mut doc, &mut dropped);
        settings.merge(&doc, id);
    }

    let trust = trust_state(&paths, &loaded);
    Resolved {
        paths,
        loaded,
        settings,
        dropped,
        trust,
    }
}

impl Resolved {
    pub fn scope(&self, src: SrcId) -> Scope {
        self.loaded.sources[src].scope
    }

    fn managed_flag(&self, key: &str) -> bool {
        self.settings
            .leaf(key)
            .is_some_and(|l| l.value == Value::Bool(true))
    }

    /// Status of a value under `key` (dotted path) contributed by `src`.
    pub fn status(&self, src: SrcId, key: &str) -> Status {
        let scope = self.scope(src);
        if key.starts_with("hooks") && scope != Scope::Managed {
            if self.managed_flag("allowManagedHooksOnly") {
                return Status::Ignored("managed policy allows managed hooks only");
            }
            if self.managed_flag("disableAllHooks") {
                return Status::Ignored("disableAllHooks is set");
            }
        }
        if key.starts_with("permissions.")
            && scope != Scope::Managed
            && self.managed_flag("allowManagedPermissionRulesOnly")
        {
            return Status::Ignored("managed policy allows managed permission rules only");
        }
        if scope.is_repo()
            && self.trust != Trust::Accepted
            && AFTER_TRUST.iter().any(|k| key.starts_with(k))
        {
            return Status::AfterTrust;
        }
        Status::Active
    }

    /// Best status across every source that listed the same value.
    pub fn best_status(&self, srcs: &[SrcId], key: &str) -> Status {
        let statuses: Vec<Status> = srcs.iter().map(|&s| self.status(s, key)).collect();
        for want in [Status::Active, Status::AfterTrust] {
            if statuses.contains(&want) {
                return want;
            }
        }
        statuses.first().copied().unwrap_or(Status::Active)
    }
}

/// Remove values Claude Code ignores for this scope, recording why.
fn drop_unsupported(scope: Scope, src: SrcId, doc: &mut Value, dropped: &mut Vec<Dropped>) {
    let Value::Object(map) = doc else { return };

    if scope != Scope::Managed {
        for &key in MANAGED_ONLY {
            if let Some(value) = map.remove(key) {
                dropped.push(Dropped {
                    key,
                    value,
                    src,
                    reason: "only read from managed settings",
                });
            }
        }
    }
    if scope.is_repo() {
        if let Some(value) = map.remove("processWrapper") {
            dropped.push(Dropped {
                key: "processWrapper",
                value,
                src,
                reason: "only read from user or managed settings",
            });
        }
        // Since v2.1.257; earlier versions accepted bypassPermissions here.
        if let Some(Value::Object(perms)) = map.get_mut("permissions") {
            let mode = perms.get("defaultMode").and_then(Value::as_str);
            if matches!(mode, Some("auto" | "bypassPermissions")) {
                let value = perms.remove("defaultMode").unwrap_or_default();
                dropped.push(Dropped {
                    key: "permissions.defaultMode",
                    value,
                    src,
                    reason: "project and local files cannot set this mode",
                });
            }
        }
    }
}

/// The `~/.claude.json` entry for this project, keyed by absolute path.
pub fn project_entry<'a>(paths: &Paths, loaded: &'a Loaded) -> Option<&'a Value> {
    let projects = projects(loaded)?;
    [paths.project.clone(), paths.local_settings_root()]
        .iter()
        .find_map(|p| projects.get(p.to_str()?))
}

fn projects(loaded: &Loaded) -> Option<&Value> {
    loaded.first(Kind::ClaudeJson)?.1.get("projects")
}

/// Trust accepted for the project or any parent directory.
///
/// Unverified: assumes trusting a folder also trusts its subfolders.
fn trust_state(paths: &Paths, loaded: &Loaded) -> Trust {
    let Some(projects) = projects(loaded) else {
        return Trust::Unknown;
    };
    let mut seen = false;
    for dir in paths.project.ancestors() {
        match dir
            .to_str()
            .and_then(|d| projects.get(d)?.get("hasTrustDialogAccepted"))
        {
            Some(Value::Bool(true)) => return Trust::Accepted,
            Some(_) => seen = true,
            None => {}
        }
    }
    if seen {
        Trust::NotAccepted
    } else {
        Trust::Unknown
    }
}
