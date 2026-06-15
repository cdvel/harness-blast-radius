mod common;

use harness_blast_radius::claude;

fn show(fixture: &str) -> String {
    common::run(fixture, |paths| {
        claude::show::render(&claude::resolve(paths.clone()))
    })
}

#[test]
fn merge() {
    insta::assert_snapshot!(show("merge"));
}

#[test]
fn mcp() {
    insta::assert_snapshot!(show("mcp"));
}

#[test]
fn managed() {
    insta::assert_snapshot!(show("managed"));
}

#[test]
fn ignored() {
    insta::assert_snapshot!(show("ignored"));
}
