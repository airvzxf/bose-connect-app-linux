//! Build script: generates the C header (`bose_connect.h`) from the
//! library's `#[no_mangle] extern "C"` surface using `cbindgen`.
//!
//! The header is written to `$OUT_DIR/bose_connect.h` and the crate
//! re-exports its path via `env!("BOSE_CONNECT_HEADER_PATH")` so Rust
//! consumers can `#include` it from `build.rs` and so that downstream
//! C/C++ projects can locate the header relative to the crate's
//! build artifacts.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/protocol.rs");
    println!("cargo:rerun-if-changed=src/types.rs");
    println!("cargo:rerun-if-changed=src/error.rs");
    println!("cargo:rerun-if-changed=src/connection.rs");

    // Probe libbluetooth so consumers who build the FFI crate get a
    // helpful error if the BlueZ headers are missing. We do not link
    // against libbluetooth ourselves — `nix` covers the POSIX surface
    // and `AF_BLUETOOTH`/`BTPROTO_RFCOMM` are defined by libc — but
    // the headers are still required at *compile time* of any
    // downstream C/C++ project that `#include`s the generated header
    // in environments without the kernel uapi headers installed.
    if let Err(e) = pkg_config::probe("bluez") {
        println!(
            "cargo:warning=libbluetooth (BlueZ) headers not found via pkg-config: {e}. \
             The Rust crate builds without them, but downstream C/C++ \
             consumers of `bose_connect.h` will need `bluez-libs` (Arch) \
             or `libbluetooth-dev` (Debian/Ubuntu) installed."
        );
    }

    // Emit a `BOSE_CONNECT_HEADER_PATH` env var that downstream code
    // (tests, examples, the `probe` binary) can read.
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR set by cargo"));
    let header_path = out_dir.join("bose_connect.h");

    let crate_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let config_path = crate_dir.join("cbindgen.toml");

    let config = if config_path.exists() {
        cbindgen::Config::from_file(&config_path).unwrap_or_else(|e| {
            println!(
                "cargo:warning=cbindgen.toml exists but failed to parse ({e}); \
                 falling back to defaults"
            );
            cbindgen::Config::default()
        })
    } else {
        cbindgen::Config::default()
    };

    match cbindgen::generate_with_config(&crate_dir, config) {
        Ok(bindings) => {
            bindings.write_to_file(&header_path);
            println!(
                "cargo:rustc-env=BOSE_CONNECT_HEADER_PATH={}",
                header_path.display()
            );
            // Inside the git checkout, also drop a copy at the crate
            // root so the source tree shows the C surface (release.yml
            // attaches it). Never when building the packaged crate
            // (crates.io, docs.rs, `cargo publish` verification): build
            // scripts must not write outside OUT_DIR, and docs.rs
            // mounts the source read-only.
            if crate_dir.join("../../.git").exists() {
                let repo_header = crate_dir.join("bose_connect.h");
                if let Ok(bindings_text) = fs::read_to_string(&header_path) {
                    let _ = fs::write(&repo_header, bindings_text);
                }
            }
        }
        Err(e) => {
            // `cbindgen` failures are not fatal: the Rust crate still
            // builds. We surface a warning so maintainers see it.
            println!("cargo:warning=cbindgen failed to generate header: {e}");
        }
    }
}

/// Tiny vendored pkg-config probe so we do not need to add the
/// `pkg-config` crate as a build-dependency for one call site.
mod pkg_config {
    use std::process::Command;

    pub fn probe(name: &str) -> Result<(), String> {
        let output = Command::new("pkg-config")
            .args(["--modversion", name])
            .output()
            .map_err(|e| format!("spawning pkg-config: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(stderr.trim().to_string())
        }
    }
}
