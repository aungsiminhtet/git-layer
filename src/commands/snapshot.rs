use crate::exclude_file::ensure_exclude_file;
use crate::git;
use crate::shadow::{self, ShadowRepo};
use crate::ui;
use anyhow::Result;

pub fn run(files: Vec<String>, message: Option<String>) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let exclude = ensure_exclude_file(&ctx.exclude_path)?;
    let entries = exclude.entries();

    if entries.is_empty() {
        println!("No layered entries. Nothing to snapshot.");
        return Ok(2);
    }

    let shadow = ShadowRepo::open(&ctx.root);
    let all_files = shadow::resolve_history_files(&ctx, &entries, shadow.as_ref())?;
    if all_files.is_empty() {
        println!("No layered files to snapshot.");
        return Ok(2);
    }

    let target_files = if files.is_empty() {
        all_files
    } else {
        let requested = files
            .iter()
            .map(|file| crate::exclude_file::normalize_entry(file))
            .collect::<std::collections::HashSet<_>>();
        let matched: Vec<String> = all_files
            .into_iter()
            .filter(|file| requested.contains(file))
            .collect();
        if matched.is_empty() {
            println!("None of the specified files are layered.");
            return Ok(2);
        }
        matched
    };

    let shadow = match shadow {
        Some(shadow) => shadow,
        None => ShadowRepo::init(&ctx.root)?,
    };

    shadow.track_files(&target_files)?;

    let msg = message.unwrap_or_else(|| {
        if target_files.len() <= 3 {
            format!("snapshot: {}", target_files.join(", "))
        } else {
            format!("snapshot: {} files", target_files.len())
        }
    });

    if shadow.snapshot_paths(&msg, &target_files)? {
        println!(
            "  {} Snapshot created ({} files)",
            ui::ok(),
            target_files.len()
        );
        Ok(0)
    } else {
        println!("  {} No changes since last snapshot.", ui::ok());
        Ok(2)
    }
}
