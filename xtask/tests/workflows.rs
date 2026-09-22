//! Red (M0-6, §5.3): the repository's GitHub Actions workflows match what the plan requires,
//! including the nightly e2e run on a self-hosted macOS runner.

use xtask::workflows::{self, Required};

fn repo_workflows() -> Vec<(String, String)> {
    let dir = xtask::repo::root().join(".github/workflows");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("reading {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.extension().is_some_and(|e| e == "yml" || e == "yaml") {
            let name = path.file_name().expect("file name").to_string_lossy().into_owned();
            out.push((name, std::fs::read_to_string(&path).expect("workflow")));
        }
    }
    out
}

#[test]
fn the_repository_has_every_workflow_the_plan_requires() {
    let problems = workflows::audit(&repo_workflows());
    assert!(
        problems.is_empty(),
        "workflow gaps:\n{}",
        problems.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn a_missing_workflow_is_reported_once() {
    let problems = workflows::audit(&[]);
    for req in workflows::REQUIRED {
        let hits: Vec<_> = problems.iter().filter(|p| p.workflow == req.file).collect();
        assert_eq!(hits.len(), 1, "{} should be reported exactly once: {hits:?}", req.file);
        assert!(hits[0].message.contains("missing"), "{:?}", hits[0]);
    }
}

#[test]
fn a_workflow_that_misses_a_requirement_is_reported() {
    let required = [Required {
        file: "e2e-nightly.yml",
        plan: "plan §5.3",
        must_contain: &[("self-hosted", "a self-hosted runner"), ("cron:", "nightly")],
    }];
    let files = [("e2e-nightly.yml".to_owned(), "runs-on: macos-15\non:\n  workflow_dispatch:\n".to_owned())];
    let problems = workflows::audit_against(&files, &required);
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems.iter().all(|p| p.workflow == "e2e-nightly.yml"));
    assert!(problems[0].message.contains("self-hosted"), "{:?}", problems[0]);
    assert!(problems[0].message.contains("a self-hosted runner"), "reason is explained: {:?}", problems[0]);
}

#[test]
fn a_complete_workflow_set_is_accepted() {
    let files: Vec<(String, String)> = workflows::REQUIRED
        .iter()
        .map(|r| {
            let body = r.must_contain.iter().map(|(n, _)| *n).collect::<Vec<_>>().join("\n");
            (r.file.to_owned(), body)
        })
        .collect();
    assert_eq!(workflows::audit(&files), Vec::new());
}

/// Plan §5.3: the nightly run reaches the GNOME host over SSH, so it cannot use a hosted runner.
#[test]
fn the_e2e_workflow_is_the_one_that_needs_ssh_access() {
    let req = workflows::REQUIRED
        .iter()
        .find(|r| r.file == "e2e-nightly.yml")
        .expect("an e2e workflow is required (plan §5.3)");
    let needles: Vec<&str> = req.must_contain.iter().map(|(n, _)| *n).collect();
    for needle in ["cargo xtask e2e", "self-hosted", "schedule:"] {
        assert!(needles.contains(&needle), "requirement `{needle}` missing from {needles:?}");
    }
}
