use crate::agent;
use crate::exclude_file::{ensure_exclude_file, normalize_entry};
use crate::git;
use crate::shadow::{self, ShadowRepo};
use crate::ui;
use anyhow::{anyhow, Result};

pub fn run(file: String) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let shadow = ShadowRepo::open(&ctx.root).ok_or_else(|| {
        anyhow!(
            "no history found — run {} to start tracking",
            ui::brand("layer snapshot")
        )
    })?;

    let normalized = normalize_entry(&file);
    let exclude = ensure_exclude_file(&ctx.exclude_path)?;
    let entries = exclude.entries();
    let files = shadow::resolve_history_files(&ctx, &entries, Some(&shadow))?;
    if !files.iter().any(|path| path == &normalized) {
        println!("No history found for '{normalized}'.");
        return Ok(2);
    }

    if !ctx.root.join(&normalized).exists() {
        return Err(anyhow!("file not found: {normalized}"));
    }

    let current_file = vec![normalized.clone()];
    let _ = shadow.track_files(&current_file);
    let agent = agent::detect_agent();
    let _ = shadow.snapshot_paths("auto: snapshot for blame", &agent, &current_file);

    let color_arg = if ui::is_stdout_tty() {
        "--color-by-age"
    } else {
        "--no-color"
    };

    match shadow.shadow_git(&["blame", color_arg, &normalized]) {
        Ok(text) if !text.trim().is_empty() => {
            print!("{text}");
            Ok(0)
        }
        _ => {
            println!("No history found for '{normalized}'.");
            Ok(2)
        }
    }
}
