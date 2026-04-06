use crate::exclude_file::{ensure_exclude_file_for_write, normalize_entry};
use crate::git;
use crate::ui;
use anyhow::{Context, Result};
use std::collections::HashSet;

pub fn run_off(files: Vec<String>, dry_run: bool) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let mut exclude = ensure_exclude_file_for_write(&ctx.exclude_path)?;
    let active = exclude.entries();

    if active.is_empty() {
        println!(
            "  {} No layered files are currently hidden from Git.",
            ui::info()
        );
        return Ok(2);
    }

    if files.is_empty() {
        if dry_run {
            for entry in &active {
                println!("  {} Would make {} visible to Git", ui::info(), entry.value);
            }
            ui::print_dry_run_notice();
            return Ok(0);
        }

        let restored_guard = restore_guard_before_off(&ctx)?;
        let disabled = exclude.disable_all();
        exclude.write(&ctx.exclude_path)?;
        for entry in &disabled {
            println!("  {} Visible to Git: {entry}", ui::ok());
        }
        print_off_footer(&ctx, restored_guard)?;
        Ok(0)
    } else {
        let active_set: HashSet<String> = active.iter().map(|e| e.value.clone()).collect();
        let disabled_set = exclude.disabled_entry_set();
        let targets: Vec<String> = files.iter().map(|f| normalize_entry(f)).collect();

        for target in &targets {
            if !active_set.contains(target.as_str()) {
                if disabled_set.contains(target.as_str()) {
                    println!("  {} {target} is already visible to Git", ui::info());
                } else {
                    println!("  {} {target} is not managed by layer", ui::info());
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
                println!("  {} Would make {target} visible to Git", ui::info());
            }
            ui::print_dry_run_notice();
            return Ok(0);
        }

        let restored_guard = restore_guard_before_off(&ctx)?;
        let disabled = exclude.disable_entries(&found);
        exclude.write(&ctx.exclude_path)?;
        for entry in &disabled {
            println!("  {} Visible to Git: {entry}", ui::ok());
        }
        print_off_footer(&ctx, restored_guard)?;
        Ok(0)
    }
}

pub fn run_on(files: Vec<String>, dry_run: bool) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let mut exclude = ensure_exclude_file_for_write(&ctx.exclude_path)?;
    let disabled_list = exclude.disabled_entries();

    if disabled_list.is_empty() {
        println!(
            "  {} No layered files are currently visible to Git.",
            ui::info()
        );
        return Ok(2);
    }

    if files.is_empty() {
        if dry_run {
            for entry in &disabled_list {
                println!("  {} Would hide {} from Git", ui::info(), entry.value);
            }
            ui::print_dry_run_notice();
            return Ok(0);
        }

        let enabled = exclude.enable_all();
        exclude.write(&ctx.exclude_path)?;
        for entry in &enabled {
            println!("  {} Hidden from Git: {entry}", ui::ok());
        }
        print_on_footer();
        Ok(0)
    } else {
        let disabled_set: HashSet<String> = disabled_list.iter().map(|e| e.value.clone()).collect();
        let active_set = exclude.entry_set();
        let targets: Vec<String> = files.iter().map(|f| normalize_entry(f)).collect();

        for target in &targets {
            if !disabled_set.contains(target.as_str()) {
                if active_set.contains(target.as_str()) {
                    println!("  {} {target} is already hidden from Git", ui::info());
                } else {
                    println!("  {} {target} is not managed by layer", ui::info());
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
                println!("  {} Would hide {target} from Git", ui::info());
            }
            ui::print_dry_run_notice();
            return Ok(0);
        }

        let enabled = exclude.enable_entries(&found);
        exclude.write(&ctx.exclude_path)?;
        for entry in &enabled {
            println!("  {} Hidden from Git: {entry}", ui::ok());
        }
        print_on_footer();
        Ok(0)
    }
}

fn restore_guard_before_off(ctx: &git::RepoContext) -> Result<bool> {
    let inspection = crate::guard::inspect(ctx)?;
    if let crate::guard::GuardHealth::NeedsRepairLocal { .. } = inspection.health {
        crate::guard::repair(ctx)
            .context("failed to restore guard before making layered files visible to Git")?;
        return Ok(true);
    }
    Ok(false)
}

fn print_off_footer(ctx: &git::RepoContext, restored_guard: bool) -> Result<()> {
    if restored_guard {
        println!(
            "  {} Guard restored — commits containing layered files will still be blocked.",
            ui::info()
        );
        return Ok(());
    }

    let inspection = crate::guard::inspect(ctx)?;
    match inspection.health {
        crate::guard::GuardHealth::ActiveDirect
        | crate::guard::GuardHealth::ActiveWrapper { .. }
        | crate::guard::GuardHealth::ActiveManual { .. } => {
            println!(
                "  {} Guard active — commits containing layered files will still be blocked.",
                ui::info()
            );
        }
        crate::guard::GuardHealth::Inactive => {
            println!(
                "  {} Guard is not installed. Run {} to block accidental commits.",
                ui::exposed(),
                ui::brand("layer guard"),
            );
        }
        crate::guard::GuardHealth::NeedsInstallLocal { framework } => match framework {
            crate::guard::HookFramework::PreCommit => {
                println!(
                    "  {} Guard is not installed. Run {} to wrap the existing Python pre-commit hook.",
                    ui::exposed(),
                    ui::brand("layer guard"),
                );
            }
            _ => {
                println!(
                    "  {} Guard is not installed. Run {} to set it up with the existing pre-commit hook.",
                    ui::exposed(),
                    ui::brand("layer guard"),
                );
            }
        },
        crate::guard::GuardHealth::NeedsManualExternal { framework } => match framework {
            crate::guard::HookFramework::Husky => {
                println!(
                    "  {} Guard is not installed. Run {} to add it to {}.",
                    ui::exposed(),
                    ui::brand("layer guard --manual"),
                    ui::brand(".husky/pre-commit"),
                );
            }
            crate::guard::HookFramework::Lefthook => {
                println!(
                    "  {} Guard is not installed. Run {} to add it to {}.",
                    ui::exposed(),
                    ui::brand("layer guard --manual"),
                    ui::brand("lefthook.yml"),
                );
            }
            _ => {
                println!(
                    "  {} Guard is not installed. Run {} for manual setup with the existing pre-commit hook.",
                    ui::exposed(),
                    ui::brand("layer guard --manual"),
                );
            }
        },
        crate::guard::GuardHealth::NeedsRepairLocal { .. } => {
            println!(
                "  {} Guard could not be restored. Run {} to repair it.",
                ui::exposed(),
                ui::brand("layer guard --wrapper"),
            );
        }
        crate::guard::GuardHealth::Broken { local, reason, .. } => match (local, reason) {
            (false, crate::guard::BrokenReason::MissingExpectedHook) => {
                println!(
                    "  {} Guard is managed outside .git. Run {} to repair it before you continue.",
                    ui::exposed(),
                    ui::brand("layer guard --manual"),
                );
            }
            (_, crate::guard::BrokenReason::MissingExpectedHook) => {
                println!(
                    "  {} Guard is missing. Run {} to restore it before you continue.",
                    ui::exposed(),
                    ui::brand("layer guard"),
                );
            }
            (_, crate::guard::BrokenReason::MissingPreservedHook) => {
                println!(
                    "  {} Guard is broken. Run {} or {} to repair it before you continue.",
                    ui::exposed(),
                    ui::brand("layer guard --remove"),
                    ui::brand("layer guard --wrapper"),
                );
            }
        },
    }
    Ok(())
}

fn print_on_footer() {
    println!("  {} Layered files are hidden from Git again.", ui::info());
}
