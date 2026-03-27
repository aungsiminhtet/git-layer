use crate::exclude_file::{ensure_exclude_file, normalize_entry};
use crate::git;
use crate::ui;
use anyhow::Result;
use std::path::Path;

pub fn run(file: String, verbose: bool) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    // Side effect: creates .git/info/exclude if missing so check-ignore works.
    let _exclude = ensure_exclude_file(&ctx.exclude_path)?;
    let normalized = normalize_entry(&file).trim_end_matches('/').to_string();

    let ignore_no_index = git::check_ignore_verbose_no_index(&ctx.root, &normalized)?;
    let ignore_match = git::check_ignore_verbose(&ctx.root, &normalized)?;
    let tracked = git::is_tracked(&ctx.root, &normalized)?;
    let exists = ctx.root.join(&normalized).exists();

    if let Some(matched) = ignore_no_index {
        if git::is_local_exclude_source(&ctx.root, &ctx.exclude_path, &matched.source) {
            if tracked {
                println!(
                    "'{}' is {} — excluded but still tracked by git.",
                    normalized,
                    ui::warn_text("exposed")
                );
                println!("  Layered in: .git/info/exclude (line {})", matched.line);
                println!("  Tracked:  YES — this is why git still sees it");
                println!("  Fix:      git rm --cached {}", normalized);
                return finish(1, verbose);
            }

            println!(
                "'{}' is {} — hidden from git.",
                normalized,
                ui::brand("layered")
            );
            println!("  Layered in: .git/info/exclude (line {})", matched.line);
            println!("  Tracked:   no");
            println!("  Exists:    {}", if exists { "yes" } else { "no" });
            return finish(0, verbose);
        }
    }

    if let Some(matched) = ignore_match {
        let source = matched.source.replace('\\', "/");
        if source.ends_with(".gitignore") {
            let source_path = relativize(&ctx.root, &source);
            println!(
                "'{}' is ignored by .gitignore — already handled — no need to layer.",
                normalized
            );
            println!("  Ignored by: {} (line {})", source_path, matched.line);
            println!("  Tracked:    {}", yes_no(tracked));
            println!("  Exists:     {}", yes_no(exists));
            return finish(if tracked { 1 } else { 0 }, verbose);
        }

        if !git::is_local_exclude_source(&ctx.root, &ctx.exclude_path, &source) {
            println!(
                "'{}' is ignored by global gitignore — already handled — no need to layer.",
                normalized
            );
            println!("  Ignored by: {} (line {})", source, matched.line);
            println!("  Tracked:    {}", yes_no(tracked));
            println!("  Exists:     {}", yes_no(exists));
            return finish(if tracked { 1 } else { 0 }, verbose);
        }
    }

    if tracked {
        println!(
            "'{}' is {} — tracked and not layered.",
            normalized,
            ui::warn_text("exposed")
        );
        println!("  Layered:  no");
        println!("  Tracked:  yes");
        println!("  Exists:   {}", yes_no(exists));
        return finish(1, verbose);
    }

    println!(
        "'{}' is {} — untracked and not in any layer.",
        normalized,
        ui::brand("discovered")
    );
    println!("  Layered:  no");
    println!("  Tracked:  no");
    println!("  Exists:   {}", yes_no(exists));
    println!("  Fix:      layer add {}", normalized);
    finish(2, verbose)
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn relativize(root: &Path, source: &str) -> String {
    let source_path = Path::new(source);
    if let Ok(rel) = source_path.strip_prefix(root) {
        return rel.to_string_lossy().to_string();
    }
    source.to_string()
}

fn finish(code: i32, verbose: bool) -> Result<i32> {
    if verbose {
        println!();
        println!(
            "{}",
            ui::dim_text("How git decides to ignore files (checked in order):")
        );
        println!("{}", ui::dim_text("  1. .git/info/exclude     — local to this repo clone, not shared (this is what layer manages)"));
        println!(
            "{}",
            ui::dim_text("  2. .gitignore            - tracked and shared with the team")
        );
        println!(
            "{}",
            ui::dim_text(
                "  3. ~/.config/git/ignore  - global, applies to all repos on this machine"
            )
        );
        println!(
            "{}",
            ui::dim_text("A file must not be tracked for any ignore rule to take effect.")
        );
    }

    Ok(code)
}
