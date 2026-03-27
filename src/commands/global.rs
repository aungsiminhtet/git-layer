use crate::exclude_file::{normalize_entry, ExcludeFile};
use crate::ui;
use anyhow::{anyhow, Context, Result};
use dialoguer::MultiSelect;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn add(files: Vec<String>) -> Result<i32> {
    if files.is_empty() {
        return Err(anyhow!(
            "no files provided. Use 'layer global add <files...>'"
        ));
    }

    let path = global_ignore_path()?;
    let mut file = ensure_global_file(&path)?;
    let mut known = all_entry_set(&file);
    let mut added = 0usize;

    for raw in files {
        let normalized = normalize_entry(&raw);
        if normalized.is_empty() {
            continue;
        }

        if known.contains(&normalized) {
            println!(
                "  {} '{normalized}' already in global gitignore",
                ui::info()
            );
            continue;
        }

        file.append_entry(&normalized);
        known.insert(normalized.clone());
        println!(
            "  {} Added '{normalized}' to global gitignore {}",
            ui::ok(),
            ui::dim_text(&format!("({})", path.display()))
        );
        added += 1;
    }

    if added == 0 {
        return Ok(2);
    }

    file.write(&path)?;
    Ok(0)
}

pub fn ls() -> Result<i32> {
    let path = global_ignore_path()?;
    let file = ensure_global_file(&path)?;
    let managed = file.entries();
    let external = file.user_entries();

    if managed.is_empty() && external.is_empty() {
        println!("Global gitignore ({}) is empty.", path.display());
        return Ok(2);
    }

    println!(
        "{}",
        ui::heading(&format!("Global gitignore ({}):", path.display()))
    );
    for entry in &managed {
        println!("  {}", entry.value);
    }
    for entry in &external {
        println!("  {}  {}", entry.value, ui::dim_text("(external)"));
    }

    Ok(0)
}

pub fn rm(files: Vec<String>) -> Result<i32> {
    let path = global_ignore_path()?;
    let mut file = ensure_global_file(&path)?;
    let all_entries = all_entries_vec(&file);

    if all_entries.is_empty() {
        println!(
            "Global gitignore ({}) is empty. Nothing to remove.",
            path.display()
        );
        return Ok(2);
    }

    if files.is_empty() {
        ui::require_tty(
            "interactive mode requires a TTY. Use 'layer global rm <files...>' instead",
        )?;

        let items = all_entries;
        println!(
            "{}",
            ui::heading("Select entries to remove from global gitignore")
        );
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
        let mut removed = file.remove_exact(&targets);
        removed.extend(file.remove_from_user(&targets));

        if removed.is_empty() {
            return Ok(2);
        }

        file.write(&path)?;
        for item in removed {
            println!("  {} Removed '{item}' from global gitignore.", ui::ok());
        }

        return Ok(0);
    }

    let existing = all_entry_set(&file);
    let targets = files
        .into_iter()
        .map(|f| f.trim().to_string())
        .filter(|f| !f.is_empty())
        .collect::<HashSet<_>>();

    let has_any = targets.iter().any(|t| existing.contains(t));

    if !has_any {
        for target in targets {
            println!("  {} '{target}' not in global gitignore", ui::info());
        }
        return Ok(2);
    }

    let mut removed = file.remove_exact(&targets);
    removed.extend(file.remove_from_user(&targets));
    let removed_set = removed.iter().cloned().collect::<HashSet<_>>();

    file.write(&path)?;

    for target in targets {
        if removed_set.contains(&target) {
            println!("  {} Removed '{target}' from global gitignore.", ui::ok());
        } else {
            println!("  {} '{target}' not in global gitignore", ui::info());
        }
    }

    Ok(0)
}

pub fn global_ignore_path() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["config", "--global", "core.excludesFile"])
        .output()
        .context("failed to read git global excludesFile")?;

    let configured = if output.status.success() {
        let value = String::from_utf8(output.stdout).context("git config output was not UTF-8")?;
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    } else {
        None
    };

    let raw = configured.unwrap_or_else(|| "~/.config/git/ignore".to_string());
    Ok(expand_tilde(&raw))
}

fn all_entry_set(file: &ExcludeFile) -> HashSet<String> {
    let mut set = file.entry_set();
    for e in file.user_entries() {
        set.insert(e.value);
    }
    set
}

fn all_entries_vec(file: &ExcludeFile) -> Vec<String> {
    file.entries()
        .into_iter()
        .chain(file.user_entries())
        .map(|e| e.value)
        .collect()
}

fn ensure_global_file(path: &Path) -> Result<ExcludeFile> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(path, "").with_context(|| format!("failed to create {}", path.display()))?;
    }

    ExcludeFile::load(path)
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home);
        }
        return PathBuf::from(path);
    }

    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }

    PathBuf::from(path)
}
