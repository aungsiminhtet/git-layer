# layer

Manage `.git/info/exclude` so your personal AI files stay out of git — without touching `.gitignore`.

Developers drop personal context files into repos. `API_SPEC.md`, `BACKEND_GUIDE.md`, `onboarding-notes.md`, custom prompts, architecture docs. Everyone's got different ones. You can't keep adding each person's files to `.gitignore`. `layer` hides them locally using git's built-in exclude mechanism. No more bloating your team's `.gitignore`.

## Install

```bash
# macOS / Linux
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/aungsiminhtet/git-layer/releases/latest/download/git-layer-installer.sh | sh

# Windows
powershell -ExecutionPolicy Bypass -c "irm https://github.com/aungsiminhtet/git-layer/releases/latest/download/git-layer-installer.ps1 | iex"

# From crates.io
cargo install git-layer
```

## Usage

```bash
layer scan                          # auto-detect AI context files and exclude them
layer add                           # interactive tree picker
layer add API_SPEC.md my-prompts/   # or add specific files
layer status                        # see what's layered, exposed, and discovered
```

That's it. The files disappear from `git status` and stay on disk.

### Toggle visibility

Editors (Cursor, Claude Code, Codex) respect git exclude rules — so layered files vanish from autocomplete and file pickers too. If you need to reference them while prompting:

```bash
layer off              # files reappear in editor
layer off CLAUDE.md    # or just one file
layer on               # re-hide before committing
```

### History tracking

Layered files are intentionally hidden from Git. That keeps them out of commits, but it also means Git cannot show you when one of those files gets rewritten, deleted, or trimmed.

`layer` keeps a private shadow history for those files, so you can inspect what changed and restore an earlier version when needed.

```bash
layer snapshot                  # save current state of all layered files
layer log                       # show change history
layer diff                      # interactive diff viewer (TUI)
layer blame CLAUDE.md           # show per-line history
layer revert CLAUDE.md          # restore from previous snapshot
```

`layer diff` opens an interactive terminal viewer for snapshot history and unsaved changes.

![layer diff viewer showing snapshot history and unsaved changes](./assets/layer-diff-viewer.png)

The shadow repo (`.layer/`) is local to your clone and initializes automatically on first snapshot.

## Commands

### Layering

| Command                | Description                                        |
| ---------------------- | -------------------------------------------------- |
| `layer add [files...]` | Exclude files (interactive tree picker if no args) |
| `layer rm [files...]`  | Remove entries (interactive if no args)            |
| `layer ls`             | List all managed entries with status               |
| `layer scan`           | Auto-detect known AI files and exclude them        |
| `layer status`         | Summary of layered, exposed, and discovered files  |

`layer scan` recognizes files from Claude Code, Cursor, Windsurf, Codex, Aider, Copilot, and many others. You can always add any file with `layer add <file>`.

### Visibility

| Command                | Description                                   |
| ---------------------- | --------------------------------------------- |
| `layer off [files...]` | Temporarily un-hide entries                   |
| `layer on [files...]`  | Re-hide disabled entries                      |
| `layer why <file>`     | Explain why a file is or isn't ignored        |
| `layer doctor`         | Find exposed, stale, and redundant entries    |

### History

| Command              | Description                                        |
| -------------------- | -------------------------------------------------- |
| `layer snapshot`     | Save current state of layered files                |
| `layer log [file]`   | Show change history                                |
| `layer diff [file]`  | Show changes since last snapshot (TUI in terminal) |
| `layer blame <file>` | Show per-line history                              |
| `layer revert <file>` | Restore file from a previous snapshot             |

### Maintenance

| Command       | Description                                   |
| ------------- | --------------------------------------------- |
| `layer clean` | Remove entries for files that no longer exist |
| `layer clear` | Remove all managed entries                    |
| `layer edit`  | Open `.git/info/exclude` in `$EDITOR`         |

### Backup

| Command         | Description                     |
| --------------- | ------------------------------- |
| `layer backup`  | Save entries to `~/.layer-backups/`  |
| `layer restore` | Restore from a backup           |

### Global

| Command            | Description                               |
| ------------------ | ----------------------------------------- |
| `layer global add` | Add to `~/.config/git/ignore` (all repos) |
| `layer global ls`  | List global gitignore entries             |
| `layer global rm`  | Remove global entries                     |

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
