use crate::commands::add;
use crate::exclude_file::{ensure_exclude_file_for_write, normalize_entry};
use crate::git;
use crate::git::RepoContext;
use crate::patterns::{PatternCategory, KNOWN_SCAN_PATTERNS};
use crate::ui;
use anyhow::{anyhow, Result};
use dialoguer::MultiSelect;
use std::collections::HashSet;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct AiDiscovery {
    pub path: String,
    pub label: String,
    pub category: PatternCategory,
    pub already_excluded: bool,
    pub is_gitignored: bool,
    pub is_tracked: bool,
}

pub fn run() -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let mut exclude = ensure_exclude_file_for_write(&ctx.exclude_path)?;
    let excluded = exclude.managed_entry_set();

    println!("{}", ui::heading("Scanning for context files..."));
    let found = discover_known_files(&ctx, &excluded)?;

    if found.is_empty() {
        println!("No context files found in this repository.");
        return Ok(2);
    }

    let mut selectable = Vec::new();
    let mut already_excluded = Vec::new();
    let mut already_gitignored = Vec::new();
    let mut tracked = Vec::new();

    for item in found {
        if item.already_excluded {
            already_excluded.push(item);
        } else if item.is_gitignored {
            already_gitignored.push(item);
        } else if item.is_tracked {
            tracked.push(item);
        } else {
            selectable.push(item);
        }
    }

    // Show context-only sections (not selectable)
    let mut has_section = false;

    if !tracked.is_empty() {
        println!(
            "  {} Exposed ({}) — tracked files can't be hidden by layering:",
            ui::exposed(),
            tracked.len()
        );
        for item in &tracked {
            println!("    {} {} ({})", ui::exposed(), item.path, item.label);
            println!(
                "      {}",
                ui::warn_text(&format!(
                    "git rm --cached {}",
                    item.path.trim_end_matches('/')
                ))
            );
        }
        has_section = true;
    }

    if !already_excluded.is_empty() {
        if has_section {
            println!();
        }
        println!("  {} Already layered:", ui::layered());
        for item in &already_excluded {
            println!("    {} {}", ui::layered(), ui::dim_text(&item.path));
        }
        has_section = true;
    }

    if !already_gitignored.is_empty() {
        if has_section {
            println!();
        }
        println!("  {} Already ignored by Git:", ui::info());
        for item in &already_gitignored {
            println!("    {} {}", ui::info(), ui::dim_text(&item.path));
        }
    }

    if selectable.is_empty() {
        println!();
        println!("No new context files found.");
        return Ok(2);
    }

    if !ui::is_stdout_tty() {
        // Non-TTY: list discovered files and exit
        println!();
        println!("  {} Discovered ({}):", ui::discovered(), selectable.len());
        for item in &selectable {
            println!("    {} {} ({})", ui::discovered(), item.path, item.label);
        }
        return Err(anyhow!(
            "interactive mode requires a TTY. Run in a terminal to select files"
        ));
    }

    // Interactive: multiselect IS the discovery UI
    let items: Vec<String> = selectable
        .iter()
        .map(|item| {
            format!(
                "{} {}",
                item.path,
                ui::dim_text(&format!("({})", item.label))
            )
        })
        .collect();
    let defaults = vec![true; items.len()];

    println!(
        "  {} Discovered {} context {} — select for your local layer",
        ui::discovered(),
        selectable.len(),
        if selectable.len() == 1 {
            "file"
        } else {
            "files"
        }
    );
    let theme = ui::layer_theme();
    ui::print_select_hint();
    let selections = MultiSelect::with_theme(&theme)
        .items(&items)
        .defaults(&defaults)
        .report(false)
        .interact_opt()?;

    let selected = selections.unwrap_or_default();
    if selected.is_empty() {
        println!(
            "No files selected. You can add files later with {}.",
            ui::brand("layer add")
        );
        return Ok(2);
    }

    let chosen: Vec<String> = selected
        .into_iter()
        .map(|idx| selectable[idx].path.clone())
        .collect();

    let summary = add::apply_add_entries(&ctx, &mut exclude, &chosen, false)?;
    if summary.added == 0 {
        return Ok(2);
    }

    Ok(0)
}

pub fn discover_known_files(
    ctx: &RepoContext,
    excluded: &HashSet<String>,
) -> Result<Vec<AiDiscovery>> {
    let tracked = git::list_tracked(&ctx.root)?;
    discover_known_files_with_tracked(ctx, excluded, &tracked)
}

pub fn discover_known_files_with_tracked(
    ctx: &RepoContext,
    excluded: &HashSet<String>,
    tracked: &HashSet<String>,
) -> Result<Vec<AiDiscovery>> {
    let mut seen = HashSet::new();
    let discovered = discover_paths(&ctx.root);

    // First pass: collect all candidate paths with their pattern metadata.
    struct Candidate {
        normalized: String,
        label: String,
        category: PatternCategory,
    }
    let mut candidates = Vec::new();
    let mut check_ignore_paths = Vec::new();

    for pattern in KNOWN_SCAN_PATTERNS {
        for path in resolve_pattern_paths_in(&discovered, pattern.entry) {
            let normalized = normalize_entry(&path);
            if normalized.is_empty() || !seen.insert(normalized.clone()) {
                continue;
            }
            let ignore_target = normalized.trim_end_matches('/').to_string();
            check_ignore_paths.push(ignore_target);
            candidates.push(Candidate {
                normalized,
                label: pattern.label.to_string(),
                category: pattern.category,
            });
        }
    }

    // Batch check-ignore call instead of per-file.
    let ignore_results = git::check_ignore_bulk(&ctx.root, &check_ignore_paths, false)?;

    let mut out = Vec::new();
    debug_assert_eq!(candidates.len(), check_ignore_paths.len());
    for (candidate, ignore_target) in candidates.into_iter().zip(check_ignore_paths.iter()) {
        let tracked_match = if candidate.normalized.ends_with('/') {
            tracked.iter().any(|p| p.starts_with(&candidate.normalized))
        } else {
            tracked.contains(&candidate.normalized)
        };

        let is_gitignored = ignore_results.contains_key(ignore_target);

        out.push(AiDiscovery {
            path: candidate.normalized.clone(),
            label: candidate.label,
            category: candidate.category,
            already_excluded: excluded.contains(&candidate.normalized),
            is_gitignored,
            is_tracked: tracked_match,
        });
    }

    // Second pass: for directory candidates not yet ignored, check if all
    // files inside are already covered by ignore rules (e.g. global gitignore
    // covers every file individually).
    let dir_indices: Vec<usize> = out
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.path.ends_with('/') && !item.already_excluded && !item.is_gitignored
        })
        .map(|(i, _)| i)
        .collect();

    for idx in dir_indices {
        let dir_path = ctx.root.join(out[idx].path.trim_end_matches('/'));
        if !dir_path.is_dir() {
            continue;
        }

        let mut files_in_dir = Vec::new();
        for entry in WalkDir::new(&dir_path).min_depth(1) {
            let entry = match entry {
                Ok(v) => v,
                Err(_) => continue,
            };
            if entry.file_type().is_file() {
                if let Ok(rel) = entry.path().strip_prefix(&ctx.root) {
                    files_in_dir.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }

        if files_in_dir.is_empty() {
            out[idx].is_gitignored = true;
            continue;
        }

        let file_ignore_results = git::check_ignore_bulk(&ctx.root, &files_in_dir, false)?;
        if files_in_dir
            .iter()
            .all(|f| file_ignore_results.contains_key(f))
        {
            out[idx].is_gitignored = true;
        }
    }

    Ok(out)
}

#[derive(Debug, Clone)]
struct DiscoveredPath {
    display: String,
    match_path: String,
    depth: usize,
    is_dir: bool,
}

fn discover_paths(repo_root: &Path) -> Vec<DiscoveredPath> {
    let mut out = Vec::new();

    // AI and config files live at the repo root or known subdirs like .github/.
    for entry in WalkDir::new(repo_root).min_depth(1).max_depth(2) {
        let entry = match entry {
            Ok(v) => v,
            Err(_) => continue,
        };

        let path = entry.path();
        if path
            .components()
            .any(|c| c.as_os_str().to_string_lossy() == ".git")
        {
            continue;
        }

        let rel = match path.strip_prefix(repo_root) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let mut rel_str = rel.to_string_lossy().replace('\\', "/");
        let is_dir = entry.file_type().is_dir();
        if is_dir && !rel_str.ends_with('/') {
            rel_str.push('/');
        }
        let depth = rel.components().count();

        out.push(DiscoveredPath {
            display: rel_str.clone(),
            match_path: rel_str.trim_end_matches('/').to_string(),
            depth,
            is_dir,
        });
    }

    out
}

fn resolve_pattern_paths_in(discovered: &[DiscoveredPath], pattern: &str) -> Vec<String> {
    discovered
        .iter()
        .filter(|item| pattern_matches_path(pattern, item))
        .map(|item| item.display.clone())
        .collect()
}

fn pattern_matches_path(pattern: &str, item: &DiscoveredPath) -> bool {
    let pattern_trimmed = pattern.trim_end_matches('/');
    let wants_dir = pattern.ends_with('/');

    if wants_dir && !item.is_dir {
        return false;
    }
    if !pattern.contains('/') && item.depth != 1 {
        return false;
    }

    if git::contains_glob(pattern_trimmed) {
        if pattern.contains('/') {
            return wildcard_match(pattern_trimmed, &item.match_path);
        }
        return wildcard_match(
            pattern_trimmed,
            item.match_path.rsplit('/').next().unwrap_or(""),
        );
    }

    if pattern.contains('/') {
        return item.match_path == pattern_trimmed;
    }

    item.match_path.rsplit('/').next().unwrap_or("") == pattern_trimmed
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p = pattern.as_bytes();
    let t = text.as_bytes();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star_idx = None;
    let mut match_idx = 0usize;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star_idx = Some(pi);
            pi += 1;
            match_idx = ti;
        } else if let Some(star) = star_idx {
            pi = star + 1;
            match_idx += 1;
            ti = match_idx;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }

    pi == p.len()
}

// Simple wildcard matcher for scanning known patterns against discovered paths.
// This intentionally doesn't delegate to git check-ignore because the patterns
// are controlled by us (KNOWN_SCAN_PATTERNS), not user input, and are simple
// enough that this matcher handles them correctly.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_exact_match() {
        assert!(wildcard_match("CLAUDE.md", "CLAUDE.md"));
        assert!(!wildcard_match("CLAUDE.md", "claude.md"));
    }

    #[test]
    fn wildcard_star() {
        assert!(wildcard_match(".aider*", ".aider"));
        assert!(wildcard_match(".aider*", ".aider.conf.yml"));
        assert!(wildcard_match(".env.*", ".env.local"));
        assert!(wildcard_match(".env.*", ".env.production"));
        assert!(!wildcard_match(".env.*", ".env"));
    }

    #[test]
    fn wildcard_question_mark() {
        assert!(wildcard_match("file?.txt", "file1.txt"));
        assert!(!wildcard_match("file?.txt", "file12.txt"));
    }

    #[test]
    fn wildcard_empty_strings() {
        assert!(wildcard_match("", ""));
        assert!(!wildcard_match("a", ""));
        assert!(wildcard_match("*", ""));
        assert!(wildcard_match("*", "anything"));
    }
}
