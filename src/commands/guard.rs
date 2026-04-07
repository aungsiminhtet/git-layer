use crate::git;
use crate::guard::{self, GuardHealth, HookFramework, InstallMode, InstallResult, RemoveResult};
use crate::ui;
use crate::GuardArgs;
use anyhow::{anyhow, Result};
use dialoguer::Select;
use std::io::{self, Write};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingHookMode {
    Wrapper,
    Manual,
}

pub fn run(args: GuardArgs) -> Result<i32> {
    let selected = [args.remove, args.status, args.check]
        .into_iter()
        .filter(|flag| *flag)
        .count();

    if selected > 1 {
        return Err(anyhow!("choose only one of --remove, --status, or --check"));
    }
    if args.wrapper && args.manual {
        return Err(anyhow!("choose only one of --wrapper or --manual"));
    }
    if (args.wrapper || args.manual) && (args.remove || args.status || args.check) {
        return Err(anyhow!(
            "--wrapper and --manual can only be used when installing the guard"
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

    run_install(args.wrapper, args.manual)
}

fn run_install(wrapper: bool, manual: bool) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let inspection = guard::inspect(&ctx)?;
    let path = inspection.path.clone();

    let install_mode = match inspection.health {
        GuardHealth::Inactive | GuardHealth::ActiveDirect | GuardHealth::ActiveWrapper { .. } => {
            if wrapper || manual {
                return Err(anyhow!(
                    "--wrapper and --manual can only be used when an existing pre-commit hook is present"
                ));
            }
            InstallMode::Auto
        }
        GuardHealth::NeedsInstallLocal { framework } => {
            if manual {
                print_manual_instructions(&path, true, true, framework)?;
                return Ok(0);
            }
            if wrapper {
                InstallMode::Wrapper
            } else {
                match choose_existing_hook_mode(&path, framework)? {
                    ExistingHookMode::Wrapper => InstallMode::Wrapper,
                    ExistingHookMode::Manual => {
                        print_manual_instructions(&path, true, true, framework)?;
                        return Ok(0);
                    }
                }
            }
        }
        GuardHealth::NeedsRepairLocal { framework, .. } => {
            if manual {
                print_manual_instructions(
                    &path,
                    true,
                    !matches!(
                        inspection.observed,
                        guard::ObservedHook::MissingLocal | guard::ObservedHook::MissingExternal
                    ),
                    framework,
                )?;
                return Ok(0);
            }
            InstallMode::Wrapper
        }
        GuardHealth::NeedsManualExternal { framework } => {
            if wrapper {
                return Err(anyhow!(
                    "the existing pre-commit hook is outside .git and will not be moved automatically. Run --manual instead"
                ));
            }
            print_manual_instructions(
                &path,
                false,
                !matches!(inspection.observed, guard::ObservedHook::MissingExternal),
                framework,
            )?;
            return Ok(0);
        }
        GuardHealth::ActiveManual { .. } => {
            println!(
                "  {} Guard is already active via the existing hook",
                ui::ok()
            );
            return Ok(0);
        }
        GuardHealth::Broken { reason, .. } => match reason {
            guard::BrokenReason::MissingExpectedHook => {
                return Err(anyhow!(
                    "guard metadata exists but the managed hook is missing. Re-run {} to restore it",
                    ui::brand("layer guard")
                ));
            }
            guard::BrokenReason::MissingPreservedHook => {
                return Err(anyhow!(
                    "guard wrapper is missing its preserved original hook. Re-run {} or {} to repair it",
                    ui::brand("layer guard --remove"),
                    ui::brand("layer guard --wrapper")
                ));
            }
        },
    };

    match guard::install(&ctx, install_mode)? {
        InstallResult::Installed => {
            println!(
                "  {} Guard installed at {}",
                ui::ok(),
                ui::dim_text(&path.display().to_string())
            );
        }
        InstallResult::Wrapped(original) => {
            println!(
                "  {} Guard installed at {}",
                ui::ok(),
                ui::dim_text(&path.display().to_string())
            );
            println!(
                "  {} Existing pre-commit hook preserved at {}",
                ui::info(),
                ui::dim_text(&original.display().to_string())
            );
            println!(
                "  {} Layer guard runs first, then your original hook.",
                ui::info()
            );
        }
        InstallResult::Restored(original) => {
            println!(
                "  {} Guard restored at {}",
                ui::ok(),
                ui::dim_text(&path.display().to_string())
            );
            println!(
                "  {} Existing pre-commit hook preserved at {}",
                ui::info(),
                ui::dim_text(&original.display().to_string())
            );
            println!(
                "  {} Layer guard runs first, then your original hook.",
                ui::info()
            );
        }
        InstallResult::Updated => {
            println!(
                "  {} Guard updated at {}",
                ui::ok(),
                ui::dim_text(&path.display().to_string())
            );
            if let Some(original) = guard::preserved_hook(&ctx)? {
                println!(
                    "  {} Existing pre-commit hook preserved at {}",
                    ui::info(),
                    ui::dim_text(&original.display().to_string())
                );
                println!(
                    "  {} Layer guard runs first, then your original hook.",
                    ui::info()
                );
            }
        }
    }
    if inspection.framework == HookFramework::PreCommit {
        println!(
            "  {} Re-running {} may replace the wrapper. {} can restore it when you run {}.",
            ui::info(),
            ui::brand("pre-commit install"),
            ui::brand("layer"),
            ui::brand("layer off")
        );
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
    let inspection = guard::inspect(&ctx)?;
    let path = inspection.path.clone();
    match inspection.health {
        GuardHealth::ActiveDirect | GuardHealth::ActiveWrapper { .. } => {
            match guard::remove(&ctx)? {
                RemoveResult::Removed => {
                    println!(
                        "  {} Guard removed from {}",
                        ui::ok(),
                        ui::dim_text(&path.display().to_string())
                    );
                }
                RemoveResult::Restored(restored) => {
                    println!(
                        "  {} Guard removed from {}",
                        ui::ok(),
                        ui::dim_text(&path.display().to_string())
                    );
                    println!(
                        "  {} Restored original pre-commit hook at {}",
                        ui::info(),
                        ui::dim_text(&restored.display().to_string())
                    );
                }
            }
            Ok(0)
        }
        GuardHealth::Inactive
        | GuardHealth::NeedsInstallLocal { .. }
        | GuardHealth::NeedsManualExternal { .. } => {
            println!("  {} Guard is not installed", ui::info());
            Ok(2)
        }
        GuardHealth::NeedsRepairLocal { .. } | GuardHealth::Broken { .. } => {
            println!(
                "  {} pre-commit hook exists but is not managed by layer",
                ui::exposed()
            );
            Ok(1)
        }
        GuardHealth::ActiveManual { .. } => {
            println!(
                "  {} Guard was integrated manually. Remove {} from your hook to deactivate it.",
                ui::info(),
                ui::brand("layer guard --check"),
            );
            Ok(1)
        }
    }
}

fn run_status() -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let inspection = guard::inspect(&ctx)?;
    let output = guard::status_output(&inspection);
    for line in &output.lines {
        println!("  {} {}", line.indicator, line.text);
    }
    Ok(output.exit_code)
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

fn choose_existing_hook_mode(path: &Path, framework: HookFramework) -> Result<ExistingHookMode> {
    ui::require_tty(
        "existing pre-commit hook detected. Re-run with --wrapper to preserve it, or --manual for setup instructions",
    )?;

    println!(
        "  {} Existing pre-commit hook detected at {}",
        ui::info(),
        ui::dim_text(&path.display().to_string())
    );
    println!(
        "  {} {} preserves your current hook as {} and runs it after layer guard.",
        ui::info(),
        ui::brand("Wrapper (Recommended)"),
        ui::dim_text("pre-commit.layer-original")
    );
    println!(
        "  {} {} leaves your hook unchanged and shows the snippet to add yourself.",
        ui::info(),
        ui::brand("Manual")
    );
    if framework == HookFramework::PreCommit {
        println!(
            "  {} Detected {}. Re-running {} may replace the wrapper later, but {} can restore it when you run {}.",
            ui::info(),
            ui::brand("Python pre-commit"),
            ui::brand("pre-commit install"),
            ui::brand("layer"),
            ui::brand("layer off")
        );
    }
    println!();

    let choice = Select::with_theme(&ui::layer_theme())
        .with_prompt("Choose how to add layer guard")
        .items(&["Wrapper (Recommended)", "Manual"])
        .default(0)
        .report(false)
        .interact()?;

    Ok(match choice {
        0 => ExistingHookMode::Wrapper,
        _ => ExistingHookMode::Manual,
    })
}

fn print_manual_instructions(
    hook_path: &Path,
    wrapper_available: bool,
    existing_hook: bool,
    framework: HookFramework,
) -> Result<()> {
    let (setup_line, snippet) = match framework {
        HookFramework::Husky => (
            format!(
                "Manual setup: add this near the start of {}:",
                ui::brand(".husky/pre-commit")
            ),
            guard::manual_husky_snippet(),
        ),
        HookFramework::Lefthook => (
            format!(
                "Manual setup: add this under your {} commands in {}:",
                ui::brand("pre-commit"),
                ui::brand("lefthook.yml")
            ),
            guard::manual_lefthook_snippet(),
        ),
        _ => (
            "Manual setup: copy this near the start of your shell pre-commit hook:".to_string(),
            guard::manual_shell_snippet()?,
        ),
    };

    if existing_hook {
        println!(
            "  {} Existing pre-commit hook: {}",
            ui::info(),
            ui::dim_text(&hook_path.display().to_string())
        );
    } else {
        println!(
            "  {} Effective pre-commit hook path: {}",
            ui::info(),
            ui::dim_text(&hook_path.display().to_string())
        );
    }
    if let Some(label) = framework.label() {
        println!("  {} Detected: {}", ui::info(), ui::brand(label));
    }
    if !wrapper_available {
        println!(
            "  {} Repo-managed hook outside {}. Layer will not move it automatically.",
            ui::info(),
            ui::dim_text(".git")
        );
    }
    if framework == HookFramework::PreCommit {
        println!(
            "  {} Wrapper is recommended because it stays local to this clone.",
            ui::info(),
        );
        println!(
            "  {} Re-running {} may replace it later. {} can restore it when you run {}.",
            ui::info(),
            ui::brand("pre-commit install"),
            ui::brand("layer"),
            ui::brand("layer off")
        );
    }
    println!("  {} {}", ui::info(), setup_line);
    println!();
    for line in snippet.lines() {
        println!("    {}", line);
    }
    let show_non_shell_hint = framework == HookFramework::Unknown;
    let show_wrapper_hint = wrapper_available;
    if show_non_shell_hint || show_wrapper_hint {
        println!();
    }
    if show_non_shell_hint {
        println!(
            "  {} If your hook is not shell-based, run {} and stop when it exits non-zero.",
            ui::info(),
            ui::brand("layer guard --check")
        );
    }
    if show_wrapper_hint {
        println!(
            "  {} Or re-run {} to let layer preserve your current hook automatically.",
            ui::info(),
            ui::brand("layer guard --wrapper")
        );
    }
    Ok(())
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
