use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn doctor_json_is_stable() {
    let output = Command::cargo_bin("kleinanzeigen")
        .expect("binary exists")
        .args(["doctor", "--json"])
        .output()
        .expect("command runs");

    assert!(output.status.success());

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
    assert!(value["provider"].is_string());
}

#[test]
fn doctor_human_output_is_readable() {
    Command::cargo_bin("kleinanzeigen")
        .expect("binary exists")
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("status: ok"))
        .stdout(predicate::str::contains("provider: not configured"));
}

#[test]
fn help_lists_doctor() {
    Command::cargo_bin("kleinanzeigen")
        .expect("binary exists")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("doctor"));
}

#[test]
fn unknown_command_exits_with_input_code() {
    Command::cargo_bin("kleinanzeigen")
        .expect("binary exists")
        .arg("nope")
        .assert()
        .failure()
        .code(2);
}
