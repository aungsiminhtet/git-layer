use console::{style, Key, Term};
use std::collections::HashSet;
use std::io::{self, Write};

// ── Public types ──────────────────────────────────────────────

pub struct TreeNode {
    pub path: String,
    pub category: String,
    pub children: Vec<TreeNode>,
}

// ── Internal types ────────────────────────────────────────────

enum FlatItem {
    /// A leaf file (TreeNode with no children).
    File {
        path: String,
        category: String,
        depth: usize,
        parent_dir: Option<String>,
    },
    /// A directory header (TreeNode with children).
    Dir {
        dir_path: String,
        category: String,
        depth: usize,
        expanded: bool,
        parent_dir: Option<String>,
    },
}

impl FlatItem {
    fn path(&self) -> &str {
        match self {
            FlatItem::File { path, .. } => path,
            FlatItem::Dir { dir_path, .. } => dir_path,
        }
    }

    fn depth(&self) -> usize {
        match self {
            FlatItem::File { depth, .. } => *depth,
            FlatItem::Dir { depth, .. } => *depth,
        }
    }
}

/// Ensures the cursor is shown when the picker exits, even on panic.
struct CursorGuard {
    term: Term,
}

impl Drop for CursorGuard {
    fn drop(&mut self) {
        let _ = self.term.show_cursor();
    }
}

// ── Public API ────────────────────────────────────────────────

/// Run the interactive tree picker. Returns `Some(selected_paths)` on confirm,
/// `None` on cancel (Esc).
pub fn run(nodes: &[TreeNode]) -> io::Result<Option<Vec<String>>> {
    let mut term = Term::stderr();
    let _guard = CursorGuard { term: term.clone() };
    let _ = term.hide_cursor();

    let mut expanded: HashSet<String> = HashSet::new();
    let mut selected: HashSet<String> = HashSet::new();
    let mut cursor: usize = 0;
    let mut scroll: usize = 0;
    let mut drawn: usize = 0;

    let max_display_width = compute_max_display_width(nodes, 0);

    loop {
        let items = flatten(nodes, &expanded);
        if items.is_empty() {
            return Ok(Some(Vec::new()));
        }

        if cursor >= items.len() {
            cursor = items.len().saturating_sub(1);
        }

        let term_height = term.size().0 as usize;
        let viewport = items.len().min(term_height.saturating_sub(2).max(3));

        if cursor < scroll {
            scroll = cursor;
        }
        if cursor >= scroll + viewport {
            scroll = cursor + 1 - viewport;
        }
        if scroll + viewport > items.len() {
            scroll = items.len().saturating_sub(viewport);
        }

        clear_last_lines(&term, drawn);

        drawn = 0;
        for (i, item) in items.iter().enumerate().skip(scroll).take(viewport) {
            let is_active = i == cursor;
            let is_selected = selected.contains(item.path());
            let line = format_row(item, is_active, is_selected, max_display_width);
            let _ = writeln!(term, "{line}");
            drawn += 1;
        }

        let key = term.read_key()?;
        match key {
            Key::ArrowUp => {
                cursor = cursor.saturating_sub(1);
            }
            Key::ArrowDown => {
                if cursor + 1 < items.len() {
                    cursor += 1;
                }
            }
            Key::Char(' ') => {
                let path = items[cursor].path().to_string();
                if selected.contains(&path) {
                    selected.remove(&path);
                } else {
                    selected.insert(path);
                }
            }
            Key::ArrowRight => {
                if let FlatItem::Dir {
                    dir_path,
                    expanded: false,
                    ..
                } = &items[cursor]
                {
                    expanded.insert(dir_path.clone());
                }
            }
            Key::ArrowLeft => match &items[cursor] {
                FlatItem::Dir {
                    dir_path,
                    expanded: true,
                    ..
                } => {
                    expanded.remove(dir_path.as_str());
                }
                FlatItem::Dir {
                    parent_dir: Some(parent),
                    expanded: false,
                    ..
                } => {
                    let parent = parent.clone();
                    expanded.remove(parent.as_str());
                    if let Some(idx) = find_dir_index(&items, &parent) {
                        cursor = idx;
                    }
                }
                FlatItem::File {
                    parent_dir: Some(parent),
                    ..
                } => {
                    let parent = parent.clone();
                    expanded.remove(parent.as_str());
                    if let Some(idx) = find_dir_index(&items, &parent) {
                        cursor = idx;
                    }
                }
                _ => {}
            },
            Key::Enter => {
                clear_last_lines(&term, drawn);
                let result = collect_selected(nodes, &selected);
                return Ok(Some(result));
            }
            Key::Escape => {
                clear_last_lines(&term, drawn);
                return Ok(None);
            }
            _ => {}
        }
    }
}

// ── Internals ─────────────────────────────────────────────────

fn flatten(nodes: &[TreeNode], expanded: &HashSet<String>) -> Vec<FlatItem> {
    let mut items = Vec::new();
    flatten_recursive(nodes, expanded, 0, None, &mut items);
    items
}

fn flatten_recursive(
    nodes: &[TreeNode],
    expanded: &HashSet<String>,
    depth: usize,
    parent_dir: Option<&str>,
    items: &mut Vec<FlatItem>,
) {
    for node in nodes {
        if node.children.is_empty() {
            items.push(FlatItem::File {
                path: node.path.clone(),
                category: node.category.clone(),
                depth,
                parent_dir: parent_dir.map(String::from),
            });
        } else {
            let is_expanded = expanded.contains(&node.path);
            items.push(FlatItem::Dir {
                dir_path: node.path.clone(),
                category: node.category.clone(),
                depth,
                expanded: is_expanded,
                parent_dir: parent_dir.map(String::from),
            });
            if is_expanded {
                flatten_recursive(&node.children, expanded, depth + 1, Some(&node.path), items);
            }
        }
    }
}

/// Compute max display width across all items at all depths (expanded or not)
/// so the category column stays stable across expand/collapse.
fn compute_max_display_width(nodes: &[TreeNode], depth: usize) -> usize {
    let mut max = 0;
    for node in nodes {
        let width = 2 * (depth + 1) + 2 + node.path.len();
        max = max.max(width);
        if !node.children.is_empty() {
            max = max.max(compute_max_display_width(&node.children, depth + 1));
        }
    }
    max
}

fn format_row(
    item: &FlatItem,
    is_active: bool,
    is_selected: bool,
    max_display_width: usize,
) -> String {
    let check = if is_selected {
        style("✓").cyan().to_string()
    } else {
        style("○").dim().to_string()
    };

    let depth = item.depth();

    let (prefix, display_path, category) = match item {
        FlatItem::File { path, category, .. } => {
            let indent = "  ".repeat(depth + 1);
            (indent, path.clone(), category.clone())
        }
        FlatItem::Dir {
            dir_path,
            category,
            expanded,
            ..
        } => {
            let indent = "  ".repeat(depth);
            let arrow = if *expanded { "▾ " } else { "▸ " };
            (
                format!("{indent}{arrow}"),
                dir_path.clone(),
                category.clone(),
            )
        }
    };

    let current_width = 2 * (depth + 1) + 2 + display_path.len();
    let padding = max_display_width.saturating_sub(current_width);

    let cat_text = style(format!("({})", category)).dim().to_string();

    let path_styled = if is_active {
        style(&display_path).cyan().bold().to_string()
    } else {
        display_path.clone()
    };

    format!(
        "{prefix}{check} {path_styled}{} {cat_text}",
        " ".repeat(padding)
    )
}

fn clear_last_lines(term: &Term, count: usize) {
    for _ in 0..count {
        let _ = term.clear_line();
        let _ = term.move_cursor_up(1);
    }
    let _ = term.clear_line();
}

fn find_dir_index(items: &[FlatItem], dir_path: &str) -> Option<usize> {
    items
        .iter()
        .position(|it| matches!(it, FlatItem::Dir { dir_path: p, .. } if p == dir_path))
}

/// Collect selected paths with dedup: if a directory is selected, skip all descendants.
fn collect_selected(nodes: &[TreeNode], selected: &HashSet<String>) -> Vec<String> {
    let mut result = Vec::new();
    collect_selected_recursive(nodes, selected, &mut result);
    result
}

fn collect_selected_recursive(
    nodes: &[TreeNode],
    selected: &HashSet<String>,
    result: &mut Vec<String>,
) {
    for node in nodes {
        if node.children.is_empty() {
            if selected.contains(&node.path) {
                result.push(node.path.clone());
            }
        } else if selected.contains(&node.path) {
            result.push(node.path.clone());
        } else {
            collect_selected_recursive(&node.children, selected, result);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_leaf(path: &str, category: &str) -> TreeNode {
        TreeNode {
            path: path.to_string(),
            category: category.to_string(),
            children: Vec::new(),
        }
    }

    fn make_dir(path: &str, category: &str, children: Vec<TreeNode>) -> TreeNode {
        TreeNode {
            path: path.to_string(),
            category: category.to_string(),
            children,
        }
    }

    #[test]
    fn visible_rows_collapsed() {
        let nodes = vec![
            make_leaf("CLAUDE.md", "context file"),
            make_dir(
                "docs/",
                "3 files",
                vec![
                    make_leaf("docs/a.md", "untracked"),
                    make_leaf("docs/b.md", "untracked"),
                    make_leaf("docs/c.md", "untracked"),
                ],
            ),
        ];
        let expanded = HashSet::new();
        let items = flatten(&nodes, &expanded);
        // Should only show CLAUDE.md + docs/ header = 2 items.
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn visible_rows_expanded() {
        let nodes = vec![
            make_leaf("CLAUDE.md", "context file"),
            make_dir(
                "docs/",
                "3 files",
                vec![
                    make_leaf("docs/a.md", "untracked"),
                    make_leaf("docs/b.md", "untracked"),
                    make_leaf("docs/c.md", "untracked"),
                ],
            ),
        ];
        let mut expanded = HashSet::new();
        expanded.insert("docs/".to_string());
        let items = flatten(&nodes, &expanded);
        // CLAUDE.md + docs/ header + 3 children = 5.
        assert_eq!(items.len(), 5);
    }

    #[test]
    fn nested_expand_shows_subdirs_only() {
        let nodes = vec![make_dir(
            "agent-docs/",
            "4 files",
            vec![
                make_leaf("agent-docs/README.md", "untracked"),
                make_dir(
                    "agent-docs/fixes/",
                    "2 files",
                    vec![
                        make_leaf("agent-docs/fixes/fix1.md", "untracked"),
                        make_leaf("agent-docs/fixes/fix2.md", "untracked"),
                    ],
                ),
            ],
        )];
        // Collapsed: just the top dir.
        let expanded = HashSet::new();
        let items = flatten(&nodes, &expanded);
        assert_eq!(items.len(), 1);

        // Expand top level: see README + fixes/ header, but not fix contents.
        let mut expanded = HashSet::new();
        expanded.insert("agent-docs/".to_string());
        let items = flatten(&nodes, &expanded);
        assert_eq!(items.len(), 3); // agent-docs/ + README + fixes/

        // Expand both levels: also see fix contents.
        expanded.insert("agent-docs/fixes/".to_string());
        let items = flatten(&nodes, &expanded);
        assert_eq!(items.len(), 5); // + fix1 + fix2
    }

    #[test]
    fn collect_selected_dedup() {
        let nodes = vec![
            make_leaf("CLAUDE.md", "context file"),
            make_dir(
                "docs/",
                "2 files",
                vec![
                    make_leaf("docs/a.md", "untracked"),
                    make_leaf("docs/b.md", "untracked"),
                ],
            ),
        ];
        let mut selected = HashSet::new();
        selected.insert("docs/".to_string());
        selected.insert("docs/a.md".to_string()); // Should be deduped.
        selected.insert("docs/b.md".to_string()); // Should be deduped.

        let result = collect_selected(&nodes, &selected);
        assert_eq!(result, vec!["docs/".to_string()]);
    }

    #[test]
    fn collect_selected_children_only() {
        let nodes = vec![make_dir(
            "docs/",
            "2 files",
            vec![
                make_leaf("docs/a.md", "untracked"),
                make_leaf("docs/b.md", "untracked"),
            ],
        )];
        let mut selected = HashSet::new();
        selected.insert("docs/a.md".to_string());

        let result = collect_selected(&nodes, &selected);
        assert_eq!(result, vec!["docs/a.md".to_string()]);
    }

    #[test]
    fn collect_selected_nested_dedup() {
        let nodes = vec![make_dir(
            "agent-docs/",
            "4 files",
            vec![
                make_leaf("agent-docs/README.md", "untracked"),
                make_dir(
                    "agent-docs/fixes/",
                    "2 files",
                    vec![
                        make_leaf("agent-docs/fixes/fix1.md", "untracked"),
                        make_leaf("agent-docs/fixes/fix2.md", "untracked"),
                    ],
                ),
            ],
        )];
        // Select top-level dir — should include everything, dedup children.
        let mut selected = HashSet::new();
        selected.insert("agent-docs/".to_string());
        selected.insert("agent-docs/fixes/fix1.md".to_string());
        let result = collect_selected(&nodes, &selected);
        assert_eq!(result, vec!["agent-docs/".to_string()]);

        // Select sub-dir only — should include just the sub-dir.
        let mut selected = HashSet::new();
        selected.insert("agent-docs/fixes/".to_string());
        let result = collect_selected(&nodes, &selected);
        assert_eq!(result, vec!["agent-docs/fixes/".to_string()]);
    }
}
