use crate::commands::scan;
use crate::exclude_file::ensure_exclude_file;
use crate::git;
use crate::git::PatternMatchSummary;
use crate::shadow::ShadowRepo;
use crate::ui;
use anyhow::Result;
use std::collections::{HashMap, HashSet};

pub fn run() -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let exclude = ensure_exclude_file(&ctx.exclude_path)?;
    let entries = exclude.entries();
    let disabled = exclude.disabled_entries();

    let tracked = git::list_tracked(&ctx.root)?;
    let pattern_index = git::build_pattern_match_index(&ctx.root, &ctx.exclude_path, &tracked)?;

    let mut layered = Vec::new();
    let mut exposed: Vec<(String, String, Vec<String>)> = Vec::new();

    for entry in &entries {
        classify_entry(
            &ctx.root,
            &entry.value,
            &tracked,
            &pattern_index,
            &mut layered,
            &mut exposed,
        );
    }

    let excluded_set = exclude.managed_entry_set();
    let discovered_items = scan::discover_known_files_with_tracked(&ctx, &excluded_set, &tracked)?;
    let gitignored_count = discovered_items
        .iter()
        .filter(|item| !item.already_excluded && item.is_gitignored)
        .count();
    let not_excluded: Vec<_> = discovered_items
        .into_iter()
        .filter(|item| !item.already_excluded && !item.is_gitignored)
        .collect();
    let mut discovered: Vec<_> = not_excluded
        .iter()
        .filter(|i| !i.is_tracked)
        .map(|i| i.path.clone())
        .collect();
    discovered.sort();
    discovered.dedup();
    let mut tracked_ctx: Vec<_> = not_excluded
        .iter()
        .filter(|i| i.is_tracked)
        .map(|i| i.path.clone())
        .collect();
    tracked_ctx.sort();
    tracked_ctx.dedup();

    let mut history_info = None;
    let mut modified_files = Vec::new();
    if let Some(shadow) = ShadowRepo::open(&ctx.root) {
        history_info = shadow.last_snapshot_info().ok().flatten();
        if let Ok(files) = crate::shadow::resolve_history_files(&ctx, &entries, Some(&shadow)) {
            modified_files = shadow.pending_snapshot_files(&files).unwrap_or_default();
        }
    }

    if disabled.is_empty()
        && exposed.is_empty()
        && discovered.is_empty()
        && tracked_ctx.is_empty()
        && history_info.is_none()
        && modified_files.is_empty()
    {
        if layered.is_empty() && gitignored_count == 0 {
            println!(
                "No context files found. Run {} to get started.",
                ui::brand("layer scan")
            );
            return Ok(0);
        } else if layered.is_empty() {
            println!(
                "  {} All clear — {} already ignored by .gitignore.",
                ui::ok(),
                gitignored_count
            );
        } else if gitignored_count > 0 {
            println!(
                "  {} {} files in your local layer. ({} others ignored by .gitignore)",
                ui::ok(),
                layered.len(),
                gitignored_count
            );
        } else {
            println!(
                "  {} {} files in your local layer.",
                ui::ok(),
                layered.len()
            );
        }
    }

    let mut has_section = false;

    let all_active_clear =
        layered.is_empty() && exposed.is_empty() && discovered.is_empty() && tracked_ctx.is_empty();
    if !disabled.is_empty() && all_active_clear {
        println!(
            "  {} Layering is off — {} disabled ({}).",
            ui::disabled(),
            disabled.len(),
            ui::brand("layer on"),
        );
        if gitignored_count > 0 {
            println!(
                "  {} {} already ignored by .gitignore.",
                ui::info(),
                gitignored_count,
            );
        }
        has_section = true;
    }

    if !layered.is_empty() {
        println!("  {} Layered ({}):", ui::layered(), layered.len());
        for entry in &layered {
            println!("    {}", ui::dim_text(entry));
        }
        has_section = true;
    }

    if !disabled.is_empty() {
        if has_section {
            println!();
        }
        println!("  {} Disabled ({}):", ui::disabled(), disabled.len());
        for entry in &disabled {
            println!("    {}", ui::dim_text(&entry.value));
        }
        has_section = true;
    }

    if !exposed.is_empty() {
        if has_section {
            println!();
        }
        print_exposed_section("Exposed", &exposed);
        has_section = true;
    }

    if !discovered.is_empty() {
        if has_section {
            println!();
        }
        println!(
            "  {} {}:",
            ui::discovered(),
            ui::warn_text(&format!("Discovered ({})", discovered.len()))
        );
        let width = discovered.iter().map(|e| e.len()).max().unwrap_or(0);
        for entry in &discovered {
            println!(
                "    {:<width$}  {}",
                entry,
                ui::dim_text(&format!("layer add {entry}")),
                width = width
            );
        }
        has_section = true;
    }

    if !tracked_ctx.is_empty() {
        if has_section {
            println!();
        }
        println!(
            "  {} Exposed — tracked ({}):",
            ui::exposed(),
            tracked_ctx.len()
        );
        let width = tracked_ctx.iter().map(|e| e.len()).max().unwrap_or(0);
        for entry in &tracked_ctx {
            println!(
                "    {:<width$}  {}",
                entry,
                ui::warn_text(&format!("git rm --cached {}", entry.trim_end_matches('/'))),
                width = width
            );
        }
    }

    if let Some(info) = history_info {
        if has_section {
            println!();
        }
        println!("  {} History: {info}", ui::dim_text("~"));
        has_section = true;
    }

    if !modified_files.is_empty() {
        if has_section {
            println!();
        }
        println!(
            "  {} Modified ({}) — run {}:",
            ui::discovered(),
            modified_files.len(),
            ui::brand("layer snapshot"),
        );
        for file in &modified_files {
            println!("    {}", ui::warn_text(file));
        }
    }

    if !exposed.is_empty() || !tracked_ctx.is_empty() {
        return Ok(1);
    }

    Ok(0)
}

fn classify_entry(
    repo_root: &std::path::Path,
    entry: &str,
    tracked: &HashSet<String>,
    pattern_index: &HashMap<String, PatternMatchSummary>,
    layered: &mut Vec<String>,
    exposed: &mut Vec<(String, String, Vec<String>)>,
) {
    if entry.ends_with('/') {
        let dir = repo_root.join(entry.trim_end_matches('/'));
        if !dir.is_dir() {
            return;
        }

        let mut tracked_files: Vec<String> = tracked
            .iter()
            .filter(|path| path.starts_with(entry))
            .cloned()
            .collect();

        if !tracked_files.is_empty() {
            tracked_files.sort();
            let summary = format!("{} tracked:", tracked_files.len());
            exposed.push((entry.to_string(), summary, tracked_files));
            return;
        }

        layered.push(entry.to_string());
        return;
    }

    if git::contains_glob(entry) {
        let summary = pattern_index.get(entry).cloned().unwrap_or_default();
        if summary.total == 0 {
            return;
        }

        if summary.tracked_count() > 0 {
            exposed.push((
                entry.to_string(),
                "tracked — exclude has no effect".to_string(),
                Vec::new(),
            ));
            return;
        }

        layered.push(entry.to_string());
        return;
    }

    if tracked.contains(entry) {
        exposed.push((
            entry.to_string(),
            format!("git rm --cached {entry}"),
            Vec::new(),
        ));
        return;
    }

    if !repo_root.join(entry).exists() {
        return;
    }

    layered.push(entry.to_string());
}

fn print_exposed_section(title: &str, exposed: &[(String, String, Vec<String>)]) {
    println!("  {} {} ({}):", ui::exposed(), title, exposed.len());
    let width = exposed.iter().map(|(e, _, _)| e.len()).max().unwrap_or(0);
    for (entry, fix, tracked_files) in exposed {
        println!(
            "    {:<width$}  {}",
            entry,
            ui::warn_text(fix),
            width = width
        );
        for file in tracked_files {
            println!(
                "      {}",
                ui::warn_text(&format!("git rm --cached {file}"))
            );
        }
    }
}
