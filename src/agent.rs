use std::process::Command;

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub name: String,
}

impl AgentInfo {
    pub fn email(&self) -> String {
        let slug = self
            .name
            .to_lowercase()
            .replace(' ', "-")
            .replace(|c: char| !c.is_ascii_alphanumeric() && c != '-', "");
        format!("{slug}@layer.local")
    }
}

/// Detect which AI agent is currently invoking `layer`.
///
/// Priority: LAYER_AGENT env > CLAUDECODE env > parent process name > GIT_AUTHOR_NAME > "manual"
pub fn detect_agent() -> AgentInfo {
    if let Ok(name) = std::env::var("LAYER_AGENT") {
        if !name.trim().is_empty() {
            return AgentInfo {
                name: name.trim().to_string(),
            };
        }
    }

    if std::env::var("CLAUDECODE").is_ok() || std::env::var("CLAUDE_CODE_ENTRYPOINT").is_ok() {
        return AgentInfo {
            name: "Claude Code".to_string(),
        };
    }

    #[cfg(unix)]
    if let Some(info) = detect_from_parent_process() {
        return info;
    }

    if let Ok(name) = std::env::var("GIT_AUTHOR_NAME") {
        if !name.trim().is_empty() {
            return AgentInfo {
                name: name.trim().to_string(),
            };
        }
    }

    AgentInfo {
        name: "manual".to_string(),
    }
}

#[cfg(unix)]
fn detect_from_parent_process() -> Option<AgentInfo> {
    use std::os::unix::process::parent_id;

    let ppid = parent_id();
    if ppid == 0 {
        return None;
    }

    let output = Command::new("ps")
        .args(["-o", "comm=", "-p", &ppid.to_string()])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let comm = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_lowercase();

    if comm.is_empty() {
        return None;
    }

    match_process_name(&comm)
}

#[cfg(unix)]
fn match_process_name(name: &str) -> Option<AgentInfo> {
    if name.contains("cursor") {
        return Some(AgentInfo {
            name: "Cursor".to_string(),
        });
    }
    if name.contains("windsurf") {
        return Some(AgentInfo {
            name: "Windsurf".to_string(),
        });
    }
    if name.contains("codex") {
        return Some(AgentInfo {
            name: "Codex".to_string(),
        });
    }
    if name.contains("code") && !name.contains("claude") {
        return Some(AgentInfo {
            name: "VS Code".to_string(),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_sanitizes_name() {
        let agent = AgentInfo {
            name: "Claude Code".to_string(),
        };
        assert_eq!(agent.email(), "claude-code@layer.local");
    }

    #[test]
    fn email_handles_special_chars() {
        let agent = AgentInfo {
            name: "My Agent (v2)".to_string(),
        };
        assert_eq!(agent.email(), "my-agent-v2@layer.local");
    }

    #[test]
    fn fallback_is_manual() {
        std::env::remove_var("LAYER_AGENT");
        std::env::remove_var("CLAUDECODE");
        std::env::remove_var("CLAUDE_CODE_ENTRYPOINT");
        let agent = detect_agent();
        assert!(!agent.name.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn match_process_cursor() {
        assert_eq!(match_process_name("cursor").unwrap().name, "Cursor");
    }

    #[cfg(unix)]
    #[test]
    fn match_process_vscode() {
        assert_eq!(match_process_name("code").unwrap().name, "VS Code");
    }

    #[cfg(unix)]
    #[test]
    fn match_process_unknown() {
        assert!(match_process_name("bash").is_none());
    }
}
