use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result};

pub struct LocalProject {
    pub root: PathBuf,
    pub project_toml: PathBuf,
}

const ZOO_TABLE_KEY: &str = "zoo";
const ZOO_PROJECT_ID_KEY: &str = "project_id";

pub fn resolve_local_project(input: &Path) -> Result<LocalProject> {
    let input = normalize_input_path(input)?;

    let root = if input.is_dir() {
        if let Some(project_toml) = crate::cmd_kcl::find_project_toml(&input)? {
            project_toml
                .parent()
                .context("project.toml is missing a parent directory")?
                .to_path_buf()
        } else if input.join("main.kcl").exists() {
            input
        } else {
            anyhow::bail!(
                "directory `{}` does not contain a main.kcl file or a project.toml file",
                input.display()
            );
        }
    } else if input.file_name().and_then(|name| name.to_str()) == Some("project.toml") {
        input
            .parent()
            .context("project.toml is missing a parent directory")?
            .to_path_buf()
    } else if input.extension().and_then(|ext| ext.to_str()) == Some("kcl") {
        if let Some(parent) = input.parent() {
            if let Some(project_toml) = crate::cmd_kcl::find_project_toml(parent)? {
                project_toml
                    .parent()
                    .context("project.toml is missing a parent directory")?
                    .to_path_buf()
            } else {
                parent.to_path_buf()
            }
        } else {
            anyhow::bail!("could not determine project root from `{}`", input.display());
        }
    } else {
        anyhow::bail!(
            "input `{}` must be a directory, a `.kcl` file, or a `project.toml` file",
            input.display()
        );
    };

    if !root.join("main.kcl").exists() {
        anyhow::bail!("project root `{}` does not contain a main.kcl file", root.display());
    }

    let project_toml = ensure_project_toml(&root)?;

    Ok(LocalProject { root, project_toml })
}

pub fn read_persisted_cloud_project_id(project_toml: &Path) -> Result<Option<uuid::Uuid>> {
    if !project_toml.exists() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(project_toml)
        .with_context(|| format!("failed to read `{}`", project_toml.display()))?;
    let doc = contents
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("failed to parse `{}`", project_toml.display()))?;

    let Some(project_id) = doc
        .get(ZOO_TABLE_KEY)
        .and_then(|item| item.get(ZOO_PROJECT_ID_KEY))
        .and_then(|item| item.as_str())
    else {
        return Ok(None);
    };

    Ok(Some(uuid::Uuid::parse_str(project_id).with_context(|| {
        format!(
            "failed to parse `{}.{}` in `{}` as a UUID",
            ZOO_TABLE_KEY,
            ZOO_PROJECT_ID_KEY,
            project_toml.display()
        )
    })?))
}

pub fn persist_cloud_project_id(project_toml: &Path, id: uuid::Uuid) -> Result<()> {
    let existing = if project_toml.exists() {
        std::fs::read_to_string(project_toml).with_context(|| format!("failed to read `{}`", project_toml.display()))?
    } else {
        toml::to_string(&kcl_lib::ProjectConfiguration::default())?
    };

    let mut doc = existing
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("failed to parse `{}`", project_toml.display()))?;

    let has_zoo_table = matches!(doc.get(ZOO_TABLE_KEY), Some(item) if item.is_table());
    if !has_zoo_table {
        doc.insert(ZOO_TABLE_KEY, toml_edit::Item::Table(toml_edit::Table::new()));
    }
    doc[ZOO_TABLE_KEY][ZOO_PROJECT_ID_KEY] = toml_edit::value(id.to_string());

    std::fs::write(project_toml, doc.to_string())
        .with_context(|| format!("failed to write `{}`", project_toml.display()))?;

    Ok(())
}

pub fn collect_project_attachments(root: &Path) -> Result<Vec<kittycad::types::multipart::Attachment>> {
    let mut dirs = VecDeque::from([root.to_path_buf()]);
    let mut files = Vec::new();

    while let Some(dir) = dirs.pop_front() {
        for entry in std::fs::read_dir(&dir).with_context(|| format!("failed to read `{}`", dir.display()))? {
            let entry = entry.with_context(|| format!("failed to inspect entry in `{}`", dir.display()))?;
            let file_type = entry
                .file_type()
                .with_context(|| format!("failed to inspect `{}`", entry.path().display()))?;

            if file_type.is_symlink() {
                continue;
            }

            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();

            if file_type.is_dir() {
                if should_skip_dir(&name) {
                    continue;
                }
                dirs.push_back(path);
                continue;
            }

            if file_type.is_file() {
                files.push(path);
            }
        }
    }

    files.sort();

    files.into_iter().map(|path| build_attachment(root, &path)).collect()
}

pub fn find_project_root_under(base: &Path) -> Result<Option<PathBuf>> {
    let mut dirs = VecDeque::from([base.to_path_buf()]);
    let mut matches = Vec::new();

    while let Some(dir) = dirs.pop_front() {
        if dir.join("main.kcl").exists() {
            matches.push(dir.clone());
        }

        for entry in std::fs::read_dir(&dir).with_context(|| format!("failed to read `{}`", dir.display()))? {
            let entry = entry.with_context(|| format!("failed to inspect entry in `{}`", dir.display()))?;
            let file_type = entry
                .file_type()
                .with_context(|| format!("failed to inspect `{}`", entry.path().display()))?;
            if file_type.is_dir() && !file_type.is_symlink() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if should_skip_dir(&name) {
                    continue;
                }
                dirs.push_back(entry.path());
            }
        }
    }

    matches.sort();
    Ok(matches.into_iter().next())
}

pub fn ensure_download_destination(output_dir: &Path, force: bool) -> Result<()> {
    if output_dir.exists() {
        let metadata =
            std::fs::metadata(output_dir).with_context(|| format!("failed to inspect `{}`", output_dir.display()))?;
        if !metadata.is_dir() {
            anyhow::bail!("download destination `{}` is not a directory", output_dir.display());
        }

        let mut entries =
            std::fs::read_dir(output_dir).with_context(|| format!("failed to read `{}`", output_dir.display()))?;
        if !force && entries.next().transpose()?.is_some() {
            anyhow::bail!(
                "download destination `{}` is not empty; pass `--force` to overwrite existing files",
                output_dir.display()
            );
        }
    } else {
        std::fs::create_dir_all(output_dir).with_context(|| format!("failed to create `{}`", output_dir.display()))?;
    }

    Ok(())
}

fn build_attachment(root: &Path, path: &Path) -> Result<kittycad::types::multipart::Attachment> {
    let mut attachment = kittycad::types::multipart::Attachment::try_from(path.to_path_buf())
        .with_context(|| format!("failed to read `{}`", path.display()))?;
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("failed to strip `{}` from `{}`", root.display(), path.display()))?;

    let relative = relative.to_path_buf();
    attachment.name = relative.to_string_lossy().to_string();
    attachment.filepath = Some(relative);
    Ok(attachment)
}

fn ensure_project_toml(root: &Path) -> Result<PathBuf> {
    let path = root.join("project.toml");
    if path.exists() {
        return Ok(path);
    }

    let contents = toml::to_string(&kcl_lib::ProjectConfiguration::default())?;
    std::fs::write(&path, contents).with_context(|| format!("failed to create `{}`", path.display()))?;
    Ok(path)
}

fn normalize_input_path(input: &Path) -> Result<PathBuf> {
    if input == Path::new(".") {
        Ok(std::env::current_dir()?)
    } else {
        Ok(input.to_path_buf())
    }
}

fn should_skip_dir(name: &str) -> bool {
    matches!(name, ".git" | ".jj" | "target" | "node_modules")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persist_cloud_project_id_round_trip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("main.kcl"), "cube(1)\n").expect("write main");

        let project = resolve_local_project(tmp.path()).expect("resolve project");
        let id = uuid::Uuid::new_v4();
        persist_cloud_project_id(&project.project_toml, id).expect("persist cloud project id");

        let got = read_persisted_cloud_project_id(&project.project_toml).expect("read cloud project id");
        assert_eq!(got, Some(id));
    }

    #[test]
    fn persist_cloud_project_id_does_not_overwrite_local_project_id() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let local_id = uuid::Uuid::new_v4();
        let cloud_id = uuid::Uuid::new_v4();
        std::fs::write(
            tmp.path().join("project.toml"),
            format!(
                "[settings.meta]\nid = \"{local_id}\"\n\n[settings.app]\n\n[settings.modeling]\n\n[settings.text_editor]\n\n[settings.command_bar]\n"
            ),
        )
        .expect("write project.toml");

        persist_cloud_project_id(&tmp.path().join("project.toml"), cloud_id).expect("persist cloud project id");

        let contents = std::fs::read_to_string(tmp.path().join("project.toml")).expect("read project.toml");
        let parsed: kcl_lib::ProjectConfiguration = toml::from_str(&contents).expect("parse project config");
        assert_eq!(parsed.settings.meta.id, local_id);

        let got = read_persisted_cloud_project_id(&tmp.path().join("project.toml")).expect("read cloud project id");
        assert_eq!(got, Some(cloud_id));
    }

    #[test]
    fn collect_project_attachments_uses_relative_paths() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("subdir")).expect("mkdir");
        std::fs::write(tmp.path().join("main.kcl"), "cube(1)\n").expect("write main");
        std::fs::write(tmp.path().join("subdir/part.kcl"), "cube(2)\n").expect("write part");

        let project = resolve_local_project(tmp.path()).expect("resolve project");
        let attachments = collect_project_attachments(&project.root).expect("collect attachments");

        let mut paths = attachments
            .iter()
            .filter_map(|attachment| attachment.filepath.as_ref())
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        paths.sort();

        assert_eq!(paths, vec!["main.kcl", "project.toml", "subdir/part.kcl"]);
    }

    #[test]
    fn find_project_root_under_prefers_the_project_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("downloaded-project/subdir")).expect("mkdir");
        std::fs::write(tmp.path().join("downloaded-project/main.kcl"), "cube(1)\n").expect("write main");

        let found = find_project_root_under(tmp.path()).expect("find project root");
        assert_eq!(found, Some(tmp.path().join("downloaded-project")));
    }

    #[test]
    fn ensure_download_destination_rejects_non_empty_dir_without_force() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("main.kcl"), "cube(1)\n").expect("write main");

        let err = ensure_download_destination(tmp.path(), false).expect_err("should reject non-empty dir");
        assert!(err.to_string().contains("pass `--force`"), "unexpected error: {err:#}");
    }

    #[test]
    fn ensure_download_destination_allows_non_empty_dir_with_force() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("main.kcl"), "cube(1)\n").expect("write main");

        ensure_download_destination(tmp.path(), true).expect("should allow non-empty dir with force");
    }

    #[test]
    fn ensure_download_destination_creates_missing_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let output_dir = tmp.path().join("downloaded-project");

        ensure_download_destination(&output_dir, false).expect("create missing output dir");

        assert!(output_dir.is_dir());
    }
}
