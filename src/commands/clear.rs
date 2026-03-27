use crate::exclude_file::ensure_exclude_file_for_write;
use crate::git;
use crate::ui;
use anyhow::Result;
use dialoguer::Confirm;

pub fn run(dry_run: bool) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let mut exclude = ensure_exclude_file_for_write(&ctx.exclude_path)?;
    let count = exclude.entries().len();

    if count == 0 {
        println!("No layered entries. Nothing to clear.");
        return Ok(2);
    }

    if dry_run {
        println!("Would remove all {count} entries.");
        ui::print_dry_run_notice();
        return Ok(0);
    }

    ui::print_warning(&format!("This will remove all {count} entries."));

    ui::require_tty(
        "interactive confirmation requires a TTY. Re-run in a terminal or use --dry-run",
    )?;

    let confirmed = Confirm::new()
        .with_prompt("Are you sure?")
        .default(false)
        .interact()?;

    if !confirmed {
        println!("No changes made.");
        return Ok(2);
    }

    exclude.clear_managed();
    exclude.write(&ctx.exclude_path)?;

    println!("  {} All entries removed.", ui::ok());
    Ok(0)
}
