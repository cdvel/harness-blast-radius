//! Machine-readable `audit` output: JSON and SARIF 2.1.0.

use std::path::Path;

use serde_json::{Value, json};

use crate::finding::{Confidence, Finding, Severity};
use crate::locate;

/// Output format for `audit --format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Text,
    Json,
    /// SARIF 2.1.0, for GitHub code scanning
    Sarif,
}

/// Findings left out of the output, and baseline entries that matched nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct Counts {
    /// Hidden by the persona.
    pub hidden: usize,
    /// Matched by the baseline.
    pub accepted: usize,
    /// Baseline entries that match no finding.
    pub stale: usize,
}

const TOOL: &str = "harness-blast";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const RULES_URL: &str = "https://github.com/cdvel/harness-blast-radius/blob/main/docs/rules.md";

fn confidence_name(c: Confidence) -> &'static str {
    match c {
        Confidence::Low => "low",
        Confidence::Medium => "medium",
        Confidence::High => "high",
    }
}

fn line(f: &Finding) -> Option<usize> {
    locate::line(f.path.as_deref()?, f.key.as_deref()?, &f.detail)
}

/// `shown` are the findings left after the persona and any baseline.
pub fn json(project: &Path, shown: &[&Finding], counts: Counts) -> String {
    let findings: Vec<Value> = shown
        .iter()
        .map(|f| {
            json!({
                "rule": f.rule.id,
                "severity": f.rule.severity.name(),
                "confidence": confidence_name(f.rule.confidence),
                "detail": f.detail,
                "summary": f.rule.summary,
                "fix": f.rule.fix,
                "path": f.path.as_ref().map(|p| p.display().to_string()),
                "key": f.key.as_deref().filter(|k| !k.is_empty()),
                "line": line(f),
            })
        })
        .collect();
    let doc = json!({
        "tool": TOOL,
        "version": VERSION,
        "project": project.display().to_string(),
        "findings": findings,
        "hidden": counts.hidden,
        "accepted": counts.accepted,
        "stale_baseline_entries": counts.stale,
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&doc).unwrap_or_default()
    )
}

fn level(s: Severity) -> &'static str {
    match s {
        Severity::High => "error",
        Severity::Medium => "warning",
        Severity::Low => "note",
    }
}

/// GitHub code scanning orders alerts by this score.
fn security_severity(s: Severity) -> &'static str {
    match s {
        Severity::High => "8.0",
        Severity::Medium => "5.0",
        Severity::Low => "2.0",
    }
}

pub fn sarif(project: &Path, shown: &[&Finding]) -> String {
    let mut rules: Vec<&crate::finding::Rule> = Vec::new();
    for f in shown {
        if !rules.iter().any(|r| r.id == f.rule.id) {
            rules.push(f.rule);
        }
    }
    let rule_entries: Vec<Value> = rules
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "shortDescription": { "text": r.summary },
                "help": { "text": format!("Fix: {}", r.fix) },
                "helpUri": RULES_URL,
                "defaultConfiguration": { "level": level(r.severity) },
                "properties": {
                    "security-severity": security_severity(r.severity),
                    "precision": confidence_name(r.confidence),
                    "tags": ["security"],
                },
            })
        })
        .collect();
    let results: Vec<Value> = shown
        .iter()
        .map(|f| {
            let mut result = json!({
                "ruleId": f.rule.id,
                "ruleIndex": rules.iter().position(|r| r.id == f.rule.id),
                "level": level(f.rule.severity),
                "message": { "text": format!("{}: {}", f.rule.summary, f.detail) },
            });
            if let Some(path) = &f.path {
                // Files inside the project are relative so code scanning can
                // place them; user and managed files keep an absolute URI.
                let artifact = match path.strip_prefix(project) {
                    Ok(rel) => json!({ "uri": uri_path(rel), "uriBaseId": "%SRCROOT%" }),
                    Err(_) => json!({ "uri": format!("file://{}", uri_path(path)) }),
                };
                let mut physical = json!({ "artifactLocation": artifact });
                if let Some(n) = line(f) {
                    physical["region"] = json!({ "startLine": n });
                }
                result["locations"] = json!([{ "physicalLocation": physical }]);
            }
            result
        })
        .collect();
    let doc = json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": {
                "name": TOOL,
                "version": VERSION,
                "informationUri": "https://github.com/cdvel/harness-blast-radius",
                "rules": rule_entries,
            }},
            "originalUriBaseIds": {
                "%SRCROOT%": { "uri": format!("file://{}/", uri_path(project)) },
            },
            "results": results,
        }],
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&doc).unwrap_or_default()
    )
}

/// Path as a URI path: `/` separators, with characters outside the
/// unreserved set and `/` percent-encoded.
fn uri_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_path_encodes_spaces() {
        assert_eq!(
            uri_path(Path::new("/Library/Application Support/x.json")),
            "/Library/Application%20Support/x.json"
        );
    }
}
