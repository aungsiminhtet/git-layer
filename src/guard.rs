use crate::exclude_file::ensure_exclude_file;
use crate::git::{self, RepoContext};
use crate::matching::wildcard_match;
use anyhow::{anyhow, Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const HOOK_MARKER: &str = "# layer-guard (managed by layer)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookState {
    Installed,
    NotInstalled,
    ForeignHook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallResult {
    Installed,
    Updated,
}

pub fn hook_script() -> Result<String> {
    let layer_bin = std::env::current_exe().context("failed to resolve layer executable path")?;
    Ok(render_hook_script(&layer_bin))
}

pub fn hook_path(ctx: &RepoContext) -> Result<PathBuf> {
    git::git_path(&ctx.root, "hooks/pre-commit")
}

pub fn hook_state(ctx: &RepoContext) -> Result<HookState> {
    let path = hook_path(ctx)?;
    if !path.exists() {
        return Ok(HookState::NotInstalled);
    }

    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    if content.contains(HOOK_MARKER) {
        Ok(HookState::Installed)
    } else {
        Ok(HookState::ForeignHook)
    }
}

pub fn install(ctx: &RepoContext, force: bool) -> Result<InstallResult> {
    let state = hook_state(ctx)?;
    if matches!(state, HookState::ForeignHook) && !force {
        return Err(anyhow!(
            "pre-commit hook already exists and is not managed by layer. Re-run with --force to overwrite it"
        ));
    }

    let path = hook_path(ctx)?;
    let script = hook_script()?;
    write_hook(&path, &script)?;

    Ok(match state {
        HookState::NotInstalled => InstallResult::Installed,
        HookState::Installed | HookState::ForeignHook => InstallResult::Updated,
    })
}

pub fn remove(ctx: &RepoContext) -> Result<bool> {
    let path = hook_path(ctx)?;
    if !path.exists() {
        return Ok(false);
    }

    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    if !content.contains(HOOK_MARKER) {
        return Err(anyhow!(
            "pre-commit hook exists and is not managed by layer"
        ));
    }

    fs::remove_file(&path).with_context(|| format!("failed to remove {}", path.display()))?;
    Ok(true)
}

pub fn check(ctx: &RepoContext) -> Result<Vec<String>> {
    let exclude = ensure_exclude_file(&ctx.exclude_path)?;
    let entries = exclude.managed_entry_set();
    if entries.is_empty() {
        return Ok(Vec::new());
    }

    let mut blocked: Vec<String> = get_staged_files(ctx)?
        .into_iter()
        .filter(|file| matches_any_entry(file, &entries))
        .collect();
    blocked.sort();
    blocked.dedup();
    Ok(blocked)
}

fn render_hook_script(layer_bin: &Path) -> String {
    let quoted = sh_quote(&layer_bin.to_string_lossy());
    format!(
        "#!/bin/sh\n{HOOK_MARKER}\nLAYER_BIN={quoted}\nif [ -x \"$LAYER_BIN\" ]; then\n    exec \"$LAYER_BIN\" guard --check\nfi\nif command -v layer >/dev/null 2>&1; then\n    exec layer guard --check\nfi\necho \"layer guard: unable to find the layer binary\" >&2\necho \"run 'layer guard --force' to refresh the hook\" >&2\nexit 1\n"
    )
}

pub(crate) fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn write_hook(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(path)
            .with_context(|| format!("failed to read metadata for {}", path.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)
            .with_context(|| format!("failed to set executable bit on {}", path.display()))?;
    }

    Ok(())
}

fn get_staged_files(ctx: &RepoContext) -> Result<Vec<String>> {
    let output = git::git_stdout_bytes(
        &["diff", "--cached", "--name-only", "-z", "--"],
        Some(&ctx.root),
    )?;
    Ok(output
        .split(|byte| *byte == b'\0')
        .filter(|item| !item.is_empty())
        .map(|item| String::from_utf8_lossy(item).into_owned())
        .collect())
}

fn matches_any_entry(file: &str, entries: &HashSet<String>) -> bool {
    entries.iter().any(|entry| matches_entry(file, entry))
}

fn matches_entry(file: &str, entry: &str) -> bool {
    if entry.ends_with('/') {
        return file.starts_with(entry);
    }

    if git::contains_glob(entry) {
        if entry.contains('/') {
            return wildcard_match(entry, file);
        }
        return wildcard_match(entry, file.rsplit('/').next().unwrap_or(file));
    }

    file == entry
}

#[cfg(test)]
mod tests {
    use super::{matches_entry, remove, render_hook_script, HOOK_MARKER};
    use crate::git::RepoContext;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use tempfile::tempdir;

    fn init_repo() -> (tempfile::TempDir, RepoContext) {
        let tmp = tempdir().expect("failed to create temp dir");

        let output = Command::new("git")
            .args(["init", "-q"])
            .current_dir(tmp.path())
            .output()
            .expect("failed to init repo");
        assert!(output.status.success(), "git init failed");

        let root = tmp.path().to_path_buf();
        let git_dir = root.join(".git");
        let ctx = RepoContext {
            root: root.clone(),
            git_dir: git_dir.clone(),
            exclude_path: git_dir.join("info").join("exclude"),
        };

        (tmp, ctx)
    }

    #[test]
    fn matches_literals_directories_and_globs() {
        assert!(matches_entry("CLAUDE.md", "CLAUDE.md"));
        assert!(matches_entry(".claude/settings.json", ".claude/"));
        assert!(matches_entry("notes/CLAUDE.md", "*.md"));
        assert!(!matches_entry("notes/CLAUDE.md", "CLAUDE.md"));
        assert!(!matches_entry("notes/todo.txt", "*.md"));
    }

    #[test]
    fn hook_script_contains_marker() {
        let script = render_hook_script(Path::new("/tmp/layer"));
        assert!(script.contains(HOOK_MARKER));
        assert!(script.contains("guard --check"));
    }

    #[test]
    fn remove_refuses_foreign_hook() {
        let (_tmp, ctx) = init_repo();
        let hook = PathBuf::from(ctx.root.join(".git").join("hooks").join("pre-commit"));
        std::fs::create_dir_all(hook.parent().unwrap()).expect("failed to create hook dir");
        std::fs::write(&hook, "#!/bin/sh\nexit 0\n").expect("failed to write foreign hook");

        let err = remove(&ctx).expect_err("foreign hook should not be removed");
        assert!(err.to_string().contains("not managed by layer"));
        assert!(hook.exists());
    }
}
