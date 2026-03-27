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
    // OpenAI Codex
    KnownPattern {
        entry: "AGENTS.md",
        label: "OpenAI Codex",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".codex/",
        label: "OpenAI Codex",
        category: PatternCategory::AiConfig,
    },
    // Google Gemini CLI
    KnownPattern {
        entry: "GEMINI.md",
        label: "Google Gemini CLI",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".gemini/",
        label: "Google Gemini CLI",
        category: PatternCategory::AiConfig,
    },
    // Cursor
    KnownPattern {
        entry: ".cursorrules",
        label: "Cursor",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".cursor/",
        label: "Cursor",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".cursorignore",
        label: "Cursor",
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
    // Cline
    KnownPattern {
        entry: ".clinerules",
        label: "Cline",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".clineignore",
        label: "Cline",
        category: PatternCategory::AiConfig,
    },
    // Roo Code
    KnownPattern {
        entry: ".roo/",
        label: "Roo Code",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".roorules",
        label: "Roo Code",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".roomodes",
        label: "Roo Code",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".rooignore",
        label: "Roo Code",
        category: PatternCategory::AiConfig,
    },
    // GitHub Copilot
    KnownPattern {
        entry: ".github/copilot-instructions.md",
        label: "GitHub Copilot",
        category: PatternCategory::AiConfig,
    },
    // JetBrains Junie
    KnownPattern {
        entry: ".junie/",
        label: "JetBrains Junie",
        category: PatternCategory::AiConfig,
    },
    // Amazon Q Developer
    KnownPattern {
        entry: ".amazonq/",
        label: "Amazon Q Developer",
        category: PatternCategory::AiConfig,
    },
    // Kiro
    KnownPattern {
        entry: ".kiro/",
        label: "Kiro",
        category: PatternCategory::AiConfig,
    },
    // Augment Code
    KnownPattern {
        entry: ".augment/",
        label: "Augment Code",
        category: PatternCategory::AiConfig,
    },
    KnownPattern {
        entry: ".augment-guidelines",
        label: "Augment Code",
        category: PatternCategory::AiConfig,
    },
    // Devin
    KnownPattern {
        entry: ".devin/",
        label: "Devin",
        category: PatternCategory::AiConfig,
    },
    // Trae
    KnownPattern {
        entry: ".trae/",
        label: "Trae",
        category: PatternCategory::AiConfig,
    },
    // Continue
    KnownPattern {
        entry: ".continuerc.json",
        label: "Continue",
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
        assert!(entries.contains(&"GEMINI.md"));
        assert!(entries.contains(&".junie/"));
        assert!(entries.contains(&".amazonq/"));
        assert!(entries.contains(&".roo/"));
        assert!(entries.contains(&".continuerc.json"));
    }

    #[test]
    fn all_patterns_are_ai_config() {
        assert!(KNOWN_SCAN_PATTERNS
            .iter()
            .all(|p| p.category == PatternCategory::AiConfig));
    }

    #[test]
    fn no_removed_patterns() {
        let entries = KNOWN_SCAN_PATTERNS
            .iter()
            .map(|p| p.entry)
            .collect::<Vec<_>>();
        // Removed incorrect patterns
        assert!(!entries.contains(&".roocodes/"));
        assert!(!entries.contains(&".roocoderules"));
        assert!(!entries.contains(&".cline/"));
        assert!(!entries.contains(&".pearai/"));
        assert!(!entries.contains(&".void/"));
        assert!(!entries.contains(&".claude.json"));
        assert!(!entries.contains(&"Agents.md"));
        assert!(!entries.contains(&".github/copilot-custom-instructions.md"));
        assert!(!entries.contains(&".continue/"));
    }
}
