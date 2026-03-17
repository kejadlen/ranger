use std::process::Command;

fn main() {
    // Release builds set RANGER_VERSION externally; don't override.
    if std::env::var("RANGER_VERSION").is_ok() {
        return;
    }

    let date = cmd("date", &["-u", "+%Y-%m-%d"]).unwrap_or("0000-00-00".into());
    let commit = cmd("git", &["rev-parse", "--short=8", "HEAD"]).unwrap_or("unknown".into());
    let version = format!("{date}-dev+{commit}");

    println!("cargo:rustc-env=RANGER_VERSION={version}");
}

fn cmd(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}
