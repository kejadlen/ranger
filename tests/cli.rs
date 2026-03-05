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
        .args(["task", "create", "Second task", "--tag", "urgent"])
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

    // Add a blocker
    let output = ranger(db_path)
        .args(["blocker", "add", &t2_key[..4], &t1_key[..4]])
        .output()
        .unwrap();
    assert!(output.status.success());

    // List tags
    let output = ranger(db_path).args(["tag", "list"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("urgent"));

    // Show task (JSON) — verify all data present
    let output = ranger(db_path)
        .args(["task", "show", &t2_key[..4], "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let detail: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(detail["task"]["title"], "Second task");
    assert_eq!(detail["tags"][0]["name"], "urgent");
    assert_eq!(detail["blockers"].as_array().unwrap().len(), 1);

    // Create a third task and use edit --before to reposition it
    let output = ranger(db_path)
        .args(["task", "create", "Third task", "--state", "queued"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = ranger(db_path)
        .args(["task", "list", "--json", "--state", "queued"])
        .output()
        .unwrap();
    let tasks_before_move: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let tasks_before_move = tasks_before_move.as_array().unwrap();
    // Third task should be after Second task (both queued, but Second was icebox — actually
    // let's just get the keys and verify edit --before works)
    let t3_key = tasks_before_move
        .iter()
        .find(|t| t["title"] == "Third task")
        .unwrap()["key"]
        .as_str()
        .unwrap()
        .to_string();

    // Edit Third task: change title AND reposition before First task
    let output = ranger(db_path)
        .args([
            "task",
            "edit",
            &t3_key[..4],
            "--title",
            "Third task (edited)",
            "--before",
            &t1_key[..4],
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Third task (edited)"));

    // Verify ordering: Third should now be before First
    let output = ranger(db_path)
        .args(["task", "list", "--json"])
        .output()
        .unwrap();
    let tasks_after_move: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let tasks_after_move = tasks_after_move.as_array().unwrap();
    let titles: Vec<&str> = tasks_after_move
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect();
    let third_pos = titles
        .iter()
        .position(|t| *t == "Third task (edited)")
        .unwrap();
    let first_pos = titles.iter().position(|t| *t == "First task").unwrap();
    assert!(
        third_pos < first_pos,
        "Third task should be before First task after edit --before, got: {:?}",
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
    assert_eq!(tasks.as_array().unwrap().len(), 2);
}
