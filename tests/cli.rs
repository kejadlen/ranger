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
}
