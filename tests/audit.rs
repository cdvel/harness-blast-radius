mod common;

use harness_blast_radius::claude;
use harness_blast_radius::finding::Persona;

fn audit(fixture: &str, persona: Persona) -> String {
    common::run(fixture, |paths| {
        let resolved = claude::resolve(paths.clone());
        claude::audit::render(&resolved.paths, &claude::rules::check(&resolved), persona)
    })
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
