// SPDX-License-Identifier: AGPL-3.0-or-later
fn main() {
    let rustc = std::env::var_os("RUSTC").expect("RUSTC");
    let output = std::process::Command::new(rustc).arg("--version").output().expect("rustc version");
    assert!(output.status.success(), "cannot identify compiler");
    let version = String::from_utf8(output.stdout).expect("utf8 rustc version");
    println!("cargo:rustc-env=KRAUSEN_ENGINE_RUSTC={}", version.trim());
    println!("cargo:rustc-env=KRAUSEN_ENGINE_TARGET={}", std::env::var("TARGET").expect("target"));
    println!("cargo:rustc-env=KRAUSEN_ENGINE_PROFILE={}", std::env::var("PROFILE").expect("profile"));
}
