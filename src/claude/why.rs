//! Terminal rendering of a permission decision (`why`).

use std::fmt::Write;

use serde_json::Value;

use super::decide::{Decision, RuleMatch, Verdict, decide};
use super::permission::ToolCall;
use super::resolve::Resolved;
use crate::merge::SrcId;
use crate::style::Style;

pub fn render(r: &Resolved, input: &str, style: Style) -> String {
    let call = ToolCall::parse(input);
    let v = decide(r, &call);
    let mut out = String::new();
    let _ = writeln!(out, "{}", style.bold(input));

    match v.decision {
        Decision::NoRule => {
            let _ = writeln!(
                out,
                "  {} {}",
                style.yellow("no rule matches:"),
                mode_outcome(r, &call)
            );
            if !v.uncovered.is_empty() {
                let _ = writeln!(
                    out,
                    "  not covered by an allow rule: {}",
                    v.uncovered.join(", ")
                );
            }
        }
        decision => {
            let name = decision.name();
            let name = match decision {
                Decision::Deny => style.red(name),
                Decision::Ask => style.yellow(name),
                _ => style.green(name),
            };
            let _ = writeln!(out, "  {name}");
            for m in &v.matches {
                let _ = writeln!(out, "    {}", rule_line(r, m));
            }
        }
    }
    after_trust(r, &v, style, &mut out);
    out
}

fn rule_line(r: &Resolved, m: &RuleMatch) -> String {
    let part = m
        .part
        .as_ref()
        .map(|p| format!("  for: {p}"))
        .unwrap_or_default();
    format!("{}  {}{part}", m.rule, source_label(r, m.src))
}

fn source_label(r: &Resolved, src: SrcId) -> String {
    let source = &r.loaded.sources[src];
    format!("{} {}", source.scope.name(), r.paths.display(&source.path))
}

fn after_trust(r: &Resolved, v: &Verdict, style: Style, out: &mut String) {
    if v.after_trust.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "  {}",
        style.yellow("after workspace trust, these repo rules would also apply:")
    );
    for m in &v.after_trust {
        let _ = writeln!(
            out,
            "    {} ({})  {}",
            m.rule,
            m.list,
            source_label(r, m.src)
        );
    }
}

/// What happens when no rule matches, by permission mode.
fn mode_outcome(r: &Resolved, call: &ToolCall) -> String {
    let mode = r
        .settings
        .leaf("permissions.defaultMode")
        .and_then(|l| l.value.as_str())
        .unwrap_or("default");
    let sandboxed_bash = matches!(call, ToolCall::Bash(_))
        && r.settings
            .leaf("sandbox.enabled")
            .is_some_and(|l| l.value == Value::Bool(true))
        && !r
            .settings
            .leaf("sandbox.autoAllowBashIfSandboxed")
            .is_some_and(|l| l.value == Value::Bool(false));
    if sandboxed_bash {
        return "runs in the sandbox without asking (sandbox.autoAllowBashIfSandboxed)".into();
    }
    match (mode, call) {
        ("bypassPermissions", _) => "runs without asking (bypassPermissions mode)".into(),
        ("dontAsk", _) => "denied (dontAsk mode allows only pre-approved tools)".into(),
        ("acceptEdits", ToolCall::Edit(_)) => "allowed (acceptEdits mode)".into(),
        ("auto", _) => "a classifier decides (auto mode)".into(),
        (_, ToolCall::Read(_)) => {
            "the default for reads applies: inside the working directory they run without asking"
                .into()
        }
        (mode, _) => format!("Claude Code asks ({mode} mode)"),
    }
}
