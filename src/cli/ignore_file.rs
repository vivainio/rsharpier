//! .csharpierignore / .gitignore handling

use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::Match;

const ALWAYS_IGNORED: [&str; 4] = ["**/bin", "**/node_modules", "**/obj", "**/.git"];

fn build(base: &Path, file: Option<&Path>) -> Result<Gitignore, String> {
    let mut builder = GitignoreBuilder::new(base);
    for p in ALWAYS_IGNORED {
        builder.add_line(None, p).map_err(|e| e.to_string())?;
    }
    if let Some(f) = file {
        let content = std::fs::read_to_string(f).map_err(|e| e.to_string())?;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let _ = builder.add_line(None, line);
        }
    }
    builder.build().map_err(|e| e.to_string())
}

pub struct IgnoreFile {
    ignores: Vec<Gitignore>,
}

impl IgnoreFile {
    /// the ignore file for a directory: the first .csharpierignore at or above, then every
    /// .gitignore up to the repository root, closest first
    pub fn create(base_dir: &Path, ignore_path: Option<&Path>) -> Result<IgnoreFile, String> {
        if let Some(p) = ignore_path {
            if !p.is_file() {
                return Err(format!("There was no ignore file found at {}", p.display()));
            }
            return Ok(IgnoreFile { ignores: vec![build(base_dir, Some(p))?] });
        }

        let mut csharpier_ignore: Option<PathBuf> = None;
        let mut git_ignores = Vec::new();
        let mut include_git_ignores = true;
        let mut cur = Some(base_dir.to_path_buf());
        while let Some(d) = cur {
            if csharpier_ignore.is_none() {
                let p = d.join(".csharpierignore");
                if p.is_file() {
                    csharpier_ignore = Some(p);
                }
            }
            if include_git_ignores {
                let p = d.join(".gitignore");
                if p.is_file() {
                    git_ignores.push(p);
                }
            }
            if d.join(".git").exists() {
                include_git_ignores = false;
            }
            cur = d.parent().map(|p| p.to_path_buf());
        }

        let mut paths = Vec::new();
        if let Some(c) = csharpier_ignore {
            paths.push(c);
        }
        paths.extend(git_ignores);
        if paths.is_empty() {
            return Ok(IgnoreFile { ignores: vec![build(base_dir, None)?] });
        }
        let mut ignores = Vec::new();
        for p in paths {
            ignores.push(build(p.parent().unwrap_or(Path::new("")), Some(&p))?);
        }
        Ok(IgnoreFile { ignores })
    }

    pub fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        for ig in &self.ignores {
            // only rules of an ignore file that actually match the path decide
            if !path.starts_with(ig.path()) {
                continue;
            }
            match ig.matched_path_or_any_parents(path, is_dir) {
                Match::Ignore(_) => return true,
                Match::Whitelist(_) => return false,
                Match::None => {}
            }
        }
        false
    }
}
