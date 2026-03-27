use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn init_repo() -> TempDir {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");

    // Create an empty file so global excludes don't leak into tests.
    let empty_ignore = tmp.path().join(".test-global-ignore");
    fs::write(&empty_ignore, "").expect("failed to write empty ignore");

    Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Isolate from user's global git config.
    Command::new("git")
        .args([
            "config",
            "core.excludesFile",
            empty_ignore.to_str().unwrap(),
        ])
        .current_dir(tmp.path())
        .assert()
        .success();

    Command::new("git")
        .args(["config", "user.email", "layer@example.com"])
        .current_dir(tmp.path())
        .assert()
        .success();

    Command::new("git")
        .args(["config", "user.name", "Layer Test"])
        .current_dir(tmp.path())
        .assert()
        .success();

    tmp
}

fn exclude_path(repo: &Path) -> std::path::PathBuf {
    repo.join(".git").join("info").join("exclude")
}

#[test]
fn add_normalizes_and_dedupes() {
    let repo = init_repo();
    fs::create_dir(repo.path().join(".claude")).expect("failed to create .claude dir");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("layer"));
    cmd.current_dir(repo.path())
        .args(["add", "./CLAUDE.md", "./CLAUDE.md", ".claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Layered 'CLAUDE.md'"))
        .stdout(predicate::str::contains("already layered"))
        .stdout(predicate::str::contains("Layered '.claude/'"));

    let exclude =
        fs::read_to_string(exclude_path(repo.path())).expect("failed to read exclude file");
    assert!(exclude.contains("# managed by layer"));
    assert!(exclude.contains("CLAUDE.md"));
    assert!(exclude.contains(".claude/"));
}

#[test]
fn add_warns_when_file_is_tracked() {
    let repo = init_repo();
    fs::write(repo.path().join("CLAUDE.md"), "hello").expect("failed to write file");

    Command::new("git")
        .args(["add", "CLAUDE.md"])
        .current_dir(repo.path())
        .assert()
        .success();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("layer"));
    cmd.current_dir(repo.path())
        .args(["add", "CLAUDE.md"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tracked by Git"))
        .stdout(predicate::str::contains("git rm --cached CLAUDE.md"));
}

#[test]
fn rm_direct_removes_exact_and_reports_missing() {
    let repo = init_repo();
    let exclude = exclude_path(repo.path());
    fs::create_dir_all(exclude.parent().unwrap()).expect("failed to make info dir");
    fs::write(&exclude, "# managed by layer\nCLAUDE.md\n*.tmp\n").expect("failed to write exclude");

    let mut remove_cmd = Command::new(assert_cmd::cargo::cargo_bin!("layer"));
    remove_cmd
        .current_dir(repo.path())
        .args(["rm", "CLAUDE.md"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed 'CLAUDE.md'"));

    let mut missing_cmd = Command::new(assert_cmd::cargo::cargo_bin!("layer"));
    missing_cmd
        .current_dir(repo.path())
        .args(["rm", "not-there.md"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("'not-there.md' is not layered"));
}

#[test]
fn why_reports_excluded_and_tracked_state() {
    let repo = init_repo();
    fs::write(repo.path().join("config.md"), "cfg").expect("failed to write file");

    Command::new("git")
        .args(["add", "config.md"])
        .current_dir(repo.path())
        .assert()
        .success();

    let mut add_cmd = Command::new(assert_cmd::cargo::cargo_bin!("layer"));
    add_cmd
        .current_dir(repo.path())
        .args(["add", "config.md"])
        .assert()
        .success();

    let mut why_cmd = Command::new(assert_cmd::cargo::cargo_bin!("layer"));
    why_cmd
        .current_dir(repo.path())
        .args(["why", "config.md"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("exposed"))
        .stdout(predicate::str::contains("git rm --cached config.md"));
}

#[test]
fn why_verbose_prints_explanation_block() {
    let repo = init_repo();
    fs::write(repo.path().join("CLAUDE.md"), "notes").expect("failed to write file");

    let mut add_cmd = Command::new(assert_cmd::cargo::cargo_bin!("layer"));
    add_cmd
        .current_dir(repo.path())
        .args(["add", "CLAUDE.md"])
        .assert()
        .success();

    let mut why_cmd = Command::new(assert_cmd::cargo::cargo_bin!("layer"));
    why_cmd
        .current_dir(repo.path())
        .args(["why", "CLAUDE.md", "--verbose"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "How git decides to ignore files (checked in order):",
        ))
        .stdout(predicate::str::contains(
            "A file must not be tracked for any ignore rule to take effect.",
        ));
}

// --- ls integration tests ---

#[test]
fn ls_empty_exclude_shows_hint() {
    let repo = init_repo();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("ls")
        .assert()
        .code(2)
        .stdout(predicate::str::contains("No layered entries"));
}

#[test]
fn ls_shows_existing_entries_with_status() {
    let repo = init_repo();
    fs::write(repo.path().join("CLAUDE.md"), "notes").expect("write");

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "CLAUDE.md", "nonexistent.md"])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("ls")
        .assert()
        .success()
        .stdout(predicate::str::contains("CLAUDE.md"))
        .stdout(predicate::str::contains("layered"))
        .stdout(predicate::str::contains("nonexistent.md"))
        .stdout(predicate::str::contains("stale"));
}

#[test]
fn ls_shows_tracked_warning() {
    let repo = init_repo();
    fs::write(repo.path().join("config.md"), "cfg").expect("write");

    Command::new("git")
        .args(["add", "config.md"])
        .current_dir(repo.path())
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "config.md"])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("ls")
        .assert()
        .success()
        .stdout(predicate::str::contains("config.md"))
        .stdout(predicate::str::contains("exposed"));
}

// --- doctor integration tests ---

#[test]
fn doctor_empty_shows_hint() {
    let repo = init_repo();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("doctor")
        .assert()
        .code(2)
        .stdout(predicate::str::contains("No layered entries"));
}

#[test]
fn doctor_healthy_entry() {
    let repo = init_repo();
    fs::write(repo.path().join("CLAUDE.md"), "notes").expect("write");

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "CLAUDE.md"])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("layered"))
        .stdout(predicate::str::contains("1 layered"));
}

#[test]
fn doctor_stale_entry() {
    let repo = init_repo();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "gone.md"])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("doctor")
        .assert()
        .code(1)
        .stdout(predicate::str::contains("stale"))
        .stdout(predicate::str::contains("1 stale"));
}

#[test]
fn doctor_tracked_entry() {
    let repo = init_repo();
    fs::write(repo.path().join("tracked.md"), "x").expect("write");

    Command::new("git")
        .args(["add", "tracked.md"])
        .current_dir(repo.path())
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "tracked.md"])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("doctor")
        .assert()
        .code(1)
        .stdout(predicate::str::contains("exposed"))
        .stdout(predicate::str::contains("1 exposed"));
}

// --- scan integration tests ---

#[test]
fn scan_no_ai_files() {
    let repo = init_repo();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("scan")
        .assert()
        .code(2)
        .stdout(predicate::str::contains("No context files found"));
}

#[test]
fn scan_finds_ai_files() {
    let repo = init_repo();
    fs::write(repo.path().join("CLAUDE.md"), "notes").expect("write");
    fs::write(repo.path().join(".cursorrules"), "rules").expect("write");

    let output = Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("scan")
        .output()
        .expect("failed to run scan");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CLAUDE.md"), "should find CLAUDE.md");
    assert!(stdout.contains(".cursorrules"), "should find .cursorrules");
}

// --- clean integration test ---

#[test]
fn clean_dry_run_shows_stale() {
    let repo = init_repo();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "gone.md"])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["clean", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Would remove 1 stale entries"))
        .stdout(predicate::str::contains("gone.md"))
        .stdout(predicate::str::contains("dry run"));
}

// --- rm dry-run integration test ---

#[test]
fn rm_dry_run_does_not_modify_file() {
    let repo = init_repo();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "CLAUDE.md"])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["rm", "--dry-run", "CLAUDE.md"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Would remove 'CLAUDE.md'"))
        .stdout(predicate::str::contains("dry run"));

    // Verify file was NOT modified
    let content = fs::read_to_string(exclude_path(repo.path())).expect("read");
    assert!(
        content.contains("CLAUDE.md"),
        "entry should still be present after dry run"
    );
}

// --- section-based ownership tests ---

#[test]
fn add_preserves_user_entries_in_exclude() {
    let repo = init_repo();
    let exclude = exclude_path(repo.path());
    fs::create_dir_all(exclude.parent().unwrap()).expect("mkdir");
    // Pre-populate with user-owned entries (no layer section)
    fs::write(&exclude, "# my custom excludes\nmy-notes.txt\n").expect("write");

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "CLAUDE.md"])
        .assert()
        .success();

    let content = fs::read_to_string(&exclude).expect("read");
    // User entry preserved in prefix
    assert!(
        content.contains("my-notes.txt"),
        "user entry should be preserved"
    );
    assert!(
        content.contains("# my custom excludes"),
        "user comment should be preserved"
    );
    // Section markers present
    assert!(
        content.contains("# managed by layer"),
        "start marker should be present"
    );
    assert!(
        content.contains("# end layer"),
        "end marker should be present"
    );
    // layer entry added
    assert!(
        content.contains("CLAUDE.md"),
        "layer entry should be present"
    );
}

#[test]
fn clear_preserves_user_entries() {
    let repo = init_repo();
    let exclude = exclude_path(repo.path());
    fs::create_dir_all(exclude.parent().unwrap()).expect("mkdir");
    fs::write(
        &exclude,
        "my-notes.txt\n# managed by layer\nCLAUDE.md\n# end layer\n",
    )
    .expect("write");

    // clear requires TTY confirmation — use dry-run to test the count
    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["clear", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Would remove all 1 entries"));

    // Verify file was NOT modified
    let content = fs::read_to_string(&exclude).expect("read");
    assert!(
        content.contains("my-notes.txt"),
        "user entry should still be present"
    );
}

#[test]
fn ls_shows_manual_entries() {
    let repo = init_repo();
    let exclude = exclude_path(repo.path());
    fs::create_dir_all(exclude.parent().unwrap()).expect("mkdir");
    fs::write(repo.path().join("CLAUDE.md"), "notes").expect("write file");
    fs::write(
        &exclude,
        "my-notes.txt\n# managed by layer\nCLAUDE.md\n# end layer\n",
    )
    .expect("write");

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("ls")
        .assert()
        .success()
        .stdout(predicate::str::contains("CLAUDE.md"))
        .stdout(predicate::str::contains("layered"))
        .stdout(predicate::str::contains("my-notes.txt"))
        .stdout(predicate::str::contains("(manual)"));
}

// --- clean --all integration test ---

#[test]
fn clean_all_dry_run_shows_user_stale() {
    let repo = init_repo();
    let exclude = exclude_path(repo.path());
    fs::create_dir_all(exclude.parent().unwrap()).expect("mkdir");
    // User entry "gone-user.md" doesn't exist on disk → stale
    // Managed entry "gone-managed.md" doesn't exist on disk → stale
    fs::write(
        &exclude,
        "gone-user.md\n# managed by layer\ngone-managed.md\n# end layer\n",
    )
    .expect("write");

    // Without --all, only managed stale entries shown
    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["clean", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Would remove 1 stale entries"))
        .stdout(predicate::str::contains("gone-managed.md"))
        .stdout(predicate::str::contains("gone-user.md").not());

    // With --all, both managed and user stale entries shown
    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["clean", "--all", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Would remove 2 stale entries"))
        .stdout(predicate::str::contains("gone-managed.md"))
        .stdout(predicate::str::contains("gone-user.md"))
        .stdout(predicate::str::contains("(manual)"));
}

// --- backup/restore integration tests ---

#[test]
fn backup_creates_file_and_restore_list_shows_it() {
    let repo = init_repo();
    fs::write(repo.path().join("CLAUDE.md"), "notes").expect("write");

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "CLAUDE.md"])
        .assert()
        .success();

    // Use isolated HOME so backup goes to temp dir, not user's real backups
    let backup_home = tempfile::tempdir().expect("backup home");

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .env("HOME", backup_home.path())
        .current_dir(repo.path())
        .arg("backup")
        .assert()
        .success()
        .stdout(predicate::str::contains("Backed up 1 entries"));

    // Verify backup directory was created
    let backup_dir = backup_home.path().join(".layer-backups");
    assert!(backup_dir.exists(), "backup dir should exist");

    // restore --list should show the backup
    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .env("HOME", backup_home.path())
        .current_dir(repo.path())
        .args(["restore", "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 entries"));
}

// --- add dry-run integration test ---

// --- off/on integration tests ---

#[test]
fn off_disables_all_entries() {
    let repo = init_repo();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "CLAUDE.md", "Agents.md"])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("off")
        .assert()
        .success()
        .stdout(predicate::str::contains("Disabled CLAUDE.md"))
        .stdout(predicate::str::contains("Disabled Agents.md"));

    let content = fs::read_to_string(exclude_path(repo.path())).expect("read");
    assert!(content.contains("# [off] CLAUDE.md"));
    assert!(content.contains("# [off] Agents.md"));
}

#[test]
fn on_enables_all_entries() {
    let repo = init_repo();
    let exclude = exclude_path(repo.path());
    fs::create_dir_all(exclude.parent().unwrap()).expect("mkdir");
    fs::write(
        &exclude,
        "# managed by layer\n# [off] CLAUDE.md\n# [off] Agents.md\n# end layer\n",
    )
    .expect("write");

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("on")
        .assert()
        .success()
        .stdout(predicate::str::contains("Enabled CLAUDE.md"))
        .stdout(predicate::str::contains("Enabled Agents.md"));

    let content = fs::read_to_string(&exclude).expect("read");
    assert!(!content.contains("# [off]"));
    assert!(content.contains("CLAUDE.md"));
    assert!(content.contains("Agents.md"));
}

#[test]
fn off_specific_entry() {
    let repo = init_repo();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "CLAUDE.md", "Agents.md"])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["off", "CLAUDE.md"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Disabled CLAUDE.md"));

    let content = fs::read_to_string(exclude_path(repo.path())).expect("read");
    assert!(content.contains("# [off] CLAUDE.md"));
    assert!(content.contains("\nAgents.md\n"));
}

#[test]
fn on_specific_entry() {
    let repo = init_repo();
    let exclude = exclude_path(repo.path());
    fs::create_dir_all(exclude.parent().unwrap()).expect("mkdir");
    fs::write(
        &exclude,
        "# managed by layer\n# [off] CLAUDE.md\n# [off] Agents.md\n# end layer\n",
    )
    .expect("write");

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["on", "CLAUDE.md"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Enabled CLAUDE.md"));

    let content = fs::read_to_string(&exclude).expect("read");
    assert!(content.contains("CLAUDE.md"));
    assert!(content.contains("# [off] Agents.md"));
}

#[test]
fn off_nothing_to_disable_exits_2() {
    let repo = init_repo();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("off")
        .assert()
        .code(2)
        .stdout(predicate::str::contains("No active entries to disable"));
}

#[test]
fn on_nothing_to_enable_exits_2() {
    let repo = init_repo();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("on")
        .assert()
        .code(2)
        .stdout(predicate::str::contains("No disabled entries to enable"));
}

#[test]
fn off_dry_run_does_not_modify() {
    let repo = init_repo();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "CLAUDE.md"])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["off", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Would disable CLAUDE.md"))
        .stdout(predicate::str::contains("dry run"));

    let content = fs::read_to_string(exclude_path(repo.path())).expect("read");
    assert!(
        !content.contains("# [off]"),
        "file should not be modified after dry run"
    );
}

#[test]
fn on_dry_run_does_not_modify() {
    let repo = init_repo();
    let exclude = exclude_path(repo.path());
    fs::create_dir_all(exclude.parent().unwrap()).expect("mkdir");
    fs::write(
        &exclude,
        "# managed by layer\n# [off] CLAUDE.md\n# end layer\n",
    )
    .expect("write");

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["on", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Would enable CLAUDE.md"))
        .stdout(predicate::str::contains("dry run"));

    let content = fs::read_to_string(&exclude).expect("read");
    assert!(
        content.contains("# [off] CLAUDE.md"),
        "file should not be modified after dry run"
    );
}

#[test]
fn ls_shows_disabled_entries() {
    let repo = init_repo();
    fs::write(repo.path().join("CLAUDE.md"), "notes").expect("write");
    let exclude = exclude_path(repo.path());
    fs::create_dir_all(exclude.parent().unwrap()).expect("mkdir");
    fs::write(
        &exclude,
        "# managed by layer\nCLAUDE.md\n# [off] Agents.md\n# end layer\n",
    )
    .expect("write");

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("ls")
        .assert()
        .success()
        .stdout(predicate::str::contains("CLAUDE.md"))
        .stdout(predicate::str::contains("layered"))
        .stdout(predicate::str::contains("Agents.md"))
        .stdout(predicate::str::contains("(disabled)"));
}

#[test]
fn status_does_not_show_disabled_entries_as_discovered() {
    let repo = init_repo();
    fs::write(repo.path().join("CLAUDE.md"), "notes").expect("write");

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "CLAUDE.md"])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("off")
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("1 disabled"))
        .stdout(predicate::str::contains("Discovered").not())
        .stdout(predicate::str::contains("layer add CLAUDE.md").not());
}

#[test]
fn status_off_state_hides_preview_details() {
    let repo = init_repo();
    fs::create_dir_all(repo.path().join("docs")).expect("mkdir");
    fs::write(repo.path().join("docs").join("tracked.md"), "tracked").expect("write");
    fs::write(repo.path().join("CLAUDE.md"), "notes").expect("write");

    Command::new("git")
        .args(["add", "docs/tracked.md"])
        .current_dir(repo.path())
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "CLAUDE.md", "docs/"])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("off")
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Layering is off"))
        .stdout(predicate::str::contains("Disabled (2):"))
        .stdout(predicate::str::contains("CLAUDE.md"))
        .stdout(predicate::str::contains("docs/"))
        .stdout(predicate::str::contains("Preview after layer on:").not())
        .stdout(predicate::str::contains("Exposed (1):").not())
        .stdout(predicate::str::contains("git rm --cached docs/tracked.md").not())
        .stdout(predicate::str::contains("All clear").not());
}

#[test]
fn roundtrip_off_then_on() {
    let repo = init_repo();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "CLAUDE.md", "Agents.md"])
        .assert()
        .success();

    let before = fs::read_to_string(exclude_path(repo.path())).expect("read");

    // Disable all
    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("off")
        .assert()
        .success();

    // Re-enable all
    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .arg("on")
        .assert()
        .success();

    let after = fs::read_to_string(exclude_path(repo.path())).expect("read");
    assert_eq!(before, after, "roundtrip should restore original file");
}

// --- add dry-run integration test ---

#[test]
fn add_dry_run_does_not_write() {
    let repo = init_repo();

    Command::new(assert_cmd::cargo::cargo_bin!("layer"))
        .current_dir(repo.path())
        .args(["add", "--dry-run", "CLAUDE.md"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Would layer 'CLAUDE.md'"))
        .stdout(predicate::str::contains("dry run"));

    // Verify file was NOT created with the entry
    let exclude = exclude_path(repo.path());
    if exclude.exists() {
        let content = fs::read_to_string(&exclude).expect("read");
        assert!(
            !content.contains("CLAUDE.md"),
            "entry should not be present after dry run"
        );
    }
}
