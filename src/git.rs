use anyhow::{anyhow, Context, Result};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct RepoContext {
    pub root: PathBuf,
    #[allow(dead_code)]
    pub git_dir: PathBuf,
    pub exclude_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct IgnoreMatch {
    pub source: String,
    pub line: usize,
    #[allow(dead_code)]
    pub pattern: String,
}

pub fn ensure_repo() -> Result<RepoContext> {
    let git_dir_raw = git_stdout(&["rev-parse", "--git-dir"], None)
        .map_err(|_| anyhow!("Error: not a git repository"))?;

    let root_raw = git_stdout(&["rev-parse", "--show-toplevel"], None)
        .map_err(|_| anyhow!("Error: not a git repository"))?;

    let root = PathBuf::from(root_raw.trim());
    let git_dir = resolve_git_dir(&root, git_dir_raw.trim());
    let exclude_path = git_dir.join("info").join("exclude");

    Ok(RepoContext {
        root,
        git_dir,
        exclude_path,
    })
}

fn resolve_git_dir(root: &Path, git_dir_raw: &str) -> PathBuf {
    let path = PathBuf::from(git_dir_raw);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

pub fn git_stdout(args: &[&str], cwd: Option<&Path>) -> Result<String> {
    let output = git_stdout_bytes(args, cwd)?;
    String::from_utf8(output).context("git output was not UTF-8")
}

pub fn git_stdout_bytes(args: &[&str], cwd: Option<&Path>) -> Result<Vec<u8>> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }

    let output = cmd
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;

    if !output.status.success() {
        return Err(anyhow!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(output.stdout)
}

pub fn git_path(repo_root: &Path, target: &str) -> Result<PathBuf> {
    let raw = git_stdout(&["rev-parse", "--git-path", target], Some(repo_root))?;
    let path = PathBuf::from(raw.trim());
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(repo_root.join(path))
    }
}

pub fn is_tracked(repo_root: &Path, file: &str) -> Result<bool> {
    if contains_glob(file) || file.ends_with('/') {
        return Ok(false);
    }

    let output = Command::new("git")
        .args(["ls-files", "--error-unmatch", "--", file])
        .current_dir(repo_root)
        .output()
        .with_context(|| format!("failed to run git ls-files for {file}"))?;

    Ok(output.status.success())
}

pub fn list_untracked(repo_root: &Path) -> Result<Vec<String>> {
    let out = git_stdout(
        &["ls-files", "--others", "--exclude-standard"],
        Some(repo_root),
    )?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

pub fn list_tracked(repo_root: &Path) -> Result<HashSet<String>> {
    let out = git_stdout(&["ls-files"], Some(repo_root))?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

pub fn check_ignore_verbose(repo_root: &Path, path: &str) -> Result<Option<IgnoreMatch>> {
    check_ignore_verbose_with_mode(repo_root, path, false)
}

pub fn check_ignore_verbose_no_index(repo_root: &Path, path: &str) -> Result<Option<IgnoreMatch>> {
    check_ignore_verbose_with_mode(repo_root, path, true)
}

fn check_ignore_verbose_with_mode(
    repo_root: &Path,
    path: &str,
    no_index: bool,
) -> Result<Option<IgnoreMatch>> {
    let mut args = vec!["check-ignore", "-v"];
    if no_index {
        args.push("--no-index");
    }
    args.extend(["--", path]);

    let output = Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .with_context(|| format!("failed to run git check-ignore for {path}"))?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout =
        String::from_utf8(output.stdout).context("git check-ignore output was not UTF-8")?;
    let first = match stdout.lines().next() {
        Some(line) if !line.trim().is_empty() => line,
        _ => return Ok(None),
    };

    let (matched, _) = match parse_check_ignore_line(first)? {
        Some(v) => v,
        None => return Ok(None),
    };
    Ok(Some(matched))
}

pub fn check_ignore_bulk(
    repo_root: &Path,
    paths: &[String],
    no_index: bool,
) -> Result<HashMap<String, IgnoreMatch>> {
    if paths.is_empty() {
        return Ok(HashMap::new());
    }

    let mut cmd = Command::new("git");
    cmd.args(["check-ignore", "-v"]);
    if no_index {
        cmd.arg("--no-index");
    }
    cmd.arg("--stdin");
    cmd.current_dir(repo_root);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().context("failed to spawn git check-ignore")?;
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("failed to open stdin for git check-ignore"))?;
        for path in paths {
            stdin
                .write_all(path.as_bytes())
                .with_context(|| format!("failed writing path '{path}' to git check-ignore"))?;
            stdin.write_all(b"\n")?;
        }
    }

    let output = child
        .wait_with_output()
        .context("failed waiting for git check-ignore output")?;

    // check-ignore exits 0 when any path matched, 1 when none matched.
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(anyhow!(
            "git check-ignore failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let stdout =
        String::from_utf8(output.stdout).context("git check-ignore output was not UTF-8")?;
    let mut out = HashMap::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((matched, path)) = parse_check_ignore_line(line)? {
            out.insert(path, matched);
        }
    }

    Ok(out)
}

pub fn list_ignored_untracked_from_exclude(
    repo_root: &Path,
    exclude_path: &Path,
) -> Result<Vec<String>> {
    let exclude_arg = format!("--exclude-from={}", exclude_path.display());
    let out = git_stdout(
        &["ls-files", "--others", "--ignored", exclude_arg.as_str()],
        Some(repo_root),
    )?;

    Ok(out
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

pub(crate) fn parse_check_ignore_line(line: &str) -> Result<Option<(IgnoreMatch, String)>> {
    let (meta, _) = match line.split_once('\t') {
        Some(v) => v,
        None => return Ok(None),
    };
    let path = match line.split_once('\t') {
        Some((_, p)) => p.to_string(),
        None => return Ok(None),
    };

    let mut parts = meta.splitn(3, ':');
    let source = match parts.next() {
        Some(v) => v.to_string(),
        None => return Ok(None),
    };
    let line_no = match parts.next() {
        Some(v) => v.parse::<usize>().unwrap_or(0),
        None => 0,
    };
    let pattern = match parts.next() {
        Some(v) => v.to_string(),
        None => String::new(),
    };

    Ok(Some((
        IgnoreMatch {
            source,
            line: line_no,
            pattern,
        },
        path,
    )))
}

pub fn contains_glob(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[')
}

/// Summary of files matching a single exclude pattern, used by ls, doctor, and status.
#[derive(Debug, Default, Clone)]
pub struct PatternMatchSummary {
    pub total: usize,
    pub tracked_files: Vec<String>,
}

impl PatternMatchSummary {
    pub fn tracked_count(&self) -> usize {
        self.tracked_files.len()
    }
}

/// Build an index mapping each exclude pattern to its match summary.
/// Shared by ls, doctor, and status commands.
pub fn build_pattern_match_index(
    repo_root: &Path,
    exclude_path: &Path,
    tracked: &HashSet<String>,
) -> Result<HashMap<String, PatternMatchSummary>> {
    let mut index: HashMap<String, PatternMatchSummary> = HashMap::new();

    let ignored_untracked = list_ignored_untracked_from_exclude(repo_root, exclude_path)?;
    let untracked_hits = check_ignore_bulk(repo_root, &ignored_untracked, false)?;
    for (path, hit) in untracked_hits {
        if !is_local_exclude_source(repo_root, exclude_path, &hit.source) {
            continue;
        }
        let summary = index.entry(hit.pattern).or_default();
        summary.total += 1;
        if tracked.contains(&path) {
            summary.tracked_files.push(path);
        }
    }

    let tracked_paths: Vec<String> = tracked.iter().cloned().collect();
    let tracked_hits = check_ignore_bulk(repo_root, &tracked_paths, true)?;
    for (path, hit) in tracked_hits {
        if !is_local_exclude_source(repo_root, exclude_path, &hit.source) {
            continue;
        }
        let summary = index.entry(hit.pattern).or_default();
        summary.total += 1;
        summary.tracked_files.push(path);
    }

    for summary in index.values_mut() {
        summary.tracked_files.sort();
        summary.tracked_files.dedup();
    }

    Ok(index)
}

/// Read the root .gitignore entries as a set of patterns.
/// Shared by ls and doctor commands.
pub fn read_root_gitignore_entries(repo_root: &Path) -> Result<HashSet<String>> {
    let path = repo_root.join(".gitignore");
    if !path.exists() {
        return Ok(HashSet::new());
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    Ok(content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect())
}

pub fn is_local_exclude_source(repo_root: &Path, exclude_path: &Path, source: &str) -> bool {
    let normalized_source = source.replace('\\', "/");
    if normalized_source.ends_with("/info/exclude") {
        return true;
    }

    let normalized_exclude = exclude_path.to_string_lossy().replace('\\', "/");
    if normalized_source == normalized_exclude {
        return true;
    }

    let repo_relative = exclude_path
        .strip_prefix(repo_root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"));
    if let Some(rel) = repo_relative {
        if normalized_source == rel {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parse_check_ignore_valid_line() {
        let line = ".git/info/exclude:3:CLAUDE.md\tCLAUDE.md";
        let result = parse_check_ignore_line(line).unwrap().unwrap();
        assert_eq!(result.0.source, ".git/info/exclude");
        assert_eq!(result.0.line, 3);
        assert_eq!(result.0.pattern, "CLAUDE.md");
        assert_eq!(result.1, "CLAUDE.md");
    }

    #[test]
    fn parse_check_ignore_no_tab_returns_none() {
        let line = "no-tab-here";
        assert!(parse_check_ignore_line(line).unwrap().is_none());
    }

    #[test]
    fn parse_check_ignore_gitignore_source() {
        let line = ".gitignore:5:*.log\tserver.log";
        let result = parse_check_ignore_line(line).unwrap().unwrap();
        assert_eq!(result.0.source, ".gitignore");
        assert_eq!(result.0.line, 5);
        assert_eq!(result.0.pattern, "*.log");
        assert_eq!(result.1, "server.log");
    }

    #[test]
    fn contains_glob_detects_wildcards() {
        assert!(contains_glob("*.md"));
        assert!(contains_glob("file?.txt"));
        assert!(contains_glob("[abc].txt"));
        assert!(!contains_glob("CLAUDE.md"));
        assert!(!contains_glob(".claude/"));
    }

    #[test]
    fn is_local_exclude_source_matches_suffix() {
        let root = PathBuf::from("/repo");
        let exclude = PathBuf::from("/repo/.git/info/exclude");
        assert!(is_local_exclude_source(
            &root,
            &exclude,
            "/repo/.git/info/exclude"
        ));
        assert!(is_local_exclude_source(
            &root,
            &exclude,
            ".git/info/exclude"
        ));
    }

    #[test]
    fn is_local_exclude_source_rejects_gitignore() {
        let root = PathBuf::from("/repo");
        let exclude = PathBuf::from("/repo/.git/info/exclude");
        assert!(!is_local_exclude_source(&root, &exclude, ".gitignore"));
        assert!(!is_local_exclude_source(
            &root,
            &exclude,
            "/home/user/.config/git/ignore"
        ));
    }
}
