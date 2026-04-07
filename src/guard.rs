use crate::exclude_file::ensure_exclude_file;
use crate::git::{self, RepoContext};
use crate::matching::wildcard_match;
use crate::ui;
use anyhow::{anyhow, Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const GUARD_START: &str = "# layer-guard-start";
pub const GUARD_END: &str = "# layer-guard-end";
pub const MANUAL_MARKER: &str = "# layer-guard-manual";
const GUARD_CHECK_COMMAND: &str = "layer guard --check";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMode {
    Auto,
    Wrapper,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallResult {
    Installed,
    Wrapped(PathBuf),
    Restored(PathBuf),
    Updated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveResult {
    Removed,
    Restored(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookFramework {
    Unknown,
    PreCommit,
    Husky,
    Lefthook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardIntent {
    None,
    Direct,
    Wrapper { framework: HookFramework },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedHook {
    MissingLocal,
    MissingExternal,
    ContainsGuard { preserved_exists: bool },
    ForeignLocal,
    ForeignExternal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokenReason {
    MissingExpectedHook,
    MissingPreservedHook,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardHealth {
    Inactive,
    ActiveDirect,
    ActiveWrapper {
        preserved: PathBuf,
        framework: HookFramework,
    },
    ActiveManual {
        local: bool,
        framework: HookFramework,
    },
    NeedsInstallLocal {
        framework: HookFramework,
    },
    NeedsManualExternal {
        framework: HookFramework,
    },
    NeedsRepairLocal {
        preserved: Option<PathBuf>,
        framework: HookFramework,
    },
    Broken {
        local: bool,
        reason: BrokenReason,
        framework: HookFramework,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardInspection {
    pub path: PathBuf,
    pub preserved_path: PathBuf,
    pub local: bool,
    pub framework: HookFramework,
    pub intent: GuardIntent,
    pub observed: ObservedHook,
    pub health: GuardHealth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardStatusLine {
    pub indicator: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardStatusOutput {
    pub exit_code: i32,
    pub lines: Vec<GuardStatusLine>,
}

fn layer_bin() -> Result<PathBuf> {
    let layer_bin = std::env::current_exe().context("failed to resolve layer executable path")?;
    Ok(layer_bin)
}

impl HookFramework {
    pub fn label(self) -> Option<&'static str> {
        match self {
            HookFramework::Unknown => None,
            HookFramework::PreCommit => Some("Python pre-commit"),
            HookFramework::Husky => Some("Husky"),
            HookFramework::Lefthook => Some("Lefthook"),
        }
    }

    fn as_state_value(self) -> &'static str {
        match self {
            HookFramework::Unknown => "unknown",
            HookFramework::PreCommit => "pre_commit",
            HookFramework::Husky => "husky",
            HookFramework::Lefthook => "lefthook",
        }
    }

    fn from_state_value(value: &str) -> Self {
        match value {
            "pre_commit" => HookFramework::PreCommit,
            "husky" => HookFramework::Husky,
            "lefthook" => HookFramework::Lefthook,
            _ => HookFramework::Unknown,
        }
    }
}

fn intent_path(ctx: &RepoContext) -> PathBuf {
    ctx.git_dir.join("layer").join("guard-state")
}

pub fn read_guard_intent(ctx: &RepoContext) -> GuardIntent {
    let path = intent_path(ctx);
    let Ok(content) = fs::read_to_string(path) else {
        return GuardIntent::None;
    };

    let mut intent = GuardIntent::None;
    let mut framework = HookFramework::Unknown;

    for line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(value) = line.strip_prefix("framework=") {
            framework = HookFramework::from_state_value(value);
            continue;
        }

        if let Some(value) = line.strip_prefix("intent=") {
            intent = match value {
                "direct" => GuardIntent::Direct,
                "wrapper" => GuardIntent::Wrapper { framework },
                _ => GuardIntent::None,
            };
        }
    }

    match intent {
        GuardIntent::Wrapper { .. } => GuardIntent::Wrapper { framework },
        _ => intent,
    }
}

pub fn write_guard_intent(ctx: &RepoContext, intent: GuardIntent) -> Result<()> {
    let path = intent_path(ctx);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let content = match intent {
        GuardIntent::None => return clear_guard_intent(ctx),
        GuardIntent::Direct => "version=1\nintent=direct\n".to_string(),
        GuardIntent::Wrapper { framework } => format!(
            "version=1\nframework={}\nintent=wrapper\n",
            framework.as_state_value()
        ),
    };

    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content).with_context(|| format!("failed to write {}", tmp.display()))?;
    fs::rename(&tmp, &path)
        .with_context(|| format!("failed to move {} into place", path.display()))?;
    Ok(())
}

pub fn clear_guard_intent(ctx: &RepoContext) -> Result<()> {
    let path = intent_path(ctx);
    if path.exists() {
        fs::remove_file(&path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

pub fn hook_path(ctx: &RepoContext) -> Result<PathBuf> {
    git::git_path(&ctx.root, "hooks/pre-commit")
}

pub fn hook_path_is_local(ctx: &RepoContext) -> Result<bool> {
    Ok(hook_path(ctx)?.starts_with(&ctx.git_dir))
}

pub fn preserved_hook_path(ctx: &RepoContext) -> Result<PathBuf> {
    Ok(preserved_hook_path_from(&hook_path(ctx)?))
}

pub fn preserved_hook(ctx: &RepoContext) -> Result<Option<PathBuf>> {
    let path = preserved_hook_path(ctx)?;
    if path.exists() {
        Ok(Some(path))
    } else {
        Ok(None)
    }
}

pub fn inspect(ctx: &RepoContext) -> Result<GuardInspection> {
    let path = hook_path(ctx)?;
    let local = hook_path_is_local(ctx)?;
    let preserved = preserved_hook_path_from(&path);
    let preserved_exists = preserved.exists();
    let intent = read_guard_intent(ctx);

    let content = if path.exists() {
        Some(
            fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?,
        )
    } else {
        None
    };

    let framework = detect_framework(ctx, &path, content.as_deref());
    let observed = match content.as_deref() {
        None if local => ObservedHook::MissingLocal,
        None => ObservedHook::MissingExternal,
        Some(content)
            if content.contains(GUARD_START) || has_uncommented_guard_check(content) =>
        {
            ObservedHook::ContainsGuard { preserved_exists }
        }
        Some(_) if local => ObservedHook::ForeignLocal,
        Some(_) => ObservedHook::ForeignExternal,
    };
    let health = match content.as_deref() {
        None if local => match intent {
            GuardIntent::None => GuardHealth::Inactive,
            GuardIntent::Direct => GuardHealth::NeedsRepairLocal {
                preserved: None,
                framework,
            },
            GuardIntent::Wrapper { .. } if preserved_exists => GuardHealth::NeedsRepairLocal {
                preserved: Some(preserved.clone()),
                framework: framework_from_intent(intent, framework),
            },
            GuardIntent::Wrapper { .. } => GuardHealth::Broken {
                local: true,
                reason: BrokenReason::MissingPreservedHook,
                framework: framework_from_intent(intent, framework),
            },
        },
        None => match intent {
            GuardIntent::None => GuardHealth::NeedsManualExternal { framework },
            _ => GuardHealth::Broken {
                local: false,
                reason: BrokenReason::MissingExpectedHook,
                framework: framework_from_intent(intent, framework),
            },
        },
        Some(content) if content.contains(GUARD_START) => {
            if content.contains("ORIGINAL_HOOK=''") {
                GuardHealth::ActiveDirect
            } else if preserved_exists {
                GuardHealth::ActiveWrapper {
                    preserved: preserved.clone(),
                    framework: framework_from_intent(intent, framework),
                }
            } else {
                GuardHealth::Broken {
                    local,
                    reason: BrokenReason::MissingPreservedHook,
                    framework: framework_from_intent(intent, framework),
                }
            }
        }
        Some(content) if has_uncommented_guard_check(content) => {
            GuardHealth::ActiveManual { local, framework }
        }
        Some(_) if local => match intent {
            GuardIntent::None => GuardHealth::NeedsInstallLocal { framework },
            _ => GuardHealth::NeedsRepairLocal {
                preserved: Some(preserved.clone()),
                framework: framework_from_intent(intent, framework),
            },
        },
        Some(_) => match intent {
            GuardIntent::None => GuardHealth::NeedsManualExternal { framework },
            _ => GuardHealth::Broken {
                local: false,
                reason: BrokenReason::MissingExpectedHook,
                framework: framework_from_intent(intent, framework),
            },
        },
    };

    // Lefthook compiles lefthook.yml into an opaque runner that doesn't contain
    // "layer guard --check", so fall back to checking the config file directly.
    let health = match health {
        GuardHealth::NeedsManualExternal {
            framework: HookFramework::Lefthook,
        } if content.is_some() && lefthook_config_contains_guard(ctx) => {
            GuardHealth::ActiveManual {
                local,
                framework: HookFramework::Lefthook,
            }
        }
        other => other,
    };

    Ok(GuardInspection {
        path,
        preserved_path: preserved,
        local,
        framework,
        intent,
        observed,
        health,
    })
}

pub fn status_output(inspection: &GuardInspection) -> GuardStatusOutput {
    match &inspection.health {
        GuardHealth::ActiveDirect | GuardHealth::ActiveWrapper { .. } => {
            let mut lines = vec![GuardStatusLine {
                indicator: ui::ok(),
                text: "Guard: pre-commit hook active".to_string(),
            }];
            if let GuardHealth::ActiveWrapper { preserved, .. } = &inspection.health {
                lines.push(GuardStatusLine {
                    indicator: ui::info(),
                    text: format!(
                        "Chaining existing pre-commit hook: {}",
                        ui::dim_text(&preserved.display().to_string())
                    ),
                });
            }
            GuardStatusOutput {
                exit_code: 0,
                lines,
            }
        }
        GuardHealth::ActiveManual { framework, .. } => GuardStatusOutput {
            exit_code: 0,
            lines: vec![GuardStatusLine {
                indicator: ui::ok(),
                text: match framework.label() {
                    Some(label) => {
                        format!("Guard: manual integration active via {}", ui::brand(label))
                    }
                    None => "Guard: manual integration active".to_string(),
                },
            }],
        },
        GuardHealth::Inactive => GuardStatusOutput {
            exit_code: 2,
            lines: vec![GuardStatusLine {
                indicator: ui::exposed(),
                text: format!(
                    "Guard: not installed — run {} to block accidental commits",
                    ui::brand("layer guard")
                ),
            }],
        },
        GuardHealth::NeedsRepairLocal { .. } => GuardStatusOutput {
            exit_code: 1,
            lines: vec![GuardStatusLine {
                indicator: ui::exposed(),
                text: format!(
                    "Guard: replaced by another hook installer — run {} to restore it",
                    ui::brand("layer guard --wrapper")
                ),
            }],
        },
        GuardHealth::NeedsInstallLocal { framework } => GuardStatusOutput {
            exit_code: 1,
            lines: vec![GuardStatusLine {
                indicator: ui::exposed(),
                text: match framework {
                    HookFramework::PreCommit => format!(
                        "Guard: not installed — run {} to wrap the existing Python pre-commit hook",
                        ui::brand("layer guard")
                    ),
                    _ => format!(
                        "Guard: not installed — run {} to set it up with the existing pre-commit hook",
                        ui::brand("layer guard")
                    ),
                },
            }],
        },
        GuardHealth::NeedsManualExternal { framework } => GuardStatusOutput {
            exit_code: 1,
            lines: vec![GuardStatusLine {
                indicator: ui::exposed(),
                text: match framework {
                    HookFramework::Husky => format!(
                        "Guard: not installed — run {} to add it to {}",
                        ui::brand("layer guard --manual"),
                        ui::brand(".husky/pre-commit")
                    ),
                    HookFramework::Lefthook => format!(
                        "Guard: not installed — run {} to add it to {}",
                        ui::brand("layer guard --manual"),
                        ui::brand("lefthook.yml")
                    ),
                    _ => format!(
                        "Guard: not installed — run {} for manual setup with the existing pre-commit hook",
                        ui::brand("layer guard --manual")
                    ),
                },
            }],
        },
        GuardHealth::Broken { local, reason, .. } => GuardStatusOutput {
            exit_code: 1,
            lines: vec![GuardStatusLine {
                indicator: ui::exposed(),
                text: match (local, reason) {
                    (false, BrokenReason::MissingExpectedHook) => format!(
                        "Guard: expected hook is managed outside .git — run {} to repair it",
                        ui::brand("layer guard --manual")
                    ),
                    (_, BrokenReason::MissingExpectedHook) => format!(
                        "Guard: expected hook is missing — run {} to restore it",
                        ui::brand("layer guard")
                    ),
                    (_, BrokenReason::MissingPreservedHook) => format!(
                        "Guard: preserved original hook is missing — run {} or {} to repair it",
                        ui::brand("layer guard --remove"),
                        ui::brand("layer guard --wrapper")
                    ),
                },
            }],
        },
    }
}

fn framework_from_intent(intent: GuardIntent, fallback: HookFramework) -> HookFramework {
    match intent {
        GuardIntent::Wrapper { framework } if framework != HookFramework::Unknown => framework,
        _ => fallback,
    }
}

fn detect_framework(
    ctx: &RepoContext,
    hook_path: &Path,
    hook_content: Option<&str>,
) -> HookFramework {
    if hook_path
        .components()
        .any(|component| component.as_os_str() == ".husky")
    {
        return HookFramework::Husky;
    }

    if ctx.root.join("lefthook.yml").exists() || ctx.root.join("lefthook.yaml").exists() {
        return HookFramework::Lefthook;
    }

    if ctx.root.join(".pre-commit-config.yaml").exists()
        || ctx.root.join(".pre-commit-config.yml").exists()
        || hook_content
            .map(|content| content.contains("generated by pre-commit"))
            .unwrap_or(false)
    {
        return HookFramework::PreCommit;
    }

    HookFramework::Unknown
}

fn has_uncommented_guard_check(content: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with('#') && trimmed.contains(GUARD_CHECK_COMMAND)
    })
}

fn lefthook_config_contains_guard(ctx: &RepoContext) -> bool {
    for name in &["lefthook.yml", "lefthook.yaml"] {
        if let Ok(content) = fs::read_to_string(ctx.root.join(name)) {
            if has_uncommented_guard_check(&content) {
                return true;
            }
        }
    }
    false
}

pub fn install(ctx: &RepoContext, mode: InstallMode) -> Result<InstallResult> {
    let inspection = inspect(ctx)?;
    let layer_bin = layer_bin()?;

    match (&inspection.health, inspection.intent, inspection.observed) {
        (GuardHealth::Inactive, _, ObservedHook::MissingLocal) => {
            write_hook(&inspection.path, &render_managed_hook(&layer_bin, None))?;
            write_guard_intent(ctx, GuardIntent::Direct)?;
            Ok(InstallResult::Installed)
        }
        (GuardHealth::ActiveDirect, _, _) => {
            write_hook(&inspection.path, &render_managed_hook(&layer_bin, None))?;
            write_guard_intent(ctx, GuardIntent::Direct)?;
            Ok(InstallResult::Updated)
        }
        (GuardHealth::ActiveWrapper { preserved, .. }, _, _) => {
            write_hook(
                &inspection.path,
                &render_managed_hook(&layer_bin, Some(preserved)),
            )?;
            write_guard_intent(
                ctx,
                GuardIntent::Wrapper {
                    framework: inspection.framework,
                },
            )?;
            Ok(InstallResult::Updated)
        }
        (GuardHealth::ActiveManual { .. }, _, _) => Err(anyhow!(
            "the effective pre-commit hook is configured manually. Remove the manual guard block first if you want layer to manage it directly"
        )),
        (GuardHealth::NeedsInstallLocal { .. }, _, ObservedHook::ForeignLocal) => {
            if mode != InstallMode::Wrapper {
                return Err(anyhow!(
                    "existing pre-commit hook detected. Re-run with --wrapper to preserve it, or --manual for setup instructions"
                ));
            }
            preserve_current_hook(&inspection.path, &inspection.preserved_path)?;
            write_hook(
                &inspection.path,
                &render_managed_hook(&layer_bin, Some(&inspection.preserved_path)),
            )?;
            write_guard_intent(
                ctx,
                GuardIntent::Wrapper {
                    framework: inspection.framework,
                },
            )?;
            Ok(InstallResult::Wrapped(inspection.preserved_path.clone()))
        }
        (GuardHealth::NeedsRepairLocal { .. }, _, _) => repair(ctx),
        (GuardHealth::NeedsManualExternal { .. }, _, _) => Err(anyhow!(
            "the effective pre-commit hook lives outside .git. Run --manual instead"
        )),
        (GuardHealth::Broken { reason, .. }, _, _) => Err(match reason {
            BrokenReason::MissingExpectedHook => anyhow!(
                "guard state expects a managed hook, but the effective pre-commit hook is missing or managed elsewhere"
            ),
            BrokenReason::MissingPreservedHook => anyhow!(
                "guard state expects a preserved pre-commit hook, but it is missing"
            ),
        }),
        _ => Err(anyhow!(
            "guard install: unexpected state (health={:?}, intent={:?}, observed={:?})",
            inspection.health,
            inspection.intent,
            inspection.observed
        )),
    }
}

pub fn repair(ctx: &RepoContext) -> Result<InstallResult> {
    let inspection = inspect(ctx)?;
    let layer_bin = layer_bin()?;

    match (&inspection.health, inspection.intent, inspection.observed) {
        (GuardHealth::NeedsRepairLocal { .. }, GuardIntent::Direct, ObservedHook::MissingLocal) => {
            write_hook(&inspection.path, &render_managed_hook(&layer_bin, None))?;
            write_guard_intent(ctx, GuardIntent::Direct)?;
            Ok(InstallResult::Installed)
        }
        (GuardHealth::NeedsRepairLocal { .. }, GuardIntent::Direct, ObservedHook::ForeignLocal) => {
            preserve_current_hook(&inspection.path, &inspection.preserved_path)?;
            write_hook(
                &inspection.path,
                &render_managed_hook(&layer_bin, Some(&inspection.preserved_path)),
            )?;
            write_guard_intent(
                ctx,
                GuardIntent::Wrapper {
                    framework: inspection.framework,
                },
            )?;
            Ok(InstallResult::Restored(inspection.preserved_path.clone()))
        }
        (
            GuardHealth::NeedsRepairLocal {
                preserved: Some(preserved),
                ..
            },
            GuardIntent::Wrapper { framework },
            ObservedHook::MissingLocal,
        ) => {
            write_hook(
                &inspection.path,
                &render_managed_hook(&layer_bin, Some(preserved)),
            )?;
            write_guard_intent(ctx, GuardIntent::Wrapper { framework })?;
            Ok(InstallResult::Restored(preserved.clone()))
        }
        (
            GuardHealth::NeedsRepairLocal { .. },
            GuardIntent::Wrapper { framework },
            ObservedHook::ForeignLocal,
        ) => {
            preserve_current_hook(&inspection.path, &inspection.preserved_path)?;
            write_hook(
                &inspection.path,
                &render_managed_hook(&layer_bin, Some(&inspection.preserved_path)),
            )?;
            write_guard_intent(ctx, GuardIntent::Wrapper { framework })?;
            Ok(InstallResult::Restored(inspection.preserved_path.clone()))
        }
        _ => Err(anyhow!(
            "guard repair is not available for the current hook state"
        )),
    }
}

pub fn remove(ctx: &RepoContext) -> Result<RemoveResult> {
    let path = hook_path(ctx)?;
    if !path.exists() {
        clear_guard_intent(ctx)?;
        return Ok(RemoveResult::Removed);
    }

    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    if !content.contains(GUARD_START) {
        return Err(anyhow!(
            "pre-commit hook exists and is not managed by layer"
        ));
    }

    let preserved = preserved_hook_path_from(&path);
    if preserved.exists() {
        fs::remove_file(&path).with_context(|| format!("failed to remove {}", path.display()))?;
        fs::rename(&preserved, &path).with_context(|| {
            format!(
                "failed to restore preserved pre-commit hook from {}",
                preserved.display()
            )
        })?;
        clear_guard_intent(ctx)?;
        return Ok(RemoveResult::Restored(path));
    }

    let remaining = strip_guard_block(&content);
    if is_guard_only(&remaining) {
        fs::remove_file(&path).with_context(|| format!("failed to remove {}", path.display()))?;
    } else {
        write_hook(&path, &remaining)?;
    }
    clear_guard_intent(ctx)?;
    Ok(RemoveResult::Removed)
}

pub fn check(ctx: &RepoContext) -> Result<Vec<String>> {
    let exclude = ensure_exclude_file(&ctx.exclude_path)?;
    let entries = exclude.managed_entry_set();
    if entries.is_empty() {
        return Ok(Vec::new());
    }

    let mut blocked: Vec<String> = get_staged_files(ctx)?
        .into_iter()
        .filter(|file| matches_any_entry(file, &entries))
        .collect();
    blocked.sort();
    blocked.dedup();
    Ok(blocked)
}

pub fn manual_shell_snippet() -> Result<String> {
    Ok(render_manual_shell_snippet(&layer_bin()?))
}

pub fn manual_husky_snippet() -> String {
    format!("{MANUAL_MARKER}\nlayer guard --check || exit $?")
}

pub fn manual_lefthook_snippet() -> String {
    format!("{MANUAL_MARKER}\nlayer-guard:\n  run: layer guard --check")
}

fn render_managed_hook(layer_bin: &Path, preserved_hook: Option<&Path>) -> String {
    let quoted = sh_quote(&layer_bin.to_string_lossy());
    let preserved = preserved_hook
        .map(|path| sh_quote(&path.to_string_lossy()))
        .unwrap_or_else(|| "''".to_string());
    format!(
        "#!/bin/sh\n{GUARD_START}\nLAYER_BIN={quoted}\nORIGINAL_HOOK={preserved}\nrun_layer_guard() {{\n    if [ -x \"$LAYER_BIN\" ]; then\n        \"$LAYER_BIN\" guard --check\n    elif command -v layer >/dev/null 2>&1; then\n        layer guard --check\n    else\n        echo \"layer guard: unable to find the layer binary\" >&2\n        echo \"run 'layer guard' to refresh the hook\" >&2\n        return 1\n    fi\n}}\n\n# Block layered files already staged before the original hook runs.\nrun_layer_guard || exit $?\n\nif [ -n \"$ORIGINAL_HOOK\" ]; then\n    if [ -x \"$ORIGINAL_HOOK\" ]; then\n        \"$ORIGINAL_HOOK\" \"$@\" || exit $?\n    else\n        echo \"layer guard: preserved pre-commit hook not found at $ORIGINAL_HOOK\" >&2\n        echo \"run 'layer guard --remove' or 'layer guard --wrapper' to repair the hook\" >&2\n        exit 1\n    fi\nfi\n\n# Re-check in case the original hook staged layered files.\nrun_layer_guard || exit $?\n{GUARD_END}\n"
    )
}

fn render_manual_shell_snippet(layer_bin: &Path) -> String {
    let quoted = sh_quote(&layer_bin.to_string_lossy());
    format!(
        "{MANUAL_MARKER}\nLAYER_BIN={quoted}\nif [ -x \"$LAYER_BIN\" ]; then\n    \"$LAYER_BIN\" guard --check || exit $?\nelif command -v layer >/dev/null 2>&1; then\n    layer guard --check || exit $?\nelse\n    echo \"layer guard: unable to find the layer binary\" >&2\n    exit 1\nfi"
    )
}

fn strip_guard_block(content: &str) -> String {
    let mut result = String::new();
    let mut in_block = false;

    for line in content.lines() {
        if line.trim() == GUARD_START {
            in_block = true;
            continue;
        }
        if in_block {
            if line.trim() == GUARD_END {
                in_block = false;
            }
            continue;
        }
        result.push_str(line);
        result.push('\n');
    }
    result
}

fn is_guard_only(content: &str) -> bool {
    let trimmed = content.trim();
    trimmed.is_empty() || trimmed == "#!/bin/sh"
}

fn preserved_hook_path_from(hook_path: &Path) -> PathBuf {
    let file_name = hook_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pre-commit".to_string());
    hook_path.with_file_name(format!("{file_name}.layer-original"))
}

fn legacy_hook_path_from(hook_path: &Path) -> PathBuf {
    let file_name = hook_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pre-commit".to_string());
    hook_path.with_file_name(format!("{file_name}.legacy"))
}

fn cleanup_stale_legacy_wrapper(hook_path: &Path) -> Result<()> {
    let legacy = legacy_hook_path_from(hook_path);
    if !legacy.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&legacy)
        .with_context(|| format!("failed to read {}", legacy.display()))?;
    if content.contains(GUARD_START) {
        fs::remove_file(&legacy)
            .with_context(|| format!("failed to remove {}", legacy.display()))?;
    }
    Ok(())
}

fn preserve_current_hook(hook_path: &Path, preserved_path: &Path) -> Result<()> {
    let current = fs::read_to_string(hook_path)
        .with_context(|| format!("failed to read {}", hook_path.display()))?;
    write_hook(preserved_path, &current)?;
    cleanup_stale_legacy_wrapper(hook_path)
}

pub(crate) fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn write_hook(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(path)
            .with_context(|| format!("failed to read metadata for {}", path.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)
            .with_context(|| format!("failed to set executable bit on {}", path.display()))?;
    }

    Ok(())
}

fn get_staged_files(ctx: &RepoContext) -> Result<Vec<String>> {
    let output = git::git_stdout_bytes(
        &["diff", "--cached", "--name-only", "-z", "--"],
        Some(&ctx.root),
    )?;
    Ok(output
        .split(|byte| *byte == b'\0')
        .filter(|item| !item.is_empty())
        .map(|item| String::from_utf8_lossy(item).into_owned())
        .collect())
}

fn matches_any_entry(file: &str, entries: &HashSet<String>) -> bool {
    entries.iter().any(|entry| matches_entry(file, entry))
}

fn matches_entry(file: &str, entry: &str) -> bool {
    if entry.ends_with('/') {
        return file.starts_with(entry);
    }

    if git::contains_glob(entry) {
        if entry.contains('/') {
            return wildcard_match(entry, file);
        }
        return wildcard_match(entry, file.rsplit('/').next().unwrap_or(file));
    }

    file == entry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::RepoContext;
    use std::process::Command;
    use tempfile::tempdir;

    fn init_repo() -> (tempfile::TempDir, RepoContext) {
        let tmp = tempdir().expect("failed to create temp dir");

        let output = Command::new("git")
            .args(["init", "-q"])
            .current_dir(tmp.path())
            .output()
            .expect("failed to init repo");
        assert!(output.status.success(), "git init failed");

        let root = tmp.path().to_path_buf();
        let git_dir = root.join(".git");
        let ctx = RepoContext {
            root: root.clone(),
            git_dir: git_dir.clone(),
            exclude_path: git_dir.join("info").join("exclude"),
        };

        (tmp, ctx)
    }

    #[test]
    fn matches_literals_directories_and_globs() {
        assert!(matches_entry("CLAUDE.md", "CLAUDE.md"));
        assert!(matches_entry(".claude/settings.json", ".claude/"));
        assert!(matches_entry("notes/CLAUDE.md", "*.md"));
        assert!(!matches_entry("notes/CLAUDE.md", "CLAUDE.md"));
        assert!(!matches_entry("notes/todo.txt", "*.md"));
    }

    #[test]
    fn managed_hook_contains_markers_and_guard_checks() {
        let hook = render_managed_hook(std::path::Path::new("/tmp/layer"), None);
        assert!(hook.contains("#!/bin/sh"));
        assert!(hook.contains(GUARD_START));
        assert!(hook.contains(GUARD_END));
        assert!(hook.contains("guard --check"));
    }

    #[test]
    fn strip_guard_block_removes_markers() {
        let content =
            "#!/bin/sh\necho 'user'\n# layer-guard-start\nlayer stuff\n# layer-guard-end\n";
        let result = strip_guard_block(content);
        assert!(result.contains("echo 'user'"));
        assert!(!result.contains("layer-guard-start"));
        assert!(!result.contains("layer stuff"));
    }

    #[test]
    fn strip_guard_block_only_shebang_left() {
        let content = "#!/bin/sh\n# layer-guard-start\nlayer stuff\n# layer-guard-end\n";
        let result = strip_guard_block(content);
        assert_eq!(result.trim(), "#!/bin/sh");
    }

    #[test]
    fn manual_shell_snippet_uses_guard_check() {
        let snippet = render_manual_shell_snippet(std::path::Path::new("/tmp/layer"));
        assert!(snippet.contains("guard --check"));
        assert!(!snippet.contains(GUARD_START));
    }

    #[test]
    fn repairable_local_overwrite_requires_local_foreign_hook_and_preserved_original() {
        let (_tmp, ctx) = init_repo();
        let hook = ctx.root.join(".git").join("hooks").join("pre-commit");
        let preserved = hook.with_file_name("pre-commit.layer-original");
        std::fs::create_dir_all(hook.parent().unwrap()).expect("failed to create hook dir");

        std::fs::write(&hook, "#!/bin/sh\nexit 0\n").expect("failed to write foreign hook");
        assert!(matches!(
            inspect(&ctx).expect("inspection should succeed").health,
            GuardHealth::NeedsInstallLocal { .. }
        ));

        write_guard_intent(
            &ctx,
            GuardIntent::Wrapper {
                framework: HookFramework::Unknown,
            },
        )
        .expect("failed to write guard intent");
        std::fs::write(&preserved, "#!/bin/sh\necho old\n").expect("failed to write preserved");
        assert!(matches!(
            inspect(&ctx).expect("inspection should succeed").health,
            GuardHealth::NeedsRepairLocal { .. }
        ));
    }

    #[test]
    fn commented_out_guard_check_is_not_detected_as_active() {
        let (_tmp, ctx) = init_repo();
        let hook = ctx.root.join(".git").join("hooks").join("pre-commit");
        std::fs::create_dir_all(hook.parent().unwrap()).expect("failed to create hook dir");

        std::fs::write(
            &hook,
            "#!/bin/sh\n# layer guard --check || exit $?\necho 'other stuff'\n",
        )
        .expect("failed to write hook");

        let inspection = inspect(&ctx).expect("inspection should succeed");
        assert!(
            matches!(inspection.health, GuardHealth::NeedsInstallLocal { .. }),
            "expected NeedsInstallLocal for commented-out guard, got {:?}",
            inspection.health,
        );
    }

    #[test]
    fn manual_detection_works_without_marker_comment() {
        let (_tmp, ctx) = init_repo();
        let hook = ctx.root.join(".git").join("hooks").join("pre-commit");
        std::fs::create_dir_all(hook.parent().unwrap()).expect("failed to create hook dir");

        std::fs::write(
            &hook,
            "#!/bin/sh\nlayer guard --check || exit $?\necho 'other stuff'\n",
        )
        .expect("failed to write hook");

        let inspection = inspect(&ctx).expect("inspection should succeed");
        assert!(
            matches!(inspection.health, GuardHealth::ActiveManual { .. }),
            "expected ActiveManual, got {:?}",
            inspection.health,
        );
    }

    #[test]
    fn lefthook_config_detection_finds_guard_in_config() {
        let (_tmp, ctx) = init_repo();

        Command::new("git")
            .args(["config", "core.hooksPath", ".githooks"])
            .current_dir(&ctx.root)
            .output()
            .expect("failed to set hooksPath");

        std::fs::write(
            ctx.root.join("lefthook.yml"),
            "pre-commit:\n  commands:\n    layer-guard:\n      run: layer guard --check\n",
        )
        .expect("failed to write lefthook config");

        let hook_dir = ctx.root.join(".githooks");
        std::fs::create_dir_all(&hook_dir).expect("failed to create hook dir");
        std::fs::write(
            hook_dir.join("pre-commit"),
            "#!/bin/sh\ncall_lefthook \"pre-commit\" \"$@\"\n",
        )
        .expect("failed to write lefthook runner");

        let inspection = inspect(&ctx).expect("inspection should succeed");
        assert!(
            matches!(
                inspection.health,
                GuardHealth::ActiveManual {
                    framework: HookFramework::Lefthook,
                    ..
                }
            ),
            "expected ActiveManual with Lefthook, got {:?}",
            inspection.health,
        );
    }

    #[test]
    fn lefthook_config_with_guard_but_no_runner_stays_needs_manual() {
        let (_tmp, ctx) = init_repo();

        Command::new("git")
            .args(["config", "core.hooksPath", ".githooks"])
            .current_dir(&ctx.root)
            .output()
            .expect("failed to set hooksPath");

        std::fs::write(
            ctx.root.join("lefthook.yml"),
            "pre-commit:\n  commands:\n    layer-guard:\n      run: layer guard --check\n",
        )
        .expect("failed to write lefthook config");

        let inspection = inspect(&ctx).expect("inspection should succeed");
        assert!(
            matches!(
                inspection.health,
                GuardHealth::NeedsManualExternal {
                    framework: HookFramework::Lefthook
                }
            ),
            "expected NeedsManualExternal when runner is missing, got {:?}",
            inspection.health,
        );
    }

    #[test]
    fn lefthook_without_guard_in_config_stays_needs_manual() {
        let (_tmp, ctx) = init_repo();

        Command::new("git")
            .args(["config", "core.hooksPath", ".githooks"])
            .current_dir(&ctx.root)
            .output()
            .expect("failed to set hooksPath");

        std::fs::write(
            ctx.root.join("lefthook.yml"),
            "pre-commit:\n  commands:\n    lint:\n      run: eslint .\n",
        )
        .expect("failed to write lefthook config");

        let hook_dir = ctx.root.join(".githooks");
        std::fs::create_dir_all(&hook_dir).expect("failed to create hook dir");
        std::fs::write(
            hook_dir.join("pre-commit"),
            "#!/bin/sh\ncall_lefthook \"pre-commit\" \"$@\"\n",
        )
        .expect("failed to write lefthook runner");

        let inspection = inspect(&ctx).expect("inspection should succeed");
        assert!(
            matches!(
                inspection.health,
                GuardHealth::NeedsManualExternal {
                    framework: HookFramework::Lefthook
                }
            ),
            "expected NeedsManualExternal with Lefthook, got {:?}",
            inspection.health,
        );
    }

    #[test]
    fn remove_refuses_foreign_hook() {
        let (_tmp, ctx) = init_repo();
        let hook = ctx.root.join(".git").join("hooks").join("pre-commit");
        std::fs::create_dir_all(hook.parent().unwrap()).expect("failed to create hook dir");
        std::fs::write(&hook, "#!/bin/sh\nexit 0\n").expect("failed to write foreign hook");

        let err = remove(&ctx).expect_err("foreign hook should not be removed");
        assert!(err.to_string().contains("not managed by layer"));
        assert!(hook.exists());
    }
}
