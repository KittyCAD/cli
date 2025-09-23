pub(crate) fn into_miette(error: kcl_lib::KclErrorWithOutputs, code: &str) -> anyhow::Error {
    let report = error.clone().into_miette_report_with_outputs(code).unwrap();
    let report = miette::Report::new(report);
    anyhow::anyhow!("{report:?}")
}

pub(crate) fn into_miette_for_parse(filename: &str, input: &str, error: kcl_lib::KclError) -> anyhow::Error {
    let report = kcl_lib::Report {
        kcl_source: input.to_string(),
        error: error.clone(),
        filename: filename.to_string(),
    };
    let report = miette::Report::new(report);
    anyhow::anyhow!("{report:?}")
}
