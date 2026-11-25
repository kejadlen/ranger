use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::tempdir;

fn ranger(db_path: &str) -> Command {
    let mut cmd = Command::from(cargo_bin_cmd!("ranger"));
    cmd.env("RANGER_DB", db_path);
    cmd.env("RANGER_DEFAULT_FILTER", "project:Ranger");
    // Keep the developer's shell settings out of the tests.
    cmd.env_remove("RANGER_DEFAULT_BACKLOG");
    cmd
}

/// Write an executable stand-in for `$EDITOR` that overwrites the file it is
/// handed with `contents`, and return its path.
#[cfg(unix)]
fn editor_writing(dir: &std::path::Path, name: &str, contents: &str) -> String {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join(name);
    std::fs::write(
        &path,
        format!("#!/bin/sh\ncat > \"$1\" <<'RANGER_EOF'\n{contents}RANGER_EOF\n"),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_str().unwrap().to_string()
}

/// Write an executable stand-in for `$EDITOR` that leaves the file alone and
/// exits with `code`, and return its path.
#[cfg(unix)]
fn editor_exiting(dir: &std::path::Path, name: &str, code: u8) -> String {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\nexit {code}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_str().unwrap().to_string()
}

#[cfg(unix)]
fn task_detail(db_path: &str, key: &str) -> serde_json::Value {
    let output = ranger(db_path)
        .args(["task", "show", key, "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[cfg(unix)]
#[test]
fn edit_without_flags_opens_editor() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("test.db");
    let db_path = db.to_str().unwrap();

    ranger(db_path)
        .args(["backlog", "create", "Ranger"])
        .assert()
        .success();
    ranger(db_path)
        .args([
            "task",
            "create",
            "Original title",
            "--description",
            "Original description",
        ])
        .assert()
        .success();

    let output = ranger(db_path)
        .args(["task", "list", "--json"])
        .output()
        .unwrap();
    let tasks: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let key = tasks[0]["key"].as_str().unwrap().to_string();

    // Title on the first line, body below — blank separator and surrounding
    // blank lines are dropped, interior ones survive.
    let editor = editor_writing(
        dir.path(),
        "replace.sh",
        "\n  Edited title  \n\nFirst paragraph\n\nSecond paragraph\n\n\n",
    );
    ranger(db_path)
        .env("EDITOR", &editor)
        .args(["task", "edit", &key[..4]])
        .assert()
        .success()
        .stdout(predicates::str::contains("Edited title"));

    let detail = task_detail(db_path, &key[..4]);
    assert_eq!(detail["task"]["title"], "Edited title");
    assert_eq!(
        detail["task"]["description"],
        "First paragraph\n\nSecond paragraph"
    );

    // A body left empty clears the description.
    let editor = editor_writing(dir.path(), "title_only.sh", "Just a title\n");
    ranger(db_path)
        .env("EDITOR", &editor)
        .args(["task", "edit", &key[..4]])
        .assert()
        .success();

    let detail = task_detail(db_path, &key[..4]);
    assert_eq!(detail["task"]["title"], "Just a title");
    assert_eq!(detail["task"]["description"], "");

    // An entirely blank file leaves the task alone rather than wiping the title.
    let editor = editor_writing(dir.path(), "blank.sh", "\n   \n\n");
    ranger(db_path)
        .env("EDITOR", &editor)
        .args(["task", "edit", &key[..4]])
        .assert()
        .success();

    let detail = task_detail(db_path, &key[..4]);
    assert_eq!(detail["task"]["title"], "Just a title");

    // A failed editor aborts the edit.
    let editor = editor_exiting(dir.path(), "fail.sh", 1);
    ranger(db_path)
        .env("EDITOR", &editor)
        .args(["task", "edit", &key[..4]])
        .assert()
        .failure()
        .stderr(predicates::str::contains("fail.sh"));

    let detail = task_detail(db_path, &key[..4]);
    assert_eq!(detail["task"]["title"], "Just a title");

    // Without $EDITOR there is nothing to open.
    ranger(db_path)
        .env_remove("EDITOR")
        .args(["task", "edit", &key[..4]])
        .assert()
        .failure()
        .stderr(predicates::str::contains("EDITOR"));

    // Flags still take the non-interactive path — a failing editor is never run.
    let editor = editor_exiting(dir.path(), "unused.sh", 1);
    ranger(db_path)
        .env("EDITOR", &editor)
        .args(["task", "edit", &key[..4], "--title", "Set by flag"])
        .assert()
        .success();

    let detail = task_detail(db_path, &key[..4]);
    assert_eq!(detail["task"]["title"], "Set by flag");
}

#[test]
fn full_workflow() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("test.db");
    let db_path = db.to_str().unwrap();

    // Create a backlog
    let output = ranger(db_path)
        .args(["backlog", "create", "Ranger"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Ranger"));

    // List backlogs (JSON)
    let output = ranger(db_path)
        .args(["backlog", "list", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let backlogs: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(backlogs[0]["name"], "Ranger");

    // Create tasks (using RANGER_DEFAULT_FILTER)
    let output = ranger(db_path)
        .args(["task", "create", "First task", "--state", "ready"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = ranger(db_path)
        .args(["task", "create", "Second task"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // List tasks (JSON) and verify ordering
    let output = ranger(db_path)
        .args(["task", "list", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let tasks: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let tasks = tasks.as_array().unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["title"], "First task");
    assert_eq!(tasks[1]["title"], "Second task");

    let t1_key = tasks[0]["key"].as_str().unwrap().to_string();
    let t2_key = tasks[1]["key"].as_str().unwrap().to_string();

    // Edit task state
    let output = ranger(db_path)
        .args(["task", "edit", &t1_key[..4], "--state", "in_progress"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("in_progress"));

    // Add a comment
    let output = ranger(db_path)
        .args(["comment", "add", &t1_key[..4], "Started working on this"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // List comments
    let output = ranger(db_path)
        .args(["comment", "list", &t1_key[..4]])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Started working on this"));

    // Show task (JSON) — verify all data present
    let output = ranger(db_path)
        .args(["task", "show", &t2_key[..4], "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let detail: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(detail["task"]["title"], "Second task");

    // Create two ready tasks and use edit --before to reposition within the same state
    let output = ranger(db_path)
        .args(["task", "create", "Third task", "--state", "ready"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = ranger(db_path)
        .args(["task", "create", "Fourth task", "--state", "ready"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = ranger(db_path)
        .args(["task", "list", "--json", "--state", "ready"])
        .output()
        .unwrap();
    let ready_tasks: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let ready_tasks = ready_tasks.as_array().unwrap();
    let t3_key = ready_tasks
        .iter()
        .find(|t| t["title"] == "Third task")
        .unwrap()["key"]
        .as_str()
        .unwrap()
        .to_string();
    let t4_key = ready_tasks
        .iter()
        .find(|t| t["title"] == "Fourth task")
        .unwrap()["key"]
        .as_str()
        .unwrap()
        .to_string();

    // Edit Fourth task: change title AND reposition before Third task
    let output = ranger(db_path)
        .args([
            "task",
            "edit",
            &t4_key[..4],
            "--title",
            "Fourth task (edited)",
            "--before",
            &t3_key[..4],
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Fourth task (edited)"));

    // Verify ordering within ready: Fourth should now be before Third
    let output = ranger(db_path)
        .args(["task", "list", "--json", "--state", "ready"])
        .output()
        .unwrap();
    let ready_after: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let ready_after = ready_after.as_array().unwrap();
    let titles: Vec<&str> = ready_after
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect();
    let fourth_pos = titles
        .iter()
        .position(|t| *t == "Fourth task (edited)")
        .unwrap();
    let third_pos = titles.iter().position(|t| *t == "Third task").unwrap();
    assert!(
        fourth_pos < third_pos,
        "Fourth should be before Third after edit --before, got: {:?}",
        titles
    );

    // Delete a task
    let output = ranger(db_path)
        .args(["task", "delete", &t2_key[..4], "--yes"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Verify deletion
    let output = ranger(db_path)
        .args(["task", "list", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let tasks: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(tasks.as_array().unwrap().len(), 3);

    // Rebalance
    ranger(db_path)
        .args(["backlog", "rebalance"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Rebalanced"));

    // Verify ordering preserved after rebalance
    let output = ranger(db_path)
        .args(["task", "list", "--json"])
        .output()
        .unwrap();
    let tasks: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let titles: Vec<&str> = tasks
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect();
    // Fourth (edited) was moved before Third — ordering should survive rebalance
    assert!(
        titles.iter().position(|t| t.contains("Fourth")).unwrap()
            < titles.iter().position(|t| *t == "Third task").unwrap()
    );

    // Archive a task
    let output = ranger(db_path)
        .args(["task", "archive", &t1_key[..4]])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Archived"));

    // Archived task hidden from default list
    let output = ranger(db_path)
        .args(["task", "list", "--json"])
        .output()
        .unwrap();
    let tasks: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(tasks.as_array().unwrap().len(), 2);

    // Visible with --archived
    let output = ranger(db_path)
        .args(["task", "list", "--json", "--archived"])
        .output()
        .unwrap();
    let tasks: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(tasks.as_array().unwrap().len(), 3);

    // Unarchive
    let output = ranger(db_path)
        .args(["task", "unarchive", &t1_key[..4]])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Unarchived"));

    // Back in default list
    let output = ranger(db_path)
        .args(["task", "list", "--json"])
        .output()
        .unwrap();
    let tasks: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(tasks.as_array().unwrap().len(), 3);

    // No args lists the tasks the default filter matches.
    let output = ranger(db_path).args(["--json"]).output().unwrap();
    assert!(output.status.success());
    let listed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 3);

    // An empty default filter matches every task.
    let output = ranger(db_path)
        .env("RANGER_DEFAULT_FILTER", "")
        .args(["task", "list", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Mark a task as done for the --done test
    let output = ranger(db_path)
        .args(["task", "edit", &t1_key[..4], "--state", "done"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Show done task — should include done_at timestamp
    let output = ranger(db_path)
        .args(["task", "show", &t1_key[..4]])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("Done:"),
        "task show should display done_at for done tasks"
    );

    // JSON detail includes done_at
    let output = ranger(db_path)
        .args(["task", "show", &t1_key[..4], "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let detail: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        detail["task"]["done_at"].is_string(),
        "JSON should include done_at for done tasks"
    );

    // Backlog show hides done tasks by default
    let output = ranger(db_path)
        .args(["backlog", "show", "Ranger"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        !stdout.contains("[done]"),
        "should not show done section by default"
    );

    // Backlog show --done shows only done tasks
    let output = ranger(db_path)
        .args(["backlog", "show", "Ranger", "--done"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("[done]"),
        "should show done section with --done"
    );
    assert!(
        !stdout.contains("[in_progress]"),
        "--done should not show in_progress"
    );
    assert!(!stdout.contains("[ready]"), "--done should not show ready");
    assert!(
        !stdout.contains("[icebox]"),
        "--done should not show icebox"
    );

    // Backlog show --done with JSON shows only done tasks
    let output = ranger(db_path)
        .args(["backlog", "show", "Ranger", "--done", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let detail: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(detail["tasks"]["done"].is_array());
    assert!(detail["tasks"]["ready"].is_null());
    assert!(detail["tasks"]["in_progress"].is_null());

    // Backlog show JSON without --done excludes done tasks
    let output = ranger(db_path)
        .args(["backlog", "show", "Ranger", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let detail: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        detail["tasks"]["done"].is_null(),
        "JSON should exclude done without --done"
    );

    // --- Tags ---

    // Add a tag to a task
    let output = ranger(db_path)
        .args(["tag", "add", &t1_key[..4], "bug"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("bug"));

    // Add another tag
    ranger(db_path)
        .args(["tag", "add", &t1_key[..4], "frontend"])
        .output()
        .unwrap();

    // Show task includes tags
    let output = ranger(db_path)
        .args(["task", "show", &t1_key[..4]])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Tags:"));
    assert!(stdout.contains("bug"));
    assert!(stdout.contains("frontend"));

    // Show task JSON includes tags
    let output = ranger(db_path)
        .args(["task", "show", &t1_key[..4], "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let detail: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(detail["tags"].is_array());
    let tag_names: Vec<&str> = detail["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    // The backlog's project tag comes along with the two added by hand.
    assert_eq!(tag_names, ["bug", "frontend", "project:Ranger"]);

    // List all tags
    let output = ranger(db_path).args(["tag", "list"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("bug"));
    assert!(stdout.contains("frontend"));

    // Filter tasks by tag
    let output = ranger(db_path)
        .args(["task", "list", "--tag", "bug"])
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(output.status.success(), "tag filter failed: {stderr}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("First task"));

    // Filter by terms, including a negated one
    let output = ranger(db_path)
        .args(["task", "list", "--filter", "frontend -bug"])
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(output.status.success(), "term filter failed: {stderr}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("First task"));

    let output = ranger(db_path)
        .args(["task", "list", "--filter", "frontend -bug"])
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        stderr.trim(),
        "No tasks matching 'project:Ranger frontend -bug'."
    );

    // Creating under a filter adds its positive tags and first state
    let output = ranger(db_path)
        .args([
            "task",
            "--filter",
            "project:ranger -bug state:ready",
            "create",
            "Filtered task",
            "--json",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(output.status.success(), "filtered create failed: {stderr}");
    let created: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(created["state"], "ready");
    let output = ranger(db_path)
        .args(["task", "show", created["key"].as_str().unwrap(), "--json"])
        .output()
        .unwrap();
    let detail: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let tags: Vec<&str> = detail["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    // The filter's tag, plus the one mirroring the task's backlog.
    assert_eq!(tags, ["project:Ranger", "project:ranger"]);

    // --state and a state: term conflict, on create and list alike
    for cmd in [
        &[
            "task",
            "--filter",
            "state:ready",
            "create",
            "--state",
            "done",
            "X",
        ][..],
        &[
            "task",
            "list",
            "--filter",
            "-state:done",
            "--state",
            "ready",
        ][..],
    ] {
        let output = ranger(db_path).args(cmd).output().unwrap();
        assert!(!output.status.success(), "{cmd:?} should fail");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("--state can't be combined"), "{stderr}");
    }

    // An invalid term is an error
    let output = ranger(db_path)
        .args(["task", "list", "--filter", "state:someday"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("invalid state: 'someday'"), "{stderr}");

    // Filter by tag that no task has
    let output = ranger(db_path)
        .args(["task", "list", "--tag", "nonexistent"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.is_empty() || !stdout.contains("First task"));

    // Remove a tag
    let output = ranger(db_path)
        .args(["tag", "remove", &t1_key[..4], "bug"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Verify tag removed
    let output = ranger(db_path)
        .args(["task", "show", &t1_key[..4]])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("bug"));
    assert!(stdout.contains("frontend"));

    // --- Backlog delete ---

    // Create a throwaway backlog with a task, then delete it
    ranger(db_path)
        .args(["backlog", "create", "Throwaway"])
        .output()
        .unwrap();
    ranger(db_path)
        .args([
            "task",
            "create",
            "Doomed task",
            "--default-filter",
            "project:Throwaway",
        ])
        .output()
        .unwrap();
    let output = ranger(db_path)
        .args(["backlog", "delete", "Throwaway", "--yes"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Deleted backlog: Throwaway"));

    // Verify backlog is gone
    let output = ranger(db_path)
        .args(["backlog", "list", "--json"])
        .output()
        .unwrap();
    let backlogs: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let names: Vec<&str> = backlogs
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["name"].as_str().unwrap())
        .collect();
    assert!(!names.contains(&"Throwaway"));

    // Deleting non-existent backlog fails
    let output = ranger(db_path)
        .args(["backlog", "delete", "Nonexistent", "--yes"])
        .output()
        .unwrap();
    assert!(!output.status.success());

    // Dynamic shell completions via COMPLETE env var
    for shell in ["bash", "zsh", "fish", "elvish", "powershell"] {
        let output = ranger(db_path).env("COMPLETE", shell).output().unwrap();
        assert!(output.status.success(), "completions failed for {shell}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            !stdout.is_empty(),
            "completions registration empty for {shell}"
        );
    }

    // Dynamic completion of task keys
    let output = ranger(db_path)
        .env("COMPLETE", "fish")
        .args(["--", "ranger", "task", "show", ""])
        .output()
        .unwrap();
    assert!(output.status.success(), "task key completion failed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should include task keys with help text showing [state] and title
    assert!(
        stdout.contains("First task"),
        "task key completions should include task titles as help text, got: {stdout}"
    );

    // Dynamic completion of backlog names
    let output = ranger(db_path)
        .env("COMPLETE", "fish")
        .args(["--", "ranger", "backlog", "show", ""])
        .output()
        .unwrap();
    assert!(output.status.success(), "backlog name completion failed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("Ranger"),
        "backlog name completions should include backlog names"
    );

    // --version prints version string
    ranger(db_path)
        .args(["--version"])
        .assert()
        .success()
        .stdout(predicates::str::contains("ranger "))
        .stdout(predicates::str::is_match(r"ranger \S+").unwrap());
}

#[test]
fn empty_list_prints_note_on_stderr() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("test.db");
    let db_path = db.to_str().unwrap();

    // A backlog with no tasks exercises the task-list empty cases.
    ranger(db_path)
        .args(["backlog", "create", "Empty"])
        .assert()
        .success();

    // Plain empty: note on stderr, stdout stays clean for pipes.
    let output = ranger(db_path)
        .args(["task", "list", "--default-filter", "project:Empty"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout).unwrap().is_empty(),
        "stdout must stay clean when the list is empty"
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "No tasks matching 'project:Empty'."
    );

    // State filter: adjective form, with snake_case rendered as hyphenated.
    let output = ranger(db_path)
        .args([
            "task",
            "list",
            "--default-filter",
            "project:Empty",
            "--state",
            "ready",
        ])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "No ready tasks matching 'project:Empty'."
    );
    let output = ranger(db_path)
        .args([
            "task",
            "list",
            "--default-filter",
            "project:Empty",
            "--state",
            "in_progress",
        ])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "No in-progress tasks matching 'project:Empty'."
    );

    // Tag filter only.
    let output = ranger(db_path)
        .args([
            "task",
            "list",
            "--default-filter",
            "project:Empty",
            "--tag",
            "bug",
        ])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "No tasks tagged #bug matching 'project:Empty'."
    );

    // State + tag together.
    let output = ranger(db_path)
        .args([
            "task",
            "list",
            "--default-filter",
            "project:Empty",
            "--state",
            "ready",
            "--tag",
            "bug",
        ])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "No ready tasks tagged #bug matching 'project:Empty'."
    );

    // JSON mode keeps printing [] and emits no note.
    let output = ranger(db_path)
        .args([
            "task",
            "list",
            "--default-filter",
            "project:Empty",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "[]");
    assert!(String::from_utf8(output.stderr).unwrap().is_empty());

    // The all-tasks path (no backlog filter) on a DB whose only backlog is empty.
    let output = ranger(db_path)
        .env_remove("RANGER_DEFAULT_FILTER")
        .args(["task", "list"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "No tasks."
    );
}

#[test]
fn empty_lists_other_commands() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("test.db");
    let db_path = db.to_str().unwrap();

    ranger(db_path)
        .args(["backlog", "create", "Ranger"])
        .assert()
        .success();
    let t1 = ranger(db_path)
        .args(["task", "create", "Lonely task", "--json"])
        .output()
        .unwrap();
    let t1_key: String = serde_json::from_slice::<serde_json::Value>(&t1.stdout).unwrap()["key"]
        .as_str()
        .unwrap()
        .to_string();

    // comment list on a task with no comments.
    let output = ranger(db_path)
        .args(["comment", "list", &t1_key])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("No comments on task "), "got: {stderr}");

    // tag prune with no unused tags.
    let output = ranger(db_path).args(["tag", "prune"]).output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "No unused tags to remove."
    );

    // tag list with only the mirrored project tags.
    let output = ranger(db_path)
        .args(["tag", "list", "--all"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["project:Ranger"]);
}

#[cfg(unix)]
#[test]
fn keys_resolve_outside_the_default_backlog() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("test.db");
    let db_path = db.to_str().unwrap();

    for name in ["Ranger", "Other"] {
        ranger(db_path)
            .args(["backlog", "create", name])
            .assert()
            .success();
    }
    let output = ranger(db_path)
        .args([
            "task",
            "create",
            "Elsewhere",
            "--default-filter",
            "project:Other",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let task: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let key = task["key"].as_str().unwrap();

    // RANGER_DEFAULT_FILTER is Ranger, but the key still resolves.
    ranger(db_path)
        .args(["task", "edit", key, "--title", "Found"])
        .assert()
        .success();
    ranger(db_path)
        .args(["comment", "add", key, "hello"])
        .assert()
        .success();
    let detail = task_detail(db_path, key);
    assert_eq!(detail["task"]["title"], "Found");
}

#[test]
fn create_without_a_filter_adds_no_tags() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("test.db");
    let output = ranger(db.to_str().unwrap())
        .env_remove("RANGER_DEFAULT_FILTER")
        .args(["task", "create", "Loose", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let task: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let detail = task_detail(db.to_str().unwrap(), task["key"].as_str().unwrap());
    assert_eq!(detail["tags"], serde_json::json!([]));
}

#[test]
fn filters_narrow_the_default_filter() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("test.db");
    let db_path = db.to_str().unwrap();
    ranger(db_path)
        .args(["backlog", "create", "Other"])
        .assert()
        .success();
    ranger(db_path)
        .args([
            "--default-filter",
            "project:Other",
            "task",
            "create",
            "Here",
        ])
        .assert()
        .success();
    ranger(db_path)
        .args([
            "--default-filter",
            "project:Other",
            "task",
            "create",
            "Bug",
            "--filter",
            "bug",
        ])
        .assert()
        .success();
    let titles = |args: &[&str]| -> Vec<String> {
        let output = ranger(db_path).args(args).output().unwrap();
        let tasks: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        tasks
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["title"].as_str().unwrap().to_string())
            .collect()
    };
    let other = [
        "--default-filter",
        "project:Other",
        "task",
        "list",
        "--json",
    ];
    assert_eq!(titles(&other), ["Here", "Bug"]);
    // --filter narrows the default rather than replacing it.
    assert_eq!(
        titles(&[&other[..], &["--filter", "bug"]].concat()),
        ["Bug"]
    );
    // The default here is project:Ranger, which has no tasks.
    assert!(titles(&["task", "list", "--json", "--filter", "bug"]).is_empty());
}

#[test]
fn internal_skill_prints_the_guide_without_touching_the_db() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("test.db");
    let output = ranger(db.to_str().unwrap())
        .args(["__internal", "skill"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        include_str!("../src/bin/ranger/skill.md")
    );
    assert!(!db.exists());
}

#[test]
fn legacy_default_backlog_still_works_with_a_warning() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("test.db");
    let db_path = db.to_str().unwrap();
    ranger(db_path)
        .args(["backlog", "create", "Old"])
        .assert()
        .success();
    let legacy = |args: &[&str]| {
        ranger(db_path)
            .env_remove("RANGER_DEFAULT_FILTER")
            .env("RANGER_DEFAULT_BACKLOG", "Old")
            .args(args)
            .output()
            .unwrap()
    };

    let output = legacy(&["task", "create", "Still here", "--json"]);
    assert!(output.status.success());
    assert!(String::from_utf8(output.stderr).unwrap().contains(
        "warning: RANGER_DEFAULT_BACKLOG is deprecated; set RANGER_DEFAULT_FILTER=project:Old instead"
    ));

    let output = legacy(&["task", "list", "--json"]);
    let tasks: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(tasks[0]["title"], "Still here");
}
