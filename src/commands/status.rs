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
    let managed_entries = exclude.managed_entries();
    let disabled = exclude.disabled_entries();
    let layer_is_off = !disabled.is_empty();

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
    let mut gitignored: Vec<_> = discovered_items
        .iter()
        .filter(|item| !item.already_excluded && item.is_gitignored)
        .map(|item| item.path.clone())
        .collect();
    gitignored.sort();
    gitignored.dedup();
    let gitignored_count = gitignored.len();
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
    let guard = crate::guard::inspect(&ctx)?;
    let mut printed_guard = false;
    if let Some(shadow) = ShadowRepo::open(&ctx.root) {
        history_info = shadow.last_snapshot_info().ok().flatten();
        if let Ok(files) =
            crate::shadow::resolve_history_files(&ctx, &managed_entries, Some(&shadow))
        {
            modified_files = shadow.pending_snapshot_files(&files).unwrap_or_default();
        }
    }

    if disabled.is_empty()
        && exposed.is_empty()
        && discovered.is_empty()
        && tracked_ctx.is_empty()
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
                "  {} {} known context {} already hidden by .gitignore.",
                ui::info(),
                gitignored_count,
                if gitignored_count == 1 {
                    "file is"
                } else {
                    "files are"
                }
            );
            return Ok(0);
        } else if gitignored_count > 0 {
            println!("  {} Layer: {}", ui::ok(), ui::state_on());
            println!(
                "  {} {} {} hidden by layer.",
                ui::info(),
                layered.len(),
                if layered.len() == 1 {
                    "file is"
                } else {
                    "files are"
                },
            );
            println!(
                "  {} {} known context {} already hidden by .gitignore.",
                ui::info(),
                gitignored_count,
                if gitignored_count == 1 {
                    "file is"
                } else {
                    "files are"
                }
            );
            println!();
            print_guard_status_line(&guard)?;
            return Ok(0);
        } else {
            println!("  {} Layer: {}", ui::ok(), ui::state_on());
            println!(
                "  {} {} {} hidden by layer.",
                ui::info(),
                layered.len(),
                if layered.len() == 1 {
                    "file is"
                } else {
                    "files are"
                }
            );
            println!();
            print_guard_status_line(&guard)?;
            return Ok(0);
        }
    }

    let mut has_section = false;

    if layer_is_off {
        println!("  {} Layer: {}", ui::disabled(), ui::state_off());
        has_section = true;
    } else if !entries.is_empty() {
        println!("  {} Layer: {}", ui::ok(), ui::state_on());
        has_section = true;
    }

    let all_active_clear =
        layered.is_empty() && exposed.is_empty() && discovered.is_empty() && tracked_ctx.is_empty();
    if !disabled.is_empty() && all_active_clear {
        println!();
        println!(
            "  {} {} layered {} currently visible to Git. Run {} before staging or committing.",
            ui::info(),
            disabled.len(),
            if disabled.len() == 1 {
                "file is"
            } else {
                "files are"
            },
            ui::brand("layer on"),
        );
        print_guard_status_line(&guard)?;
        printed_guard = true;
    }

    if !layered.is_empty() {
        println!("  {} Hidden by layer ({}):", ui::layered(), layered.len());
        for entry in &layered {
            println!("    {}", ui::dim_text(entry));
        }
        has_section = true;
    }

    if !disabled.is_empty() {
        if has_section {
            println!();
        }
        println!("  {} Visible to Git ({}):", ui::disabled(), disabled.len());
        for entry in &disabled {
            println!("    {}", ui::dim_text(&entry.value));
        }
        has_section = true;
    }

    if !exposed.is_empty() {
        if has_section {
            println!();
        }
        print_exposed_section("Still visible to Git", &exposed);
        has_section = true;
    }

    if !discovered.is_empty() {
        if has_section {
            println!();
        }
        println!(
            "  {} Available to add ({}):",
            ui::discovered(),
            discovered.len()
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
            "  {} Tracked files still visible to Git ({}):",
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
        has_section = true;
    }

    if !gitignored.is_empty() {
        if has_section {
            println!();
        }
        println!(
            "  {} Already hidden by .gitignore ({}):",
            ui::info(),
            gitignored.len()
        );
        for entry in &gitignored {
            println!("    {}", ui::dim_text(entry));
        }
        has_section = true;
    }

    if !modified_files.is_empty() {
        if let Some(info) = history_info {
            if has_section {
                println!();
            }
            println!("  {} History: {info}", ui::dim_text("~"));
            has_section = true;
        }
    }

    if !modified_files.is_empty() {
        if has_section {
            println!();
        }
        println!(
            "  {} Modified since last snapshot ({}) — run {}:",
            ui::discovered(),
            modified_files.len(),
            ui::brand("layer snapshot"),
        );
        for file in &modified_files {
            println!("    {}", ui::warn_text(file));
        }
        has_section = true;
    }

    if has_section && !printed_guard {
        println!();
    }
    if !printed_guard {
        print_guard_status_line(&guard)?;
    }

    if !exposed.is_empty() || !tracked_ctx.is_empty() {
        return Ok(1);
    }

    Ok(0)
}

fn print_guard_status_line(guard: &crate::guard::GuardInspection) -> Result<()> {
    match &guard.health {
        crate::guard::GuardHealth::ActiveDirect
        | crate::guard::GuardHealth::ActiveWrapper { .. } => {
            println!("  {} Guard: pre-commit hook active", ui::ok());
        }
        crate::guard::GuardHealth::ActiveManual { framework, .. } => {
            if let Some(label) = framework.label() {
                println!(
                    "  {} Guard: manual integration active via {}",
                    ui::ok(),
                    ui::brand(label)
                );
            } else {
                println!("  {} Guard: manual integration active", ui::ok());
            }
        }
        crate::guard::GuardHealth::Inactive => {
            println!(
                "  {} Guard: not installed — run {} to block accidental commits",
                ui::exposed(),
                ui::brand("layer guard")
            );
        }
        crate::guard::GuardHealth::NeedsRepairLocal { .. } => {
            println!(
                "  {} Guard: replaced by another hook installer — run {} to restore it",
                ui::exposed(),
                ui::brand("layer guard --wrapper")
            );
        }
        crate::guard::GuardHealth::NeedsInstallLocal { framework } => match framework {
            crate::guard::HookFramework::PreCommit => {
                println!(
                    "  {} Guard: not installed — run {} to wrap the existing Python pre-commit hook",
                    ui::exposed(),
                    ui::brand("layer guard")
                );
            }
            _ => {
                println!(
                    "  {} Guard: not installed — run {} to set it up with the existing pre-commit hook",
                    ui::exposed(),
                    ui::brand("layer guard")
                );
            }
        },
        crate::guard::GuardHealth::NeedsManualExternal { framework } => match framework {
            crate::guard::HookFramework::Husky => {
                println!(
                    "  {} Guard: not installed — run {} to add it to {}",
                    ui::exposed(),
                    ui::brand("layer guard --manual"),
                    ui::brand(".husky/pre-commit")
                );
            }
            crate::guard::HookFramework::Lefthook => {
                println!(
                    "  {} Guard: not installed — run {} to add it to {}",
                    ui::exposed(),
                    ui::brand("layer guard --manual"),
                    ui::brand("lefthook.yml")
                );
            }
            _ => {
                println!(
                    "  {} Guard: not installed — run {} for manual setup with the existing pre-commit hook",
                    ui::exposed(),
                    ui::brand("layer guard --manual")
                );
            }
        },
        crate::guard::GuardHealth::Broken { reason, local, .. } => match (local, reason) {
            (false, crate::guard::BrokenReason::MissingExpectedHook) => {
                println!(
                    "  {} Guard: expected hook is managed outside .git — run {} to repair it",
                    ui::exposed(),
                    ui::brand("layer guard --manual")
                );
            }
            (_, crate::guard::BrokenReason::MissingExpectedHook) => {
                println!(
                    "  {} Guard: expected hook is missing — run {} to restore it",
                    ui::exposed(),
                    ui::brand("layer guard")
                );
            }
            (_, crate::guard::BrokenReason::MissingPreservedHook) => {
                println!(
                    "  {} Guard: preserved original hook is missing — run {} or {} to repair it",
                    ui::exposed(),
                    ui::brand("layer guard --remove"),
                    ui::brand("layer guard --wrapper")
                );
            }
        },
    }
    Ok(())
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
            let summary = format!(
                "{} tracked {} — remove {} from Git first",
                tracked_files.len(),
                if tracked_files.len() == 1 {
                    "file"
                } else {
                    "files"
                },
                if tracked_files.len() == 1 {
                    "it"
                } else {
                    "them"
                }
            );
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
                "tracked files — remove them from Git first".to_string(),
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
            format!("tracked — run git rm --cached {entry}"),
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
