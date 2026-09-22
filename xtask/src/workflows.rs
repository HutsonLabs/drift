//! Audit of `.github/workflows/` against the plan.
//!
//! The plan names three automated runs, and a missing workflow file is invisible otherwise:
//!
//! - plan M0-6: a per-PR workflow running `cargo xtask ci` on a macOS arm64 runner;
//! - plan §5.3: "Nightly e2e runs on a self-hosted macOS runner with SSH access to the host";
//! - plan M9-2: nightly fuzzing.
//!
//! The audit is deliberately textual (the workflows are short and hand-written) and pure, so
//! `cargo xtask ci` can run it in-process over the real directory.

/// Something a required workflow does not do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The workflow file name, e.g. `e2e-nightly.yml`.
    pub workflow: String,
    /// What is wrong, and which part of the plan asks for it.
    pub message: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.workflow, self.message)
    }
}

/// One required workflow and the text it must contain.
#[derive(Debug, Clone, Copy)]
pub struct Required {
    /// File name inside `.github/workflows/`.
    pub file: &'static str,
    /// Where the plan asks for it.
    pub plan: &'static str,
    /// Substrings the file must contain, each with the reason it is needed.
    pub must_contain: &'static [(&'static str, &'static str)],
}

/// The workflows the plan requires.
pub const REQUIRED: &[Required] = &[
    Required {
        file: "ci.yml",
        plan: "plan M0-6",
        must_contain: &[
            ("cargo xtask ci", "the merge gate runs `cargo xtask ci`"),
            ("macos-15", "plan §5.3: GitHub Actions `macos-15` arm64"),
            ("pull_request", "the gate runs on every PR"),
        ],
    },
    Required {
        file: "e2e-nightly.yml",
        plan: "plan §5.3",
        must_contain: &[
            ("cargo xtask e2e", "the nightly e2e suite runs through `cargo xtask e2e`"),
            ("schedule:", "plan §5.3: the e2e suite runs nightly"),
            ("cron:", "plan §5.3: the e2e suite runs nightly"),
            ("self-hosted", "plan §5.3: a self-hosted runner with SSH access to the GNOME host"),
            ("macOS", "plan §5.3: the runner is a macOS machine"),
            ("DRIFT_E2E_SYS_PASS", "plan §5.3: credentials come from `DRIFT_E2E_*` variables"),
            ("cargo xtask host-setup-check", "plan M0-4: the host is verified before the suite runs"),
        ],
    },
    Required {
        file: "fuzz-nightly.yml",
        plan: "plan M9-2",
        must_contain: &[
            ("cargo +nightly fuzz run", "plan M1-2/M9-2: nightly fuzzing"),
            ("schedule:", "plan M9-2: fuzzing runs nightly"),
        ],
    },
];

/// Audits `(file name, contents)` pairs against [`REQUIRED`].
pub fn audit(files: &[(String, String)]) -> Vec<Problem> {
    audit_against(files, REQUIRED)
}

/// Audits `(file name, contents)` pairs against an explicit requirement list.
pub fn audit_against(files: &[(String, String)], required: &[Required]) -> Vec<Problem> {
    let mut problems = Vec::new();
    for req in required {
        let Some((_, text)) = files.iter().find(|(name, _)| name == req.file) else {
            problems.push(Problem {
                workflow: req.file.to_owned(),
                message: format!("missing from .github/workflows/ ({})", req.plan),
            });
            continue;
        };
        for (needle, why) in req.must_contain {
            if !text.contains(needle) {
                problems.push(Problem {
                    workflow: req.file.to_owned(),
                    message: format!("does not mention `{needle}` — {why}"),
                });
            }
        }
    }
    problems
}
