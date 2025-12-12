use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_binary_runs_help() {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));

    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "A lightning-fast toy package manager",
        ));
}

#[test]
fn test_binary_fails_invalid_command() {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));

    cmd.arg("not-a-command")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[test]
fn test_binary_version() {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));

    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("rush 0.1.0"));
}
