use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use tempfile::tempdir;

fn ranger(db_path: &str) -> Command {
    let mut cmd = Command::from(cargo_bin_cmd!("ranger"));
    cmd.env("RANGER_DB", db_path);
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

    // List backlogs (JSON) and extract key
    let output = ranger(db_path)
        .args(["backlog", "list", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let backlogs: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap();
    let backlog_key = backlogs[0]["key"].as_str().unwrap().to_string();
    let bl_prefix = &backlog_key[..4];

    // Create tasks
    let output = ranger(db_path)
        .args(["task", "create", "First task", "--backlog", bl_prefix, "--state", "queued"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = ranger(db_path)
        .args(["task", "create", "Second task", "--backlog", bl_prefix, "--tag", "urgent"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // List tasks (JSON) and verify ordering
    let output = ranger(db_path)
        .args(["task", "list", "--backlog", bl_prefix, "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let tasks: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap();
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

    // Add a blocker
    let output = ranger(db_path)
        .args(["blocker", "add", &t2_key[..4], &t1_key[..4]])
        .output()
        .unwrap();
    assert!(output.status.success());

    // List tags
    let output = ranger(db_path)
        .args(["tag", "list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("urgent"));

    // Show task (JSON) — verify all data present
    let output = ranger(db_path)
        .args(["task", "show", &t2_key[..4], "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let detail: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(detail["task"]["title"], "Second task");
    assert_eq!(detail["tags"][0]["name"], "urgent");
    assert_eq!(detail["blockers"].as_array().unwrap().len(), 1);

    // Delete a task
    let output = ranger(db_path)
        .args(["task", "delete", &t2_key[..4]])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Verify deletion
    let output = ranger(db_path)
        .args(["task", "list", "--backlog", bl_prefix, "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let tasks: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(tasks.as_array().unwrap().len(), 1);
}
