pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p = pattern.as_bytes();
    let t = text.as_bytes();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star_idx = None;
    let mut match_idx = 0usize;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star_idx = Some(pi);
            pi += 1;
            match_idx = ti;
        } else if let Some(star) = star_idx {
            pi = star + 1;
            match_idx += 1;
            ti = match_idx;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }

    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::wildcard_match;

    #[test]
    fn wildcard_matches_literals_and_stars() {
        assert!(wildcard_match("CLAUDE.md", "CLAUDE.md"));
        assert!(!wildcard_match("CLAUDE.md", "AGENTS.md"));
        assert!(wildcard_match(".aider*", ".aider.conf.yml"));
        assert!(wildcard_match(".env.*", ".env.local"));
        assert!(!wildcard_match(".env.*", ".env"));
    }

    #[test]
    fn wildcard_matches_question_mark() {
        assert!(wildcard_match("file?.txt", "file1.txt"));
        assert!(!wildcard_match("file?.txt", "file12.txt"));
    }

    #[test]
    fn wildcard_matches_empty_inputs() {
        assert!(wildcard_match("", ""));
        assert!(!wildcard_match("a", ""));
        assert!(wildcard_match("*", ""));
        assert!(wildcard_match("*", "anything"));
    }
}
