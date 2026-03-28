use crate::exclude_file::ensure_exclude_file_for_write;
use crate::git;
use crate::ui;
use anyhow::Result;
use dialoguer::MultiSelect;
use std::collections::HashSet;

pub fn run(files: Vec<String>, dry_run: bool) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let mut exclude = ensure_exclude_file_for_write(&ctx.exclude_path)?;
    let entries = exclude.entries();

    if entries.is_empty() {
        println!("No layered entries to remove.");
        return Ok(2);
    }

    if files.is_empty() {
        ui::require_tty("interactive mode requires a TTY. Use 'layer rm <files...>' instead")?;

        let items: Vec<String> = entries.iter().map(|e| e.value.clone()).collect();
        println!("{}", ui::heading("Select entries to remove"));
        let theme = ui::layer_theme();
        ui::print_select_hint();
        let selections = MultiSelect::with_theme(&theme)
            .items(&items)
            .report(false)
            .interact_opt()?;

        let Some(selected) = selections else {
            return Ok(2);
        };

        if selected.is_empty() {
            println!("No entries selected.");
            return Ok(2);
        }

        let targets = selected
            .into_iter()
            .map(|idx| items[idx].clone())
            .collect::<HashSet<_>>();

        if dry_run {
            for item in &targets {
                println!("  {} Would remove '{item}'", ui::info());
            }
            ui::print_dry_run_notice();
            return Ok(0);
        }

        let removed = exclude.remove_exact(&targets);
        if removed.is_empty() {
            return Ok(2);
        }

        exclude.write(&ctx.exclude_path)?;
        for item in &removed {
            println!("  {} Removed '{item}'", ui::ok());
        }

        return Ok(0);
    }

    let current = entries.into_iter().map(|e| e.value).collect::<HashSet<_>>();
    let targets = files
        .iter()
        .map(|f| f.trim().to_string())
        .filter(|f| !f.is_empty())
        .collect::<HashSet<_>>();

    for target in &targets {
        if !current.contains(target) {
            println!("  '{target}' is not layered");
        }
    }

    let found: HashSet<_> = targets
        .iter()
        .filter(|t| current.contains(*t))
        .cloned()
        .collect();
    if found.is_empty() {
        if dry_run {
            ui::print_dry_run_notice();
        }
        return Ok(2);
    }

    if dry_run {
        for target in &found {
            println!("  {} Would remove '{target}'", ui::info());
        }
        ui::print_dry_run_notice();
        return Ok(0);
    }

    let removed = exclude.remove_exact(&found);
    for item in &removed {
        println!("  {} Removed '{item}'", ui::ok());
    }

    exclude.write(&ctx.exclude_path)?;

    Ok(0)
}
