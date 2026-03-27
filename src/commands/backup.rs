use crate::exclude_file::{ensure_exclude_file, ensure_exclude_file_for_write};
use crate::git;
use crate::ui;
use anyhow::{Context, Result};
use dialoguer::Confirm;
use std::fs;
use std::path::{Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub fn backup() -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let exclude = ensure_exclude_file(&ctx.exclude_path)?;
    let entries = exclude
        .entries()
        .into_iter()
        .map(|e| e.value)
        .collect::<Vec<_>>();

    let identity = current_repo_identity(&ctx)?;
    let backup_dir = backup_dir_path()?;
    fs::create_dir_all(&backup_dir)
        .with_context(|| format!("failed to create {}", backup_dir.display()))?;

    let backup_path = backup_dir.join(format!("{}.txt", identity.repo_name));
    let existed = backup_path.exists();

    let now = OffsetDateTime::now_utc().format(&Rfc3339)?;
    let source = identity.source.as_deref().unwrap_or("(no origin remote)");

    let mut out = String::new();
    out.push_str("# layer backup\n");
    out.push_str(&format!("# repo: {}\n", identity.repo_name));
    out.push_str(&format!("# source: {}\n", source));
    out.push_str(&format!("# date: {}\n", now));
    out.push_str(&format!("# entries: {}\n", entries.len()));
    for entry in &entries {
        out.push_str(entry);
        out.push('\n');
    }

    fs::write(&backup_path, out)
        .with_context(|| format!("failed to write {}", backup_path.display()))?;

    if existed {
        println!(
            "  {} Updated backup for '{}' at {}",
            ui::ok(),
            identity.repo_name,
            backup_path.display()
        );
    } else {
        println!(
            "  {} Backed up {} entries to {}",
            ui::ok(),
            entries.len(),
            backup_path.display()
        );
    }

    Ok(0)
}

pub fn restore(list: bool) -> Result<i32> {
    if list {
        return list_backups();
    }

    let ctx = git::ensure_repo()?;
    let identity = current_repo_identity(&ctx)?;
    let backup_path = backup_dir_path()?.join(format!("{}.txt", identity.repo_name));

    if !backup_path.exists() {
        println!(
            "No backup found for '{}'. Run 'layer backup' to create one.",
            identity.repo_name
        );
        return Ok(2);
    }

    let backup = parse_backup_file(&backup_path)?;
    println!(
        "{}",
        ui::heading(&format!(
            "Found backup for '{}' ({} entries, saved {})",
            identity.repo_name,
            backup.entries.len(),
            format_backup_date(&backup.date)
        ))
    );

    ui::require_tty("interactive confirmation requires a TTY. Re-run in a terminal")?;

    let confirmed = Confirm::new()
        .with_prompt("Restore these entries?")
        .default(false)
        .interact()?;

    if !confirmed {
        println!("No changes made.");
        return Ok(2);
    }

    let mut exclude = ensure_exclude_file_for_write(&ctx.exclude_path)?;
    let mut current = exclude.managed_entry_set();
    let mut added = 0usize;

    for entry in backup.entries {
        if current.contains(&entry) {
            continue;
        }
        exclude.append_entry(&entry);
        current.insert(entry);
        added += 1;
    }

    if added == 0 {
        println!("All backup entries are already present in .git/info/exclude.");
        return Ok(2);
    }

    exclude.write(&ctx.exclude_path)?;

    println!("  {} Restored {} entries.", ui::ok(), added);
    Ok(0)
}

fn list_backups() -> Result<i32> {
    let dir = backup_dir_path()?;
    if !dir.exists() {
        println!("No backups found in {}.", dir.display());
        return Ok(2);
    }

    let mut backups = Vec::new();
    for item in fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let item = item?;
        let path = item.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let parsed = parse_backup_file(&path)?;
        backups.push(parsed);
    }

    if backups.is_empty() {
        println!("No backups found in {}.", dir.display());
        return Ok(2);
    }

    backups.sort_by(|a, b| a.repo.cmp(&b.repo));

    println!("Available backups:");
    for backup in backups {
        println!(
            "  {:<20} {:>3} entries    {}",
            backup.repo,
            backup.entries.len(),
            format_backup_date(&backup.date)
        );
    }

    Ok(0)
}

#[derive(Debug, Clone)]
struct RepoIdentity {
    repo_name: String,
    source: Option<String>,
}

fn current_repo_identity(ctx: &git::RepoContext) -> Result<RepoIdentity> {
    let source = git::git_stdout(&["remote", "get-url", "origin"], Some(&ctx.root))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let repo_name = if let Some(src) = &source {
        let candidate = src
            .replace('\\', "/")
            .split('/')
            .next_back()
            .unwrap_or("repo")
            .trim_end_matches(".git")
            .to_string();
        sanitize_repo_name(&candidate)
    } else {
        let fallback = ctx
            .root
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("repo");
        sanitize_repo_name(fallback)
    };

    Ok(RepoIdentity { repo_name, source })
}

fn sanitize_repo_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect::<String>();

    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "repo".to_string()
    } else {
        trimmed.to_string()
    }
}

fn backup_dir_path() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("could not determine home directory")?;
    Ok(PathBuf::from(home).join(".layer-backups"))
}

#[derive(Debug, Clone)]
struct ParsedBackup {
    repo: String,
    date: Option<String>,
    entries: Vec<String>,
}

fn parse_backup_file(path: &Path) -> Result<ParsedBackup> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    let mut repo = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let mut date = None;
    let mut entries = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("# repo:") {
            repo = value.trim().to_string();
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("# date:") {
            date = Some(value.trim().to_string());
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        entries.push(trimmed.to_string());
    }

    Ok(ParsedBackup {
        repo,
        date,
        entries,
    })
}

fn format_backup_date(raw: &Option<String>) -> String {
    let Some(raw) = raw else {
        return "unknown date".to_string();
    };

    if let Ok(dt) = OffsetDateTime::parse(raw, &Rfc3339) {
        let month = match dt.month() {
            time::Month::January => "Jan",
            time::Month::February => "Feb",
            time::Month::March => "Mar",
            time::Month::April => "Apr",
            time::Month::May => "May",
            time::Month::June => "Jun",
            time::Month::July => "Jul",
            time::Month::August => "Aug",
            time::Month::September => "Sep",
            time::Month::October => "Oct",
            time::Month::November => "Nov",
            time::Month::December => "Dec",
        };
        return format!("{} {}, {}", month, dt.day(), dt.year());
    }

    raw.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_repo_name_simple() {
        assert_eq!(sanitize_repo_name("my-project"), "my-project");
    }

    #[test]
    fn sanitize_repo_name_strips_slashes_and_colons() {
        assert_eq!(sanitize_repo_name("user/repo"), "user-repo");
        assert_eq!(sanitize_repo_name("C:\\repo"), "C--repo");
        assert_eq!(sanitize_repo_name("git:repo"), "git-repo");
    }

    #[test]
    fn sanitize_repo_name_trims_dashes() {
        assert_eq!(sanitize_repo_name("/repo/"), "repo");
    }

    #[test]
    fn sanitize_repo_name_empty_becomes_repo() {
        assert_eq!(sanitize_repo_name(""), "repo");
        assert_eq!(sanitize_repo_name("///"), "repo");
    }

    #[test]
    fn format_backup_date_none() {
        assert_eq!(format_backup_date(&None), "unknown date");
    }

    #[test]
    fn format_backup_date_valid_rfc3339() {
        let date = Some("2026-02-08T12:00:00Z".to_string());
        assert_eq!(format_backup_date(&date), "Feb 8, 2026");
    }

    #[test]
    fn format_backup_date_unparseable_returns_raw() {
        let date = Some("not-a-date".to_string());
        assert_eq!(format_backup_date(&date), "not-a-date");
    }
}
