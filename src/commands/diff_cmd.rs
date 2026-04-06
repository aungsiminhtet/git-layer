use crate::diff_viewer;
use crate::exclude_file::ensure_exclude_file;
use crate::git;
use crate::shadow::{self, ShadowRepo};
use crate::ui;
use anyhow::{anyhow, Result};

pub fn run(file: Option<String>) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let shadow = ShadowRepo::open(&ctx.root).ok_or_else(|| {
        anyhow!(
            "no history found — run {} to start tracking",
            ui::brand("layer snapshot")
        )
    })?;

    let exclude = ensure_exclude_file(&ctx.exclude_path)?;
    let entries = exclude.managed_entries();
    let files = shadow::resolve_history_files(&ctx, &entries, Some(&shadow))?;

    if files.is_empty() {
        println!("No files managed by layer are available to diff.");
        return Ok(2);
    }

    shadow.track_files(&files)?;

    if ui::is_stdout_tty() && file.is_none() {
        diff_viewer::run_interactive(&shadow)?;
        return Ok(0);
    }

    let color_arg = if ui::is_stdout_tty() {
        "--color=always"
    } else {
        "--color=never"
    };

    let mut args = vec!["diff", "--cached", "HEAD", color_arg];

    if let Some(ref f) = file {
        args.push("--");
        args.push(f);
    }

    let output = shadow.shadow_git(&args)?;
    if output.trim().is_empty() {
        println!("No changes since last snapshot.");
        return Ok(2);
    }

    print!("{output}");
    Ok(0)
}
