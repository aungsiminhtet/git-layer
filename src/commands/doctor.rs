use crate::exclude_file::ensure_exclude_file;
use crate::git;
use crate::git::PatternMatchSummary;
use crate::ui;
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use walkdir::WalkDir;

pub fn run() -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let exclude = ensure_exclude_file(&ctx.exclude_path)?;
    let entries = exclude.entries();

    if entries.is_empty() {
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

    let mut n_layered = 0usize;
    let mut n_exposed = 0usize;
    let mut n_stale = 0usize;
    let mut n_redundant = 0usize;

    for entry in entries {
        let diagnosis = diagnose_entry(
            &ctx.root,
            &entry.value,
            &tracked,
            &gitignore_entries,
            &pattern_match_index,
        )?;

        match diagnosis.kind {
            DiagnosisKind::Layered => {
                n_layered += 1;
                println!("  {} {} — layered", ui::layered(), entry.value);
            }
            DiagnosisKind::Exposed => {
                n_exposed += 1;
                println!(
                    "  {} {} — {}",
                    ui::exposed(),
                    entry.value,
                    ui::warn_text(&diagnosis.message)
                );
                for line in diagnosis.details {
                    println!("    {}", ui::warn_text(&line));
                }
            }
            DiagnosisKind::Stale => {
                n_stale += 1;
                println!(
                    "  {} {} — {}",
                    ui::stale(),
                    entry.value,
                    ui::err_text("stale — file not found")
                );
                println!("    {}", ui::dim_text(&format!("layer rm {}", entry.value)));
            }
            DiagnosisKind::Redundant => {
                n_redundant += 1;
                println!(
                    "  {} {} — {}",
                    ui::info(),
                    entry.value,
                    ui::dim_text("redundant — already in .gitignore")
                );
                println!("    {}", ui::dim_text(&format!("layer rm {}", entry.value)));
            }
        }
    }

    println!();
    print!("  ");
    let mut parts = Vec::new();
    if n_layered > 0 {
        parts.push(format!("{} layered", n_layered));
    }
    if n_exposed > 0 {
        parts.push(ui::warn_text(&format!("{} exposed", n_exposed)));
    }
    if n_stale > 0 {
        parts.push(ui::err_text(&format!("{} stale", n_stale)));
    }
    if n_redundant > 0 {
        parts.push(ui::dim_text(&format!("{} redundant", n_redundant)));
    }
    println!("{}", parts.join(" · "));

    if n_exposed > 0 || n_stale > 0 {
        return Ok(1);
    }

    if n_layered == 0 && n_redundant > 0 {
        return Ok(2);
    }

    Ok(0)
}

#[derive(Debug)]
struct Diagnosis {
    kind: DiagnosisKind,
    message: String,
    details: Vec<String>,
}

#[derive(Debug)]
enum DiagnosisKind {
    Layered,
    Exposed,
    Stale,
    Redundant,
}

fn diagnose_entry(
    repo_root: &Path,
    entry: &str,
    tracked: &HashSet<String>,
    gitignore_entries: &HashSet<String>,
    pattern_match_index: &HashMap<String, PatternMatchSummary>,
) -> Result<Diagnosis> {
    let resolved = resolve_entry(repo_root, entry, tracked, pattern_match_index)?;

    if !resolved.exists {
        return Ok(Diagnosis {
            kind: DiagnosisKind::Stale,
            message: String::new(),
            details: Vec::new(),
        });
    }

    if !resolved.tracked_matches.is_empty() {
        let mut details = Vec::new();

        if resolved.total_matches > resolved.tracked_matches.len() {
            details.push(format!(
                "Fix tracked files with: git rm --cached <file> ({} tracked of {} matches)",
                resolved.tracked_matches.len(),
                resolved.total_matches
            ));
        } else if entry.ends_with('/') {
            details.push(format!(
                "Fix: git rm --cached -r {}",
                entry.trim_end_matches('/')
            ));
        } else {
            details.push(format!("Fix: git rm --cached {}", entry));
        }

        if resolved.tracked_matches.len() <= 3 {
            for file in &resolved.tracked_matches {
                details.push(format!("Tracked: {file}"));
            }
        }

        return Ok(Diagnosis {
            kind: DiagnosisKind::Exposed,
            message: if git::contains_glob(entry) {
                format!(
                    "exposed — {} files match, {} tracked",
                    resolved.total_matches,
                    resolved.tracked_matches.len()
                )
            } else {
                "exposed — tracked by git".to_string()
            },
            details,
        });
    }

    if gitignore_entries.contains(entry) {
        return Ok(Diagnosis {
            kind: DiagnosisKind::Redundant,
            message: String::new(),
            details: Vec::new(),
        });
    }

    Ok(Diagnosis {
        kind: DiagnosisKind::Layered,
        message: String::new(),
        details: Vec::new(),
    })
}

#[derive(Debug)]
struct ResolvedEntry {
    exists: bool,
    total_matches: usize,
    tracked_matches: Vec<String>,
}

fn resolve_entry(
    repo_root: &Path,
    entry: &str,
    tracked: &HashSet<String>,
    pattern_match_index: &HashMap<String, PatternMatchSummary>,
) -> Result<ResolvedEntry> {
    if entry.ends_with('/') {
        return resolve_directory(repo_root, entry, tracked);
    }

    if git::contains_glob(entry) {
        return resolve_pattern(entry, pattern_match_index);
    }

    resolve_literal(repo_root, entry, tracked)
}

fn resolve_literal(
    repo_root: &Path,
    entry: &str,
    tracked: &HashSet<String>,
) -> Result<ResolvedEntry> {
    let path = repo_root.join(entry);
    if !path.exists() {
        return Ok(ResolvedEntry {
            exists: false,
            total_matches: 0,
            tracked_matches: Vec::new(),
        });
    }

    let is_tracked = tracked.contains(entry);

    Ok(ResolvedEntry {
        exists: true,
        total_matches: 1,
        tracked_matches: if is_tracked {
            vec![entry.to_string()]
        } else {
            Vec::new()
        },
    })
}

fn resolve_directory(
    repo_root: &Path,
    entry: &str,
    tracked: &HashSet<String>,
) -> Result<ResolvedEntry> {
    let dir = repo_root.join(entry.trim_end_matches('/'));
    if !dir.is_dir() {
        return Ok(ResolvedEntry {
            exists: false,
            total_matches: 0,
            tracked_matches: Vec::new(),
        });
    }

    let mut total = 0usize;
    let mut tracked_matches = Vec::new();

    for item in WalkDir::new(&dir) {
        let item = item.with_context(|| format!("failed walking {}", dir.display()))?;
        if !item.path().is_file() {
            continue;
        }

        total += 1;
        if let Ok(rel) = item.path().strip_prefix(repo_root) {
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if tracked.contains(&rel_str) {
                tracked_matches.push(rel_str);
            }
        }
    }

    Ok(ResolvedEntry {
        exists: true,
        total_matches: total,
        tracked_matches,
    })
}

fn resolve_pattern(
    entry: &str,
    pattern_match_index: &HashMap<String, PatternMatchSummary>,
) -> Result<ResolvedEntry> {
    let Some(summary) = pattern_match_index.get(entry) else {
        return Ok(ResolvedEntry {
            exists: false,
            total_matches: 0,
            tracked_matches: Vec::new(),
        });
    };

    Ok(ResolvedEntry {
        exists: summary.total > 0,
        total_matches: summary.total,
        tracked_matches: summary.tracked_files.clone(),
    })
}
