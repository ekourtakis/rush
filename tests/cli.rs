mod common;
use assert_cmd::Command;
use common::MockEnvironment;
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

    // Dynamically get the version from Cargo.toml at compile time
    let expected_version = format!("rush {}", env!("CARGO_PKG_VERSION"));

    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(expected_version));
}

#[test]
fn test_clean_command_runs() {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));

    // We just want to ensure it runs successfully (exit code 0)
    // We don't necessarily need to mock the filesystem here since core::tests covers that.
    cmd.arg("clean")
        .assert()
        .success()
        .stdout(predicate::str::contains("No trash found").or(predicate::str::contains("Cleaned")));
}

#[test]
fn test_security_checksum_mismatch() {
    let mock = MockEnvironment::new();
    // Create a package where the TOML hash does NOT match the tarball content
    mock.add_malicious_package("bad-pkg", "6.6.6", "malware");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    cmd.envs(mock.envs());

    // 1. Update (should succeed, we don't check hashes on update)
    cmd.args(&["update"]).assert().success();

    // 2. Install (MUST FAIL)
    let mut install_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    install_cmd.envs(mock.envs());

    install_cmd
        .args(&["install", "bad-pkg"])
        .assert()
        // We expect it to print an Error and likely return non-zero?
        // Note: Your main() returns Result<()>, so anyhow will print Error: ... and exit 1
        .failure()
        .stdout(predicate::str::contains(
            "Error: Security check failed: Checksum mismatch",
        ));

    // 3. Verify it was NOT installed
    assert!(!mock.home.join(".local/bin/malware").exists());
}

#[test]
fn test_stress_registry_performance() {
    let mock = MockEnvironment::new();

    // Generate 50 packages (Small stress test for CI speed)
    // In a real stress scenario, you might bump this to 1000
    for i in 0..50 {
        let name = format!("tool-{}", i);
        mock.add_package(&name, "1.0.0", "tool");
    }

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    cmd.envs(mock.envs());

    // Update should handle 50 files efficiently
    cmd.args(&["update"]).assert().success();

    // Search should list them all
    let mut search_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    search_cmd.envs(mock.envs());

    search_cmd
        .args(&["search"])
        .assert()
        .success()
        // Spot check first and last
        .stdout(predicate::str::contains("tool-0"))
        .stdout(predicate::str::contains("tool-49"));
}
