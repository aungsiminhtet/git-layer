use crate::exclude_file::{ensure_exclude_file_for_write, normalize_entry};
use crate::git;
use crate::guard::HookState;
use crate::ui;
use anyhow::Result;
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

        let disabled = exclude.disable_all();
        exclude.write(&ctx.exclude_path)?;
        for entry in &disabled {
            println!("  {} Now visible to Git: {entry}", ui::ok());
        }
        print_off_footer(&ctx)?;
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

        let disabled = exclude.disable_entries(&found);
        exclude.write(&ctx.exclude_path)?;
        for entry in &disabled {
            println!("  {} Now visible to Git: {entry}", ui::ok());
        }
        print_off_footer(&ctx)?;
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
            println!("  {} Now hidden from Git: {entry}", ui::ok());
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
            println!("  {} Now hidden from Git: {entry}", ui::ok());
        }
        print_on_footer();
        Ok(0)
    }
}

fn print_off_footer(ctx: &git::RepoContext) -> Result<()> {
    match crate::guard::hook_state(ctx)? {
        HookState::Installed => {
            println!(
                "  {} Guard active — commits containing layered files will still be blocked.",
                ui::info()
            );
        }
        HookState::NotInstalled => {
            println!(
                "  {} Guard is not installed. Run {} to block accidental commits.",
                ui::exposed(),
                ui::brand("layer guard"),
            );
        }
        HookState::ForeignHook => {
            println!(
                "  {} Layered files are now visible to Git. The existing pre-commit hook is not managed by layer.",
                ui::exposed(),
            );
        }
    }
    Ok(())
}

fn print_on_footer() {
    println!("  {} Layered files are hidden from Git again.", ui::info());
}
