use crate::exclude_file::Entry;
use crate::git::{self, RepoContext};
use crate::matching::wildcard_match;
use anyhow::{anyhow, Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use walkdir::WalkDir;

const SHADOW_AUTHOR: &str = "layer <layer@layer.local>";
pub const SHADOW_INIT_MESSAGE: &str = "layer: init history tracking";

#[derive(Debug, Clone)]
pub struct ShadowRepo {
    pub git_dir: PathBuf,
    pub work_tree: PathBuf,
}

impl ShadowRepo {
    pub fn open(repo_root: &Path) -> Option<Self> {
        let git_dir = repo_root.join(".layer");
        if git_dir.join("HEAD").exists() {
            Some(Self {
                git_dir,
                work_tree: repo_root.to_path_buf(),
            })
        } else {
            None
        }
    }

    pub fn init(repo_root: &Path) -> Result<Self> {
        let layer_dir = repo_root.join(".layer");

        git_stdout_simple(&["init", "--bare", &layer_dir.to_string_lossy()])?;

        let shadow = Self {
            git_dir: layer_dir.clone(),
            work_tree: repo_root.to_path_buf(),
        };

        shadow.shadow_git(&["config", "core.worktree", &repo_root.to_string_lossy()])?;

        let exclude_dir = layer_dir.join("info");
        std::fs::create_dir_all(&exclude_dir)
            .with_context(|| format!("failed to create {}", exclude_dir.display()))?;
        std::fs::write(exclude_dir.join("exclude"), "*\n")
            .context("failed to write shadow exclude")?;

        shadow.shadow_git(&[
            "commit",
            "--allow-empty",
            "-m",
            SHADOW_INIT_MESSAGE,
            "--author",
            SHADOW_AUTHOR,
        ])?;

        // Ensure .layer/ is in prefix (not managed section) so layer commands ignore it
        let ctx = git::ensure_repo()?;
        let mut exclude = crate::exclude_file::ensure_exclude_file_for_write(&ctx.exclude_path)?;

        let in_prefix = exclude.prefix.iter().any(|l| l.trim() == ".layer/");
        let in_managed = exclude.managed.iter().any(|l| {
            let t = l.trim();
            t == ".layer/" || t == "# [off] .layer/"
        });

        let mut changed = false;
        // Remove from managed if it ended up there
        if in_managed {
            exclude
                .managed
                .retain(|l| l.trim() != ".layer/" && l.trim() != "# [off] .layer/");
            changed = true;
        }
        // Ensure it's in prefix
        if !in_prefix {
            exclude.prefix.push(".layer/".to_string());
            changed = true;
        }
        if changed {
            exclude.write(&ctx.exclude_path)?;
        }

        Ok(shadow)
    }

    fn shadow_command(&self) -> Command {
        let git_dir_arg = format!("--git-dir={}", self.git_dir.display());
        let work_tree_arg = format!("--work-tree={}", self.work_tree.display());

        let mut cmd = Command::new("git");
        cmd.arg(git_dir_arg).arg(work_tree_arg);
        cmd
    }

    fn run_shadow_git(&self, args: &[&str], allowed_nonzero: &[i32]) -> Result<Output> {
        let output = self
            .shadow_command()
            .args(args)
            .output()
            .with_context(|| format!("failed to run shadow git {}", args.join(" ")))?;

        if output.status.success() {
            return Ok(output);
        }

        if let Some(code) = output.status.code() {
            if allowed_nonzero.contains(&code) {
                return Ok(output);
            }
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!(
            "shadow git {} failed: {}",
            args.join(" "),
            stderr.trim()
        ))
    }

    pub fn shadow_git(&self, args: &[&str]) -> Result<String> {
        let output = self.run_shadow_git(args, &[])?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub fn shadow_git_bytes(&self, args: &[&str]) -> Result<Vec<u8>> {
        let output = self.run_shadow_git(args, &[])?;
        Ok(output.stdout)
    }

    pub fn track_files(&self, files: &[String]) -> Result<()> {
        let filtered: Vec<&String> = files
            .iter()
            .filter(|f| !f.starts_with(".layer/") && *f != ".layer")
            .collect();

        if filtered.is_empty() {
            return Ok(());
        }

        let mut args: Vec<&str> = vec!["add", "--all", "--force", "--"];
        for f in &filtered {
            args.push(f);
        }
        self.shadow_git(&args)?;
        Ok(())
    }

    pub fn has_staged_changes(&self, files: &[String]) -> Result<bool> {
        if files.is_empty() {
            return Ok(false);
        }

        let mut args: Vec<&str> = vec!["diff", "--cached", "--quiet", "HEAD", "--"];
        for file in files {
            args.push(file);
        }

        let output = self.run_shadow_git(&args, &[1])?;
        Ok(output.status.code() == Some(1))
    }

    pub fn snapshot_paths(&self, message: &str, files: &[String]) -> Result<bool> {
        if !self.has_staged_changes(files)? {
            return Ok(false);
        }

        let mut args = vec!["commit", "-m", message, "--author", SHADOW_AUTHOR, "--"];
        for file in files {
            args.push(file);
        }
        self.shadow_git(&args)?;
        Ok(true)
    }

    pub fn tracked_files(&self) -> Result<Vec<String>> {
        let output = self.shadow_git(&["ls-tree", "-r", "--name-only", "HEAD"])?;
        Ok(output
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.trim().to_string())
            .collect())
    }

    pub fn pending_snapshot_files(&self, current_files: &[String]) -> Result<Vec<String>> {
        if current_files.is_empty() {
            return Ok(Vec::new());
        }

        let tracked_files: HashSet<String> = self.tracked_files()?.into_iter().collect();
        let mut pending = Vec::new();
        let mut diff_targets = Vec::new();

        for file in current_files {
            if tracked_files.contains(file) {
                diff_targets.push(file.as_str());
            } else {
                pending.push(file.clone());
            }
        }

        if !diff_targets.is_empty() {
            let mut args = vec!["diff", "--name-only", "HEAD", "--"];
            args.extend(diff_targets);
            let output = self.shadow_git(&args)?;
            pending.extend(
                output
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .map(|line| line.trim().to_string()),
            );
        }

        pending.sort();
        pending.dedup();
        Ok(pending)
    }

    pub fn commit_count(&self) -> Result<usize> {
        let output = self.shadow_git(&["rev-list", "--count", "HEAD"])?;
        let count: usize = output.trim().parse().unwrap_or(0);
        Ok(count.saturating_sub(1))
    }

    pub fn last_snapshot_info(&self) -> Result<Option<String>> {
        let count = self.commit_count()?;
        if count == 0 {
            return Ok(None);
        }

        let output = self.shadow_git(&["log", "-1", "--format=%cr"])?;
        let info = output.trim();
        if info.is_empty() {
            return Ok(None);
        }

        Ok(Some(format!("{count} snapshots, last: {info}")))
    }
}

pub fn resolve_history_files(
    ctx: &RepoContext,
    entries: &[Entry],
    shadow: Option<&ShadowRepo>,
) -> Result<Vec<String>> {
    let mut files = Vec::new();
    let needs_glob_scan = entries.iter().any(|entry| git::contains_glob(&entry.value));
    let ignored = if needs_glob_scan {
        git::list_ignored_untracked_from_exclude(&ctx.root, &ctx.exclude_path)?
    } else {
        Vec::new()
    };
    let shadow_files = match shadow {
        Some(shadow) => shadow.tracked_files()?,
        None => Vec::new(),
    };

    for entry in entries {
        let value = &entry.value;

        if value == ".layer/" || value == ".layer" {
            continue;
        }

        if value.ends_with('/') {
            let dir = ctx.root.join(value.trim_end_matches('/'));
            if dir.is_dir() {
                for item in WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
                    if item.path().is_file() {
                        if let Ok(rel) = item.path().strip_prefix(&ctx.root) {
                            files.push(rel.to_string_lossy().replace('\\', "/"));
                        }
                    }
                }
            }
        } else if git::contains_glob(value) {
            for path in &ignored {
                if entry_matches_file(value, path) {
                    files.push(path.clone());
                }
            }
        } else if ctx.root.join(value).exists() {
            files.push(value.clone());
        }
    }

    for path in shadow_files {
        if entries
            .iter()
            .any(|entry| entry_matches_file(&entry.value, &path))
        {
            files.push(path);
        }
    }

    files.sort();
    files.dedup();
    Ok(files)
}

fn entry_matches_file(entry: &str, path: &str) -> bool {
    if entry.ends_with('/') {
        return path.starts_with(entry);
    }

    if git::contains_glob(entry) {
        return wildcard_match(entry, path)
            || wildcard_match(entry, path.rsplit('/').next().unwrap_or(path));
    }

    path == entry
}

fn git_stdout_simple(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git {} failed: {}", args.join(" "), stderr.trim()));
    }

    String::from_utf8(output.stdout).context("git output was not UTF-8")
}
