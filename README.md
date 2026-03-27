# layer

Manage `.git/info/exclude` so your personal AI files stay out of git — without touching `.gitignore`.

Developers drop personal context files into repos. `API_SPEC.md`, `BACKEND_GUIDE.md`, `onboarding-notes.md`, custom prompts, architecture docs. Everyone's got different ones. You can't keep adding each person's files to `.gitignore`. `layer` hides them locally using git's built-in exclude mechanism. No more bloating your team's `.gitignore`.

## Install

```bash
cargo install git-layer
```

## Usage

```bash
layer scan                          # auto-detect AI context files and exclude them
layer add API_SPEC.md my-prompts/   # or add specific files
layer status                        # see what's layered, exposed, and discovered
```

That's it. The files disappear from `git status` and stay on disk.

### Toggle visibility

Editors (VS Code, Cursor, Claude Code) respect git exclude rules — so layered files vanish from autocomplete and file pickers too. If you need to reference them while prompting:

```bash
layer off              # files reappear in editor
layer off CLAUDE.md    # or just one file
layer on               # re-hide before committing
```

## Commands

| Command                | Description                                        |
| ---------------------- | -------------------------------------------------- |
| `layer add [files...]` | Exclude files (interactive tree picker if no args) |
| `layer rm [files...]`  | Remove entries (interactive if no args)            |
| `layer ls`             | List all managed entries with status               |
| `layer scan`           | Auto-detect known AI files and exclude them        |
| `layer status`         | Summary of layered, exposed, and discovered files  |
| `layer off [files...]` | Temporarily un-hide entries                        |
| `layer on [files...]`  | Re-hide disabled entries                           |
| `layer doctor`         | Find exposed, stale, and redundant entries         |
| `layer why <file>`     | Explain why a file is or isn't ignored             |
| `layer clean`          | Remove entries for files that no longer exist      |
| `layer clear`          | Remove all managed entries                         |
| `layer edit`           | Open `.git/info/exclude` in `$EDITOR`              |
| `layer backup`         | Save entries to `~/.layer-backups/`                |
| `layer restore`        | Restore from a backup                              |
| `layer global add`     | Add to `~/.config/git/ignore` (all repos)          |
| `layer global ls`      | List global gitignore entries                      |
| `layer global rm`      | Remove global entries                              |

## Auto-detected files

`layer scan` recognizes files from these tools:

| Tool | Files |
| --- | --- |
| Claude Code | `CLAUDE.md`, `.claude/` |
| OpenAI Codex | `AGENTS.md`, `.codex/` |
| Google Gemini CLI | `GEMINI.md`, `.gemini/` |
| Cursor | `.cursorrules`, `.cursor/`, `.cursorignore` |
| Windsurf | `.windsurfrules`, `.windsurf/` |
| Aider | `.aider*`, `.aider.conf.yml`, `.aiderignore` |
| Cline | `.clinerules`, `.clineignore` |
| Roo Code | `.roo/`, `.roorules`, `.roomodes`, `.rooignore` |
| GitHub Copilot | `.github/copilot-instructions.md` |
| JetBrains Junie | `.junie/` |
| Amazon Q Developer | `.amazonq/` |
| Kiro | `.kiro/` |
| Augment Code | `.augment/`, `.augment-guidelines` |
| Devin | `.devin/` |
| Trae | `.trae/` |
| Continue | `.continuerc.json` |
| Generic | `agents.md`, `AI.md`, `AI_CONTEXT.md`, `CONTEXT.md`, `INSTRUCTIONS.md`, `PROMPT.md`, `SYSTEM.md` |

You can always add any file with `layer add <file>`.

## How it works

Git checks ignore rules in this order:

1. **`.git/info/exclude`** — local to this clone, never shared (this is what `layer` manages)
2. `.gitignore` — tracked and shared with the team
3. `~/.config/git/ignore` — global, applies to all repos

A file must be **untracked** for ignore rules to apply. If it's already tracked, `layer` will flag it as "exposed" and tell you how to fix it (`git rm --cached`).

## Terminology

- **Layered** — in `.git/info/exclude`, hidden from git
- **Exposed** — excluded but still tracked (needs `git rm --cached`)
- **Discovered** — known AI file on disk, not yet layered
- **Stale** — entry that no longer matches any file

## Development

```bash
cargo check
cargo test
cargo clippy
cargo build --release
```

## License

MIT
