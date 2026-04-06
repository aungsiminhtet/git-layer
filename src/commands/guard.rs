use crate::git;
use crate::guard::{self, HookState, InstallResult};
use crate::ui;
use crate::GuardArgs;
use anyhow::{anyhow, Result};
use std::io::{self, Write};

pub fn run(args: GuardArgs) -> Result<i32> {
    let selected = [args.remove, args.status, args.check]
        .into_iter()
        .filter(|flag| *flag)
        .count();

    if selected > 1 {
        return Err(anyhow!("choose only one of --remove, --status, or --check"));
    }

    if args.force && (args.remove || args.status || args.check) {
        return Err(anyhow!(
            "--force can only be used when installing the guard"
        ));
    }

    if args.check {
        return run_check();
    }
    if args.remove {
        return run_remove();
    }
    if args.status {
        return run_status();
    }

    run_install(args.force)
}

fn run_install(force: bool) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let path = guard::hook_path(&ctx)?;
    match guard::install(&ctx, force)? {
        InstallResult::Installed => {
            println!(
                "  {} Guard installed at {}",
                ui::ok(),
                ui::dim_text(&path.display().to_string())
            );
        }
        InstallResult::Updated => {
            println!(
                "  {} Guard updated at {}",
                ui::ok(),
                ui::dim_text(&path.display().to_string())
            );
        }
    }
    println!(
        "  {} Protects layered files even while {} is active.",
        ui::info(),
        ui::brand("layer off")
    );
    Ok(0)
}

fn run_remove() -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let path = guard::hook_path(&ctx)?;
    match guard::hook_state(&ctx)? {
        HookState::Installed => {
            guard::remove(&ctx)?;
            println!(
                "  {} Guard removed from {}",
                ui::ok(),
                ui::dim_text(&path.display().to_string())
            );
            Ok(0)
        }
        HookState::NotInstalled => {
            println!("  {} Guard is not installed", ui::info());
            Ok(2)
        }
        HookState::ForeignHook => {
            ui::print_warning("pre-commit hook exists but is not managed by layer");
            Ok(1)
        }
    }
}

fn run_status() -> Result<i32> {
    let ctx = git::ensure_repo()?;
    match guard::hook_state(&ctx)? {
        HookState::Installed => {
            println!("  {} Guard: pre-commit hook active", ui::ok());
            Ok(0)
        }
        HookState::NotInstalled => {
            println!(
                "  {} Guard: not installed — run {} to block accidental commits",
                ui::exposed(),
                ui::brand("layer guard")
            );
            Ok(2)
        }
        HookState::ForeignHook => {
            println!(
                "  {} Guard: existing pre-commit hook not managed by layer",
                ui::exposed()
            );
            Ok(1)
        }
    }
}

fn run_check() -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let blocked = guard::check(&ctx)?;
    if blocked.is_empty() {
        return Ok(0);
    }

    let mut stderr = io::stderr();
    writeln!(stderr, "layer guard: commit blocked")?;
    writeln!(stderr)?;
    writeln!(stderr, "  Staged layered files:")?;
    for file in &blocked {
        writeln!(stderr, "    - {file}")?;
    }
    writeln!(stderr)?;
    writeln!(stderr, "  These files are managed by layer.")?;
    writeln!(stderr)?;
    writeln!(stderr, "  Fix:")?;
    for file in &blocked {
        writeln!(stderr, "    {}", unstage_command(file))?;
    }
    writeln!(stderr, "    layer on")?;
    Ok(1)
}

fn unstage_command(file: &str) -> String {
    format!("git reset HEAD -- {}", guard::sh_quote(file))
}

#[cfg(test)]
mod tests {
    use super::unstage_command;

    #[test]
    fn unstage_command_quotes_paths() {
        assert_eq!(
            unstage_command("docs/My Notes.md"),
            "git reset HEAD -- 'docs/My Notes.md'"
        );
        assert_eq!(
            unstage_command("weird'file.md"),
            "git reset HEAD -- 'weird'\"'\"'file.md'"
        );
    }
}
