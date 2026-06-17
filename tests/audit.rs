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
