//! Terminal rendering of findings (`audit`).

use std::fmt::Write;

use super::Paths;
use crate::finding::{Finding, Persona, Severity};
use crate::style::Style;

pub fn render(paths: &Paths, findings: &[Finding], persona: Persona, style: Style) -> String {
    let mut out = String::new();
    let shown: Vec<&Finding> = findings.iter().filter(|f| f.visible(persona)).collect();
    for f in &shown {
        let severity = format!("{:<6}", f.rule.severity.name());
        let _ = writeln!(
            out,
            "{}  {}  {}",
            style.severity(f.rule.severity, &severity),
            style.bold(f.rule.id),
            f.detail
        );
        if let Some(path) = &f.path {
            let key = f.key.as_deref().filter(|k| !k.is_empty());
            let at = key.map(|k| format!("  {k}")).unwrap_or_default();
            let _ = writeln!(
                out,
                "        {}",
                style.dim(&format!("at {}{at}", paths.display(path)))
            );
        }
        let _ = writeln!(out, "        {}", f.rule.summary);
        let _ = writeln!(out, "        {} {}\n", style.green("fix:"), f.rule.fix);
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
            let _ = write!(
                out,
                ", {}",
                style.severity(sev, &format!("{n} {}", sev.name()))
            );
        }
    }
    if hidden > 0 {
        let _ = write!(out, " ({hidden} hidden, --persona auditor shows all)");
    }
    out.push('\n');
    out
}
