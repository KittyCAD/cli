use std::{
    collections::HashSet,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result};
use clap::Parser;

use crate::types::FormatOutput;

const PROJECT_ARCHIVE_ACCEPT: &str = "application/x-tar";

// Keep accepting legacy arrays until all API deployments return Dropshot pages.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum ProjectListResponse<T> {
    Legacy(Vec<T>),
    Page { items: Vec<T>, next_page: Option<String> },
}

async fn fetch_project_list<T: serde::de::DeserializeOwned>(
    client: &kittycad::Client,
    endpoint: &str,
) -> Result<Vec<T>> {
    let mut items = Vec::new();
    let mut page_token: Option<String> = None;
    let mut seen_tokens = HashSet::new();
    loop {
        let mut request = client.request_raw(reqwest::Method::GET, endpoint, None).await?.0;
        if let Some(token) = &page_token {
            request = request.query(&[("page_token", token)]);
        }
        let response = request
            .send()
            .await?
            .error_for_status()?
            .json::<ProjectListResponse<T>>()
            .await?;
        let (page, next_page) = match response {
            ProjectListResponse::Legacy(items) => {
                anyhow::ensure!(
                    page_token.is_none(),
                    "unexpected legacy list after a paginated response"
                );
                return Ok(items);
            }
            ProjectListResponse::Page { items, next_page } => (items, next_page),
        };
        items.extend(page);
        match next_page {
            Some(token) => {
                anyhow::ensure!(
                    !token.is_empty() && seen_tokens.insert(token.clone()),
                    "pagination returned an empty or repeated page token"
                );
                page_token = Some(token);
            }
            None => return Ok(items),
        }
    }
}

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
    Delete(CmdProjectDelete),
    Download(CmdProjectDownload),
    List(CmdProjectList),
    Publish(CmdProjectPublish),
    #[clap(alias = "get")]
    View(CmdProjectView),
    Upload(CmdProjectUpload),
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdProject {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Categories(cmd) => cmd.run(ctx).await,
            SubCommand::Delete(cmd) => cmd.run(ctx).await,
            SubCommand::Download(cmd) => cmd.run(ctx).await,
            SubCommand::List(cmd) => cmd.run(ctx).await,
            SubCommand::Publish(cmd) => cmd.run(ctx).await,
            SubCommand::View(cmd) => cmd.run(ctx).await,
            SubCommand::Upload(cmd) => cmd.run(ctx).await,
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
        let categories =
            fetch_project_list::<kittycad::types::ProjectCategoryResponse>(&client, "/projects/categories").await?;
        let categories = categories
            .into_iter()
            .map(project_category_output_row)
            .collect::<Vec<_>>();
        let format = ctx.format(&self.format)?;
        ctx.io.write_output_for_vec(&format, categories)?;
        Ok(())
    }
}

/// Delete one of your uploaded projects.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdProjectDelete {
    /// The project id, or a local project directory, `.kcl` file, or `project.toml`.
    /// When a local path is provided, the persisted Zoo cloud project id will be removed from
    /// `project.toml` after the remote project is deleted.
    #[clap(name = "id-or-path", required = true)]
    pub input: String,
}

enum ProjectTarget {
    Id(uuid::Uuid),
    Local {
        local: crate::project::LocalProject,
        id: uuid::Uuid,
    },
}

impl ProjectTarget {
    fn id(&self) -> uuid::Uuid {
        match self {
            Self::Id(id) => *id,
            Self::Local { id, .. } => *id,
        }
    }
}

fn resolve_project_target(input: &str, environment: &str) -> Result<ProjectTarget> {
    let path = PathBuf::from(input);
    if input == "." || path.exists() {
        let local = crate::project::resolve_local_project(&path)?;
        let id = crate::project::read_persisted_cloud_project_id(&local.project_toml, environment)?
            .with_context(|| format!("no Zoo cloud project id found in `{}`", local.project_toml.display()))?;
        return Ok(ProjectTarget::Local { local, id });
    }

    if let Ok(id) = uuid::Uuid::parse_str(input) {
        return Ok(ProjectTarget::Id(id));
    }

    anyhow::bail!("input `{input}` must be an existing project path or a project id");
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdProjectDelete {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let environment = ctx.project_cloud_environment_name("")?;
        let target = resolve_project_target(&self.input, &environment)?;
        let id = target.id();

        let client = ctx.api_client("")?;
        client.projects().delete(id).await?;

        if let ProjectTarget::Local { local, .. } = target {
            crate::project::clear_persisted_cloud_project_id(&local.project_toml, &environment)?;
            writeln!(
                ctx.io.out,
                "{} Deleted Zoo cloud project {} and cleared {}",
                ctx.io.color_scheme().success_icon(),
                id,
                local.project_toml.display()
            )?;
        } else {
            writeln!(
                ctx.io.out,
                "{} Deleted Zoo cloud project {}",
                ctx.io.color_scheme().success_icon(),
                id
            )?;
        }

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
        let environment = ctx.project_cloud_environment_name("")?;

        let endpoint = format!("/user/projects/{}/download", self.id);
        let resp = ctx
            .raw_http_request("", reqwest::Method::GET, &endpoint)?
            .header(reqwest::header::ACCEPT, PROJECT_ARCHIVE_ACCEPT)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("{} {}", status, body);
        }

        let body = resp.bytes().await?;
        let project_root = extract_project_archive(body.as_ref(), &self.output_dir)?;
        let project_toml = project_root.join("project.toml");
        crate::project::persist_cloud_project_id(&project_toml, &environment, self.id)?;
        writeln!(
            ctx.io.out,
            "{} Downloaded project {} into {}",
            ctx.io.color_scheme().success_icon(),
            self.id,
            project_root.display()
        )?;

        Ok(())
    }
}

fn extract_project_archive(archive_bytes: &[u8], output_dir: &Path) -> Result<PathBuf> {
    if archive_bytes.is_empty() {
        anyhow::bail!("downloaded project archive was empty");
    }

    let mut archive = tar::Archive::new(std::io::Cursor::new(archive_bytes));
    archive
        .unpack(output_dir)
        .with_context(|| format!("failed to extract archive into `{}`", output_dir.display()))?;

    crate::project::find_project_root_under(output_dir)?.with_context(|| {
        format!(
            "downloaded project archive did not contain a project root under `{}`",
            output_dir.display()
        )
    })
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
struct ProjectCategoryOutputRow {
    description: String,
    display_name: String,
    slug: String,
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

fn project_category_output_row(category: kittycad::types::ProjectCategoryResponse) -> ProjectCategoryOutputRow {
    ProjectCategoryOutputRow {
        description: category.description,
        display_name: category.display_name,
        slug: category.slug,
    }
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
        let projects = fetch_project_list::<kittycad::types::ProjectSummaryResponse>(&client, "/user/projects").await?;
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
    /// The project id, or a local project directory, `.kcl` file, or `project.toml`.
    #[clap(name = "id-or-path", required = true)]
    pub input: String,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdProjectView {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let environment = ctx.project_cloud_environment_name("")?;
        let target = resolve_project_target(&self.input, &environment)?;
        let project_id = target.id();
        let client = ctx.api_client("")?;
        let project = client.projects().get(project_id).await?;
        let format = ctx.format(&self.format)?;
        write_project_output(ctx, &format, &project)?;
        Ok(())
    }
}

/// Submit an existing cloud project for publication review.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdProjectPublish {
    /// The project id, or a local project directory, `.kcl` file, or `project.toml`.
    #[clap(name = "id-or-path", required = true)]
    pub input: String,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdProjectPublish {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let format = ctx.format(&self.format)?;
        let environment = ctx.project_cloud_environment_name("")?;
        let target = resolve_project_target(&self.input, &environment)?;
        let project_id = target.id();

        let client = ctx.api_client("")?;
        let project = client.projects().publish(project_id).await?;

        if let ProjectTarget::Local { local, .. } = target {
            crate::project::persist_cloud_project_id(&local.project_toml, &environment, project.id)?;
        }
        ctx.io.write_status(
            &format,
            format_args!(
                "{} Submitted Zoo cloud project {} for publication review",
                ctx.io.color_scheme().success_icon(),
                project.id
            ),
        )?;

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
    #[clap(long, default_value = "false", conflicts_with = "id")]
    pub new: bool,

    /// Override the persisted Zoo cloud project id from `project.toml`.
    #[clap(long, conflicts_with = "new")]
    pub id: Option<uuid::Uuid>,

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
        let format = ctx.format(&self.format)?;
        let local = crate::project::resolve_local_project(&self.input)?;
        let environment = ctx.project_cloud_environment_name("")?;
        let existing_id = match self.id {
            Some(id) => Some(id),
            None if self.new => None,
            None => crate::project::read_persisted_cloud_project_id(&local.project_toml, &environment)?,
        };
        let attachments = crate::project::collect_project_attachments(&local.root)?;
        let client = ctx.api_client("")?;

        let project = if let Some(id) = existing_id {
            let existing = client.projects().get(id).await?;
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

        crate::project::persist_cloud_project_id(&local.project_toml, &environment, project.id)?;
        ctx.io.write_status(
            &format,
            format_args!(
                "{} {} Zoo cloud project id {} in {}",
                ctx.io.color_scheme().success_icon(),
                if existing_id.is_some() { "Updated" } else { "Stored" },
                project.id,
                local.project_toml.display()
            ),
        )?;

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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[derive(serde::Serialize)]
    struct TestPage<'a, T> {
        items: &'a [T],
        next_page: Option<&'a str>,
    }

    #[derive(serde::Deserialize)]
    struct ListOutput {
        description: String,
    }

    async fn run_list_command(
        command: &CmdProject,
        responses: Vec<Option<(u16, String)>>,
    ) -> (Result<()>, String, Vec<String>) {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

        use crate::{cmd::Command, config::Config};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host = format!("http://{}/custom-api", listener.local_addr().unwrap());
        let (requests_tx, requests_rx) = std::sync::mpsc::channel();
        let server = tokio::spawn(async move {
            let mut responses = responses.into_iter();
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let mut reader = tokio::io::BufReader::new(stream);
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    assert_ne!(reader.read_line(&mut line).await.unwrap(), 0);
                    request.push_str(&line);
                    if line == "\r\n" {
                        break;
                    }
                }
                requests_tx.send(request).unwrap();
                let Some((status, body)) = responses.next().unwrap_or(Some((418, String::new()))) else {
                    continue;
                };
                let response = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                reader.get_mut().write_all(response.as_bytes()).await.unwrap();
            }
        });
        let mut config = crate::config::new_blank_config().unwrap();
        config.set(&host, "token", Some("cli-pagination-test")).unwrap();
        let (mut io, stdout_path, stderr_path) = crate::iostreams::IoStreams::test();
        io.set_color_enabled(false);
        let mut ctx = crate::context::Context {
            config: &mut config,
            io,
            debug: false,
            override_host: Some(host),
        };
        let result = tokio::time::timeout(std::time::Duration::from_secs(10), command.run(&mut ctx))
            .await
            .expect("list command should finish");
        server.abort();
        let _ = server.await;
        drop(ctx);
        let stdout = std::fs::read_to_string(&stdout_path).unwrap();
        assert!(std::fs::read_to_string(&stderr_path).unwrap().is_empty());
        std::fs::remove_file(stdout_path).unwrap();
        std::fs::remove_file(stderr_path).unwrap();
        (result, stdout, requests_rx.try_iter().collect())
    }

    async fn assert_list_compatibility<T: serde::Serialize>(command: CmdProject, endpoint: &str, items: Vec<T>) {
        let first_cursor = "opaque +/=?&cursor";
        let second_cursor = "after-empty-page";
        let first_page = serde_json::to_string(&TestPage {
            items: &items[..1],
            next_page: Some(first_cursor),
        })
        .unwrap();
        let empty_page = serde_json::to_string(&TestPage::<T> {
            items: &[],
            next_page: Some(second_cursor),
        })
        .unwrap();
        let last_page = serde_json::to_string(&TestPage {
            items: &items[1..],
            next_page: None,
        })
        .unwrap();
        for responses in [
            vec![(200, serde_json::to_string(&items).unwrap())],
            vec![(200, first_page.clone()), (200, empty_page), (200, last_page)],
        ] {
            let page_count = responses.len();
            let (result, stdout, requests) =
                run_list_command(&command, responses.into_iter().map(Some).collect()).await;
            result.unwrap();
            let output: Vec<ListOutput> = serde_json::from_str(&stdout).unwrap();
            assert_eq!(
                output.iter().map(|row| row.description.as_str()).collect::<Vec<_>>(),
                ["one", "two"]
            );
            assert_eq!(requests.len(), page_count);
            for (index, request) in requests.iter().enumerate() {
                assert!(
                    request
                        .to_ascii_lowercase()
                        .contains("authorization: bearer cli-pagination-test\r\n")
                );
                let target = request.lines().next().unwrap().split_whitespace().nth(1).unwrap();
                let url = url::Url::parse(&format!("http://localhost{target}")).unwrap();
                assert_eq!(url.path(), format!("/custom-api{endpoint}"));
                let query = url.query_pairs().collect::<Vec<_>>();
                match index {
                    0 => assert!(query.is_empty()),
                    1 => assert_eq!(query, [("page_token".into(), first_cursor.into())]),
                    2 => assert_eq!(query, [("page_token".into(), second_cursor.into())]),
                    _ => panic!("unexpected extra page"),
                }
            }
        }
        let empty_cursor_page = serde_json::to_string(&TestPage {
            items: &items[..1],
            next_page: Some(""),
        })
        .unwrap();
        for (responses, expected_error) in [
            (vec![(401, "unauthorized".into())], "401"),
            (vec![(403, "permission denied".into())], "403"),
            (vec![(404, "not found".into())], "404"),
            // Keep the configured SDK retry behavior without ever printing partial results.
            (vec![(503, "unavailable".into()); 4], "503"),
            (vec![(200, first_page.clone()), (404, "not found".into())], "404"),
            (
                vec![(200, first_page.clone()), (403, "permission denied".into())],
                "403",
            ),
            (vec![(200, empty_cursor_page)], "empty or repeated page token"),
            (
                vec![(200, first_page.clone()), (200, first_page.clone())],
                "empty or repeated page token",
            ),
            (
                vec![(200, first_page), (200, serde_json::to_string(&items).unwrap())],
                "unexpected legacy list after a paginated response",
            ),
        ] {
            let page_count = responses.len();
            let (result, stdout, requests) =
                run_list_command(&command, responses.into_iter().map(Some).collect()).await;
            let error = result.unwrap_err();
            assert!(
                format!("{error:#}").contains(expected_error),
                "unexpected error: {error:#}"
            );
            assert_eq!(requests.len(), page_count);
            assert!(
                requests
                    .iter()
                    .all(|request| { request.starts_with(&format!("GET /custom-api{endpoint}")) })
            );
            assert!(stdout.is_empty(), "a failed traversal must not print partial results");
        }
        // A dropped connection is retried by the SDK and still leaves stdout empty.
        let (result, stdout, requests) = run_list_command(&command, vec![None; 4]).await;
        assert!(result.is_err());
        assert!(stdout.is_empty());
        assert_eq!(requests.len(), 4);
        assert!(
            requests
                .iter()
                .all(|request| { request.starts_with(&format!("GET /custom-api{endpoint} ")) })
        );
    }

    #[tokio::test]
    async fn project_categories_accept_arrays_and_pages() {
        let categories = ["one", "two"]
            .map(|name| kittycad::types::ProjectCategoryResponse {
                description: name.into(),
                display_name: name.into(),
                id: uuid::Uuid::new_v4(),
                slug: name.into(),
                sort_order: 0,
            })
            .to_vec();
        assert_list_compatibility(
            CmdProject {
                subcmd: SubCommand::Categories(CmdProjectCategories {
                    format: Some(FormatOutput::Json),
                }),
            },
            "/projects/categories",
            categories,
        )
        .await;
    }

    #[tokio::test]
    async fn project_list_accepts_arrays_and_pages() {
        use kittycad::types::*;
        let projects = ["one", "two"]
            .map(|name| ProjectSummaryResponse {
                access: ProjectAccessResponse {
                    can_delete: true,
                    can_edit: true,
                    can_manage_organization: false,
                    organization_id: None,
                    scope: ProjectAccessScope::Personal,
                },
                category_ids: Vec::new(),
                created_at: chrono::Utc::now(),
                description: name.into(),
                entrypoint_path: "main.kcl".into(),
                id: uuid::Uuid::new_v4(),
                preview_status: KclProjectPreviewStatus::Pending,
                preview_url: None,
                project_toml_path: "project.toml".into(),
                publication: ProjectPublicationInfoResponse {
                    feedback: None,
                    has_unpublished_changes: false,
                    last_published_at: None,
                    last_published_version_id: None,
                    submitted_at: None,
                },
                publication_status: KclProjectPublicationStatus::Private,
                revision: "revision".into(),
                title: name.into(),
                updated_at: chrono::Utc::now(),
            })
            .to_vec();
        assert_list_compatibility(
            CmdProject {
                subcmd: SubCommand::List(CmdProjectList {
                    format: Some(FormatOutput::Json),
                }),
            },
            "/user/projects",
            projects,
        )
        .await;
    }

    fn build_project_archive(files: &[(&str, &str)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut builder = tar::Builder::new(&mut bytes);

        for (path, contents) in files {
            let mut header = tar::Header::new_gnu();
            header.set_path(path).expect("set path");
            header.set_mode(0o644);
            header.set_size(contents.len() as u64);
            header.set_cksum();
            builder.append(&header, contents.as_bytes()).expect("append file");
        }

        builder.finish().expect("finish archive");
        drop(builder);
        bytes
    }

    #[test]
    fn resolve_project_target_accepts_uuid() {
        let id = uuid::Uuid::new_v4();

        let target = resolve_project_target(&id.to_string(), "zoo.dev").expect("resolve project target");

        match target {
            ProjectTarget::Id(got) => assert_eq!(got, id),
            ProjectTarget::Local { .. } => panic!("expected uuid target"),
        }
    }

    #[test]
    fn resolve_project_target_accepts_project_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("main.kcl"), "cube(1)\n").expect("write main");
        let project_toml = tmp.path().join("project.toml");
        let id = uuid::Uuid::new_v4();
        crate::project::persist_cloud_project_id(&project_toml, "zoo.dev", id).expect("persist cloud project id");

        let target =
            resolve_project_target(tmp.path().to_str().expect("path utf8"), "zoo.dev").expect("resolve project target");

        match target {
            ProjectTarget::Local { local, id: got } => {
                assert_eq!(got, id);
                assert_eq!(local.root, PathBuf::from(tmp.path()));
                assert_eq!(local.project_toml, project_toml);
            }
            ProjectTarget::Id(_) => panic!("expected local target"),
        }
    }

    #[test]
    fn extract_project_archive_rejects_empty_archive() {
        let tmp = tempfile::tempdir().expect("tempdir");

        let err = extract_project_archive(&[], tmp.path()).expect_err("empty archive should fail");

        assert!(
            err.to_string().contains("archive was empty"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn extract_project_archive_returns_project_root() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let archive = build_project_archive(&[
            ("downloaded-project/main.kcl", "cube(1)\n"),
            ("downloaded-project/project.toml", ""),
            ("downloaded-project/readme.md", "hello\n"),
        ]);

        let project_root = extract_project_archive(&archive, tmp.path()).expect("extract project archive");

        assert_eq!(project_root, tmp.path().join("downloaded-project"));
        assert!(project_root.join("main.kcl").is_file());
    }
}
