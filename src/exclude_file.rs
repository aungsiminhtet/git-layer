use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub const SECTION_START: &str = "# managed by layer";
pub const SECTION_END: &str = "# end layer";
pub const DISABLED_PREFIX: &str = "# [off] ";

/// Internal layer directory — must never appear in user-facing entry lists.
fn is_layer_internal(entry: &str) -> bool {
    entry == ".layer/" || entry == ".layer"
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub value: String,
}

/// Represents `.git/info/exclude` with section-based ownership.
///
/// The file is split into three regions:
///   - `prefix`  — lines before the layer section (user-owned, never touched)
///   - `managed` — lines between `# managed by layer` and `# end layer` (layer-owned)
///   - `suffix`  — lines after the layer section (user-owned, never touched)
#[derive(Debug, Clone)]
pub struct ExcludeFile {
    pub prefix: Vec<String>,
    pub managed: Vec<String>,
    pub suffix: Vec<String>,
}

impl ExcludeFile {
    pub fn empty() -> Self {
        Self {
            prefix: Vec::new(),
            managed: Vec::new(),
            suffix: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::empty());
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        Ok(Self::parse(&content))
    }

    fn parse(content: &str) -> Self {
        let lines: Vec<String> = content.lines().map(ToOwned::to_owned).collect();

        let start_idx = lines.iter().position(|l| l.trim() == SECTION_START);

        let Some(start) = start_idx else {
            // No section found — all lines are user-owned prefix
            return Self {
                prefix: lines,
                managed: Vec::new(),
                suffix: Vec::new(),
            };
        };

        let end_idx = lines[start + 1..]
            .iter()
            .position(|l| l.trim() == SECTION_END)
            .map(|i| i + start + 1);

        let prefix = lines[..start].to_vec();

        match end_idx {
            Some(end) => {
                let managed = lines[start + 1..end].to_vec();
                let suffix = lines[end + 1..].to_vec();
                Self {
                    prefix,
                    managed,
                    suffix,
                }
            }
            None => {
                // Migration: start marker exists but no end marker.
                // Treat everything after the start marker as managed.
                let managed = lines[start + 1..].to_vec();
                Self {
                    prefix,
                    managed,
                    suffix: Vec::new(),
                }
            }
        }
    }

    /// Returns entries within the layer-managed section only.
    /// Filters out `.layer/` — the shadow repo must never appear in user-facing lists.
    pub fn entries(&self) -> Vec<Entry> {
        self.managed
            .iter()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') || is_layer_internal(trimmed) {
                    None
                } else {
                    Some(Entry {
                        value: trimmed.to_string(),
                    })
                }
            })
            .collect()
    }

    /// Returns entries outside the layer-managed section (user-added).
    pub fn user_entries(&self) -> Vec<Entry> {
        self.prefix
            .iter()
            .chain(self.suffix.iter())
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    None
                } else {
                    Some(Entry {
                        value: trimmed.to_string(),
                    })
                }
            })
            .collect()
    }

    pub fn disabled_entries(&self) -> Vec<Entry> {
        self.managed
            .iter()
            .filter_map(|line| {
                let trimmed = line.trim();
                trimmed.strip_prefix(DISABLED_PREFIX).and_then(|value| {
                    let value = value.trim();
                    if value.is_empty() || is_layer_internal(value) {
                        None
                    } else {
                        Some(Entry {
                            value: value.to_string(),
                        })
                    }
                })
            })
            .collect()
    }

    pub fn disabled_entry_set(&self) -> HashSet<String> {
        self.disabled_entries()
            .into_iter()
            .map(|e| e.value)
            .collect()
    }

    pub fn entry_set(&self) -> HashSet<String> {
        self.entries().into_iter().map(|e| e.value).collect()
    }

    pub fn managed_entry_set(&self) -> HashSet<String> {
        let mut set = self.entry_set();
        set.extend(self.disabled_entries().into_iter().map(|e| e.value));
        set
    }

    pub fn disable_entries(&mut self, targets: &HashSet<String>) -> Vec<String> {
        let mut disabled = Vec::new();
        for line in &mut self.managed {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || is_layer_internal(trimmed) {
                continue;
            }
            if targets.contains(trimmed) {
                disabled.push(trimmed.to_string());
                *line = format!("{DISABLED_PREFIX}{trimmed}");
            }
        }
        disabled
    }

    pub fn disable_all(&mut self) -> Vec<String> {
        let mut disabled = Vec::new();
        for line in &mut self.managed {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || is_layer_internal(trimmed) {
                continue;
            }
            disabled.push(trimmed.to_string());
            *line = format!("{DISABLED_PREFIX}{trimmed}");
        }
        disabled
    }

    pub fn enable_entries(&mut self, targets: &HashSet<String>) -> Vec<String> {
        let mut enabled = Vec::new();
        for line in &mut self.managed {
            let trimmed = line.trim();
            if let Some(value) = trimmed.strip_prefix(DISABLED_PREFIX) {
                let value = value.trim();
                if targets.contains(value) {
                    enabled.push(value.to_string());
                    *line = value.to_string();
                }
            }
        }
        enabled
    }

    pub fn enable_all(&mut self) -> Vec<String> {
        let mut enabled = Vec::new();
        for line in &mut self.managed {
            let trimmed = line.trim();
            if let Some(value) = trimmed.strip_prefix(DISABLED_PREFIX) {
                let value = value.trim();
                if !value.is_empty() {
                    enabled.push(value.to_string());
                    *line = value.to_string();
                }
            }
        }
        enabled
    }

    pub fn append_entry(&mut self, entry: &str) {
        self.managed.push(entry.to_string());
    }

    pub fn remove_exact(&mut self, targets: &HashSet<String>) -> Vec<String> {
        let mut removed = Vec::new();
        let mut kept = Vec::with_capacity(self.managed.len());

        for line in &self.managed {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') && targets.contains(trimmed) {
                removed.push(trimmed.to_string());
            } else {
                kept.push(line.clone());
            }
        }

        self.managed = kept;
        removed
    }

    /// Remove matching entries from the user-owned prefix and suffix.
    pub fn remove_from_user(&mut self, targets: &HashSet<String>) -> Vec<String> {
        let mut removed = Vec::new();

        for section in [&mut self.prefix, &mut self.suffix] {
            let mut kept = Vec::with_capacity(section.len());
            for line in section.iter() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') && targets.contains(trimmed) {
                    removed.push(trimmed.to_string());
                } else {
                    kept.push(line.clone());
                }
            }
            *section = kept;
        }

        removed
    }

    /// Remove all entries from the managed section.
    pub fn clear_managed(&mut self) {
        self.managed.clear();
    }

    /// Write the file, reconstructing: prefix + section markers + managed + suffix.
    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let mut out = Vec::new();
        out.extend(self.prefix.iter().cloned());
        out.push(SECTION_START.to_string());
        out.extend(self.managed.iter().cloned());
        out.push(SECTION_END.to_string());
        out.extend(self.suffix.iter().cloned());

        let mut content = out.join("\n");
        if !content.is_empty() {
            content.push('\n');
        }

        fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
    }
}

/// Load the exclude file for read-only commands (ls, doctor, status, why, clean).
/// Creates parent dirs if missing, but does NOT write anything.
pub fn ensure_exclude_file(path: &Path) -> Result<ExcludeFile> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        return Ok(ExcludeFile::empty());
    }

    ExcludeFile::load(path)
}

/// Load the exclude file for write commands (add, rm, scan, init, clear).
/// Creates the file with section markers if missing.
pub fn ensure_exclude_file_for_write(path: &Path) -> Result<ExcludeFile> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let exclude = ExcludeFile::empty();
        exclude.write(path)?;
        return Ok(exclude);
    }

    ExcludeFile::load(path)
}

pub fn normalize_entry(input: &str) -> String {
    let mut s = input.trim().replace('\\', "/");
    while let Some(stripped) = s.strip_prefix("./") {
        s = stripped.to_string();
    }

    if s != "." && !s.is_empty() && !s.ends_with('/') {
        let p = Path::new(&s);
        if p.is_dir() {
            s.push('/');
        }
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_dot_slash() {
        assert_eq!(normalize_entry("./CLAUDE.md"), "CLAUDE.md");
    }

    #[test]
    fn normalize_strips_multiple_dot_slashes() {
        assert_eq!(normalize_entry("././CLAUDE.md"), "CLAUDE.md");
    }

    #[test]
    fn normalize_backslashes_to_forward() {
        assert_eq!(normalize_entry(".claude\\settings"), ".claude/settings");
    }

    #[test]
    fn normalize_trims_whitespace() {
        assert_eq!(normalize_entry("  CLAUDE.md  "), "CLAUDE.md");
    }

    #[test]
    fn normalize_empty_input() {
        assert_eq!(normalize_entry(""), "");
        assert_eq!(normalize_entry("  "), "");
    }

    #[test]
    fn normalize_glob_pattern_untouched() {
        assert_eq!(normalize_entry("*.prompt.md"), "*.prompt.md");
        assert_eq!(normalize_entry(".env.*"), ".env.*");
    }

    // --- Section parsing tests ---

    #[test]
    fn parse_full_section() {
        let file = ExcludeFile::parse(
            "user-stuff\n# managed by layer\nCLAUDE.md\nAgents.md\n# end layer\nmore-user-stuff",
        );
        assert_eq!(file.prefix, vec!["user-stuff"]);
        assert_eq!(file.managed, vec!["CLAUDE.md", "Agents.md"]);
        assert_eq!(file.suffix, vec!["more-user-stuff"]);
    }

    #[test]
    fn parse_no_section_all_prefix() {
        let file = ExcludeFile::parse("# some comment\nfoo.txt\nbar.txt");
        assert_eq!(file.prefix, vec!["# some comment", "foo.txt", "bar.txt"]);
        assert!(file.managed.is_empty());
        assert!(file.suffix.is_empty());
    }

    #[test]
    fn parse_migration_no_end_marker() {
        let file = ExcludeFile::parse("# managed by layer\nCLAUDE.md\n.claude/");
        assert!(file.prefix.is_empty());
        assert_eq!(file.managed, vec!["CLAUDE.md", ".claude/"]);
        assert!(file.suffix.is_empty());
    }

    #[test]
    fn parse_empty_section() {
        let file = ExcludeFile::parse("# managed by layer\n# end layer");
        assert!(file.prefix.is_empty());
        assert!(file.managed.is_empty());
        assert!(file.suffix.is_empty());
    }

    #[test]
    fn parse_prefix_only_before_section() {
        let file = ExcludeFile::parse(
            "# git default comment\n# another comment\n# managed by layer\nCLAUDE.md\n# end layer",
        );
        assert_eq!(
            file.prefix,
            vec!["# git default comment", "# another comment"]
        );
        assert_eq!(file.managed, vec!["CLAUDE.md"]);
        assert!(file.suffix.is_empty());
    }

    // --- entries / user_entries ---

    #[test]
    fn entries_returns_only_managed() {
        let file = ExcludeFile {
            prefix: vec!["user-file.txt".into()],
            managed: vec![
                "CLAUDE.md".into(),
                "".into(),
                "# comment".into(),
                "Agents.md".into(),
            ],
            suffix: vec!["other.txt".into()],
        };
        let entries = file.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].value, "CLAUDE.md");
        assert_eq!(entries[1].value, "Agents.md");
    }

    #[test]
    fn user_entries_returns_prefix_and_suffix() {
        let file = ExcludeFile {
            prefix: vec!["# comment".into(), "user-file.txt".into()],
            managed: vec!["CLAUDE.md".into()],
            suffix: vec!["other.txt".into()],
        };
        let user = file.user_entries();
        assert_eq!(user.len(), 2);
        assert_eq!(user[0].value, "user-file.txt");
        assert_eq!(user[1].value, "other.txt");
    }

    #[test]
    fn dedupe_via_entry_set() {
        let file = ExcludeFile {
            prefix: Vec::new(),
            managed: vec!["CLAUDE.md".into(), "CLAUDE.md".into(), "Agents.md".into()],
            suffix: Vec::new(),
        };
        let set = file.entry_set();
        assert_eq!(set.len(), 2);
        assert!(set.contains("CLAUDE.md"));
        assert!(set.contains("Agents.md"));
    }

    #[test]
    fn managed_entry_set_includes_disabled_entries() {
        let file = ExcludeFile {
            prefix: Vec::new(),
            managed: vec!["CLAUDE.md".into(), "# [off] Agents.md".into()],
            suffix: Vec::new(),
        };
        let set = file.managed_entry_set();
        assert_eq!(set.len(), 2);
        assert!(set.contains("CLAUDE.md"));
        assert!(set.contains("Agents.md"));
    }

    #[test]
    fn remove_exact_only_from_managed() {
        let mut file = ExcludeFile {
            prefix: vec!["CLAUDE.md".into()],
            managed: vec!["CLAUDE.md".into(), "# keep".into(), "*.tmp".into()],
            suffix: Vec::new(),
        };
        let removed = file.remove_exact(&HashSet::from(["CLAUDE.md".to_string()]));
        assert_eq!(removed, vec!["CLAUDE.md"]);
        // managed section updated
        assert_eq!(file.managed, vec!["# keep", "*.tmp"]);
        // prefix untouched
        assert_eq!(file.prefix, vec!["CLAUDE.md"]);
    }

    #[test]
    fn remove_from_user_only_touches_prefix_suffix() {
        let mut file = ExcludeFile {
            prefix: vec![
                "gone.txt".into(),
                "# comment".into(),
                "keep-prefix.txt".into(),
            ],
            managed: vec!["gone.txt".into()],
            suffix: vec!["gone.txt".into(), "keep-suffix.txt".into()],
        };
        let removed = file.remove_from_user(&HashSet::from(["gone.txt".to_string()]));
        assert_eq!(removed, vec!["gone.txt", "gone.txt"]);
        // managed section untouched
        assert_eq!(file.managed, vec!["gone.txt"]);
        // prefix and suffix cleaned
        assert_eq!(file.prefix, vec!["# comment", "keep-prefix.txt"]);
        assert_eq!(file.suffix, vec!["keep-suffix.txt"]);
    }

    #[test]
    fn disabled_entries_returns_off_prefixed_lines() {
        let file = ExcludeFile {
            prefix: Vec::new(),
            managed: vec![
                "CLAUDE.md".into(),
                "# [off] Agents.md".into(),
                "# [off] .claude/".into(),
                "# regular comment".into(),
            ],
            suffix: Vec::new(),
        };
        let disabled = file.disabled_entries();
        assert_eq!(disabled.len(), 2);
        assert_eq!(disabled[0].value, "Agents.md");
        assert_eq!(disabled[1].value, ".claude/");
    }

    #[test]
    fn entries_does_not_include_disabled() {
        let file = ExcludeFile {
            prefix: Vec::new(),
            managed: vec!["CLAUDE.md".into(), "# [off] Agents.md".into()],
            suffix: Vec::new(),
        };
        let active = file.entries();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].value, "CLAUDE.md");
    }

    #[test]
    fn clear_managed_preserves_prefix_suffix() {
        let mut file = ExcludeFile {
            prefix: vec!["user-stuff".into()],
            managed: vec!["CLAUDE.md".into(), "Agents.md".into()],
            suffix: vec!["more-stuff".into()],
        };
        file.clear_managed();
        assert!(file.managed.is_empty());
        assert_eq!(file.prefix, vec!["user-stuff"]);
        assert_eq!(file.suffix, vec!["more-stuff"]);
    }

    #[test]
    fn disable_all_disables_everything() {
        let mut file = ExcludeFile {
            prefix: Vec::new(),
            managed: vec!["CLAUDE.md".into(), "Agents.md".into()],
            suffix: Vec::new(),
        };
        let disabled = file.disable_all();
        assert_eq!(disabled.len(), 2);
        assert_eq!(file.managed, vec!["# [off] CLAUDE.md", "# [off] Agents.md"]);
    }

    #[test]
    fn disable_skips_already_disabled() {
        let mut file = ExcludeFile {
            prefix: Vec::new(),
            managed: vec!["CLAUDE.md".into(), "# [off] Agents.md".into()],
            suffix: Vec::new(),
        };
        let disabled = file.disable_all();
        assert_eq!(disabled, vec!["CLAUDE.md"]);
        assert_eq!(file.managed[1], "# [off] Agents.md");
    }

    #[test]
    fn enable_all_enables_everything() {
        let mut file = ExcludeFile {
            prefix: Vec::new(),
            managed: vec![
                "# [off] CLAUDE.md".into(),
                "# [off] Agents.md".into(),
                "active.md".into(),
            ],
            suffix: Vec::new(),
        };
        let enabled = file.enable_all();
        assert_eq!(enabled.len(), 2);
        assert_eq!(file.managed, vec!["CLAUDE.md", "Agents.md", "active.md"]);
    }

    #[test]
    fn disable_specific_entries() {
        let mut file = ExcludeFile {
            prefix: Vec::new(),
            managed: vec!["CLAUDE.md".into(), "Agents.md".into(), ".claude/".into()],
            suffix: Vec::new(),
        };
        let disabled = file.disable_entries(&HashSet::from(["CLAUDE.md".to_string()]));
        assert_eq!(disabled, vec!["CLAUDE.md"]);
        assert_eq!(file.managed[0], "# [off] CLAUDE.md");
        assert_eq!(file.managed[1], "Agents.md");
    }

    #[test]
    fn enable_specific_entries() {
        let mut file = ExcludeFile {
            prefix: Vec::new(),
            managed: vec!["# [off] CLAUDE.md".into(), "# [off] Agents.md".into()],
            suffix: Vec::new(),
        };
        let enabled = file.enable_entries(&HashSet::from(["CLAUDE.md".to_string()]));
        assert_eq!(enabled, vec!["CLAUDE.md"]);
        assert_eq!(file.managed[0], "CLAUDE.md");
        assert_eq!(file.managed[1], "# [off] Agents.md");
    }

    #[test]
    fn roundtrip_disable_then_enable() {
        let mut file = ExcludeFile {
            prefix: Vec::new(),
            managed: vec!["CLAUDE.md".into(), "Agents.md".into()],
            suffix: Vec::new(),
        };
        let original = file.managed.clone();
        file.disable_all();
        assert_eq!(file.entries().len(), 0);
        assert_eq!(file.disabled_entries().len(), 2);
        file.enable_all();
        assert_eq!(file.managed, original);
    }
}
