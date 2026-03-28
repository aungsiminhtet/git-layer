use crate::git;
use crate::shadow::ShadowRepo;
use crate::ui;
use anyhow::{anyhow, Result};

pub fn run(file: Option<String>, count: Option<usize>) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let shadow = ShadowRepo::open(&ctx.root).ok_or_else(|| {
        anyhow!(
            "no history found — run {} to start tracking",
            ui::brand("layer snapshot")
        )
    })?;

    let format_arg = if ui::is_stdout_tty() {
        "--format=%C(cyan)%h%Creset %s %C(dim)(%ar by %an)%Creset"
    } else {
        "--format=%h %s (%ar by %an)"
    };

    let mut args = vec!["log", format_arg];

    let color_arg;
    if ui::is_stdout_tty() {
        color_arg = "--color=always".to_string();
        args.push(&color_arg);
    }

    let count_arg;
    if let Some(n) = count {
        count_arg = format!("-{n}");
        args.push(&count_arg);
    }

    if let Some(ref f) = file {
        args.push("--");
        args.push(f);
    }

    let output = shadow.shadow_git(&args)?;
    if output.trim().is_empty() {
        let suffix = file
            .as_ref()
            .map(|f| format!(" for '{f}'"))
            .unwrap_or_default();
        println!("No history found{suffix}.");
        return Ok(2);
    }

    for line in output.lines() {
        println!("  {line}");
    }

    Ok(0)
}
