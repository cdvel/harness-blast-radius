//! Fixture helpers: each fixture directory holds `home/`, `project/` and an
//! optional `managed/` tree. `$PROJECT` in `home/.claude.json` is replaced
//! with the copied project's absolute path.

use std::fs;
use std::path::Path;

use harness_blast_radius::claude::Paths;

pub fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), dest).unwrap();
        }
    }
}

/// Copy a fixture to a temp dir and render it with `f`.
pub fn run(fixture: &str, f: impl Fn(&Paths) -> String) -> String {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture);
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    copy_dir(&src, &root);

    let home = root.join("home");
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    let state = home.join(".claude.json");
    if state.exists() {
        let text = fs::read_to_string(&state).unwrap();
        fs::write(&state, text.replace("$PROJECT", project.to_str().unwrap())).unwrap();
    }

    let paths = Paths::for_home(&home, &project, Some(root.join("managed"))).unwrap();
    f(&paths).replace(root.to_str().unwrap(), "<tmp>")
}
