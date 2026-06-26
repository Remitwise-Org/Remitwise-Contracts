use std::fs;
use std::path::PathBuf;

fn get_workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get parent of manifest dir")
        .to_path_buf()
}

fn check_lockfile(lockfile_path: &std::path::Path, expected_version: &str) -> Result<(), String> {
    let content =
        fs::read_to_string(lockfile_path).map_err(|e| format!("Failed to read lockfile: {}", e))?;

    // Normalize CRLF to LF
    let content = content.replace("\r\n", "\n");

    let packages = content.split("[[package]]");
    let mut found = false;
    for pkg in packages {
        let lines = pkg.lines().map(|l| l.trim());
        let mut is_soroban_sdk = false;
        let mut version = None;
        for line in lines {
            if line.starts_with("name = \"soroban-sdk\"") {
                is_soroban_sdk = true;
            } else if line.starts_with("version = ") {
                version = Some(line.trim_start_matches("version = ").trim_matches('"'));
            }
        }
        if is_soroban_sdk {
            found = true;
            if let Some(ver) = version {
                if ver != expected_version {
                    return Err(format!(
                        "soroban-sdk version in Cargo.lock is '{}', but expected '{}'.",
                        ver, expected_version
                    ));
                }
            } else {
                return Err(
                    "soroban-sdk package entry in Cargo.lock is missing a version.".to_string(),
                );
            }
        }
    }

    if !found {
        return Err("soroban-sdk package entry not found in Cargo.lock.".to_string());
    }

    Ok(())
}

#[test]
fn test_lockfile_expected_soroban_sdk_version() {
    let workspace_root = get_workspace_root();
    let lockfile_path = workspace_root.join("Cargo.lock");
    let expected = "21.7.7";

    assert!(
        check_lockfile(&lockfile_path, expected).is_ok(),
        "Happy path: expected soroban-sdk version 21.7.7 should pass"
    );
}

#[test]
fn test_lockfile_unexpected_soroban_sdk_version_fails() {
    let mock_lockfile_content = r#"
[[package]]
name = "soroban-sdk"
version = "22.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "7dcdf04484af7cc731a7a48ad1d9f5f940370edeea84734434ceaf398a6b862e"
"#;

    let temp_dir = std::env::temp_dir();
    let mock_path = temp_dir.join("mock_Cargo.lock");
    fs::write(&mock_path, mock_lockfile_content).unwrap();

    let result = check_lockfile(&mock_path, "21.7.7");

    let _ = fs::remove_file(&mock_path);

    assert!(
        result.is_err(),
        "Sad path: unexpected soroban-sdk version 22.0.0 should fail validation"
    );
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("expected '21.7.7'"),
        "Error message should contain actionable details: {}",
        err_msg
    );
}
