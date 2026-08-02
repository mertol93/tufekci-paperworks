fn main() {
    #[cfg(target_os = "macos")]
    build_macos_scanner_bridge();
    tauri_build::build();
}

#[cfg(target_os = "macos")]
fn build_macos_scanner_bridge() {
    use std::path::PathBuf;
    use std::process::Command;

    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is unavailable"),
    );
    let script = manifest_dir
        .parent()
        .expect("the project root is unavailable")
        .join("scripts")
        .join("build-macos-scanner.mjs");
    let source = manifest_dir
        .join("native")
        .join("macos-scanner")
        .join("main.m");
    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", script.display());
    let status = Command::new("node")
        .arg(&script)
        .status()
        .unwrap_or_else(|error| {
            panic!("the macOS scanner bridge builder could not start: {error}")
        });
    assert!(
        status.success(),
        "the macOS scanner bridge builder failed with {status}"
    );
}
