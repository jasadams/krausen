// SPDX-License-Identifier: AGPL-3.0-or-later
//! The server loads its shared constants at startup, before anything else needs
//! them: a malformed constants file fails the boot with a clear message instead
//! of letting the process come up and panic on the first request.

use std::process::Command;

#[test]
fn malformed_constants_file_fails_the_boot() {
    let path = std::env::temp_dir().join(format!("constants-{}.toml", uuid::Uuid::new_v4()));
    std::fs::write(&path, "this is = not [valid toml").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_krausen"))
        .env("SHARED_CONSTANTS_PATH", &path)
        .env("DATABASE_URL", "sqlite::memory:")
        .env("JWT_SECRET", "test-secret-at-least-32-bytes-long")
        .env_remove("RUST_LOG")
        .output()
        .expect("run the server binary");
    std::fs::remove_file(&path).unwrap();
    assert!(!output.status.success(), "boot must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to load shared constants"),
        "stderr: {stderr}"
    );
}
