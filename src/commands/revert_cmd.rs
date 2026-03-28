use crate::agent;
use crate::exclude_file::normalize_entry;
use crate::git;
use crate::shadow::ShadowRepo;
use crate::ui;
use anyhow::{anyhow, Result};

pub fn run(file: String, to: usize) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let shadow = ShadowRepo::open(&ctx.root).ok_or_else(|| {
        anyhow!(
            "no history found — run {} to start tracking",
            ui::brand("layer snapshot")
        )
    })?;

    let normalized = normalize_entry(&file);
    let rev = format!("HEAD~{to}");

    shadow
        .shadow_git(&["rev-parse", "--verify", &rev])
        .map_err(|_| {
            let count = shadow.commit_count().unwrap_or(0);
            anyhow!("not enough history — only {count} snapshots exist")
        })?;

    let file_ref = format!("{rev}:{normalized}");
    shadow
        .shadow_git(&["cat-file", "-e", &file_ref])
        .map_err(|_| anyhow!("'{normalized}' not found at {rev}"))?;

    let agent = agent::detect_agent();
    if ctx.root.join(&normalized).exists() {
        let current_files = vec![normalized.clone()];
        let _ = shadow.track_files(&current_files);
        let _ = shadow.snapshot_paths(
            &format!("auto: before revert of {normalized}"),
            &agent,
            &current_files,
        );
    }

    let content = shadow.shadow_git_bytes(&["show", &file_ref])?;
    let target_path = ctx.root.join(&normalized);

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| anyhow!("failed to create directory: {e}"))?;
    }

    std::fs::write(&target_path, &content)
        .map_err(|e| anyhow!("failed to write {normalized}: {e}"))?;

    let restored_files = vec![normalized.clone()];
    shadow.track_files(&restored_files)?;
    if !shadow.snapshot_paths(
        &format!("revert: {normalized} to {rev}"),
        &agent,
        &restored_files,
    )? {
        println!("  {} '{}' already matches {}", ui::ok(), normalized, rev);
        return Ok(2);
    }

    println!(
        "  {} Reverted '{}' to {} snapshots ago",
        ui::ok(),
        normalized,
        to
    );
    Ok(0)
}
