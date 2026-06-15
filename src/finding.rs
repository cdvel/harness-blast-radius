//! Findings shared by every harness adapter.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Medium,
    High,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Low => "low",
            Severity::Medium => "medium",
            Severity::High => "high",
        }
    }

    /// Process exit code when this is the highest severity found.
    pub fn exit_code(self) -> i32 {
        match self {
            Severity::Low => 11,
            Severity::Medium => 12,
            Severity::High => 13,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// Which audience a rule is meant for, from least to most output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub enum Persona {
    /// Findings worth acting on.
    Regular,
    /// Adds weaker signals and missing hardening.
    Pedantic,
    /// Everything, including low-confidence findings.
    Auditor,
}

/// Static description of a rule.
#[derive(Debug)]
pub struct Rule {
    pub id: &'static str,
    pub severity: Severity,
    pub confidence: Confidence,
    pub persona: Persona,
    pub summary: &'static str,
    pub fix: &'static str,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub rule: &'static Rule,
    /// What was found, e.g. the rule text or setting value.
    pub detail: String,
    /// File the value came from, when there is one.
    pub path: Option<PathBuf>,
    /// Dotted settings key or other location inside the file.
    pub key: Option<String>,
}

impl Finding {
    pub fn new(rule: &'static Rule, detail: impl Into<String>) -> Self {
        Self {
            rule,
            detail: detail.into(),
            path: None,
            key: None,
        }
    }

    pub fn at(mut self, path: impl Into<PathBuf>, key: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self.key = Some(key.into());
        self
    }

    /// Shown for this persona: regular hides low-confidence and pedantic rules.
    pub fn visible(&self, persona: Persona) -> bool {
        match persona {
            Persona::Auditor => true,
            Persona::Pedantic => self.rule.confidence > Confidence::Low,
            Persona::Regular => {
                self.rule.persona == Persona::Regular && self.rule.confidence > Confidence::Low
            }
        }
    }
}
