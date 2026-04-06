use console::{style, Style, Term};
use dialoguer::theme::ColorfulTheme;
use std::io::{self, Write};

// ── Status indicators ──────────────────────────────────────────

/// Layered — file is in your local layer. Dim because no action needed.
pub fn layered() -> String {
    style("✓").dim().to_string()
}

/// Exposed — file is excluded but still tracked. Needs attention.
pub fn exposed() -> String {
    style("!").yellow().bold().to_string()
}

/// Discovered — known context file found on disk that isn't layered yet.
pub fn discovered() -> String {
    style("+").cyan().to_string()
}

/// Stale — entry points to nothing. Should be cleaned.
pub fn stale() -> String {
    style("x").red().to_string()
}

/// Info — secondary/redundant note.
pub fn info() -> String {
    style("-").dim().to_string()
}

/// Manual — user-added entry outside layer section.
pub fn manual() -> String {
    style("~").dim().to_string()
}

/// Disabled — entry is temporarily turned off.
pub fn disabled() -> String {
    style("○").dim().to_string()
}

/// Success — action completed. Cyan brand accent.
pub fn ok() -> String {
    style("✓").cyan().bold().to_string()
}

// ── Text styling ───────────────────────────────────────────────

/// Brand accent — cyan bold for "layer" name and key terms.
pub fn brand(text: &str) -> String {
    style(text).cyan().bold().to_string()
}

/// Heading — cyan bold for step titles and prompts.
pub fn heading(text: &str) -> String {
    style(text).cyan().bold().to_string()
}

/// Dim text — secondary info, paths that need no action.
pub fn dim_text(text: &str) -> String {
    style(text).dim().to_string()
}

/// Warning text — yellow for things that need attention.
pub fn warn_text(text: &str) -> String {
    style(text).yellow().to_string()
}

/// Error text — red for failures.
pub fn err_text(text: &str) -> String {
    style(text).red().to_string()
}

/// Layer state — green bold for ON.
pub fn state_on() -> String {
    style("ON").green().bold().to_string()
}

/// Layer state — red bold for OFF.
pub fn state_off() -> String {
    style("OFF").red().bold().to_string()
}

// ── Output helpers ─────────────────────────────────────────────

/// Print to stderr with red "error:" prefix.
pub fn print_error(msg: &str) {
    let _ = writeln!(io::stderr(), "{} {}", style("error:").red().bold(), msg);
}

/// Print a warning line with yellow "!" prefix.
pub fn print_warning(msg: &str) {
    println!("{} {}", exposed(), style(msg).yellow());
}

/// Check if stdout is a TTY.
pub fn is_stdout_tty() -> bool {
    Term::stdout().is_term()
}

/// Bail if stdout is not a TTY. Used before interactive prompts.
pub fn require_tty(message: &str) -> anyhow::Result<()> {
    if is_stdout_tty() {
        return Ok(());
    }
    anyhow::bail!("{message}")
}

/// Print the standard dry-run footer.
pub fn print_dry_run_notice() {
    println!("{}", dim_text("(dry run — no changes made)"));
}

/// Print keyboard guide for MultiSelect prompts.
pub fn print_select_hint() {
    eprintln!(
        "  {}",
        dim_text("↑/↓ move · space select/deselect · enter confirm")
    );
}

/// Print keyboard guide for tree picker prompts.
pub fn print_tree_picker_hint() {
    eprintln!(
        "  {}",
        dim_text("↑/↓ move · space select · ←/→ expand/collapse · enter confirm")
    );
}

// ── Interactive theme ─────────────────────────────────────────

/// Custom dialoguer theme for MultiSelect prompts.
pub fn layer_theme() -> ColorfulTheme {
    ColorfulTheme {
        prompt_prefix: style("".to_string()).for_stderr(),
        prompt_suffix: style("".to_string()).for_stderr(),
        checked_item_prefix: style("  ✓".to_string()).cyan().for_stderr(),
        unchecked_item_prefix: style("  ○".to_string()).dim().for_stderr(),
        active_item_style: Style::new().cyan().bold().for_stderr(),
        inactive_item_style: Style::new().for_stderr(),
        ..ColorfulTheme::default()
    }
}
