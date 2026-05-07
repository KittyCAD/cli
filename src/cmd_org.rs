use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result};
use clap::Parser;

use crate::types::FormatOutput;

type ConversionStatus = kittycad::types::OrgDatasetFileConversionStatus;

const CAD_FILE_EXTENSIONS: &[&str] = &[
    "dwg",
    "dxf",
    "ipt",
    "iam",
    "model",
    "dlv",
    "exp",
    "session",
    "catdrawing",
    "catpart",
    "catproduct",
    "catshape",
    "cgr",
    "3dxml",
    "asm",
    "neu",
    "prt",
    "xas",
    "xpr",
    "par",
    "pwd",
    "psm",
    "sldasm",
    "sldprt",
];
const ARCHIVE_FILE_SUFFIXES: &[&str] = &["zip", "tar", "tar.gz", "tgz", "gz", "bz2", "7z", "rar"];

/// Manage Zoo organization resources.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrg {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Dataset(CmdOrgDataset),
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrg {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Dataset(cmd) => cmd.run(ctx).await,
        }
    }
}

/// Manage organization datasets.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDataset {
    #[clap(subcommand)]
    subcmd: DatasetSubCommand,
}

#[derive(Parser, Debug, Clone)]
enum DatasetSubCommand {
    Create(CmdOrgDatasetCreate),
    Delete(CmdOrgDatasetDelete),
    List(CmdOrgDatasetList),
    #[clap(name = "s3-policies")]
    S3Policies(CmdOrgDatasetS3Policies),
    SemanticSearch(CmdOrgDatasetSemanticSearch),
    Stats(CmdOrgDatasetStats),
    Upload(CmdOrgDatasetUpload),
    Update(CmdOrgDatasetUpdate),
    #[clap(alias = "get")]
    View(CmdOrgDatasetView),
    Conversions(CmdOrgDatasetConversions),
    Conversion(CmdOrgDatasetConversion),
    Retrigger(CmdOrgDatasetRetrigger),
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDataset {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            DatasetSubCommand::Create(cmd) => cmd.run(ctx).await,
            DatasetSubCommand::Delete(cmd) => cmd.run(ctx).await,
            DatasetSubCommand::List(cmd) => cmd.run(ctx).await,
            DatasetSubCommand::S3Policies(cmd) => cmd.run(ctx).await,
            DatasetSubCommand::SemanticSearch(cmd) => cmd.run(ctx).await,
            DatasetSubCommand::Stats(cmd) => cmd.run(ctx).await,
            DatasetSubCommand::Upload(cmd) => cmd.run(ctx).await,
            DatasetSubCommand::Update(cmd) => cmd.run(ctx).await,
            DatasetSubCommand::View(cmd) => cmd.run(ctx).await,
            DatasetSubCommand::Conversions(cmd) => cmd.run(ctx).await,
            DatasetSubCommand::Conversion(cmd) => cmd.run(ctx).await,
            DatasetSubCommand::Retrigger(cmd) => cmd.run(ctx).await,
        }
    }
}

/// List organization datasets.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetList {
    /// Maximum number of items returned by one API call.
    #[clap(long)]
    pub limit: Option<u32>,

    /// Token returned by a previous list call.
    #[clap(long)]
    pub page_token: Option<String>,

    /// Sort order.
    #[clap(long, value_enum)]
    pub sort_by: Option<kittycad::types::CreatedAtSortMode>,

    /// Follow pagination until every dataset is returned.
    #[clap(long)]
    pub paginate: bool,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetList {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let client = ctx.api_client("")?;
        let format = ctx.format(&self.format)?;
        if self.paginate {
            let datasets = collect_all_datasets(&client, self.limit, self.sort_by.clone()).await?;
            write_dataset_collection_output(ctx, &format, &datasets)?;
            return Ok(());
        }

        let page = client
            .orgs()
            .list_datasets(self.limit, self.page_token.clone(), self.sort_by.clone())
            .await?;
        write_dataset_page_output(ctx, &format, &page)?;
        Ok(())
    }
}

/// View an organization dataset.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetView {
    /// Dataset ID.
    #[clap(name = "dataset-id", required = true)]
    pub dataset_id: uuid::Uuid,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetView {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let client = ctx.api_client("")?;
        let dataset = client.orgs().get_dataset(self.dataset_id).await?;
        let format = ctx.format(&self.format)?;
        write_dataset_output(ctx, &format, &dataset)?;
        Ok(())
    }
}

/// Create an organization dataset.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetCreate {
    /// Display name for the dataset.
    #[clap(long, required = true)]
    pub name: String,

    /// Storage provider for the dataset.
    #[clap(long, value_enum, default_value = "zoo-managed")]
    provider: DatasetStorageProvider,

    /// Fully-qualified dataset URI, required for S3 datasets.
    #[clap(long)]
    pub uri: Option<String>,

    /// Role ARN Zoo should assume when reading an S3 dataset.
    #[clap(long)]
    pub access_role_arn: Option<String>,

    /// Files or directories to upload after creating a Zoo-managed dataset.
    #[clap(long, value_name = "PATH")]
    pub upload: Vec<PathBuf>,

    /// Recurse into upload directories.
    #[clap(long)]
    pub recursive: bool,

    /// Base directory used to derive relative upload paths.
    #[clap(long)]
    pub base_dir: Option<PathBuf>,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetCreate {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        validate_create_source(self.provider, self.uri.as_deref(), self.access_role_arn.as_deref())?;

        let upload_attachments = collect_create_dataset_upload_attachments(
            self.provider,
            &self.upload,
            self.recursive,
            self.base_dir.as_deref(),
        )?;

        let body = kittycad::types::CreateOrgDataset {
            name: self.name.clone(),
            source: kittycad::types::OrgDatasetSource {
                access_role_arn: self.access_role_arn.clone(),
                provider: self.provider.into(),
                uri: self.uri.clone(),
            },
        };
        let client = ctx.api_client("")?;
        let dataset = client.orgs().create_dataset(&body).await?;

        let upload = if let Some(attachments) = upload_attachments {
            Some(client.orgs().upload_dataset_files(attachments, dataset.id).await?)
        } else {
            None
        };

        let format = ctx.format(&self.format)?;
        write_dataset_create_output(ctx, &format, dataset, upload)?;
        Ok(())
    }
}

/// Upload files into a Zoo-managed organization dataset.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetUpload {
    /// Dataset ID.
    #[clap(name = "dataset-id", required = true)]
    pub dataset_id: uuid::Uuid,

    /// Files or directories to upload.
    #[clap(name = "path", required = true)]
    pub paths: Vec<PathBuf>,

    /// Recurse into upload directories.
    #[clap(long)]
    pub recursive: bool,

    /// Base directory used to derive relative upload paths.
    #[clap(long)]
    pub base_dir: Option<PathBuf>,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetUpload {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let attachments = collect_dataset_upload_attachments(&self.paths, self.recursive, self.base_dir.as_deref())?;
        let client = ctx.api_client("")?;
        let response = client.orgs().upload_dataset_files(attachments, self.dataset_id).await?;
        let format = ctx.format(&self.format)?;
        ctx.io.write_output(&format, &response)?;
        Ok(())
    }
}

/// Update organization dataset metadata or storage credentials.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetUpdate {
    /// Dataset ID.
    #[clap(name = "dataset-id", required = true)]
    pub dataset_id: uuid::Uuid,

    /// New display name.
    #[clap(long)]
    pub name: Option<String>,

    /// Updated storage provider.
    #[clap(long, value_enum)]
    provider: Option<DatasetStorageProvider>,

    /// Updated fully-qualified dataset URI.
    #[clap(long)]
    pub uri: Option<String>,

    /// Updated role ARN Zoo should assume when reading the dataset.
    #[clap(long)]
    pub access_role_arn: Option<String>,

    /// Confirm that you are intentionally changing storage connection details.
    #[clap(long)]
    pub confirm_source_change: bool,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetUpdate {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let source_changed = self.provider.is_some() || self.uri.is_some() || self.access_role_arn.is_some();
        if self.name.is_none() && !source_changed {
            anyhow::bail!("nothing to update");
        }

        if source_changed && !self.confirm_source_change {
            anyhow::bail!(
                "storage connection changes can strand in-flight conversions; pass --confirm-source-change to continue"
            );
        }

        let body = kittycad::types::UpdateOrgDataset {
            name: self.name.clone(),
            source: if source_changed {
                Some(kittycad::types::UpdateOrgDatasetSource {
                    access_role_arn: self.access_role_arn.clone(),
                    provider: self.provider.map(Into::into),
                    uri: self.uri.clone(),
                })
            } else {
                None
            },
        };

        let client = ctx.api_client("")?;
        let dataset = client.orgs().update_dataset(self.dataset_id, &body).await?;
        let format = ctx.format(&self.format)?;
        write_dataset_output(ctx, &format, &dataset)?;
        Ok(())
    }
}

/// Delete an organization dataset.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetDelete {
    /// Dataset ID.
    #[clap(name = "dataset-id", required = true)]
    pub dataset_id: uuid::Uuid,

    /// Skip the confirmation prompt.
    #[clap(long, visible_alias = "yes")]
    pub confirm: bool,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetDelete {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        confirm_or_bail(
            ctx,
            self.confirm,
            &format!("Delete org dataset {}? This cannot be undone.", self.dataset_id),
            "--confirm",
        )?;
        let client = ctx.api_client("")?;
        client.orgs().delete_dataset(self.dataset_id).await?;
        writeln!(
            ctx.io.out,
            "{} Deleted org dataset {}",
            ctx.io.color_scheme().success_icon(),
            self.dataset_id
        )?;
        Ok(())
    }
}

/// Print IAM policies for onboarding an S3 organization dataset.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetS3Policies {
    /// Dataset URI used to scope generated IAM policies.
    #[clap(long, required = true)]
    pub uri: String,

    /// IAM role ARN Zoo should assume when reading the dataset.
    #[clap(long, required = true)]
    pub role_arn: String,

    /// Write trust-policy.json, permission-policy.json, and bucket-policy.json to this directory.
    #[clap(long)]
    pub output_dir: Option<PathBuf>,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetS3Policies {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let client = ctx.api_client("")?;
        let policies = client.orgs().dataset_s_3_policies(&self.role_arn, &self.uri).await?;

        if let Some(output_dir) = &self.output_dir {
            write_s3_policy_files(output_dir, &policies)?;
            if self.format.is_none() {
                writeln!(
                    ctx.io.out,
                    "{} Wrote S3 policy files to {}",
                    ctx.io.color_scheme().success_icon(),
                    output_dir.display()
                )?;
                return Ok(());
            }
            writeln!(
                ctx.io.err_out,
                "{} Wrote S3 policy files to {}",
                ctx.io.color_scheme().success_icon(),
                output_dir.display()
            )?;
        }

        let format = ctx.format(&self.format)?;
        write_s3_policies_output(ctx, &format, &policies)?;
        Ok(())
    }
}

/// Return aggregate conversion stats for an organization dataset.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetStats {
    /// Dataset ID.
    #[clap(name = "dataset-id", required = true)]
    pub dataset_id: uuid::Uuid,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetStats {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let client = ctx.api_client("")?;
        let stats = client.orgs().get_dataset_conversion_stats(self.dataset_id).await?;
        let format = ctx.format(&self.format)?;
        write_stats_output(ctx, &format, &stats)?;
        Ok(())
    }
}

/// Search converted KCL chunks semantically for an organization dataset.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetSemanticSearch {
    /// Dataset ID.
    #[clap(name = "dataset-id", required = true)]
    pub dataset_id: uuid::Uuid,

    /// Natural-language query text.
    #[clap(name = "query", required = true)]
    pub query: String,

    /// Maximum number of matching chunks to return.
    #[clap(long)]
    pub limit: Option<u32>,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetSemanticSearch {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let client = ctx.api_client("")?;
        let matches = client
            .orgs()
            .search_dataset_semantic(self.dataset_id, self.limit, &self.query)
            .await?;
        let format = ctx.format(&self.format)?;
        write_semantic_search_output(ctx, &format, &matches)?;
        Ok(())
    }
}

/// Retrigger conversions for an organization dataset.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetRetrigger {
    /// Dataset ID.
    #[clap(name = "dataset-id", required = true)]
    pub dataset_id: uuid::Uuid,

    /// Statuses to retrigger. Repeat or pass comma-separated values.
    #[clap(long, value_parser = parse_conversion_status, value_delimiter = ',')]
    pub status: Vec<ConversionStatus>,

    /// Predefined retrigger scope.
    #[clap(long, value_enum)]
    scope: Option<RetriggerScope>,

    /// Skip the confirmation prompt.
    #[clap(long, visible_alias = "yes")]
    pub confirm: bool,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetRetrigger {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        if !self.status.is_empty() && self.scope.is_some() {
            anyhow::bail!("use either --status or --scope, not both");
        }

        let statuses = if self.status.is_empty() {
            self.scope.and_then(statuses_for_scope)
        } else {
            Some(statuses_to_query(&self.status))
        };

        confirm_or_bail(
            ctx,
            self.confirm,
            &format!("Retrigger conversions for org dataset {}?", self.dataset_id),
            "--confirm",
        )?;
        let client = ctx.api_client("")?;
        client.orgs().retrigger_dataset(self.dataset_id, statuses).await?;
        writeln!(
            ctx.io.out,
            "{} Requested org dataset {} conversion retrigger",
            ctx.io.color_scheme().success_icon(),
            self.dataset_id
        )?;
        Ok(())
    }
}

/// List or search conversions for an organization dataset.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetConversions {
    #[clap(subcommand)]
    subcmd: Option<ConversionsSubCommand>,

    /// Dataset ID. If no subcommand is provided, conversions are listed.
    #[clap(name = "dataset-id")]
    pub dataset_id: Option<uuid::Uuid>,

    /// Filter by conversion status when listing.
    #[clap(long, value_parser = parse_conversion_status)]
    pub status: Option<ConversionStatus>,

    /// Maximum number of items returned by one API call.
    #[clap(long)]
    pub limit: Option<u32>,

    /// Token returned by a previous list call.
    #[clap(long)]
    pub page_token: Option<String>,

    /// Sort order.
    #[clap(long, value_enum)]
    pub sort_by: Option<kittycad::types::ConversionSortMode>,

    /// Follow pagination until every conversion is returned.
    #[clap(long)]
    pub paginate: bool,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[derive(Parser, Debug, Clone)]
enum ConversionsSubCommand {
    List(CmdOrgDatasetConversionsList),
    Search(CmdOrgDatasetConversionsSearch),
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetConversions {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            Some(ConversionsSubCommand::List(cmd)) => cmd.run(ctx).await,
            Some(ConversionsSubCommand::Search(cmd)) => cmd.run(ctx).await,
            None => {
                let dataset_id = self.dataset_id.context("dataset id is required")?;
                list_conversions(
                    ctx,
                    ConversionListArgs {
                        dataset_id,
                        status: self.status.clone(),
                        limit: self.limit,
                        page_token: self.page_token.clone(),
                        sort_by: self.sort_by.clone(),
                        paginate: self.paginate,
                        format: self.format.clone(),
                    },
                )
                .await
            }
        }
    }
}

/// List conversions for an organization dataset.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetConversionsList {
    /// Dataset ID.
    #[clap(name = "dataset-id", required = true)]
    pub dataset_id: uuid::Uuid,

    /// Filter by conversion status.
    #[clap(long, value_parser = parse_conversion_status)]
    pub status: Option<ConversionStatus>,

    /// Maximum number of items returned by one API call.
    #[clap(long)]
    pub limit: Option<u32>,

    /// Token returned by a previous list call.
    #[clap(long)]
    pub page_token: Option<String>,

    /// Sort order.
    #[clap(long, value_enum)]
    pub sort_by: Option<kittycad::types::ConversionSortMode>,

    /// Follow pagination until every conversion is returned.
    #[clap(long)]
    pub paginate: bool,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetConversionsList {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        list_conversions(
            ctx,
            ConversionListArgs {
                dataset_id: self.dataset_id,
                status: self.status.clone(),
                limit: self.limit,
                page_token: self.page_token.clone(),
                sort_by: self.sort_by.clone(),
                paginate: self.paginate,
                format: self.format.clone(),
            },
        )
        .await
    }
}

/// Search conversions by conversion ID or file path.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetConversionsSearch {
    /// Dataset ID.
    #[clap(name = "dataset-id", required = true)]
    pub dataset_id: uuid::Uuid,

    /// Search text matched against conversion ID or file path.
    #[clap(name = "query", required = true)]
    pub query: String,

    /// Maximum number of items returned by one API call.
    #[clap(long)]
    pub limit: Option<u32>,

    /// Token returned by a previous search call.
    #[clap(long)]
    pub page_token: Option<String>,

    /// Sort order.
    #[clap(long, value_enum)]
    pub sort_by: Option<kittycad::types::ConversionSortMode>,

    /// Follow pagination until every match is returned.
    #[clap(long)]
    pub paginate: bool,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetConversionsSearch {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let client = ctx.api_client("")?;
        let format = ctx.format(&self.format)?;
        if self.paginate {
            let conversions = collect_all_conversion_search_matches(
                &client,
                self.dataset_id,
                self.limit,
                self.query.clone(),
                self.sort_by.clone(),
            )
            .await?;
            write_conversion_collection_output(ctx, &format, &conversions)?;
            return Ok(());
        }

        let page = client
            .orgs()
            .search_dataset_conversions(
                self.dataset_id,
                self.limit,
                self.page_token.clone(),
                Some(self.query.clone()),
                self.sort_by.clone(),
            )
            .await?;
        write_conversion_page_output(ctx, &format, &page)?;
        Ok(())
    }
}

/// Manage one organization dataset conversion.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetConversion {
    #[clap(subcommand)]
    subcmd: DatasetConversionSubCommand,
}

#[derive(Parser, Debug, Clone)]
enum DatasetConversionSubCommand {
    Artifact(CmdOrgDatasetConversionArtifact),
    DownloadOriginal(CmdOrgDatasetConversionDownloadOriginal),
    Kcl(CmdOrgDatasetConversionKcl),
    Retrigger(CmdOrgDatasetConversionRetrigger),
    #[clap(alias = "get")]
    View(CmdOrgDatasetConversionView),
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetConversion {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            DatasetConversionSubCommand::Artifact(cmd) => cmd.run(ctx).await,
            DatasetConversionSubCommand::DownloadOriginal(cmd) => cmd.run(ctx).await,
            DatasetConversionSubCommand::Kcl(cmd) => cmd.run(ctx).await,
            DatasetConversionSubCommand::Retrigger(cmd) => cmd.run(ctx).await,
            DatasetConversionSubCommand::View(cmd) => cmd.run(ctx).await,
        }
    }
}

/// View a single organization dataset conversion.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetConversionView {
    /// Dataset ID.
    #[clap(name = "dataset-id", required = true)]
    pub dataset_id: uuid::Uuid,

    /// Conversion ID.
    #[clap(name = "conversion-id", required = true)]
    pub conversion_id: uuid::Uuid,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetConversionView {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let client = ctx.api_client("")?;
        let conversion = client
            .orgs()
            .get_dataset_conversion(self.conversion_id, self.dataset_id)
            .await?;
        let format = ctx.format(&self.format)?;
        write_conversion_details_output(ctx, &format, &conversion)?;
        Ok(())
    }
}

/// Write final KCL for a successful organization dataset conversion.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetConversionKcl {
    /// Dataset ID.
    #[clap(name = "dataset-id", required = true)]
    pub dataset_id: uuid::Uuid,

    /// Conversion ID.
    #[clap(name = "conversion-id", required = true)]
    pub conversion_id: uuid::Uuid,

    /// Output file. Use "-" to write to stdout.
    #[clap(long, short)]
    pub output: Option<PathBuf>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetConversionKcl {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let client = ctx.api_client("")?;
        let conversion = client
            .orgs()
            .get_dataset_conversion(self.conversion_id, self.dataset_id)
            .await?;
        if conversion.status != ConversionStatus::Success {
            anyhow::bail!(
                "conversion {} is {}; final KCL is only available for successful conversions",
                self.conversion_id,
                conversion.status
            );
        }
        let output = conversion
            .output
            .as_deref()
            .filter(|output| !output.is_empty())
            .context("successful conversion has no final KCL output")?;
        write_text_artifact(ctx, self.output.as_deref(), output)?;
        Ok(())
    }
}

/// Write a conversion artifact.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetConversionArtifact {
    /// Dataset ID.
    #[clap(name = "dataset-id", required = true)]
    pub dataset_id: uuid::Uuid,

    /// Conversion ID.
    #[clap(name = "conversion-id", required = true)]
    pub conversion_id: uuid::Uuid,

    /// Artifact to write.
    #[clap(long, value_enum, default_value = "final")]
    kind: ConversionArtifactKind,

    /// Output file. Use "-" to write to stdout.
    #[clap(long, short)]
    pub output: Option<PathBuf>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetConversionArtifact {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let client = ctx.api_client("")?;
        let conversion = client
            .orgs()
            .get_dataset_conversion(self.conversion_id, self.dataset_id)
            .await?;
        let contents = artifact_contents(&conversion, self.kind)?;
        write_text_artifact(ctx, self.output.as_deref(), &contents)?;
        Ok(())
    }
}

/// Download the original source file for one organization dataset conversion.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetConversionDownloadOriginal {
    /// Dataset ID.
    #[clap(name = "dataset-id", required = true)]
    pub dataset_id: uuid::Uuid,

    /// Conversion ID.
    #[clap(name = "conversion-id", required = true)]
    pub conversion_id: uuid::Uuid,

    /// Output file or directory. Use "-" to write bytes to stdout.
    #[clap(long, short)]
    pub output: Option<PathBuf>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetConversionDownloadOriginal {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let endpoint = format!(
            "/org/datasets/{}/conversions/{}/original",
            self.dataset_id, self.conversion_id
        );
        let resp = ctx
            .raw_http_request("", reqwest::Method::GET, &endpoint)?
            .send()
            .await?;
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            let body = String::from_utf8_lossy(&bytes);
            anyhow::bail!("{status} {body}");
        }

        let default_filename =
            filename_from_content_disposition(&headers).unwrap_or_else(|| format!("{}-original", self.conversion_id));
        if let Some(path) = write_binary_artifact(ctx, self.output.as_deref(), &default_filename, &bytes)? {
            writeln!(
                ctx.io.out,
                "{} Downloaded original source file to {}",
                ctx.io.color_scheme().success_icon(),
                path.display()
            )?;
        }
        Ok(())
    }
}

/// Retrigger one organization dataset conversion.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrgDatasetConversionRetrigger {
    /// Dataset ID.
    #[clap(name = "dataset-id", required = true)]
    pub dataset_id: uuid::Uuid,

    /// Conversion ID.
    #[clap(name = "conversion-id", required = true)]
    pub conversion_id: uuid::Uuid,

    /// Skip the confirmation prompt.
    #[clap(long, visible_alias = "yes")]
    pub confirm: bool,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOrgDatasetConversionRetrigger {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        confirm_or_bail(
            ctx,
            self.confirm,
            &format!(
                "Retrigger conversion {} for dataset {}?",
                self.conversion_id, self.dataset_id
            ),
            "--confirm",
        )?;
        let client = ctx.api_client("")?;
        client
            .orgs()
            .retrigger_dataset_conversion(self.conversion_id, self.dataset_id)
            .await?;
        writeln!(
            ctx.io.out,
            "{} Requested conversion {} retrigger",
            ctx.io.color_scheme().success_icon(),
            self.conversion_id
        )?;
        Ok(())
    }
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
enum DatasetStorageProvider {
    S3,
    ZooManaged,
}

impl From<DatasetStorageProvider> for kittycad::types::StorageProvider {
    fn from(provider: DatasetStorageProvider) -> Self {
        match provider {
            DatasetStorageProvider::S3 => Self::S3,
            DatasetStorageProvider::ZooManaged => Self::ZooManaged,
        }
    }
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
enum RetriggerScope {
    DefaultNonSuccess,
    AllStatuses,
    ErrorsAndCanceled,
    CanceledOnly,
    InProgressOnly,
    SuccessOnly,
    QueuedOnly,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
enum ConversionArtifactKind {
    Final,
    Raw,
    Salon,
    Manual,
    Metadata,
}

#[derive(Debug, serde::Serialize)]
struct DatasetCreateOutput {
    dataset: kittycad::types::OrgDataset,
    #[serde(skip_serializing_if = "Option::is_none")]
    upload: Option<kittycad::types::UploadOrgDatasetFilesResponse>,
}

#[derive(Debug, serde::Serialize, tabled::Tabled)]
struct OrgDatasetRow {
    name: String,
    id: uuid::Uuid,
    status: String,
    storage_provider: String,
    source_uri: String,
    updated_at: chrono::DateTime<chrono::Utc>,
    last_sync_error: String,
}

#[derive(Debug, serde::Serialize, tabled::Tabled)]
struct ConversionSummaryRow {
    id: uuid::Uuid,
    file_path: String,
    status: String,
    phase: String,
    raw_kcl_similarity_score: String,
    salon_kcl_similarity_score: String,
    file_size: i64,
    updated_at: chrono::DateTime<chrono::Utc>,
    status_message: String,
}

#[derive(Debug, serde::Serialize, tabled::Tabled)]
struct ConversionDetailsRow {
    id: uuid::Uuid,
    dataset_id: uuid::Uuid,
    file_path: String,
    status: String,
    phase: String,
    file_size: i64,
    raw_kcl_similarity_score: String,
    salon_kcl_similarity_score: String,
    manual_kcl_override_active: bool,
    output_bytes: usize,
    original_snapshot_images: usize,
    raw_kcl_snapshot_images: usize,
    salon_kcl_snapshot_images: usize,
    status_message: String,
    updated_at: chrono::DateTime<chrono::Utc>,
    completed_at: String,
}

#[derive(Debug, serde::Serialize, tabled::Tabled)]
struct StatsRow {
    dataset_id: uuid::Uuid,
    total: i64,
    successes: i64,
    failures: i64,
    success_rate: String,
    by_status: String,
}

#[derive(Debug, serde::Serialize, tabled::Tabled)]
struct SemanticSearchRow {
    conversion_id: uuid::Uuid,
    source_file_path: String,
    chunk_index: i32,
    similarity: String,
    content: String,
}

#[derive(Debug, serde::Serialize, tabled::Tabled)]
struct S3PolicyRow {
    name: &'static str,
    json: String,
}

fn validate_create_source(provider: DatasetStorageProvider, uri: Option<&str>, role_arn: Option<&str>) -> Result<()> {
    match provider {
        DatasetStorageProvider::S3 => {
            if uri.is_none() {
                anyhow::bail!("--uri is required when --provider s3");
            }
            if role_arn.is_none() {
                anyhow::bail!("--access-role-arn is required when --provider s3");
            }
        }
        DatasetStorageProvider::ZooManaged => {
            if uri.is_some() || role_arn.is_some() {
                anyhow::bail!("Zoo-managed datasets cannot set --uri or --access-role-arn");
            }
        }
    }

    Ok(())
}

fn collect_create_dataset_upload_attachments(
    provider: DatasetStorageProvider,
    upload: &[PathBuf],
    recursive: bool,
    base_dir: Option<&Path>,
) -> Result<Option<Vec<kittycad::types::multipart::Attachment>>> {
    if upload.is_empty() {
        return Ok(None);
    }
    if provider != DatasetStorageProvider::ZooManaged {
        anyhow::bail!("--upload is only supported for Zoo-managed datasets");
    }
    collect_dataset_upload_attachments(upload, recursive, base_dir).map(Some)
}

async fn collect_all_datasets(
    client: &kittycad::Client,
    limit: Option<u32>,
    sort_by: Option<kittycad::types::CreatedAtSortMode>,
) -> Result<Vec<kittycad::types::OrgDataset>> {
    let mut datasets = Vec::new();
    let mut page_token = None;
    loop {
        let page = client
            .orgs()
            .list_datasets(limit, page_token.take(), sort_by.clone())
            .await?;
        datasets.extend(page.items);
        page_token = page.next_page;
        if page_token.is_none() {
            break;
        }
    }
    Ok(datasets)
}

#[derive(Debug, Clone)]
struct ConversionListArgs {
    dataset_id: uuid::Uuid,
    status: Option<ConversionStatus>,
    limit: Option<u32>,
    page_token: Option<String>,
    sort_by: Option<kittycad::types::ConversionSortMode>,
    paginate: bool,
    format: Option<FormatOutput>,
}

async fn list_conversions(ctx: &mut crate::context::Context<'_>, args: ConversionListArgs) -> Result<()> {
    let client = ctx.api_client("")?;
    let format = ctx.format(&args.format)?;
    let filter = args.status.map(status_filter);
    if args.paginate {
        let conversions = collect_all_conversions(&client, args.dataset_id, filter, args.limit, args.sort_by).await?;
        write_conversion_collection_output(ctx, &format, &conversions)?;
        return Ok(());
    }

    let page = client
        .orgs()
        .list_dataset_conversions(filter, args.dataset_id, args.limit, args.page_token, args.sort_by)
        .await?;
    write_conversion_page_output(ctx, &format, &page)?;
    Ok(())
}

async fn collect_all_conversions(
    client: &kittycad::Client,
    dataset_id: uuid::Uuid,
    filter: Option<String>,
    limit: Option<u32>,
    sort_by: Option<kittycad::types::ConversionSortMode>,
) -> Result<Vec<kittycad::types::OrgDatasetFileConversionSummary>> {
    let mut conversions = Vec::new();
    let mut page_token = None;
    loop {
        let page = client
            .orgs()
            .list_dataset_conversions(filter.clone(), dataset_id, limit, page_token.take(), sort_by.clone())
            .await?;
        conversions.extend(page.items);
        page_token = page.next_page;
        if page_token.is_none() {
            break;
        }
    }
    Ok(conversions)
}

async fn collect_all_conversion_search_matches(
    client: &kittycad::Client,
    dataset_id: uuid::Uuid,
    limit: Option<u32>,
    query: String,
    sort_by: Option<kittycad::types::ConversionSortMode>,
) -> Result<Vec<kittycad::types::OrgDatasetFileConversionSummary>> {
    let mut conversions = Vec::new();
    let mut page_token = None;
    loop {
        let page = client
            .orgs()
            .search_dataset_conversions(
                dataset_id,
                limit,
                page_token.take(),
                Some(query.clone()),
                sort_by.clone(),
            )
            .await?;
        conversions.extend(page.items);
        page_token = page.next_page;
        if page_token.is_none() {
            break;
        }
    }
    Ok(conversions)
}

fn status_filter(status: ConversionStatus) -> String {
    format!("status:{status}")
}

fn parse_conversion_status(value: &str) -> std::result::Result<ConversionStatus, String> {
    match value.replace('-', "_").to_ascii_lowercase().as_str() {
        "queued" => Ok(ConversionStatus::Queued),
        "canceled" => Ok(ConversionStatus::Canceled),
        "in_progress" => Ok(ConversionStatus::InProgress),
        "success" => Ok(ConversionStatus::Success),
        "error_user" => Ok(ConversionStatus::ErrorUser),
        "error_geometry_mismatch" => Ok(ConversionStatus::ErrorGeometryMismatch),
        "error_unsupported" => Ok(ConversionStatus::ErrorUnsupported),
        "error_internal" => Ok(ConversionStatus::ErrorInternal),
        _ => Err(format!(
            "invalid conversion status `{value}`; expected one of queued, canceled, in_progress, success, error_user, error_geometry_mismatch, error_unsupported, error_internal"
        )),
    }
}

fn statuses_to_query(statuses: &[ConversionStatus]) -> String {
    statuses.iter().map(ToString::to_string).collect::<Vec<_>>().join(",")
}

fn statuses_for_scope(scope: RetriggerScope) -> Option<String> {
    let statuses = match scope {
        RetriggerScope::DefaultNonSuccess => return None,
        RetriggerScope::AllStatuses => vec![
            ConversionStatus::Queued,
            ConversionStatus::Canceled,
            ConversionStatus::InProgress,
            ConversionStatus::Success,
            ConversionStatus::ErrorUser,
            ConversionStatus::ErrorGeometryMismatch,
            ConversionStatus::ErrorUnsupported,
            ConversionStatus::ErrorInternal,
        ],
        RetriggerScope::ErrorsAndCanceled => vec![
            ConversionStatus::Canceled,
            ConversionStatus::ErrorUser,
            ConversionStatus::ErrorGeometryMismatch,
            ConversionStatus::ErrorUnsupported,
            ConversionStatus::ErrorInternal,
        ],
        RetriggerScope::CanceledOnly => vec![ConversionStatus::Canceled],
        RetriggerScope::InProgressOnly => vec![ConversionStatus::InProgress],
        RetriggerScope::SuccessOnly => vec![ConversionStatus::Success],
        RetriggerScope::QueuedOnly => vec![ConversionStatus::Queued],
    };
    Some(statuses_to_query(&statuses))
}

fn collect_dataset_upload_attachments(
    inputs: &[PathBuf],
    recursive: bool,
    base_dir: Option<&Path>,
) -> Result<Vec<kittycad::types::multipart::Attachment>> {
    let files = collect_dataset_upload_files(inputs, recursive, base_dir)?;
    files
        .into_iter()
        .enumerate()
        .map(|(index, file)| {
            let mut attachment = kittycad::types::multipart::Attachment::try_from(file.path.clone())
                .with_context(|| format!("failed to read `{}`", file.path.display()))?;
            attachment.name = file.upload_path.clone();
            attachment.filepath = Some(PathBuf::from(file.upload_path));
            if attachment.content_type.is_none() {
                attachment.content_type = Some("application/octet-stream".to_string());
            }
            if attachment.name.is_empty() {
                attachment.name = format!("file_{index}");
            }
            Ok(attachment)
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DatasetUploadFile {
    path: PathBuf,
    upload_path: String,
}

fn collect_dataset_upload_files(
    inputs: &[PathBuf],
    recursive: bool,
    base_dir: Option<&Path>,
) -> Result<Vec<DatasetUploadFile>> {
    let canonical_base_dir = base_dir.map(canonicalize_existing_path).transpose()?;
    let mut files = Vec::new();

    for input in inputs {
        let metadata = fs::metadata(input).with_context(|| format!("failed to inspect `{}`", input.display()))?;
        if metadata.is_dir() {
            if !recursive {
                anyhow::bail!(
                    "`{}` is a directory; pass --recursive to upload directory contents",
                    input.display()
                );
            }
            collect_files_from_dir(input, &mut files)?;
        } else if metadata.is_file() {
            files.push(canonicalize_existing_path(input)?);
        } else {
            anyhow::bail!("`{}` is not a regular file or directory", input.display());
        }
    }

    files.sort();
    let mut upload_files = Vec::with_capacity(files.len());
    let mut seen_upload_paths = HashSet::new();

    for path in files {
        validate_dataset_upload_path(&path)?;
        let upload_path = relative_upload_path(&path, inputs, canonical_base_dir.as_deref())?;
        if !seen_upload_paths.insert(upload_path.clone()) {
            anyhow::bail!("multiple files resolve to upload path `{upload_path}`; pass --base-dir to disambiguate");
        }
        upload_files.push(DatasetUploadFile { path, upload_path });
    }

    if upload_files.is_empty() {
        anyhow::bail!("no files to upload");
    }

    Ok(upload_files)
}

fn collect_files_from_dir(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read directory `{}`", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_files_from_dir(&path, files)?;
        } else if metadata.is_file() {
            files.push(canonicalize_existing_path(&path)?);
        }
    }
    Ok(())
}

fn canonicalize_existing_path(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).with_context(|| format!("failed to canonicalize `{}`", path.display()))
}

fn relative_upload_path(path: &Path, inputs: &[PathBuf], base_dir: Option<&Path>) -> Result<String> {
    let relative = if let Some(base_dir) = base_dir {
        path.strip_prefix(base_dir)
            .with_context(|| format!("`{}` is not under --base-dir `{}`", path.display(), base_dir.display()))?
            .to_path_buf()
    } else {
        let owning_dir = inputs.iter().find_map(|input| {
            let metadata = fs::metadata(input).ok()?;
            if !metadata.is_dir() {
                return None;
            }
            let dir = fs::canonicalize(input).ok()?;
            path.strip_prefix(&dir).ok().map(Path::to_path_buf)
        });
        owning_dir.unwrap_or_else(|| {
            path.file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(path))
        })
    };

    let upload_path = path_to_upload_string(&relative);
    if upload_path.is_empty() || upload_path == "." {
        anyhow::bail!("could not derive a relative upload path for `{}`", path.display());
    }
    if upload_path
        .split('/')
        .any(|component| component == ".." || component.is_empty())
    {
        anyhow::bail!("derived upload path `{upload_path}` is not a normalized relative path");
    }
    Ok(upload_path)
}

fn path_to_upload_string(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn validate_dataset_upload_path(path: &Path) -> Result<()> {
    if is_archive_path(path) {
        anyhow::bail!(
            "archives are not accepted for org dataset uploads: `{}`",
            path.display()
        );
    }
    if !is_supported_dataset_file(path) {
        anyhow::bail!(
            "unsupported org dataset file extension for `{}`; expected a supported CAD source file",
            path.display()
        );
    }
    Ok(())
}

fn is_archive_path(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    let lower = file_name.to_ascii_lowercase();
    ARCHIVE_FILE_SUFFIXES
        .iter()
        .any(|suffix| lower == *suffix || lower.ends_with(&format!(".{suffix}")))
}

fn is_supported_dataset_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    let mut lower = file_name.to_ascii_lowercase();
    if let Some((stem, suffix)) = lower.rsplit_once('.') {
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            lower = stem.to_string();
        }
    }
    let Some((_, extension)) = lower.rsplit_once('.') else {
        return false;
    };
    CAD_FILE_EXTENSIONS.contains(&extension)
}

fn write_dataset_page_output(
    ctx: &mut crate::context::Context<'_>,
    format: &FormatOutput,
    page: &kittycad::types::OrgDatasetResultsPage,
) -> Result<()> {
    match format {
        FormatOutput::Json => ctx.io.write_output_json(&serde_json::to_value(page)?)?,
        FormatOutput::Yaml => ctx.io.write_output_yaml(page)?,
        FormatOutput::Table => ctx
            .io
            .write_output_for_vec(format, page.items.iter().map(dataset_row).collect::<Vec<_>>())?,
    }
    Ok(())
}

fn write_dataset_collection_output(
    ctx: &mut crate::context::Context<'_>,
    format: &FormatOutput,
    datasets: &[kittycad::types::OrgDataset],
) -> Result<()> {
    match format {
        FormatOutput::Json => ctx.io.write_output_json(&serde_json::to_value(datasets)?)?,
        FormatOutput::Yaml => ctx.io.write_output_yaml(&datasets.to_vec())?,
        FormatOutput::Table => ctx
            .io
            .write_output_for_vec(format, datasets.iter().map(dataset_row).collect::<Vec<_>>())?,
    }
    Ok(())
}

fn write_dataset_output(
    ctx: &mut crate::context::Context<'_>,
    format: &FormatOutput,
    dataset: &kittycad::types::OrgDataset,
) -> Result<()> {
    match format {
        FormatOutput::Json => ctx.io.write_output_json(&serde_json::to_value(dataset)?)?,
        FormatOutput::Yaml => ctx.io.write_output_yaml(dataset)?,
        FormatOutput::Table => ctx.io.write_output_for_vec(format, vec![dataset_row(dataset)])?,
    }
    Ok(())
}

fn write_dataset_create_output(
    ctx: &mut crate::context::Context<'_>,
    format: &FormatOutput,
    dataset: kittycad::types::OrgDataset,
    upload: Option<kittycad::types::UploadOrgDatasetFilesResponse>,
) -> Result<()> {
    match format {
        FormatOutput::Json => ctx
            .io
            .write_output_json(&serde_json::to_value(DatasetCreateOutput { dataset, upload })?)?,
        FormatOutput::Yaml => ctx.io.write_output_yaml(&DatasetCreateOutput { dataset, upload })?,
        FormatOutput::Table => {
            ctx.io.write_output_for_vec(format, vec![dataset_row(&dataset)])?;
            if let Some(upload) = upload {
                ctx.io.write_output(format, &upload)?;
            }
        }
    }
    Ok(())
}

fn dataset_row(dataset: &kittycad::types::OrgDataset) -> OrgDatasetRow {
    OrgDatasetRow {
        name: dataset.name.clone(),
        id: dataset.id,
        status: dataset.status.to_string(),
        storage_provider: dataset.storage_provider.to_string(),
        source_uri: dataset.source_uri.clone(),
        updated_at: dataset.updated_at,
        last_sync_error: dataset.last_sync_error.clone().unwrap_or_default(),
    }
}

fn write_conversion_page_output(
    ctx: &mut crate::context::Context<'_>,
    format: &FormatOutput,
    page: &kittycad::types::OrgDatasetFileConversionSummaryResultsPage,
) -> Result<()> {
    match format {
        FormatOutput::Json => ctx.io.write_output_json(&serde_json::to_value(page)?)?,
        FormatOutput::Yaml => ctx.io.write_output_yaml(page)?,
        FormatOutput::Table => ctx.io.write_output_for_vec(
            format,
            page.items.iter().map(conversion_summary_row).collect::<Vec<_>>(),
        )?,
    }
    Ok(())
}

fn write_conversion_collection_output(
    ctx: &mut crate::context::Context<'_>,
    format: &FormatOutput,
    conversions: &[kittycad::types::OrgDatasetFileConversionSummary],
) -> Result<()> {
    match format {
        FormatOutput::Json => ctx.io.write_output_json(&serde_json::to_value(conversions)?)?,
        FormatOutput::Yaml => ctx.io.write_output_yaml(&conversions.to_vec())?,
        FormatOutput::Table => ctx.io.write_output_for_vec(
            format,
            conversions.iter().map(conversion_summary_row).collect::<Vec<_>>(),
        )?,
    }
    Ok(())
}

fn conversion_summary_row(conversion: &kittycad::types::OrgDatasetFileConversionSummary) -> ConversionSummaryRow {
    ConversionSummaryRow {
        id: conversion.id,
        file_path: conversion.file_path.clone(),
        status: conversion.status.to_string(),
        phase: conversion.phase.to_string(),
        raw_kcl_similarity_score: format_score(conversion.raw_kcl_similarity_score),
        salon_kcl_similarity_score: format_score(conversion.salon_kcl_similarity_score),
        file_size: conversion.file_size,
        updated_at: conversion.updated_at,
        status_message: conversion.status_message.clone().unwrap_or_default(),
    }
}

fn write_conversion_details_output(
    ctx: &mut crate::context::Context<'_>,
    format: &FormatOutput,
    conversion: &kittycad::types::OrgDatasetFileConversionDetails,
) -> Result<()> {
    match format {
        FormatOutput::Json => ctx.io.write_output_json(&serde_json::to_value(conversion)?)?,
        FormatOutput::Yaml => ctx.io.write_output_yaml(conversion)?,
        FormatOutput::Table => ctx
            .io
            .write_output_for_vec(format, vec![conversion_details_row(conversion)])?,
    }
    Ok(())
}

fn conversion_details_row(conversion: &kittycad::types::OrgDatasetFileConversionDetails) -> ConversionDetailsRow {
    ConversionDetailsRow {
        id: conversion.id,
        dataset_id: conversion.dataset_id,
        file_path: conversion.file_path.clone(),
        status: conversion.status.to_string(),
        phase: conversion.phase.to_string(),
        file_size: conversion.file_size,
        raw_kcl_similarity_score: format_score(conversion.raw_kcl_similarity_score),
        salon_kcl_similarity_score: format_score(conversion.salon_kcl_similarity_score),
        manual_kcl_override_active: conversion.manual_kcl_override_active,
        output_bytes: conversion.output.as_deref().map(str::len).unwrap_or_default(),
        original_snapshot_images: conversion.original_snapshot_images.len(),
        raw_kcl_snapshot_images: conversion.raw_kcl_snapshot_images.len(),
        salon_kcl_snapshot_images: conversion.salon_kcl_snapshot_images.len(),
        status_message: conversion.status_message.clone().unwrap_or_default(),
        updated_at: conversion.updated_at,
        completed_at: conversion
            .completed_at
            .map(|completed_at| completed_at.to_string())
            .unwrap_or_default(),
    }
}

fn write_stats_output(
    ctx: &mut crate::context::Context<'_>,
    format: &FormatOutput,
    stats: &kittycad::types::OrgDatasetConversionStatsResponse,
) -> Result<()> {
    match format {
        FormatOutput::Json => ctx.io.write_output_json(&serde_json::to_value(stats)?)?,
        FormatOutput::Yaml => ctx.io.write_output_yaml(stats)?,
        FormatOutput::Table => ctx.io.write_output_for_vec(format, vec![stats_row(stats)])?,
    }
    Ok(())
}

fn stats_row(stats: &kittycad::types::OrgDatasetConversionStatsResponse) -> StatsRow {
    let success_rate = if stats.total == 0 {
        "n/a".to_string()
    } else {
        format!("{:.2}%", (stats.successes as f64 / stats.total as f64) * 100.0)
    };
    StatsRow {
        dataset_id: stats.dataset_id,
        total: stats.total,
        successes: stats.successes,
        failures: stats.failures,
        success_rate,
        by_status: sorted_status_counts(&stats.by_status),
    }
}

fn sorted_status_counts(counts: &HashMap<String, i64>) -> String {
    let mut counts = counts.iter().collect::<Vec<_>>();
    counts.sort_by_key(|(status, _)| *status);
    counts
        .into_iter()
        .map(|(status, count)| format!("{status}:{count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn write_semantic_search_output(
    ctx: &mut crate::context::Context<'_>,
    format: &FormatOutput,
    matches: &[kittycad::types::OrgDatasetSemanticSearchMatch],
) -> Result<()> {
    match format {
        FormatOutput::Json => ctx.io.write_output_json(&serde_json::to_value(matches)?)?,
        FormatOutput::Yaml => ctx.io.write_output_yaml(&matches.to_vec())?,
        FormatOutput::Table => ctx.io.write_output_for_vec(
            format,
            matches
                .iter()
                .map(|item| SemanticSearchRow {
                    conversion_id: item.conversion_id,
                    source_file_path: item.source_file_path.clone(),
                    chunk_index: item.chunk_index,
                    similarity: format!("{:.4}", item.similarity),
                    content: truncate_for_table(&item.content, 120),
                })
                .collect::<Vec<_>>(),
        )?,
    }
    Ok(())
}

fn write_s3_policies_output(
    ctx: &mut crate::context::Context<'_>,
    format: &FormatOutput,
    policies: &kittycad::types::DatasetS3Policies,
) -> Result<()> {
    match format {
        FormatOutput::Json => ctx.io.write_output_json(&serde_json::to_value(policies)?)?,
        FormatOutput::Yaml => ctx.io.write_output_yaml(policies)?,
        FormatOutput::Table => ctx.io.write_output_for_vec(
            format,
            vec![
                S3PolicyRow {
                    name: "trust_policy",
                    json: serde_json::to_string_pretty(&policies.trust_policy)?,
                },
                S3PolicyRow {
                    name: "permission_policy",
                    json: serde_json::to_string_pretty(&policies.permission_policy)?,
                },
                S3PolicyRow {
                    name: "bucket_policy",
                    json: serde_json::to_string_pretty(&policies.bucket_policy)?,
                },
            ],
        )?,
    }
    Ok(())
}

fn write_s3_policy_files(output_dir: &Path, policies: &kittycad::types::DatasetS3Policies) -> Result<()> {
    fs::create_dir_all(output_dir).with_context(|| format!("failed to create `{}`", output_dir.display()))?;
    write_json_file(&output_dir.join("trust-policy.json"), &policies.trust_policy)?;
    write_json_file(&output_dir.join("permission-policy.json"), &policies.permission_policy)?;
    write_json_file(&output_dir.join("bucket-policy.json"), &policies.bucket_policy)?;
    Ok(())
}

fn write_json_file(path: &Path, value: &serde_json::Value) -> Result<()> {
    let mut contents = serde_json::to_vec_pretty(value)?;
    contents.push(b'\n');
    fs::write(path, contents).with_context(|| format!("failed to write `{}`", path.display()))
}

fn artifact_contents(
    conversion: &kittycad::types::OrgDatasetFileConversionDetails,
    kind: ConversionArtifactKind,
) -> Result<String> {
    match kind {
        ConversionArtifactKind::Final => {
            if conversion.status != ConversionStatus::Success {
                anyhow::bail!(
                    "conversion {} is {}; final KCL is only available for successful conversions",
                    conversion.id,
                    conversion.status
                );
            }
            conversion
                .output
                .clone()
                .filter(|output| !output.is_empty())
                .context("successful conversion has no final KCL output")
        }
        ConversionArtifactKind::Raw => conversion
            .raw_kcl_output
            .clone()
            .filter(|output| !output.is_empty())
            .context("conversion has no raw KCL output"),
        ConversionArtifactKind::Salon => conversion
            .salon_kcl_output
            .clone()
            .filter(|output| !output.is_empty())
            .context("conversion has no salon KCL output"),
        ConversionArtifactKind::Manual => conversion
            .manual_kcl_override
            .clone()
            .filter(|output| !output.is_empty())
            .context("conversion has no manual KCL override"),
        ConversionArtifactKind::Metadata => {
            let metadata = conversion.metadata.as_ref().context("conversion has no metadata")?;
            Ok(serde_json::to_string_pretty(metadata)?)
        }
    }
}

fn write_text_artifact(ctx: &mut crate::context::Context<'_>, output: Option<&Path>, contents: &str) -> Result<()> {
    match output {
        Some(path) if path.as_os_str() != "-" => {
            if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
                fs::create_dir_all(parent).with_context(|| format!("failed to create `{}`", parent.display()))?;
            }
            fs::write(path, contents).with_context(|| format!("failed to write `{}`", path.display()))?;
        }
        _ => {
            ctx.io.out.write_all(contents.as_bytes())?;
            if !contents.ends_with('\n') {
                writeln!(ctx.io.out)?;
            }
        }
    }
    Ok(())
}

fn write_binary_artifact(
    ctx: &mut crate::context::Context<'_>,
    output: Option<&Path>,
    default_filename: &str,
    contents: &[u8],
) -> Result<Option<PathBuf>> {
    if matches!(output, Some(path) if path.as_os_str() == "-") {
        ctx.io.out.write_all(contents)?;
        return Ok(None);
    }

    let path = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(default_filename));
    let path = if path.is_dir() {
        path.join(default_filename)
    } else {
        path
    };
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent).with_context(|| format!("failed to create `{}`", parent.display()))?;
    }
    fs::write(&path, contents).with_context(|| format!("failed to write `{}`", path.display()))?;
    Ok(Some(path))
}

fn filename_from_content_disposition(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let value = headers.get(reqwest::header::CONTENT_DISPOSITION)?.to_str().ok()?;
    value
        .split(';')
        .map(str::trim)
        .find_map(|part| {
            part.strip_prefix("filename=")
                .or_else(|| part.strip_prefix("filename*=UTF-8''"))
        })
        .and_then(sanitize_filename)
}

fn sanitize_filename(value: &str) -> Option<String> {
    let value = value.trim_matches('"').trim();
    let filename = Path::new(value).file_name()?.to_str()?.trim();
    if filename.is_empty() {
        None
    } else {
        Some(filename.to_string())
    }
}

fn confirm_or_bail(ctx: &crate::context::Context<'_>, confirmed: bool, prompt: &str, flag: &str) -> Result<()> {
    if confirmed {
        return Ok(());
    }
    if !ctx.io.can_prompt() {
        anyhow::bail!("refusing to continue without confirmation; pass {flag}");
    }
    let confirmed = dialoguer::Confirm::new()
        .with_prompt(prompt)
        .default(false)
        .interact()
        .map_err(|err| anyhow::anyhow!("prompt failed: {err}"))?;
    if !confirmed {
        anyhow::bail!("aborted");
    }
    Ok(())
}

fn format_score(score: Option<f64>) -> String {
    score.map(|score| format!("{score:.4}")).unwrap_or_default()
}

fn truncate_for_table(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATASET_ID: &str = "d9797f8d-9ad6-4e08-90d7-2ec17e13471c";

    #[test]
    fn parse_conversion_status_accepts_api_and_cli_spellings() {
        assert_eq!(
            parse_conversion_status("error_geometry_mismatch").unwrap(),
            ConversionStatus::ErrorGeometryMismatch
        );
        assert_eq!(
            parse_conversion_status("error-geometry-mismatch").unwrap(),
            ConversionStatus::ErrorGeometryMismatch
        );
        assert_eq!(
            parse_conversion_status("in-progress").unwrap(),
            ConversionStatus::InProgress
        );
    }

    #[test]
    fn retrigger_scope_includes_geometry_mismatch_errors() {
        assert_eq!(
            statuses_for_scope(RetriggerScope::ErrorsAndCanceled).unwrap(),
            "canceled,error_user,error_geometry_mismatch,error_unsupported,error_internal"
        );
    }

    #[test]
    fn dataset_upload_file_validation_accepts_supported_numeric_suffix() {
        assert!(is_supported_dataset_file(Path::new("part.sldprt")));
        assert!(is_supported_dataset_file(Path::new("part.CATPart.1")));
        assert!(!is_supported_dataset_file(Path::new("metadata.json")));
    }

    #[test]
    fn dataset_upload_file_validation_rejects_archives() {
        assert!(is_archive_path(Path::new("dataset.zip")));
        assert!(is_archive_path(Path::new("dataset.tar.gz")));
        assert!(!is_archive_path(Path::new("part.sldprt")));
    }

    #[test]
    fn create_upload_validation_rejects_non_zoo_managed_before_reading_files() {
        let missing_file = PathBuf::from("missing.sldprt");
        let err = collect_create_dataset_upload_attachments(DatasetStorageProvider::S3, &[missing_file], false, None)
            .unwrap_err();
        assert_eq!(err.to_string(), "--upload is only supported for Zoo-managed datasets");
    }

    #[test]
    fn conversions_command_defaults_to_list() {
        let cmd = CmdOrg::try_parse_from(["org", "dataset", "conversions", DATASET_ID]).unwrap();
        let SubCommand::Dataset(dataset) = cmd.subcmd;
        let DatasetSubCommand::Conversions(conversions) = dataset.subcmd else {
            panic!("expected conversions command");
        };
        assert!(conversions.subcmd.is_none());
        assert_eq!(conversions.dataset_id.unwrap().to_string(), DATASET_ID);
    }

    #[test]
    fn conversions_command_accepts_search_subcommand() {
        let cmd = CmdOrg::try_parse_from(["org", "dataset", "conversions", "search", DATASET_ID, "bracket"]).unwrap();
        let SubCommand::Dataset(dataset) = cmd.subcmd;
        let DatasetSubCommand::Conversions(conversions) = dataset.subcmd else {
            panic!("expected conversions command");
        };
        assert!(matches!(conversions.subcmd, Some(ConversionsSubCommand::Search(_))));
    }
}
