mod commands;
mod diff_viewer;
mod exclude_file;
mod git;
mod guard;
mod matching;
mod patterns;
mod shadow;
mod tree_picker;
mod ui;

use anyhow::Result;
use clap::{Args, CommandFactory, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "layer")]
#[command(
    author,
    version,
    about = "layer — Context layers for git & agentic coding workflows. A fast CLI to manage local-only context files using Git's .git/info/exclude."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Add files or patterns to your local layer
    Add(AddArgs),
    /// Remove layered entries
    Rm(RmArgs),
    /// List all layered entries with status
    #[command(alias = "list")]
    Ls,
    /// Scan for context files and layer them
    Scan,
    /// List all known context-file patterns
    Patterns(PatternsArgs),
    /// Diagnose layered entries for issues
    Doctor,
    /// Remove stale entries that no longer match files
    Clean(CleanArgs),
    /// Remove all layered entries
    Clear(ClearArgs),
    /// Temporarily disable layered entries (files become visible to git)
    Off(OffArgs),
    /// Re-enable disabled layered entries
    On(OnArgs),
    /// Dashboard showing layered, exposed, and discovered files
    Status,
    /// Backup layered entries
    Backup,
    /// Restore layered entries from backup
    Restore(RestoreArgs),
    /// Manage global gitignore entries
    Global(GlobalArgs),
    /// Explain why a file is or isn't ignored by git
    Why(WhyArgs),
    /// Open .git/info/exclude in your editor
    Edit,
    /// Protect commits with a pre-commit hook
    Guard(GuardArgs),
    /// Create a snapshot of all layered files
    Snapshot(SnapshotArgs),
    /// Show change history for layered files
    Log(LogArgs),
    /// Show changes since last snapshot
    Diff(DiffArgs),
    /// Show per-line snapshot history for a layered file
    Blame(BlameArgs),
    /// Restore a layered file from history
    Revert(RevertArgs),
}

#[derive(Args, Debug)]
struct AddArgs {
    /// Files or patterns to add
    files: Vec<String>,
    /// Interactive picker mode
    #[arg(short, long)]
    interactive: bool,
    /// Preview changes without writing
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args, Debug)]
struct RmArgs {
    /// Files or patterns to remove
    files: Vec<String>,
    /// Preview changes without writing
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args, Debug)]
struct CleanArgs {
    /// Preview changes without writing
    #[arg(long)]
    dry_run: bool,
    /// Also clean stale entries you added manually to the exclude file
    #[arg(long)]
    all: bool,
}

#[derive(Args, Debug)]
struct ClearArgs {
    /// Preview changes without writing
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args, Debug)]
struct OffArgs {
    /// Entries to disable (all if omitted)
    files: Vec<String>,
    /// Preview changes without writing
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args, Debug)]
struct OnArgs {
    /// Entries to enable (all if omitted)
    files: Vec<String>,
    /// Preview changes without writing
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args, Debug)]
struct GlobalArgs {
    #[command(subcommand)]
    command: GlobalSubcommand,
}

#[derive(Subcommand, Debug)]
enum GlobalSubcommand {
    /// Add entries to global gitignore
    Add(GlobalAddArgs),
    /// List global gitignore entries
    Ls,
    /// Remove entries from global gitignore
    Rm(GlobalRmArgs),
}

#[derive(Args, Debug)]
struct GlobalAddArgs {
    /// Files or patterns to add
    files: Vec<String>,
}

#[derive(Args, Debug)]
struct GlobalRmArgs {
    /// Files or patterns to remove
    files: Vec<String>,
}

#[derive(Args, Debug)]
struct RestoreArgs {
    /// List available backups
    #[arg(long)]
    list: bool,
}

#[derive(Args, Debug)]
struct PatternsArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,
    /// Show only patterns that match files in the current repo
    #[arg(long)]
    matched: bool,
    /// Show matched file paths (requires --matched)
    #[arg(long)]
    show_files: bool,
}

#[derive(Args, Debug)]
struct WhyArgs {
    /// A single file path to diagnose
    file: String,
    /// Show extra explanation about git ignore precedence
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Args, Debug)]
struct GuardArgs {
    /// Remove the installed pre-commit hook
    #[arg(long)]
    remove: bool,
    /// Show guard installation status
    #[arg(long)]
    status: bool,
    /// Internal hook check mode
    #[arg(long, hide = true)]
    check: bool,
    /// Overwrite an existing foreign pre-commit hook
    #[arg(long)]
    force: bool,
}

#[derive(Args, Debug)]
struct SnapshotArgs {
    /// Files to snapshot (all if omitted)
    files: Vec<String>,
    /// Snapshot message
    #[arg(short, long)]
    message: Option<String>,
}

#[derive(Args, Debug)]
struct LogArgs {
    /// Show history for a specific file
    file: Option<String>,
    /// Number of entries to show
    #[arg(short = 'n', long)]
    count: Option<usize>,
}

#[derive(Args, Debug)]
struct DiffArgs {
    /// Show diff for a specific file
    file: Option<String>,
}

#[derive(Args, Debug)]
struct BlameArgs {
    /// File to show blame for
    file: String,
}

#[derive(Args, Debug)]
struct RevertArgs {
    /// File to revert
    file: String,
    /// Number of snapshots to go back
    #[arg(long, default_value = "1")]
    to: usize,
}

fn dispatch(cli: Cli) -> Result<i32> {
    match cli.command {
        Some(Commands::Add(args)) => commands::add::run(args.files, args.interactive, args.dry_run),
        Some(Commands::Rm(args)) => commands::rm::run(args.files, args.dry_run),
        Some(Commands::Ls) => commands::ls::run(),
        Some(Commands::Scan) => commands::scan::run(),
        Some(Commands::Patterns(args)) => {
            commands::patterns::run(args.json, args.matched, args.show_files)
        }
        Some(Commands::Doctor) => commands::doctor::run(),
        Some(Commands::Clean(args)) => commands::clean::run(args.dry_run, args.all),
        Some(Commands::Clear(args)) => commands::clear::run(args.dry_run),
        Some(Commands::Off(args)) => commands::on_off::run_off(args.files, args.dry_run),
        Some(Commands::On(args)) => commands::on_off::run_on(args.files, args.dry_run),
        Some(Commands::Status) => commands::status::run(),
        Some(Commands::Backup) => commands::backup::backup(),
        Some(Commands::Restore(args)) => commands::backup::restore(args.list),
        Some(Commands::Global(args)) => match args.command {
            GlobalSubcommand::Add(add) => commands::global::add(add.files),
            GlobalSubcommand::Ls => commands::global::ls(),
            GlobalSubcommand::Rm(rm) => commands::global::rm(rm.files),
        },
        Some(Commands::Why(args)) => commands::why_cmd::run(args.file, args.verbose),
        Some(Commands::Edit) => commands::edit::run(),
        Some(Commands::Guard(args)) => commands::guard::run(args),
        Some(Commands::Snapshot(args)) => commands::snapshot::run(args.files, args.message),
        Some(Commands::Log(args)) => commands::log_cmd::run(args.file, args.count),
        Some(Commands::Diff(args)) => commands::diff_cmd::run(args.file),
        Some(Commands::Blame(args)) => commands::blame_cmd::run(args.file),
        Some(Commands::Revert(args)) => commands::revert_cmd::run(args.file, args.to),
        None => {
            let mut cmd = Cli::command();
            cmd.print_help()?;
            Ok(0)
        }
    }
}

fn main() {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => match e.kind() {
            clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
                println!();
                let _ = e.print();
                println!();
                std::process::exit(0);
            }
            _ => e.exit(),
        },
    };
    let should_frame = !matches!(cli.command.as_ref(), Some(Commands::Guard(args)) if args.check);
    if should_frame {
        println!();
    }
    let code = match dispatch(cli) {
        Ok(code) => {
            if should_frame {
                println!();
            }
            code
        }
        Err(err) => {
            ui::print_error(&format!("{err:#}"));
            1
        }
    };
    std::process::exit(code);
}
