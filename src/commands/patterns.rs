use crate::commands::scan;
use crate::exclude_file::ensure_exclude_file;
use crate::git;
use crate::patterns::KNOWN_SCAN_PATTERNS;
use crate::ui;
use anyhow::{bail, Result};
use std::collections::HashMap;

fn detection_kind(entry: &str) -> &'static str {
    if entry.ends_with('/') {
        "dir"
    } else if entry.contains('*') || entry.contains('?') {
        "glob"
    } else {
        "file"
    }
}

pub fn run(json: bool, matched: bool, show_files: bool) -> Result<i32> {
    if show_files && !matched {
        bail!("--show-files requires --matched");
    }

    if matched {
        run_matched(json, show_files)
    } else if json {
        run_json_static()
    } else {
        run_static()
    }
}

/// Default static listing grouped by tool label with kind annotations.
fn run_static() -> Result<i32> {
    let mut current_label = "";

    for pat in KNOWN_SCAN_PATTERNS {
        if pat.label != current_label {
            if !current_label.is_empty() {
                println!();
            }
            println!("{}", ui::heading(pat.label));
            current_label = pat.label;
        }
        println!(
            "  {}  {}",
            pat.entry,
            ui::dim_text(&format!("({})", detection_kind(pat.entry)))
        );
    }

    Ok(0)
}

/// JSON output for static pattern list.
fn run_json_static() -> Result<i32> {
    let groups = build_groups();

    let mut json = String::from("[\n");
    for (gi, (label, patterns)) in groups.iter().enumerate() {
        json.push_str("  {\n");
        json.push_str(&format!("    \"tool\": {},\n", json_escape(label)));
        json.push_str("    \"patterns\": [\n");
        for (pi, entry) in patterns.iter().enumerate() {
            json.push_str(&format!(
                "      {{ \"entry\": {}, \"kind\": {} }}",
                json_escape(entry),
                json_escape(detection_kind(entry))
            ));
            if pi + 1 < patterns.len() {
                json.push(',');
            }
            json.push('\n');
        }
        json.push_str("    ]\n");
        json.push_str("  }");
        if gi + 1 < groups.len() {
            json.push(',');
        }
        json.push('\n');
    }
    json.push(']');

    println!("{json}");
    Ok(0)
}

/// --matched mode: show patterns that have actual files in the current repo.
fn run_matched(json: bool, show_files: bool) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let exclude = ensure_exclude_file(&ctx.exclude_path)?;
    let excluded = exclude.managed_entry_set();

    let discoveries = scan::discover_known_files(&ctx, &excluded)?;

    // Build a map from pattern label to list of matched entries.
    // Each matched entry has the pattern entry string and the list of discovered file paths.
    let mut match_map: HashMap<&str, Vec<MatchedPattern>> = HashMap::new();

    for pat in KNOWN_SCAN_PATTERNS {
        let files: Vec<String> = discoveries
            .iter()
            .filter(|d| d.label == pat.label && pattern_covers_discovery(pat.entry, &d.path))
            .map(|d| d.path.clone())
            .collect();

        if !files.is_empty() {
            match_map
                .entry(pat.label)
                .or_default()
                .push(MatchedPattern {
                    entry: pat.entry,
                    files,
                });
        }
    }

    if json {
        return print_matched_json(&match_map, show_files);
    }

    if match_map.is_empty() {
        println!("No known patterns match files in this repository.");
        return Ok(2);
    }

    let mut has_section = false;
    let mut current_label = "";

    for pat in KNOWN_SCAN_PATTERNS {
        let Some(matched_list) = match_map.get(pat.label) else {
            continue;
        };
        let Some(mp) = matched_list.iter().find(|m| m.entry == pat.entry) else {
            continue;
        };

        if pat.label != current_label {
            if has_section {
                println!();
            }
            println!("{}", ui::heading(pat.label));
            current_label = pat.label;
            has_section = true;
        }

        let count = mp.files.len();
        println!(
            "  {}  {} {}",
            pat.entry,
            ui::dim_text(&format!("({})", detection_kind(pat.entry))),
            ui::dim_text(&format!(
                "[{count} match{}]",
                if count == 1 { "" } else { "es" }
            ))
        );

        if show_files {
            for file in &mp.files {
                println!("    {}", ui::dim_text(file));
            }
        }
    }

    Ok(0)
}

struct MatchedPattern {
    entry: &'static str,
    files: Vec<String>,
}

/// Check if a known pattern entry covers a discovered path.
fn pattern_covers_discovery(pattern_entry: &str, discovered_path: &str) -> bool {
    // Directory patterns: the discovered path starts with the pattern prefix
    if pattern_entry.ends_with('/') {
        return discovered_path == pattern_entry || discovered_path.starts_with(pattern_entry);
    }

    // Glob patterns: simple prefix match (e.g. .aider* matches .aider.conf.yml)
    if let Some(prefix) = pattern_entry.strip_suffix('*') {
        return discovered_path.starts_with(prefix);
    }

    // Exact match
    discovered_path == pattern_entry
}

/// JSON output for --matched (and optionally --show-files).
fn print_matched_json(
    match_map: &HashMap<&str, Vec<MatchedPattern>>,
    show_files: bool,
) -> Result<i32> {
    let groups = build_groups();

    // Filter to only groups that have matches
    let matched_groups: Vec<_> = groups
        .iter()
        .filter(|(label, _)| match_map.contains_key(label.as_str()))
        .collect();

    let mut json = String::from("[\n");
    for (gi, (label, patterns)) in matched_groups.iter().enumerate() {
        let matched_list = &match_map[label.as_str()];
        json.push_str("  {\n");
        json.push_str(&format!("    \"tool\": {},\n", json_escape(label)));
        json.push_str("    \"patterns\": [\n");

        let mut pi_count = 0;
        let total_matched = patterns
            .iter()
            .filter(|e| matched_list.iter().any(|m| m.entry == **e))
            .count();

        for entry in patterns {
            let mp = matched_list.iter().find(|m| m.entry == *entry);
            let is_matched = mp.is_some();

            if !is_matched {
                continue;
            }

            json.push_str(&format!(
                "      {{ \"entry\": {}, \"kind\": {}, \"matched\": true",
                json_escape(entry),
                json_escape(detection_kind(entry))
            ));

            if show_files {
                if let Some(mp) = mp {
                    json.push_str(", \"files\": [");
                    for (fi, file) in mp.files.iter().enumerate() {
                        json.push_str(&json_escape(file));
                        if fi + 1 < mp.files.len() {
                            json.push_str(", ");
                        }
                    }
                    json.push(']');
                }
            }

            json.push_str(" }");
            pi_count += 1;
            if pi_count < total_matched {
                json.push(',');
            }
            json.push('\n');
        }

        json.push_str("    ]\n");
        json.push_str("  }");
        if gi + 1 < matched_groups.len() {
            json.push(',');
        }
        json.push('\n');
    }
    json.push(']');

    println!("{json}");
    Ok(0)
}

/// Build ordered groups: [(label, [entries...])]
fn build_groups() -> Vec<(String, Vec<&'static str>)> {
    let mut groups: Vec<(String, Vec<&'static str>)> = Vec::new();
    for pat in KNOWN_SCAN_PATTERNS {
        if let Some(last) = groups.last_mut() {
            if last.0 == pat.label {
                last.1.push(pat.entry);
                continue;
            }
        }
        groups.push((pat.label.to_string(), vec![pat.entry]));
    }
    groups
}

/// Minimal JSON string escaping.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_kind_file() {
        assert_eq!(detection_kind("CLAUDE.md"), "file");
        assert_eq!(detection_kind(".cursorrules"), "file");
    }

    #[test]
    fn detection_kind_dir() {
        assert_eq!(detection_kind(".claude/"), "dir");
        assert_eq!(detection_kind(".cursor/"), "dir");
    }

    #[test]
    fn detection_kind_glob() {
        assert_eq!(detection_kind(".aider*"), "glob");
        assert_eq!(detection_kind("file?.txt"), "glob");
    }

    #[test]
    fn json_escape_basic() {
        assert_eq!(json_escape("hello"), "\"hello\"");
        assert_eq!(json_escape("a\"b"), "\"a\\\"b\"");
        assert_eq!(json_escape("a\\b"), "\"a\\\\b\"");
    }

    #[test]
    fn pattern_covers_discovery_exact() {
        assert!(pattern_covers_discovery("CLAUDE.md", "CLAUDE.md"));
        assert!(!pattern_covers_discovery("CLAUDE.md", "claude.md"));
    }

    #[test]
    fn pattern_covers_discovery_dir() {
        assert!(pattern_covers_discovery(".claude/", ".claude/"));
        assert!(pattern_covers_discovery(
            ".claude/",
            ".claude/settings.json"
        ));
        assert!(!pattern_covers_discovery(".claude/", ".cursorrules"));
    }

    #[test]
    fn pattern_covers_discovery_glob() {
        assert!(pattern_covers_discovery(".aider*", ".aider.conf.yml"));
        assert!(pattern_covers_discovery(".aider*", ".aiderignore"));
        assert!(!pattern_covers_discovery(".aider*", ".cursor"));
    }

    #[test]
    fn build_groups_preserves_order() {
        let groups = build_groups();
        assert!(!groups.is_empty());
        assert_eq!(groups[0].0, "Claude Code");
        assert!(groups[0].1.contains(&"CLAUDE.md"));
    }

    #[test]
    fn static_run_succeeds() {
        // Just verify it doesn't panic
        let result = run(false, false, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn json_static_run_succeeds() {
        let result = run(true, false, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn show_files_without_matched_errors() {
        let result = run(false, false, true);
        assert!(result.is_err());
    }
}
