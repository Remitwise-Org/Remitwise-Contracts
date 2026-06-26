use std::process::Command;
use std::path::PathBuf;

fn get_workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get parent of manifest dir")
        .to_path_buf()
}

fn get_cargo_deny_cmd() -> Option<Command> {
    // Check in common locations first
    let home = std::env::var("HOME").unwrap_or_default();
    let local_path = PathBuf::from(&home).join(".cargo/bin/cargo-deny");
    if local_path.exists() {
        return Some(Command::new(local_path));
    }

    // Try system path
    let output = if cfg!(target_os = "windows") {
        Command::new("where.exe").arg("cargo-deny").output()
    } else {
        Command::new("which").arg("cargo-deny").output()
    };

    if let Ok(out) = output {
        if out.status.success() {
            return Some(Command::new("cargo-deny"));
        }
    }

    // Try through cargo deny check
    let output = Command::new("cargo").arg("deny").arg("--version").output();
    if let Ok(out) = output {
        if out.status.success() {
            let mut cmd = Command::new("cargo");
            cmd.arg("deny");
            return Some(cmd);
        }
    }

    None
}

#[test]
fn test_workspace_passes_dependency_policy() {
    let mut cmd = match get_cargo_deny_cmd() {
        Some(cmd) => cmd,
        None => {
            println!("cargo-deny not found, skipping workspace check");
            return;
        }
    };

    let workspace_root = get_workspace_root();
    let config_path = workspace_root.join("deny.toml");
    let manifest_path = workspace_root.join("Cargo.toml");

    // Clear target dir env var if set to ensure clean run or inherit it
    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "/tmp/target".to_string());

    let status = cmd
        .arg("--config")
        .arg(&config_path)
        .arg("--manifest-path")
        .arg(&manifest_path)
        .arg("check")
        .env("CARGO_TARGET_DIR", &target_dir)
        .status()
        .expect("failed to execute cargo-deny");

    assert!(status.success(), "Workspace did not pass cargo-deny dependency check");
}

#[test]
fn test_gpl_dependency_rejected() {
    let mut cmd = match get_cargo_deny_cmd() {
        Some(cmd) => cmd,
        None => {
            println!("cargo-deny not found, skipping gpl rejection check");
            return;
        }
    };

    let workspace_root = get_workspace_root();
    let config_path = workspace_root.join("deny.toml");
    let fixture_manifest = workspace_root
        .join("testutils")
        .join("fixtures")
        .join("gpl_fixture")
        .join("Cargo.toml");

    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "/tmp/target".to_string());

    let output = cmd
        .arg("--config")
        .arg(&config_path)
        .arg("--manifest-path")
        .arg(&fixture_manifest)
        .arg("check")
        .arg("licenses")
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("failed to execute cargo-deny");

    // The validation MUST fail (exit status non-zero) because the fixture contains a GPL-licensed dependency
    assert!(!output.status.success(), "GPL dependency check should have failed, but it succeeded");
    
    let stderr_str = String::from_utf8_lossy(&output.stderr);
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    
    assert!(
        stderr_str.contains("rejected") || stdout_str.contains("rejected"),
        "Failure logs should indicate the GPL license was rejected. Stdout:\n{}\nStderr:\n{}",
        stdout_str,
        stderr_str
    );
}

#[test]
fn test_yanked_crate_rejected() {
    let mut cmd = match get_cargo_deny_cmd() {
        Some(cmd) => cmd,
        None => {
            println!("cargo-deny not found, skipping yanked crate rejection check");
            return;
        }
    };

    let workspace_root = get_workspace_root();
    let config_path = workspace_root.join("deny.toml");
    let fixture_manifest = workspace_root
        .join("testutils")
        .join("fixtures")
        .join("yanked_fixture")
        .join("Cargo.toml");

    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "/tmp/target".to_string());

    let output = cmd
        .arg("--config")
        .arg(&config_path)
        .arg("--manifest-path")
        .arg(&fixture_manifest)
        .arg("check")
        .arg("advisories")
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("failed to execute cargo-deny");

    // The validation MUST fail because the fixture depends on a yanked crate (serde 1.0.95)
    assert!(!output.status.success(), "Yanked crate check should have failed, but it succeeded");
    
    let stderr_str = String::from_utf8_lossy(&output.stderr);
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    
    assert!(
        stderr_str.contains("yanked") || stdout_str.contains("yanked"),
        "Failure logs should indicate a yanked crate was detected. Stdout:\n{}\nStderr:\n{}",
        stdout_str,
        stderr_str
    );
}
