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

#[test]
fn test_upgrade_flow() {
    let mock = MockEnvironment::new();

    // 1. Publish v1.0.0 and install it
    mock.add_package("my-tool", "1.0.0", "tool");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    cmd.envs(mock.envs());

    // Install v1
    cmd.args(&["update"]).assert().success();

    let mut install_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    install_cmd.envs(mock.envs());
    install_cmd.args(&["install", "my-tool"]).assert().success();

    // Verify v1 output
    let tool_path = mock.home.join(".local/bin/tool");
    let output_v1 = std::process::Command::new(&tool_path).output().unwrap();
    assert!(
        String::from_utf8(output_v1.stdout)
            .unwrap()
            .contains("v1.0.0")
    );

    // 2. Publish v2.0.0 (Simulate registry update)
    // We overwrite the existing package definition in the "remote" source
    mock.add_package("my-tool", "2.0.0", "tool");

    // 3. Run Upgrade
    let mut upgrade_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    upgrade_cmd.envs(mock.envs());

    // We need to run update first to fetch the new TOML
    let mut update_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    update_cmd.envs(mock.envs());
    update_cmd.args(&["update"]).assert().success();

    // Now upgrade
    upgrade_cmd.args(&["upgrade"]).assert().success().stdout(
        predicate::str::contains("Upgrading").and(predicate::str::contains("v1.0.0 -> v2.0.0")),
    );

    // 4. Verify v2 output
    let output_v2 = std::process::Command::new(&tool_path).output().unwrap();
    assert!(
        String::from_utf8(output_v2.stdout)
            .unwrap()
            .contains("v2.0.0")
    );

    // 5. Verify State
    let mut list_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    list_cmd.envs(mock.envs());
    list_cmd
        .args(&["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("my-tool").and(predicate::str::contains("v2.0.0")));
}

#[test]
fn test_install_already_installed() {
    let mock = MockEnvironment::new();
    mock.add_package("pkg-a", "1.0.0", "bin-a");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    cmd.envs(mock.envs());

    // First install
    cmd.args(&["update"]).assert().success();

    let mut install_1 = Command::new(env!("CARGO_BIN_EXE_rush"));
    install_1.envs(mock.envs());
    install_1.args(&["install", "pkg-a"]).assert().success();

    // Second install (Should succeed gracefully)
    let mut install_2 = Command::new(env!("CARGO_BIN_EXE_rush"));
    install_2.envs(mock.envs());
    install_2
        .args(&["install", "pkg-a"])
        .assert()
        .success() // Should exit 0
        .stdout(predicate::str::contains("is already installed")); // Should warn
}
