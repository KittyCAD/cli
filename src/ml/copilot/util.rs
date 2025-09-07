// keep Result import only when used
use std::path::{Component, Path, PathBuf};

#[allow(unused_imports)]
use anyhow::Result;

// Public constants to keep scan behavior configurable in one place.
pub(crate) const SCAN_MAX_DEPTH: usize = 256;
pub(crate) const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules"];

/// Join `base` with `candidate` and ensure the resulting path stays under `base`.
///
/// Security notes:
/// - `base` must canonicalize (we return an error if it doesn't) so ancestry checks are stable.
/// - We lexically build the joined path and verify it never escapes `base` while processing.
/// - Finally, we canonicalize the nearest existing ancestor to prevent symlink escapes.
pub(crate) fn join_secure(base: &Path, candidate: &Path) -> anyhow::Result<PathBuf> {
    let base_canon = std::fs::canonicalize(base)
        .map_err(|e| anyhow::anyhow!("failed to canonicalize base '{}': {e}", base.display()))?;

    let mut out_lex = base_canon.clone();
    for comp in candidate.components() {
        match comp {
            Component::Prefix(_) | Component::RootDir => anyhow::bail!("absolute paths are not allowed"),
            Component::ParentDir => {
                if !out_lex.pop() || !out_lex.starts_with(&base_canon) {
                    anyhow::bail!("path escapes project root")
                }
            }
            Component::CurDir => {}
            Component::Normal(seg) => {
                out_lex.push(seg);
                if !out_lex.starts_with(&base_canon) {
                    anyhow::bail!("path escapes project root")
                }
            }
        }
    }

    // Resolve the nearest existing ancestor to detect symlink escapes.
    let mut probe = out_lex.clone();
    while !probe.exists() {
        if !probe.pop() {
            break;
        }
    }
    if probe.exists() {
        let probe_canon = std::fs::canonicalize(&probe)
            .map_err(|e| anyhow::anyhow!("failed to canonicalize ancestor '{}': {e}", probe.display()))?;
        if !probe_canon.starts_with(&base_canon) {
            anyhow::bail!("path escapes project root via symlink")
        }
    }

    Ok(out_lex)
}

/// Walk `root` and collect files with extensions in `kcl_lib::RELEVANT_FILE_EXTENSIONS`.
///
/// Iterative DFS, limited depth, and skipping symlinks to avoid cycles.
pub(crate) fn scan_relevant_files(root: &Path) -> std::collections::HashMap<String, Vec<u8>> {
    let mut out = std::collections::HashMap::new();
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];

    while let Some((dir, depth)) = stack.pop() {
        if depth >= SCAN_MAX_DEPTH {
            continue;
        }
        // Re-check the popped path to avoid simple TOCTOU issues.
        if let Ok(md) = std::fs::symlink_metadata(&dir) {
            let ft = md.file_type();
            if !ft.is_dir() || ft.is_symlink() {
                continue;
            }
        } else {
            continue;
        }

        let rd = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for ent in rd.flatten() {
            let path = ent.path();
            let name = ent.file_name().to_string_lossy().to_string();
            let ft = match ent.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            // Avoid following symlinks to prevent cycles (cross-platform)
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                if SKIP_DIRS.contains(&name.as_str()) || name.starts_with('.') {
                    continue;
                }
                stack.push((path, depth + 1));
            } else if ft.is_file() {
                let is_relevant = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_ascii_lowercase())
                    .map(|e| kcl_lib::RELEVANT_FILE_EXTENSIONS.contains(&e))
                    .unwrap_or(false);
                if is_relevant {
                    let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
                    if let Ok(bytes) = std::fs::read(&path) {
                        out.insert(rel, bytes);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn join_secure_rejects_absolute_and_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let abs = if cfg!(windows) {
            Path::new("C:/windows/system32")
        } else {
            Path::new("/etc/passwd")
        };
        assert!(join_secure(root, abs).is_err());
        assert!(join_secure(root, Path::new("../../evil.txt")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn join_secure_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let base = tmp.path();
        std::fs::create_dir(base.join("linkdir")).unwrap();
        symlink(outside.path(), base.join("linkdir/outside")).unwrap();
        let candidate = Path::new("linkdir/outside/file.txt");
        assert!(join_secure(base, candidate).is_err());
    }

    #[test]
    fn scan_only_relevant_file_extensions() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let root = tmp.path();
        // Relevant
        std::fs::write(root.join("main.kcl"), b"cube(1)").unwrap();
        std::fs::write(root.join("foo.KCL"), b"sphere(2)").unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/bar.kcl"), b"cylinder(3)").unwrap();
        // Irrelevant
        std::fs::write(root.join("README.md"), b"docs").unwrap();
        std::fs::create_dir(root.join("target")).unwrap();
        std::fs::write(root.join("target/skip.kcl"), b"nope").unwrap();
        let files = scan_relevant_files(root);
        let mut keys: Vec<_> = files.keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, vec!["foo.KCL", "main.kcl", "sub/bar.kcl"]);
        assert_eq!(files.get("main.kcl").unwrap(), b"cube(1)");
    }

    #[cfg(unix)]
    #[test]
    fn scan_skips_symlink_loops_unix() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().expect("create tempdir");
        let root = tmp.path();
        std::fs::create_dir(root.join("dir")).unwrap();
        symlink(root, root.join("dir/link")).unwrap();
        std::fs::write(root.join("main.kcl"), b"cube(1)\n").unwrap();
        let files = scan_relevant_files(root);
        assert!(files.keys().any(|k| k == "main.kcl"));
    }

    #[test]
    fn scan_depth_limit_skips_beyond() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let mut cur = tmp.path().to_path_buf();
        for _ in 0..(SCAN_MAX_DEPTH + 10) {
            cur.push("d");
            std::fs::create_dir(&cur).unwrap();
        }
        std::fs::write(cur.join("main.kcl"), b"cube(9)\n").unwrap();
        let files = scan_relevant_files(tmp.path());
        assert!(files.keys().all(|k| !k.ends_with("main.kcl")));
    }
}
