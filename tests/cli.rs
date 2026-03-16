use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::tempdir;

fn ranger(db_path: &str) -> Command {
    let mut cmd = Command::from(cargo_bin_cmd!("ranger"));
    cmd.env("RANGER_DB", db_path);
    cmd.env("RANGER_DEFAULT_BACKLOG", "Ranger");
    cmd
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

    // Create tasks (using RANGER_DEFAULT_BACKLOG)
    let output = ranger(db_path)
        .args(["task", "create", "First task", "--state", "queued"])
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

    // Create two queued tasks and use edit --before to reposition within the same state
    let output = ranger(db_path)
        .args(["task", "create", "Third task", "--state", "queued"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = ranger(db_path)
        .args(["task", "create", "Fourth task", "--state", "queued"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = ranger(db_path)
        .args(["task", "list", "--json", "--state", "queued"])
        .output()
        .unwrap();
    let queued_tasks: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let queued_tasks = queued_tasks.as_array().unwrap();
    let t3_key = queued_tasks
        .iter()
        .find(|t| t["title"] == "Third task")
        .unwrap()["key"]
        .as_str()
        .unwrap()
        .to_string();
    let t4_key = queued_tasks
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

    // Verify ordering within queued: Fourth should now be before Third
    let output = ranger(db_path)
        .args(["task", "list", "--json", "--state", "queued"])
        .output()
        .unwrap();
    let queued_after: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let queued_after = queued_after.as_array().unwrap();
    let titles: Vec<&str> = queued_after
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
        .args(["task", "delete", &t2_key[..4]])
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

    // No-args with RANGER_DEFAULT_BACKLOG shows the default backlog
    let output = ranger(db_path).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Ranger"));
    // Should show task state sections
    assert!(
        stdout.contains("[in_progress]")
            || stdout.contains("[queued]")
            || stdout.contains("[icebox]")
    );

    // No-args with JSON flag
    let output = ranger(db_path).args(["--json"]).output().unwrap();
    assert!(output.status.success());
    let detail: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(detail["backlog"]["name"], "Ranger");

    // No-args without RANGER_DEFAULT_BACKLOG lists all backlogs
    let mut cmd = Command::from(cargo_bin_cmd!("ranger"));
    cmd.env("RANGER_DB", db_path);
    cmd.env_remove("RANGER_DEFAULT_BACKLOG");
    let output = cmd.output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Ranger"));

    // Mark a task as done for the --done test
    let output = ranger(db_path)
        .args(["task", "edit", &t1_key[..4], "--state", "done"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Backlog show hides done tasks by default
    let output = ranger(db_path).args(["backlog", "show"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        !stdout.contains("[done]"),
        "should not show done section by default"
    );

    // Backlog show --done shows only done tasks
    let output = ranger(db_path)
        .args(["backlog", "show", "--done"])
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
    assert!(
        !stdout.contains("[queued]"),
        "--done should not show queued"
    );
    assert!(
        !stdout.contains("[icebox]"),
        "--done should not show icebox"
    );

    // Backlog show --done with JSON shows only done tasks
    let output = ranger(db_path)
        .args(["backlog", "show", "--done", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let detail: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(detail["tasks"]["done"].is_array());
    assert!(detail["tasks"]["queued"].is_null());
    assert!(detail["tasks"]["in_progress"].is_null());

    // Backlog show JSON without --done excludes done tasks
    let output = ranger(db_path)
        .args(["backlog", "show", "--json"])
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
    assert_eq!(detail["tags"].as_array().unwrap().len(), 2);

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
        .args(["task", "create", "Doomed task", "--backlog", "Throwaway"])
        .output()
        .unwrap();
    let output = ranger(db_path)
        .args(["backlog", "delete", "Throwaway"])
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
        .args(["backlog", "delete", "Nonexistent"])
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
}
