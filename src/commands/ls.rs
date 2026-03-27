use crate::exclude_file::ensure_exclude_file;
use crate::git;
use crate::git::PatternMatchSummary;
use crate::ui;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use walkdir::WalkDir;

pub fn run() -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let exclude = ensure_exclude_file(&ctx.exclude_path)?;
    let entries = exclude.entries();
    let disabled = exclude.disabled_entries();
    let user_entries = exclude.user_entries();

    if entries.is_empty() && disabled.is_empty() && user_entries.is_empty() {
        println!(
            "No layered entries. Run {} or {} to get started.",
            ui::brand("layer add"),
            ui::brand("layer scan")
        );
        return Ok(2);
    }

    let tracked = git::list_tracked(&ctx.root)?;
    let gitignore_entries = git::read_root_gitignore_entries(&ctx.root)?;
    let pattern_match_index =
        git::build_pattern_match_index(&ctx.root, &ctx.exclude_path, &tracked)?;

    let all_names = entries
        .iter()
        .map(|e| e.value.len())
        .chain(disabled.iter().map(|e| e.value.len()))
        .chain(user_entries.iter().map(|e| e.value.len()));
    let max_name = all_names.max().unwrap_or(10);

    for entry in &entries {
        let status = classify_entry(&ctx.root, &entry.value, &tracked, &pattern_match_index);

        let gitignore_note = if gitignore_entries.contains(&entry.value) {
            format!("  {}", ui::dim_text("redundant (in .gitignore)"))
        } else {
            String::new()
        };

        let name = format!("{:<width$}", entry.value, width = max_name);

        match status {
            EntryStatus::Layered(detail) => {
                println!(
                    "  {} {}  {}{}",
                    ui::layered(),
                    name,
                    ui::dim_text(&detail),
                    gitignore_note
                );
            }
            EntryStatus::Exposed(detail) => {
                println!(
                    "  {} {}  {}{}",
                    ui::exposed(),
                    name,
                    ui::warn_text(&detail),
                    gitignore_note
                );
            }
            EntryStatus::Stale(detail) => {
                println!(
                    "  {} {}  {}{}",
                    ui::stale(),
                    name,
                    ui::err_text(&detail),
                    gitignore_note
                );
            }
        }
    }

    if !disabled.is_empty() {
        if !entries.is_empty() {
            println!();
        }
        for entry in &disabled {
            let name = format!("{:<width$}", entry.value, width = max_name);
            println!(
                "  {} {}  {}",
                ui::disabled(),
                name,
                ui::dim_text("(disabled)")
            );
        }
    }

    if !user_entries.is_empty() {
        if !entries.is_empty() || !disabled.is_empty() {
            println!();
        }
        for entry in &user_entries {
            let name = format!("{:<width$}", entry.value, width = max_name);
            println!("  {} {}  {}", ui::manual(), name, ui::dim_text("(manual)"));
        }
    }

    Ok(0)
}

enum EntryStatus {
    Layered(String),
    Exposed(String),
    Stale(String),
}

fn classify_entry(
    repo_root: &Path,
    entry: &str,
    tracked: &HashSet<String>,
    pattern_match_index: &HashMap<String, PatternMatchSummary>,
) -> EntryStatus {
    if entry.ends_with('/') {
        return classify_directory(repo_root, entry, tracked);
    }
    if git::contains_glob(entry) {
        return classify_pattern(entry, pattern_match_index);
    }
    classify_literal(repo_root, entry, tracked)
}

fn classify_literal(repo_root: &Path, entry: &str, tracked: &HashSet<String>) -> EntryStatus {
    let exists = repo_root.join(entry).exists();
    let is_tracked = tracked.contains(entry);

    if is_tracked {
        return EntryStatus::Exposed(format!("exposed — git rm --cached {entry}"));
    }

    if exists {
        return EntryStatus::Layered("layered".to_string());
    }

    EntryStatus::Stale("stale".to_string())
}

fn classify_directory(repo_root: &Path, entry: &str, tracked: &HashSet<String>) -> EntryStatus {
    let dir = repo_root.join(entry.trim_end_matches('/'));
    if !dir.is_dir() {
        return EntryStatus::Stale("stale".to_string());
    }

    let mut count = 0usize;
    for item in WalkDir::new(&dir) {
        let item = match item {
            Ok(v) => v,
            Err(_) => continue,
        };
        if item.path().is_file() {
            count += 1;
        }
    }

    let tracked_count = tracked.iter().filter(|p| p.starts_with(entry)).count();
    if tracked_count > 0 {
        return EntryStatus::Exposed(format!(
            "exposed — {} tracked (git rm --cached -r {})",
            tracked_count,
            entry.trim_end_matches('/')
        ));
    }

    EntryStatus::Layered(format!("layered ({count} files)"))
}

fn classify_pattern(
    entry: &str,
    pattern_match_index: &HashMap<String, PatternMatchSummary>,
) -> EntryStatus {
    let Some(summary) = pattern_match_index.get(entry) else {
        return EntryStatus::Stale("stale — no matches".to_string());
    };
    if summary.total == 0 {
        return EntryStatus::Stale("stale — no matches".to_string());
    }

    if summary.tracked_count() > 0 {
        return EntryStatus::Exposed("exposed — tracked files match".to_string());
    }

    EntryStatus::Layered(format!("layered ({} files)", summary.total))
}
