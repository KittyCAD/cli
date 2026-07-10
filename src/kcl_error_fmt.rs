pub(crate) fn into_miette(error: kcl_lib::KclErrorWithOutputs, code: &str) -> anyhow::Error {
    let inner_err = &error.error;
    if is_nonsense_source_range(inner_err) {
        return anyhow::anyhow!("{}", inner_err.get_message());
    }
    let report = error.clone().into_miette_report_with_outputs(code).unwrap();
    let report = miette::Report::new(report);
    anyhow::anyhow!("{report:?}")
}

/// Sometimes the KCL runtime doesn't have a good source range, because some error
/// is not really about a single KCL function. For example, missing auth or not
/// being able to connect to the engine at all.
///
/// This detects those circumstances. If true, you probably shouldn't format the error
/// with miette + KCL source, because the error isn't really about any particular KCL line.
fn is_nonsense_source_range(error: &kcl_lib::KclError) -> bool {
    error.source_ranges().is_empty() || error.source_ranges() == vec![Default::default()]
}

pub(crate) fn into_miette_for_parse(filename: &str, input: &str, error: kcl_lib::KclError) -> anyhow::Error {
    let report = kcl_lib::Report {
        kcl_source: input.to_string(),
        error,
        filename: filename.to_string(),
    };
    let report = miette::Report::new(report);
    anyhow::anyhow!("{report:?}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KclIssueCheck {
    DenyErrors,
    AllowErrors,
    Ignore,
}

impl KclIssueCheck {
    pub(crate) fn from_allow_errors(allow_errors: bool) -> Self {
        if allow_errors {
            Self::AllowErrors
        } else {
            Self::DenyErrors
        }
    }
}

pub(crate) fn check_exec_state_issues(
    err_out: &mut impl std::io::Write,
    filename: &str,
    code: &str,
    state: &kcl_lib::ExecState,
    issue_check: KclIssueCheck,
) -> anyhow::Result<()> {
    dbg!();
    check_compilation_issues(err_out, filename, code, state.issues(), issue_check)
}

pub(crate) fn check_compilation_issues(
    err_out: &mut impl std::io::Write,
    filename: &str,
    code: &str,
    issues: &[kcl_lib::CompilationIssue],
    issue_check: KclIssueCheck,
) -> anyhow::Result<()> {
    dbg!();
    if issue_check == KclIssueCheck::Ignore || issues.is_empty() {
        return Ok(());
    }

    for (i, issue) in issues.iter().enumerate() {
        if i > 0 {
            writeln!(err_out)?;
        }
        writeln!(
            err_out,
            "{}",
            kcl_lib::render_compilation_issue_miette(filename, code, issue.clone())
        )?;
    }

    if issue_check == KclIssueCheck::DenyErrors && issues.iter().any(|issue| issue.is_err()) {
        anyhow::bail!(
            "KCL execution reported errors. Please fix your KCL program before continuing. If you really want to proceed anyway, rerun this command with `--allow-errors`."
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_errors_prints_all_issues_and_returns_guidance() {
        let issues = vec![
            kcl_lib::CompilationIssue::err(kcl_lib::SourceRange::default(), "first issue"),
            kcl_lib::CompilationIssue::fatal(kcl_lib::SourceRange::default(), "second issue"),
        ];
        let mut err_out = Vec::new();

        let err = check_compilation_issues(&mut err_out, "main.kcl", "x = 1\n", &issues, KclIssueCheck::DenyErrors)
            .unwrap_err();
        let stderr = String::from_utf8(err_out).unwrap();

        assert!(stderr.contains("first issue"), "{stderr}");
        assert!(stderr.contains("second issue"), "{stderr}");
        assert!(err.to_string().contains("--allow-errors"), "{err}");
    }

    #[test]
    fn allow_errors_prints_issues_and_continues() {
        let issues = vec![kcl_lib::CompilationIssue::err(
            kcl_lib::SourceRange::default(),
            "bad but allowed",
        )];
        let mut err_out = Vec::new();

        check_compilation_issues(&mut err_out, "main.kcl", "x = 1\n", &issues, KclIssueCheck::AllowErrors).unwrap();
        let stderr = String::from_utf8(err_out).unwrap();

        assert!(stderr.contains("bad but allowed"), "{stderr}");
    }
}
