// A lot of the below is based upon https://github.com/AlexanderThaller/format_serde_error/tree/main
// which is licensed under the MIT license. Thank you!

pub(crate) fn into_miette(input: &str, error: kcl_lib::KclErrorWithOutputs) -> anyhow::Error {
    let report = error.clone().into_miette_report_with_outputs(input).unwrap();
    let report = miette::Report::new(report);
    anyhow::anyhow!("{:?}", report)
}

pub(crate) fn into_miette_for_parse(filename: &str, input: &str, error: kcl_lib::KclError) -> anyhow::Error {
    let report = kcl_lib::Report {
        kcl_source: input.to_string(),
        error: error.clone(),
        filename: filename.to_string(),
    };
    let report = miette::Report::new(report);
    anyhow::anyhow!("{:?}", report)
}
