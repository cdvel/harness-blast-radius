mod common;

use harness_blast_radius::claude;
use harness_blast_radius::finding::{Finding, Persona};
use harness_blast_radius::report::{self, Counts};
use harness_blast_radius::style::Style;

fn audit(fixture: &str, persona: Persona) -> String {
    common::run(fixture, |paths| {
        let resolved = claude::resolve(paths.clone());
        let findings = claude::rules::check(&resolved);
        let shown: Vec<&Finding> = findings.iter().filter(|f| f.visible(persona)).collect();
        let counts = Counts {
            hidden: findings.len() - shown.len(),
            ..Counts::default()
        };
        claude::audit::render(&resolved.paths, &shown, counts, Style::PLAIN)
    })
}

fn machine(fixture: &str, sarif: bool) -> String {
    common::run(fixture, |paths| {
        let resolved = claude::resolve(paths.clone());
        let findings = claude::rules::check(&resolved);
        let shown: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.visible(Persona::Regular))
            .collect();
        let project = &resolved.paths.project;
        if sarif {
            report::sarif(project, &shown)
        } else {
            let counts = Counts {
                hidden: findings.len() - shown.len(),
                ..Counts::default()
            };
            report::json(project, &shown, counts)
        }
    })
    .replace(env!("CARGO_PKG_VERSION"), "<version>")
}

#[test]
fn risky_json() {
    insta::assert_snapshot!(machine("risky", false));
}

#[test]
fn risky_sarif() {
    insta::assert_snapshot!(machine("risky", true));
}

#[test]
fn risky_regular() {
    insta::assert_snapshot!(audit("risky", Persona::Regular));
}

#[test]
fn risky_auditor() {
    insta::assert_snapshot!(audit("risky", Persona::Auditor));
}

#[test]
fn merge_regular() {
    insta::assert_snapshot!(audit("merge", Persona::Regular));
}

#[test]
fn ignored_regular() {
    insta::assert_snapshot!(audit("ignored", Persona::Regular));
}

#[test]
fn mcp_auditor() {
    insta::assert_snapshot!(audit("mcp", Persona::Auditor));
}

#[test]
fn sandbox_auditor() {
    insta::assert_snapshot!(audit("sandbox", Persona::Auditor));
}

#[cfg(unix)]
#[test]
fn readable_state_files() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().canonicalize().unwrap();
    let config = home.join(".claude");
    let project = home.join("project");
    for dir in [
        &config,
        &config.join("file-history"),
        &config.join("sessions"),
        &project,
    ] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(config.join("history.jsonl"), "{}").unwrap();
    fs::write(config.join(".credentials.json"), "{}").unwrap();
    let chmod =
        |p: &std::path::Path, m| fs::set_permissions(p, fs::Permissions::from_mode(m)).unwrap();
    chmod(&home, 0o711);
    chmod(&config, 0o755);
    chmod(&config.join("history.jsonl"), 0o644);
    chmod(&config.join(".credentials.json"), 0o600);
    chmod(&config.join("file-history"), 0o755);
    chmod(&config.join("sessions"), 0o700);

    let paths = claude::Paths::for_home(&home, &project, Some(home.join("managed"))).unwrap();
    let resolved = claude::resolve(paths);
    let ids: Vec<(&str, String)> = claude::rules::check(&resolved)
        .into_iter()
        .filter(|f| f.rule.id.starts_with("state-"))
        .map(|f| (f.rule.id, f.detail))
        .collect();
    assert_eq!(
        ids,
        vec![
            ("state-file-readable", "~/.claude/history.jsonl".to_string()),
            ("state-dir-readable", "file-history".to_string()),
        ]
    );
}

#[test]
fn baseline_accepts_known_findings() {
    use harness_blast_radius::baseline::{self, Baseline};

    let out = common::run("risky", |paths| {
        let resolved = claude::resolve(paths.clone());
        let findings = claude::rules::check(&resolved);
        let shown: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.visible(Persona::Regular))
            .collect();
        let display = |p: &std::path::Path| resolved.paths.display(p);

        // Accept everything except the repo hooks, and add an entry that matches nothing.
        let written: serde_json::Value =
            serde_json::from_str(&baseline::write(&shown, &display)).unwrap();
        let mut entries: Vec<serde_json::Value> = written["accepted"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["rule"] != "repo-hook")
            .cloned()
            .collect();
        assert!(
            entries
                .iter()
                .all(|e| !e["detail"].as_str().unwrap().contains("workspace trust")),
            "trust note must not be part of the baseline"
        );
        entries.push(serde_json::json!({ "rule": "repo-hook", "detail": "gone" }));
        let file = resolved.paths.project.join("baseline.json");
        std::fs::write(
            &file,
            serde_json::json!({ "accepted": entries }).to_string(),
        )
        .unwrap();

        let applied = Baseline::load(&file).unwrap().apply(shown, &display);
        let kept: Vec<&str> = applied.kept.iter().map(|f| f.rule.id).collect();
        format!(
            "kept {kept:?}\naccepted {}\nstale {}",
            applied.accepted, applied.stale
        )
    });
    assert_eq!(
        out,
        "kept [\"repo-hook\", \"repo-hook\"]\naccepted 15\nstale 1"
    );
}
