use crate::exclude_file::{ensure_exclude_file_for_write, normalize_entry};
use crate::git;
use crate::ui;
use anyhow::Result;
use std::collections::HashSet;

pub fn run_off(files: Vec<String>, dry_run: bool) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let mut exclude = ensure_exclude_file_for_write(&ctx.exclude_path)?;
    let active = exclude.entries();

    if active.is_empty() {
        println!("No active entries to disable.");
        return Ok(2);
    }

    if files.is_empty() {
        // Disable all
        if dry_run {
            for entry in &active {
                println!("  {} Would disable {}", ui::info(), entry.value);
            }
            ui::print_dry_run_notice();
            return Ok(0);
        }

        let disabled = exclude.disable_all();
        exclude.write(&ctx.exclude_path)?;
        for entry in &disabled {
            println!("  {} Disabled {entry}", ui::ok());
        }
        Ok(0)
    } else {
        // Disable specific entries
        let active_set: HashSet<String> = active.iter().map(|e| e.value.clone()).collect();
        let disabled_set = exclude.disabled_entry_set();
        let targets: Vec<String> = files.iter().map(|f| normalize_entry(f)).collect();

        for target in &targets {
            if !active_set.contains(target.as_str()) {
                if disabled_set.contains(target.as_str()) {
                    println!("  {} {target} is already disabled", ui::info());
                } else {
                    println!("  {} {target} is not layered", ui::info());
                }
            }
        }

        let found: HashSet<String> = targets
            .into_iter()
            .filter(|t| active_set.contains(t.as_str()))
            .collect();
        if found.is_empty() {
            return Ok(2);
        }

        if dry_run {
            for target in &found {
                println!("  {} Would disable {target}", ui::info());
            }
            ui::print_dry_run_notice();
            return Ok(0);
        }

        let disabled = exclude.disable_entries(&found);
        exclude.write(&ctx.exclude_path)?;
        for entry in &disabled {
            println!("  {} Disabled {entry}", ui::ok());
        }
        Ok(0)
    }
}

pub fn run_on(files: Vec<String>, dry_run: bool) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let mut exclude = ensure_exclude_file_for_write(&ctx.exclude_path)?;
    let disabled_list = exclude.disabled_entries();

    if disabled_list.is_empty() {
        println!("No disabled entries to enable.");
        return Ok(2);
    }

    if files.is_empty() {
        // Enable all
        if dry_run {
            for entry in &disabled_list {
                println!("  {} Would enable {}", ui::info(), entry.value);
            }
            ui::print_dry_run_notice();
            return Ok(0);
        }

        let enabled = exclude.enable_all();
        exclude.write(&ctx.exclude_path)?;
        for entry in &enabled {
            println!("  {} Enabled {entry}", ui::ok());
        }
        Ok(0)
    } else {
        // Enable specific entries
        let disabled_set: HashSet<String> = disabled_list.iter().map(|e| e.value.clone()).collect();
        let active_set = exclude.entry_set();
        let targets: Vec<String> = files.iter().map(|f| normalize_entry(f)).collect();

        for target in &targets {
            if !disabled_set.contains(target.as_str()) {
                if active_set.contains(target.as_str()) {
                    println!("  {} {target} is already enabled", ui::info());
                } else {
                    println!("  {} {target} is not layered", ui::info());
                }
            }
        }

        let found: HashSet<String> = targets
            .into_iter()
            .filter(|t| disabled_set.contains(t.as_str()))
            .collect();
        if found.is_empty() {
            return Ok(2);
        }

        if dry_run {
            for target in &found {
                println!("  {} Would enable {target}", ui::info());
            }
            ui::print_dry_run_notice();
            return Ok(0);
        }

        let enabled = exclude.enable_entries(&found);
        exclude.write(&ctx.exclude_path)?;
        for entry in &enabled {
            println!("  {} Enabled {entry}", ui::ok());
        }
        Ok(0)
    }
}
