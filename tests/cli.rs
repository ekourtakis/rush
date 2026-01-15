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
    cmd.args(["update"]).assert().success();

    // 2. Install (MUST FAIL)
    let mut install_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    install_cmd.envs(mock.envs());

    install_cmd
        .args(["install", "bad-pkg"])
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
    cmd.args(["update"]).assert().success();

    // Search should list them all
    let mut search_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    search_cmd.envs(mock.envs());

    search_cmd
        .args(["search"])
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
    cmd.args(["update"]).assert().success();

    let mut install_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    install_cmd.envs(mock.envs());
    install_cmd.args(["install", "my-tool"]).assert().success();

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
    update_cmd.args(["update"]).assert().success();

    // Now upgrade
    upgrade_cmd.args(["upgrade"]).assert().success().stdout(
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
        .args(["list"])
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
    cmd.args(["update"]).assert().success();

    let mut install_1 = Command::new(env!("CARGO_BIN_EXE_rush"));
    install_1.envs(mock.envs());
    install_1.args(["install", "pkg-a"]).assert().success();

    // Second install (Should succeed gracefully)
    let mut install_2 = Command::new(env!("CARGO_BIN_EXE_rush"));
    install_2.envs(mock.envs());
    install_2
        .args(["install", "pkg-a"])
        .assert()
        .success() // Should exit 0
        .stdout(predicate::str::contains("is already installed")); // Should warn
}

#[test]
fn test_full_install_lifecycle() {
    // 1. Setup Mock Environment
    let mock = MockEnvironment::new();
    mock.add_package("dummy-tool", "1.0.0", "dummy");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    cmd.envs(mock.envs());

    // 2. Update & Search
    cmd.args(["update"]).assert().success();

    let mut search_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    search_cmd.envs(mock.envs());
    search_cmd
        .args(["search"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dummy-tool"));

    // 3. Install
    let mut install_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    install_cmd.envs(mock.envs());
    install_cmd
        .args(["install", "dummy-tool"])
        .assert()
        .success();

    // 4. Verify Binary Execution
    let installed_bin = mock.home.join(".local/bin/dummy");
    assert!(
        installed_bin.exists(),
        "Binary was not installed to expected path"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&installed_bin).unwrap();
        let mode = metadata.permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "Binary is not executable");
    }

    // 5. List
    let mut list_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    list_cmd.envs(mock.envs());
    list_cmd
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dummy-tool"));

    // 6. Uninstall
    let mut uninstall_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    uninstall_cmd.envs(mock.envs());
    uninstall_cmd
        .args(["uninstall", "dummy-tool"])
        .assert()
        .success();

    // 7. Verify Removal
    assert!(!installed_bin.exists(), "Binary should have been deleted");

    let mut list_cmd_2 = Command::new(env!("CARGO_BIN_EXE_rush"));
    list_cmd_2.envs(mock.envs());
    list_cmd_2
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No packages installed"));
}

#[test]
fn test_dev_add_flow() {
    let mock = MockEnvironment::new();
    // We don't add a package initially; we want the CLI to create one.

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    cmd.envs(mock.envs());

    // Run 'rush dev add ...'
    // We use a file:// URL to pass the download check
    // We reuse the registry source path just to have a valid file to point to,
    // even though it's not a real tarball, the checksum logic just reads bytes.
    let dummy_file_path = mock.registry_source.join("dummy-source");
    std::fs::write(&dummy_file_path, "fake binary content").unwrap();
    let file_url = format!("file://{}", dummy_file_path.to_str().unwrap());

    cmd.args([
        "dev",
        "add",
        "new-tool",
        "1.0.0",
        "x86_64-linux",
        &file_url,
        "--bin",
        "tool-bin",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Added new-tool"));

    // Verify the file was created in the registry source
    let manifest_path = mock.registry_source.join("packages/n/new-tool.toml");
    assert!(manifest_path.exists(), "Manifest file was not created");

    let content = std::fs::read_to_string(manifest_path).unwrap();
    assert!(content.contains("version = \"1.0.0\""));
    assert!(content.contains("x86_64-linux"));
}

#[test]
fn test_completions_generation() {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));

    // We don't need a mock env for this, it's pure logic
    cmd.arg("completions")
        .arg("bash")
        .assert()
        .success()
        // Check for standard bash completion signature
        .stdout(predicate::str::contains("complete -F"));
}

#[test]
fn test_install_fails_on_broken_url() {
    let mock = MockEnvironment::new();

    // Add a package, but force the URL to point to a non-existent file
    // We use the internal helper to bypass the valid-file check usually done in add_package
    // (We construct the TOML manually here effectively)

    // 1. Create a dummy package entry manually
    let target_arch = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
    let bad_url = "file:///path/to/nowhere/ghost.tar.gz";

    let toml_content = format!(
        r#"
        version = "1.0.0"
        [targets.{target_arch}]
        url = "{bad_url}"
        bin = "ghost"
        sha256 = "fakehash"
        "#
    );

    let pkg_dir = mock.registry_source.join("packages/g");
    std::fs::create_dir_all(&pkg_dir).unwrap();
    std::fs::write(pkg_dir.join("ghost.toml"), toml_content).unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    cmd.envs(mock.envs());

    // 2. Update to pull in the broken definition
    cmd.args(["update"]).assert().success();

    // 3. Try to install
    let mut install_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    install_cmd.envs(mock.envs());

    install_cmd
        .args(["install", "ghost"])
        .assert()
        .failure() // Should exit 1
        .stdout(
            predicate::str::contains("No such file").or(predicate::str::contains("cannot find")),
        );
}

#[test]
fn test_recovers_from_corrupt_state() {
    let mock = MockEnvironment::new();

    // 1. Corrupt the installed.json file directly
    let state_path = mock.home.join(".local/share/rush/installed.json");
    std::fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    std::fs::write(&state_path, "{ broken_json: [ }").unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    cmd.envs(mock.envs());

    // 2. Run a command that reads state
    // It should NOT panic. It should just treat state as empty.
    cmd.args(["list"])
        .assert()
        .success() // Should still work
        .stdout(predicate::str::contains("No packages installed"));
}

#[test]
fn test_arch_mismatch() {
    let mock = MockEnvironment::new();

    // 1. Manually create a package ONLY for an architecture that is definitely NOT the host
    // We'll use a made-up architecture "fake-arch-os"
    let toml_content = r#"
        version = "1.0.0"
        description = "Tool for aliens only"
        [targets.fake-arch-os]
        url = "http://ignore.me"
        bin = "ignore"
        sha256 = "ignore"
    "#;

    let pkg_dir = mock.registry_source.join("packages/e");
    std::fs::create_dir_all(&pkg_dir).unwrap();
    std::fs::write(pkg_dir.join("exclusive-tool.toml"), toml_content).unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    cmd.envs(mock.envs());

    // 2. Update
    cmd.args(["update"]).assert().success();

    // 3. Search
    // Current behavior: It should NOT appear in the list because it filters by current target
    let mut search_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    search_cmd.envs(mock.envs());
    search_cmd
        .args(["search"])
        .assert()
        .success()
        .stdout(predicate::str::contains("exclusive-tool").not());

    // 4. Install
    // Should find the manifest, but realize no target matches
    let mut install_cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    install_cmd.envs(mock.envs());

    install_cmd
        .args(["install", "exclusive-tool"])
        .assert()
        .failure() // Should exit 1
        .stdout(predicate::str::contains("No compatible binary for"));
}

#[test]
fn test_dev_verify_failure() {
    let mock = MockEnvironment::new();
    // Add a package where content doesn't match the hash
    mock.add_malicious_package("bad-pkg", "1.0.0", "bin");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    cmd.envs(mock.envs());

    cmd.args(["dev", "verify"])
        .assert()
        .failure() // Must exit non-zero
        .stdout(predicate::str::contains("Verification failed"));
}

#[test]
fn test_dev_verify_success() {
    let mock = MockEnvironment::new();
    // Add a valid package
    mock.add_package("good-pkg", "1.0.0", "bin");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rush"));
    cmd.envs(mock.envs());

    cmd.args(["dev", "verify"])
        .assert()
        .success()
        .stdout(predicate::str::contains("All clean"));
}
