//! Allow, ask or deny for a tool call, across the effective permission rules.
//!
//! Deny is checked first, then ask, then allow; the first match decides.
//! Bash calls are split into simple commands: a deny or ask rule matching any
//! part (including commands nested in substitutions) decides, while allow
//! needs every part to match an allow rule.

use super::permission::{PathContext, PermissionRule, ToolCall};
use super::resolve::{Resolved, Status};
use super::shell;
use crate::merge::SrcId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Deny,
    Ask,
    Allow,
    /// No rule matches; the permission mode decides.
    NoRule,
}

impl Decision {
    pub fn name(self) -> &'static str {
        match self {
            Decision::Deny => "deny",
            Decision::Ask => "ask",
            Decision::Allow => "allow",
            Decision::NoRule => "no rule",
        }
    }
}

/// A rule that matched, and the part of a Bash command it matched.
#[derive(Debug, Clone)]
pub struct RuleMatch {
    pub list: &'static str,
    pub rule: String,
    pub src: SrcId,
    pub part: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Verdict {
    pub decision: Decision,
    /// Rules that decided the outcome.
    pub matches: Vec<RuleMatch>,
    /// Bash parts no allow rule covers (when the decision is `NoRule`).
    pub uncovered: Vec<String>,
    /// Matching repo rules that would apply after workspace trust.
    pub after_trust: Vec<RuleMatch>,
}

struct Candidate {
    rule: PermissionRule,
    text: String,
    src: SrcId,
}

/// Directories path rules resolve against for this project.
pub fn path_context(r: &Resolved) -> PathContext {
    // Unverified: `/path` rules from user settings resolve against the
    // project too, the same as project and local files.
    PathContext {
        home: r.paths.home.clone(),
        cwd: r.paths.project.clone(),
        source_root: r.paths.project.clone(),
    }
}

pub fn decide(r: &Resolved, call: &ToolCall) -> Verdict {
    let ctx = path_context(r);
    let (active, pending) = rules(r);
    let mut verdict = evaluate(&active, call, &ctx);
    let later = evaluate(&pending, call, &ctx);
    verdict.after_trust = later.matches;
    verdict
}

/// Active rules per list, and repo rules waiting for workspace trust.
fn rules(r: &Resolved) -> (Lists, Lists) {
    let mut active = Lists::default();
    let mut pending = Lists::default();
    for (i, list) in LISTS.iter().enumerate() {
        let key = format!("permissions.{list}");
        for item in r.settings.items(&key) {
            let Some(text) = item.value.as_str() else {
                continue;
            };
            let Some(rule) = PermissionRule::parse(text) else {
                continue;
            };
            let status = r.best_status(&item.srcs, &key);
            let target = match status {
                Status::Active => &mut active,
                Status::AfterTrust => &mut pending,
                Status::Ignored(_) => continue,
            };
            target.0[i].push(Candidate {
                rule,
                text: text.to_string(),
                src: item.srcs[0],
            });
        }
    }
    (active, pending)
}

const LISTS: [&str; 3] = ["deny", "ask", "allow"];

#[derive(Default)]
struct Lists([Vec<Candidate>; 3]);

fn evaluate(lists: &Lists, call: &ToolCall, ctx: &PathContext) -> Verdict {
    let mut verdict = Verdict {
        decision: Decision::NoRule,
        matches: Vec::new(),
        uncovered: Vec::new(),
        after_trust: Vec::new(),
    };
    let found = |c: &Candidate, list, part: Option<&str>| RuleMatch {
        list,
        rule: c.text.clone(),
        src: c.src,
        part: part.map(str::to_string),
    };

    if let ToolCall::Bash(command) = call {
        let parts = shell::simple_commands(command, true);
        for (i, decision) in [(0, Decision::Deny), (1, Decision::Ask)] {
            for part in &parts {
                if let Some(c) = lists.0[i].iter().find(|c| c.rule.matches_command(part)) {
                    verdict.decision = decision;
                    verdict.matches.push(found(c, LISTS[i], Some(part)));
                    return verdict;
                }
            }
        }
        for part in &parts {
            match lists.0[2].iter().find(|c| c.rule.matches_command(part)) {
                Some(c) => verdict.matches.push(found(c, "allow", Some(part))),
                None => verdict.uncovered.push(part.clone()),
            }
        }
        if verdict.uncovered.is_empty() && !parts.is_empty() {
            verdict.decision = Decision::Allow;
        } else {
            verdict.matches.clear();
        }
        return verdict;
    }

    for (i, decision) in [
        (0, Decision::Deny),
        (1, Decision::Ask),
        (2, Decision::Allow),
    ] {
        if let Some(c) = lists.0[i].iter().find(|c| c.rule.matches_call(call, ctx)) {
            verdict.decision = decision;
            verdict.matches.push(found(c, LISTS[i], None));
            return verdict;
        }
    }
    verdict
}
