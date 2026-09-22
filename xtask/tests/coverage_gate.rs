//! Coverage gate evaluation tests on synthetic llvm-cov JSON summaries.

use std::path::Path;

use xtask::coverage::{Target, evaluate};

const TARGETS: &[Target] = &[
    Target { name: "core", prefixes: &["crates/drift-core/src/"], min_percent: 85.0 },
    Target { name: "rdp-redirect", prefixes: &["crates/drift-rdp/src/redirect", "crates/drift-rdp/src/rdstls"], min_percent: 85.0 },
    Target { name: "empty", prefixes: &["crates/drift-input/src/"], min_percent: 85.0 },
];

fn file(name: &str, count: u64, covered: u64) -> String {
    format!(r#"{{"filename":"/repo/{name}","summary":{{"lines":{{"count":{count},"covered":{covered},"percent":0}}}}}}"#)
}

fn summary(files: &[String]) -> String {
    format!(r#"{{"type":"llvm.coverage.json.export","version":"2.0.1","data":[{{"files":[{}],"totals":{{}}}}]}}"#, files.join(","))
}

#[test]
fn aggregates_per_target_and_applies_threshold() {
    let json = summary(&[
        file("crates/drift-core/src/a.rs", 100, 90),
        file("crates/drift-core/src/b.rs", 100, 70),
        file("crates/drift-rdp/src/redirect.rs", 50, 50),
        file("crates/drift-rdp/src/rdstls/mod.rs", 50, 40),
        file("crates/drift-rdp/src/actor.rs", 1000, 0),
        file("xtask/src/lib.rs", 10, 0),
    ]);
    let r = evaluate(&json, Path::new("/repo"), TARGETS).unwrap();
    assert_eq!(r.len(), 3);

    assert_eq!((r[0].lines, r[0].covered), (200, 160));
    assert!((r[0].percent() - 80.0).abs() < 1e-9);
    assert!(r[0].enforced());
    assert!(!r[0].passed());

    assert_eq!((r[1].lines, r[1].covered), (100, 90));
    assert!(r[1].passed());

    assert_eq!(r[2].lines, 0);
    assert!(!r[2].enforced());
    assert!(r[2].passed(), "targets without code are not yet enforced");
    assert!((r[2].percent() - 100.0).abs() < 1e-9);
}

#[test]
fn rejects_malformed_json() {
    assert!(evaluate("not json", Path::new("/repo"), TARGETS).is_err());
    assert!(evaluate(r#"{"data":"x"}"#, Path::new("/repo"), TARGETS).is_err());
}
