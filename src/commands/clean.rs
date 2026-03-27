use crate::exclude_file::{ensure_exclude_file, Entry};
use crate::git;
use crate::git::RepoContext;
use crate::ui;
use anyhow::Result;
use dialoguer::Confirm;
use std::collections::HashSet;

pub fn run(dry_run: bool, all: bool) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let mut exclude = ensure_exclude_file(&ctx.exclude_path)?;
    let entries = exclude.entries();

    let stale_managed = collect_stale_entries(&ctx, &entries)?;

    let stale_user = if all {
        let user_entries = exclude.user_entries();
        collect_stale_entries(&ctx, &user_entries)?
    } else {
        Vec::new()
    };

    if stale_managed.is_empty() && stale_user.is_empty() {
        println!("  {} No stale entries found.", ui::ok());
        return Ok(2);
    }

    if dry_run {
        let total = stale_managed.len() + stale_user.len();
        println!(
            "{}",
            ui::heading(&format!("Would remove {} stale entries:", total))
        );
        for item in &stale_managed {
            println!("  {} {}", ui::stale(), item);
        }
        for item in &stale_user {
            println!("  {} {} {}", ui::stale(), item, ui::dim_text("(manual)"));
        }
        ui::print_dry_run_notice();
        return Ok(0);
    }

    let total = stale_managed.len() + stale_user.len();
    println!(
        "{}",
        ui::heading(&format!("Found {} stale entries:", total))
    );
    for item in &stale_managed {
        println!("  {} {}", ui::stale(), item);
    }
    for item in &stale_user {
        println!("  {} {} {}", ui::stale(), item, ui::dim_text("(manual)"));
    }

    ui::require_tty(
        "interactive confirmation requires a TTY. Re-run in a terminal or use --dry-run",
    )?;

    let confirmed = Confirm::new()
        .with_prompt("Remove these entries?")
        .default(false)
        .interact()?;

    if !confirmed {
        println!("No changes made.");
        return Ok(2);
    }

    let mut total_removed = 0usize;

    if !stale_managed.is_empty() {
        let targets = stale_managed.into_iter().collect::<HashSet<_>>();
        let removed = exclude.remove_exact(&targets);
        total_removed += removed.len();
    }

    if !stale_user.is_empty() {
        let targets = stale_user.into_iter().collect::<HashSet<_>>();
        let removed = exclude.remove_from_user(&targets);
        total_removed += removed.len();
    }

    if total_removed == 0 {
        println!("No stale entries removed.");
        return Ok(2);
    }

    exclude.write(&ctx.exclude_path)?;

    println!("  {} Removed {} stale entries.", ui::ok(), total_removed);
    Ok(0)
}

pub fn collect_stale_entries(ctx: &RepoContext, entries: &[Entry]) -> Result<Vec<String>> {
    let tracked = git::list_tracked(&ctx.root)?;
    let pattern_index = git::build_pattern_match_index(&ctx.root, &ctx.exclude_path, &tracked)?;

    let mut stale = Vec::new();

    for entry in entries {
        let value = entry.value.as_str();
        if value.ends_with('/') {
            if !ctx.root.join(value.trim_end_matches('/')).is_dir() {
                stale.push(entry.value.clone());
            }
            continue;
        }

        if git::contains_glob(value) {
            let count = pattern_index.get(value).map_or(0, |s| s.total);
            if count == 0 {
                stale.push(entry.value.clone());
            }
            continue;
        }

        if !ctx.root.join(value).exists() {
            stale.push(entry.value.clone());
        }
    }

    Ok(stale)
}
