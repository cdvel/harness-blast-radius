//! Accepted findings (`audit --baseline`), so CI fails only on new ones.
//!
//! The file is only read when passed explicitly. `audit` never picks one up
//! from the project, or a repository could ship a baseline that hides its own
//! hooks from the person deciding whether to trust it.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::finding::{AFTER_TRUST, Finding};

#[derive(Debug, PartialEq, Eq)]
struct Entry {
    rule: String,
    path: Option<String>,
    key: Option<String>,
    detail: String,
}

/// How a finding is identified across machines: the path as `./` (project)
/// or `~/` (home), and the detail without the trust note, which differs
/// between a trusted checkout and CI.
fn entry(f: &Finding, display: &dyn Fn(&Path) -> String) -> Entry {
    Entry {
        rule: f.rule.id.to_string(),
        path: f.path.as_deref().map(display),
        key: f.key.clone().filter(|k| !k.is_empty()),
        detail: f
            .detail
            .strip_suffix(AFTER_TRUST)
            .unwrap_or(&f.detail)
            .to_string(),
    }
}

#[derive(Debug)]
pub struct Baseline {
    entries: Vec<Entry>,
}

/// Result of applying a baseline to the shown findings.
pub struct Applied<'a> {
    pub kept: Vec<&'a Finding>,
    pub accepted: usize,
    /// Entries that match no finding, so they can be removed.
    pub stale: usize,
}

impl Baseline {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading baseline {}", path.display()))?;
        let doc: Value = serde_json::from_str(&text)
            .with_context(|| format!("parsing baseline {}", path.display()))?;
        let Some(items) = doc.get("accepted").and_then(Value::as_array) else {
            bail!("baseline {} has no \"accepted\" list", path.display());
        };
        let field = |v: &Value, name: &str| v.get(name).and_then(Value::as_str).map(String::from);
        let mut entries = Vec::new();
        for (i, v) in items.iter().enumerate() {
            let (Some(rule), Some(detail)) = (field(v, "rule"), field(v, "detail")) else {
                bail!(
                    "baseline {}: entry {} needs \"rule\" and \"detail\"",
                    path.display(),
                    i + 1
                );
            };
            entries.push(Entry {
                rule,
                path: field(v, "path"),
                key: field(v, "key"),
                detail,
            });
        }
        Ok(Self { entries })
    }

    pub fn apply<'a>(
        &self,
        shown: Vec<&'a Finding>,
        display: &dyn Fn(&Path) -> String,
    ) -> Applied<'a> {
        let mut used = vec![false; self.entries.len()];
        let mut kept = Vec::new();
        let mut accepted = 0;
        for f in shown {
            let e = entry(f, display);
            match self.entries.iter().position(|b| *b == e) {
                Some(i) => {
                    used[i] = true;
                    accepted += 1;
                }
                None => kept.push(f),
            }
        }
        let stale = used.iter().filter(|u| !**u).count();
        Applied {
            kept,
            accepted,
            stale,
        }
    }
}

/// A baseline accepting every finding in `shown`, with an empty `reason` for
/// each to fill in.
pub fn write(shown: &[&Finding], display: &dyn Fn(&Path) -> String) -> String {
    let accepted: Vec<Value> = shown
        .iter()
        .map(|f| {
            let e = entry(f, display);
            json!({
                "rule": e.rule,
                "path": e.path,
                "key": e.key,
                "detail": e.detail,
                "reason": "",
            })
        })
        .collect();
    let doc = json!({ "version": 1, "accepted": accepted });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&doc).unwrap_or_default()
    )
}
