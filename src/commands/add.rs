use crate::commands::scan;
use crate::exclude_file::{ensure_exclude_file_for_write, normalize_entry, ExcludeFile};
use crate::git;
use crate::git::RepoContext;
use crate::patterns::PatternCategory;
use crate::tree_picker;
use crate::ui;
use anyhow::{anyhow, Result};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Default)]
pub struct AddSummary {
    pub added: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone)]
struct InteractiveCandidate {
    path: String,
    category: &'static str,
}

pub fn run(files: Vec<String>, interactive: bool, dry_run: bool) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let mut exclude = ensure_exclude_file_for_write(&ctx.exclude_path)?;

    if interactive || (files.is_empty() && ui::is_stdout_tty()) {
        return run_interactive(&ctx, &mut exclude, dry_run);
    }

    if files.is_empty() {
        return Err(anyhow!("no files provided. Use 'layer add <files...>' or run in a terminal for interactive mode"));
    }

    let summary = apply_add_entries(&ctx, &mut exclude, &files, dry_run)?;
    if dry_run {
        ui::print_dry_run_notice();
    }
    if summary.added == 0 {
        return Ok(2);
    }

    Ok(0)
}

pub fn apply_add_entries(
    ctx: &RepoContext,
    exclude: &mut ExcludeFile,
    entries: &[String],
    dry_run: bool,
) -> Result<AddSummary> {
    let mut summary = AddSummary::default();
    let mut known_entries = exclude.managed_entry_set();

    for raw in entries {
        let normalized = normalize_entry(raw);
        if normalized.is_empty() {
            summary.skipped += 1;
            continue;
        }

        if known_entries.contains(&normalized) {
            println!(
                "  {} '{normalized}' is already managed by layer",
                ui::info()
            );
            summary.skipped += 1;
            continue;
        }

        if git::is_tracked(&ctx.root, &normalized)? {
            ui::print_warning(&format!(
                "'{normalized}' is tracked by Git — layer cannot hide it until you untrack it"
            ));
            println!(
                "  {}",
                ui::warn_text(&format!("git rm --cached {normalized}"))
            );
        }

        if dry_run {
            println!(
                "  {} Would add '{normalized}' to your local layer",
                ui::discovered()
            );
        } else {
            exclude.append_entry(&normalized);
            println!("  {} Added '{normalized}' to your local layer", ui::ok());
        }
        known_entries.insert(normalized);
        summary.added += 1;
    }

    if summary.added > 0 && !dry_run {
        exclude.write(&ctx.exclude_path)?;
    }

    Ok(summary)
}

fn run_interactive(ctx: &RepoContext, exclude: &mut ExcludeFile, dry_run: bool) -> Result<i32> {
    ui::require_tty("interactive mode requires a TTY. Use 'layer add <files...>' instead")?;

    let candidates = collect_candidates(ctx, exclude)?;
    if candidates.is_empty() {
        println!("No context files found.");
        return Ok(2);
    }

    let nodes = build_tree(candidates);

    println!("{}", ui::heading("Select files to add to your local layer"));
    ui::print_tree_picker_hint();

    let chosen = match tree_picker::run(&nodes)? {
        Some(paths) if !paths.is_empty() => paths,
        _ => {
            println!("No files selected.");
            return Ok(2);
        }
    };

    let summary = apply_add_entries(ctx, exclude, &chosen, dry_run)?;
    if dry_run {
        ui::print_dry_run_notice();
    }
    if summary.added == 0 {
        return Ok(2);
    }

    Ok(0)
}

/// Groups flat candidates into a recursive tree structure for the tree picker.
/// At each level: root-level files come first, then BTreeMap-sorted directory
/// groups. Directories with only 1 file are promoted to the parent level.
fn build_tree(candidates: Vec<InteractiveCandidate>) -> Vec<tree_picker::TreeNode> {
    build_subtree(candidates, "")
}

fn build_subtree(
    candidates: Vec<InteractiveCandidate>,
    prefix: &str,
) -> Vec<tree_picker::TreeNode> {
    let mut root_files: Vec<tree_picker::TreeNode> = Vec::new();
    let mut dir_groups: BTreeMap<String, Vec<InteractiveCandidate>> = BTreeMap::new();

    for c in candidates {
        let relative = &c.path[prefix.len()..];
        if let Some(slash_pos) = relative.find('/') {
            let full_dir = format!("{}{}", prefix, &relative[..=slash_pos]);
            dir_groups.entry(full_dir).or_default().push(c);
        } else {
            root_files.push(tree_picker::TreeNode {
                path: c.path,
                category: c.category.to_string(),
                children: Vec::new(),
            });
        }
    }

    let mut result = root_files;

    for (dir, files) in dir_groups {
        let children = build_subtree(files, &dir);
        if children.len() == 1 && children[0].children.is_empty() {
            // Single-file directory — promote the file to this level.
            result.push(children.into_iter().next().unwrap());
        } else {
            let file_count = count_leaf_files(&children);
            result.push(tree_picker::TreeNode {
                path: dir,
                category: format!("{} files", file_count),
                children,
            });
        }
    }

    result
}

fn count_leaf_files(nodes: &[tree_picker::TreeNode]) -> usize {
    nodes
        .iter()
        .map(|n| {
            if n.children.is_empty() {
                1
            } else {
                count_leaf_files(&n.children)
            }
        })
        .sum()
}

fn collect_candidates(
    ctx: &RepoContext,
    exclude: &ExcludeFile,
) -> Result<Vec<InteractiveCandidate>> {
    let excluded = exclude.managed_entry_set();
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for found in scan::discover_known_files(ctx, &excluded)? {
        if found.already_excluded || found.is_gitignored || found.is_tracked {
            continue;
        }
        if seen.insert(found.path.clone()) {
            let category = match found.category {
                PatternCategory::AiConfig => "context file",
            };
            out.push(InteractiveCandidate {
                path: found.path,
                category,
            });
        }
    }

    for file in git::list_untracked(&ctx.root)? {
        let normalized = normalize_entry(&file);
        if normalized.is_empty() || excluded.contains(&normalized) {
            continue;
        }
        if seen.insert(normalized.clone()) {
            out.push(InteractiveCandidate {
                path: normalized,
                category: "untracked",
            });
        }
    }

    Ok(out)
}
