//! Builds the rustc driver from its source in this crate's `driver/`
//! directory. It must be built with the toolchain it will run as.

use crate::cargo::run;
use project_graph::GraphingError;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Builds the driver with `toolchain` into `target_dir` and returns the path
/// of the executable. The toolchain needs its `rustc-dev` component.
pub fn build_driver(toolchain: &str, target_dir: &Path) -> Result<PathBuf, GraphingError> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("driver");
    let sysroot = run(
        Command::new("rustup").args(["run", toolchain, "rustc", "--print", "sysroot"]),
        "rustc --print sysroot",
    )?;
    let sysroot = String::from_utf8_lossy(&sysroot.stdout).trim().to_string();
    run(
        Command::new("rustup")
            .args([
                "run",
                toolchain,
                "cargo",
                "build",
                "--release",
                "--locked",
                "--manifest-path",
            ])
            .arg(source.join("Cargo.toml"))
            .arg("--target-dir")
            .arg(target_dir)
            // The driver uses the compiler's unstable interface.
            .env("RUSTC_BOOTSTRAP", "1")
            // It loads the compiler's shared library from the toolchain.
            .env("RUSTFLAGS", format!("-C link-arg=-Wl,-rpath,{sysroot}/lib")),
        "building the rustc driver",
    )?;
    Ok(target_dir.join("release").join("srs-rustc-driver"))
}
