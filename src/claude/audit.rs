//! Terminal rendering of findings (`audit`).

use std::fmt::Write;

use super::Paths;
use crate::finding::{Finding, Persona, Severity};

pub fn render(paths: &Paths, findings: &[Finding], persona: Persona) -> String {
    let mut out = String::new();
    let shown: Vec<&Finding> = findings.iter().filter(|f| f.visible(persona)).collect();
    for f in &shown {
        let _ = writeln!(
            out,
            "{:<6}  {}  {}",
            f.rule.severity.name(),
            f.rule.id,
            f.detail
        );
        if let Some(path) = &f.path {
            let key = f.key.as_deref().filter(|k| !k.is_empty());
            let at = key.map(|k| format!("  {k}")).unwrap_or_default();
            let _ = writeln!(out, "        at {}{at}", paths.display(path));
        }
        let _ = writeln!(out, "        {}", f.rule.summary);
        let _ = writeln!(out, "        fix: {}\n", f.rule.fix);
    }
    let hidden = findings.len() - shown.len();
    let _ = write!(
        out,
        "{} finding{}",
        shown.len(),
        if shown.len() == 1 { "" } else { "s" }
    );
    for sev in [Severity::High, Severity::Medium, Severity::Low] {
        let n = shown.iter().filter(|f| f.rule.severity == sev).count();
        if n > 0 {
            let _ = write!(out, ", {n} {}", sev.name());
        }
    }
    if hidden > 0 {
        let _ = write!(out, " ({hidden} hidden, --persona auditor shows all)");
    }
    out.push('\n');
    out
}
