use std::{io::Write, path::PathBuf};

use anyhow::{Context as _, Result};
use clap::Parser;

use crate::types::FormatOutput;

/// Manage Zoo projects.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdProject {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Categories(CmdProjectCategories),
    Download(CmdProjectDownload),
    List(CmdProjectList),
    Publish(CmdProjectPublish),
    #[clap(alias = "get")]
    View(CmdProjectView),
    Upload(CmdProjectUpload),
    Update(CmdProjectUpdate),
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdProject {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Categories(cmd) => cmd.run(ctx).await,
            SubCommand::Download(cmd) => cmd.run(ctx).await,
            SubCommand::List(cmd) => cmd.run(ctx).await,
            SubCommand::Publish(cmd) => cmd.run(ctx).await,
            SubCommand::View(cmd) => cmd.run(ctx).await,
            SubCommand::Upload(cmd) => cmd.run(ctx).await,
            SubCommand::Update(cmd) => cmd.run(ctx).await,
        }
    }
}

/// List the active project categories available for submission.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdProjectCategories {
    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdProjectCategories {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let client = ctx.api_client("")?;
        let categories = client.users().list_project_categories().await?;
        let format = ctx.format(&self.format)?;
        ctx.io.write_output_for_vec(&format, categories)?;
        Ok(())
    }
}

/// Download one of your projects into a local directory.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdProjectDownload {
    /// The project id.
    #[clap(name = "id", required = true)]
    pub id: uuid::Uuid,

    /// The directory to extract the project into.
    #[clap(name = "output-dir", default_value = ".")]
    pub output_dir: PathBuf,

    /// Allow extracting into a non-empty destination, overwriting existing files in place.
    #[clap(long, default_value = "false")]
    pub force: bool,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdProjectDownload {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        crate::project::ensure_download_destination(&self.output_dir, self.force)?;

        let client = ctx.api_client("")?;
        let endpoint = format!("/user/projects/{}/download", self.id);
        let req = client.request_raw(http::Method::GET, &endpoint, None).await?;
        let resp = req.0.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("{} {}", status, body);
        }

        let body = resp.bytes().await?;
        let mut archive = tar::Archive::new(std::io::Cursor::new(body));
        archive
            .unpack(&self.output_dir)
            .with_context(|| format!("failed to extract archive into `{}`", self.output_dir.display()))?;

        if let Some(project_root) = crate::project::find_project_root_under(&self.output_dir)? {
            let project_toml = project_root.join("project.toml");
            crate::project::persist_cloud_project_id(&project_toml, self.id)?;
            writeln!(
                ctx.io.out,
                "{} Downloaded project {} into {}",
                ctx.io.color_scheme().success_icon(),
                self.id,
                project_root.display()
            )?;
        } else {
            writeln!(
                ctx.io.out,
                "{} Downloaded project {} into {}",
                ctx.io.color_scheme().success_icon(),
                self.id,
                self.output_dir.display()
            )?;
            writeln!(
                ctx.io.out,
                "Could not locate a project root to persist the project id automatically."
            )?;
        }

        Ok(())
    }
}

/// List your projects.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdProjectList {
    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[derive(Debug, Clone, serde::Serialize, tabled::Tabled)]
struct ProjectListTableRow {
    title: String,
    description: String,
    id: uuid::Uuid,
    #[tabled(rename = "updated")]
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, tabled::Tabled)]
struct ProjectViewTableRow {
    title: String,
    description: String,
    id: uuid::Uuid,
    #[tabled(rename = "publication")]
    publication_status: kittycad::types::KclProjectPublicationStatus,
    #[tabled(rename = "files")]
    file_count: usize,
    #[tabled(rename = "created")]
    created_at: chrono::DateTime<chrono::Utc>,
    #[tabled(rename = "updated")]
    updated_at: chrono::DateTime<chrono::Utc>,
}

fn project_view_table_row(project: &kittycad::types::ProjectResponse) -> ProjectViewTableRow {
    ProjectViewTableRow {
        title: project.title.clone(),
        description: project.description.clone(),
        id: project.id,
        publication_status: project.publication_status.clone(),
        file_count: project.files.len(),
        created_at: project.created_at,
        updated_at: project.updated_at,
    }
}

fn write_project_output(
    ctx: &mut crate::context::Context<'_>,
    format: &FormatOutput,
    project: &kittycad::types::ProjectResponse,
) -> Result<()> {
    match format {
        FormatOutput::Json => ctx.io.write_output_json(&serde_json::to_value(project)?)?,
        FormatOutput::Yaml => ctx.io.write_output_yaml(project)?,
        FormatOutput::Table => ctx
            .io
            .write_output_for_vec(format, vec![project_view_table_row(project)])?,
    }

    Ok(())
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdProjectList {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let client = ctx.api_client("")?;
        let projects = client.users().list_projects().await?;
        let format = ctx.format(&self.format)?;
        match format {
            FormatOutput::Json => ctx.io.write_output_json(&serde_json::to_value(&projects)?)?,
            FormatOutput::Yaml => ctx.io.write_output_yaml(&projects)?,
            FormatOutput::Table => {
                let rows = projects
                    .into_iter()
                    .map(|project| ProjectListTableRow {
                        title: project.title,
                        description: project.description,
                        id: project.id,
                        updated_at: project.updated_at,
                    })
                    .collect::<Vec<_>>();
                ctx.io.write_output_for_vec(&format, rows)?
            }
        }
        Ok(())
    }
}

/// View one of your projects.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdProjectView {
    /// The project id.
    #[clap(name = "id", required = true)]
    pub id: uuid::Uuid,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdProjectView {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let client = ctx.api_client("")?;
        let project = client.users().get_project(self.id).await?;
        let format = ctx.format(&self.format)?;
        write_project_output(ctx, &format, &project)?;
        Ok(())
    }
}

/// Submit an existing cloud project for publication review.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdProjectPublish {
    /// The project directory, a `.kcl` file within it, or `project.toml`.
    ///
    /// Used to look up the persisted Zoo cloud project id when `--id` is not passed.
    #[clap(name = "input")]
    pub input: Option<PathBuf>,

    /// Override the persisted Zoo cloud project id from `project.toml`.
    #[clap(long)]
    pub id: Option<uuid::Uuid>,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdProjectPublish {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let local = self
            .input
            .as_ref()
            .map(|input| crate::project::resolve_local_project(input))
            .transpose()?;
        let project_id = match (self.id, local.as_ref()) {
            (Some(id), _) => id,
            (None, Some(local)) => {
                crate::project::read_persisted_cloud_project_id(&local.project_toml)?.with_context(|| {
                    format!(
                        "no Zoo cloud project id found in `{}`; pass `--id`",
                        local.project_toml.display()
                    )
                })?
            }
            (None, None) => anyhow::bail!("pass a local project path or `--id`"),
        };

        let client = ctx.api_client("")?;
        let project = client.users().publish_project(project_id).await?;

        if let Some(local) = local {
            crate::project::persist_cloud_project_id(&local.project_toml, project.id)?;
        }
        writeln!(
            ctx.io.out,
            "{} Submitted Zoo cloud project {} for publication review",
            ctx.io.color_scheme().success_icon(),
            project.id
        )?;

        let format = ctx.format(&self.format)?;
        write_project_output(ctx, &format, &project)?;
        Ok(())
    }
}

/// Upload a local project.
///
/// If the local `project.toml` already contains a Zoo cloud project id, this
/// will update that project unless `--new` is passed.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdProjectUpload {
    /// The project directory, a `.kcl` file within it, or `project.toml`.
    #[clap(name = "input", default_value = ".")]
    pub input: PathBuf,

    /// Always create a new remote project even if one is already persisted locally.
    #[clap(long, default_value = "false")]
    pub new: bool,

    /// Title to use for the cloud project. Defaults to the local project directory name.
    #[clap(long)]
    pub title: Option<String>,

    /// Description to use for the cloud project. Defaults to the existing remote description when updating.
    #[clap(long)]
    pub description: Option<String>,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdProjectUpload {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let local = crate::project::resolve_local_project(&self.input)?;
        let existing_id = if self.new {
            None
        } else {
            crate::project::read_persisted_cloud_project_id(&local.project_toml)?
        };
        let attachments = crate::project::collect_project_attachments(&local.root)?;
        let client = ctx.api_client("")?;

        let project = if let Some(id) = existing_id {
            let existing = client.users().get_project(id).await?;
            let body = ProjectUpsertBody {
                title: self.title.clone().unwrap_or(existing.title),
                description: self.description.clone().unwrap_or(existing.description),
            };
            update_project_with_body(ctx, attachments, id, &body).await?
        } else {
            let body = ProjectUpsertBody {
                title: self.title.clone().unwrap_or_else(|| default_project_title(&local.root)),
                description: self.description.clone().unwrap_or_default(),
            };
            create_project_with_body(ctx, attachments, &body).await?
        };

        crate::project::persist_cloud_project_id(&local.project_toml, project.id)?;
        writeln!(
            ctx.io.out,
            "{} {} Zoo cloud project id {} in {}",
            ctx.io.color_scheme().success_icon(),
            if existing_id.is_some() { "Updated" } else { "Stored" },
            project.id,
            local.project_toml.display()
        )?;

        let format = ctx.format(&self.format)?;
        write_project_output(ctx, &format, &project)?;
        Ok(())
    }
}

/// Replace an existing remote project with your local project files.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdProjectUpdate {
    /// The project directory, a `.kcl` file within it, or `project.toml`.
    #[clap(name = "input", default_value = ".")]
    pub input: PathBuf,

    /// Override the persisted Zoo cloud project id from `project.toml`.
    #[clap(long)]
    pub id: Option<uuid::Uuid>,

    /// Title to use for the cloud project. Defaults to the existing remote title.
    #[clap(long)]
    pub title: Option<String>,

    /// Description to use for the cloud project. Defaults to the existing remote description.
    #[clap(long)]
    pub description: Option<String>,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdProjectUpdate {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let local = crate::project::resolve_local_project(&self.input)?;
        let project_id = match self.id {
            Some(id) => id,
            None => crate::project::read_persisted_cloud_project_id(&local.project_toml)?.with_context(|| {
                format!(
                    "no Zoo cloud project id found in `{}`; pass `--id`",
                    local.project_toml.display()
                )
            })?,
        };
        let attachments = crate::project::collect_project_attachments(&local.root)?;
        let client = ctx.api_client("")?;
        let existing = client.users().get_project(project_id).await?;
        let body = ProjectUpsertBody {
            title: self.title.clone().unwrap_or(existing.title),
            description: self.description.clone().unwrap_or(existing.description),
        };
        let project = update_project_with_body(ctx, attachments, project_id, &body).await?;

        crate::project::persist_cloud_project_id(&local.project_toml, project.id)?;
        writeln!(
            ctx.io.out,
            "{} Stored Zoo cloud project id {} in {}",
            ctx.io.color_scheme().success_icon(),
            project.id,
            local.project_toml.display()
        )?;

        let format = ctx.format(&self.format)?;
        write_project_output(ctx, &format, &project)?;
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct ProjectUpsertBody {
    title: String,
    description: String,
}

fn default_project_title(root: &std::path::Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("project")
        .to_string()
}

fn build_project_form(
    attachments: Vec<kittycad::types::multipart::Attachment>,
    body: &ProjectUpsertBody,
) -> Result<reqwest::multipart::Form> {
    use std::convert::TryInto;

    let mut form = reqwest::multipart::Form::new();
    let mut json_part = reqwest::multipart::Part::text(serde_json::to_string(body)?);
    json_part = json_part.file_name("body.json");
    json_part = json_part.mime_str("application/json")?;
    form = form.part("body", json_part);

    for attachment in attachments {
        form = form.part(attachment.name.clone(), attachment.try_into()?);
    }

    Ok(form)
}

async fn create_project_with_body(
    ctx: &crate::context::Context<'_>,
    attachments: Vec<kittycad::types::multipart::Attachment>,
    body: &ProjectUpsertBody,
) -> Result<kittycad::types::ProjectResponse> {
    let req = ctx.raw_http_request("", reqwest::Method::POST, "/user/projects")?;
    send_project_form(req, attachments, body).await
}

async fn update_project_with_body(
    ctx: &crate::context::Context<'_>,
    attachments: Vec<kittycad::types::multipart::Attachment>,
    id: uuid::Uuid,
    body: &ProjectUpsertBody,
) -> Result<kittycad::types::ProjectResponse> {
    let endpoint = format!("/user/projects/{id}");
    let req = ctx.raw_http_request("", reqwest::Method::PUT, &endpoint)?;
    send_project_form(req, attachments, body).await
}

async fn send_project_form(
    req: reqwest::RequestBuilder,
    attachments: Vec<kittycad::types::multipart::Attachment>,
    body: &ProjectUpsertBody,
) -> Result<kittycad::types::ProjectResponse> {
    let form = build_project_form(attachments, body)?;
    let resp = req.multipart(form).send().await?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        anyhow::bail!("{} {}", status, text);
    }

    serde_json::from_str(&text).with_context(|| format!("failed to parse project response body: {text}"))
}
