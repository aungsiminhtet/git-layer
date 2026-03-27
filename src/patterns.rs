#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternCategory {
    AiConfig,
}

#[derive(Debug, Clone)]
pub struct KnownPattern {
    pub entry: &'static str,
    pub label: &'static str,
    pub category: PatternCategory,
}

pub const KNOWN_SCAN_PATTERNS: &[KnownPattern] = &[
    // Claude Code
    KnownPattern {
        entry: "CLAUDE.md",
        label: "Claude Code",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".claude/",
        label: "Claude Code",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".claude.json",
        label: "Claude Code",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: "Agents.md",
        label: "Claude Code",
        category: PatternCategory::AiConfig,
    },
    // Cursor / PearAI
    KnownPattern {
        entry: ".cursorrules",
        label: "Cursor / PearAI",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".cursor/",
        label: "Cursor / PearAI",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".cursorignore",
        label: "Cursor / PearAI",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".pearai/",
        label: "Cursor / PearAI",
        category: PatternCategory::AiConfig,
    },
    // Windsurf
    KnownPattern {
        entry: ".windsurfrules",
        label: "Windsurf",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".windsurf/",
        label: "Windsurf",
        category: PatternCategory::AiConfig,
    },
    // Aider
    KnownPattern {
        entry: ".aider*",
        label: "Aider",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".aider.conf.yml",
        label: "Aider",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".aiderignore",
        label: "Aider",
        category: PatternCategory::AiConfig,
    },
    // Cline / Roo Code
    KnownPattern {
        entry: ".clinerules",
        label: "Cline / Roo Code",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".cline/",
        label: "Cline / Roo Code",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".roocodes/",
        label: "Cline / Roo Code",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".roocoderules",
        label: "Cline / Roo Code",
        category: PatternCategory::AiConfig,
    },
    // GitHub Copilot
    KnownPattern {
        entry: ".github/copilot-instructions.md",
        label: "GitHub Copilot",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".github/copilot-custom-instructions.md",
        label: "GitHub Copilot",
        category: PatternCategory::AiConfig,
    },
    // OpenAI Codex
    KnownPattern {
        entry: "AGENTS.md",
        label: "OpenAI Codex",
        category: PatternCategory::AiConfig,
    },
    // Generic AI Context
    KnownPattern {
        entry: "agents.md",
        label: "Generic AI Context",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: "AI.md",
        label: "Generic AI Context",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: "AI_CONTEXT.md",
        label: "Generic AI Context",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: "CONTEXT.md",
        label: "Generic AI Context",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: "INSTRUCTIONS.md",
        label: "Generic AI Context",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: "PROMPT.md",
        label: "Generic AI Context",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: "SYSTEM.md",
        label: "Generic AI Context",
        category: PatternCategory::AiConfig,
    },
    // Continue / Void
    KnownPattern {
        entry: ".continue/",
        label: "Continue / Void",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".void/",
        label: "Continue / Void",
        category: PatternCategory::AiConfig,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_required_patterns() {
        let entries = KNOWN_SCAN_PATTERNS
            .iter()
            .map(|p| p.entry)
            .collect::<Vec<_>>();
        assert!(entries.contains(&"CLAUDE.md"));
        assert!(entries.contains(&".cursorrules"));
        assert!(entries.contains(&".github/copilot-instructions.md"));
        assert!(entries.contains(&".aider*"));
        assert!(entries.contains(&".roocodes/"));
        assert!(entries.contains(&".continue/"));
    }

    #[test]
    fn all_patterns_are_ai_config() {
        assert!(KNOWN_SCAN_PATTERNS
            .iter()
            .all(|p| p.category == PatternCategory::AiConfig));
    }

    #[test]
    fn no_removed_patterns() {
        let labels = KNOWN_SCAN_PATTERNS
            .iter()
            .map(|p| p.label)
            .collect::<Vec<_>>();
        assert!(!labels.contains(&"Augment"));
        let entries = KNOWN_SCAN_PATTERNS
            .iter()
            .map(|p| p.entry)
            .collect::<Vec<_>>();
        assert!(!entries.contains(&"AI_INSTRUCTIONS.md"));
    }
}
