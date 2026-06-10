use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn zig_target(target: &str) -> &str {
    match target {
        "x86_64-unknown-linux-gnu" => "x86_64-linux-gnu",
        "aarch64-unknown-linux-gnu" => "aarch64-linux-gnu",
        "x86_64-unknown-linux-musl" => "x86_64-linux-musl",
        "aarch64-unknown-linux-musl" => "aarch64-linux-musl",
        "x86_64-apple-darwin" => "x86_64-macos",
        "aarch64-apple-darwin" => "aarch64-macos",
        "x86_64-pc-windows-msvc" => "x86_64-windows-msvc",
        "aarch64-pc-windows-msvc" => "aarch64-windows-msvc",
        other => panic!("unsupported target for libghostty-vt build: {other}"),
    }
}

fn env_bool(name: &str) -> Option<bool> {
    match env::var(name) {
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            other => panic!("invalid boolean value for {name}: {other}"),
        },
        Err(env::VarError::NotPresent) => None,
        Err(err) => panic!("failed to read {name}: {err}"),
    }
}

fn required_zig_version(vendored_dir: &Path) -> String {
    let zon = fs::read_to_string(vendored_dir.join("build.zig.zon"))
        .expect("failed to read vendored libghostty-vt build.zig.zon");

    for line in zon.lines() {
        if !line.contains(".minimum_zig_version") {
            continue;
        }

        let Some((_, quoted)) = line.split_once('"') else {
            break;
        };
        let Some((version, _)) = quoted.split_once('"') else {
            break;
        };
        return version.to_string();
    }

    panic!("failed to parse minimum_zig_version from vendored libghostty-vt build.zig.zon");
}

fn zig_version(zig: &str) -> Option<String> {
    let output = Command::new(zig).arg("version").output().ok()?;
    if !output.status.success() {
        return None;
    }

    Some(String::from_utf8(output.stdout).ok()?.trim().to_string())
}

fn compatible_zig_version(version: &str, required_version: &str) -> bool {
    version == required_version
}

fn homebrew_zig_formula_version(required_version: &str) -> &str {
    required_version
        .rsplit_once('.')
        .map_or(required_version, |(major_minor, _)| major_minor)
}

fn resolve_zig(required_version: &str) -> String {
    match env::var("ZIG") {
        Ok(zig) => {
            let version =
                zig_version(&zig).unwrap_or_else(|| panic!("failed to execute Zig from ZIG={zig}"));
            if !compatible_zig_version(&version, required_version) {
                panic!(
                    "vendored libghostty-vt requires Zig {required_version}; found Zig {version} from ZIG={zig}"
                );
            }
            return zig;
        }
        Err(env::VarError::NotPresent) => {}
        Err(err) => panic!("failed to read ZIG: {err}"),
    }

    let homebrew_formula_version = homebrew_zig_formula_version(required_version);
    let candidates = [
        "zig".to_string(),
        format!("/opt/homebrew/opt/zig@{homebrew_formula_version}/bin/zig"),
        format!("/usr/local/opt/zig@{homebrew_formula_version}/bin/zig"),
    ];
    let mut found_zig: Option<(String, String)> = None;

    for candidate in candidates {
        let Some(version) = zig_version(&candidate) else {
            continue;
        };
        if compatible_zig_version(&version, required_version) {
            return candidate;
        }
        if found_zig.is_none() {
            found_zig = Some((candidate, version));
        }
    }

    if let Some((zig, version)) = found_zig {
        panic!(
            "vendored libghostty-vt requires Zig {required_version}; found Zig {version} at {zig}. Install zig@{homebrew_formula_version} or set ZIG to a compatible zig binary"
        );
    }

    panic!(
        "vendored libghostty-vt requires Zig {required_version}; install zig@{homebrew_formula_version} or set ZIG to a compatible zig binary"
    );
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=vendor/libghostty-vt.vendor.json");
    println!("cargo:rerun-if-changed=vendor/libghostty-vt/build.zig");
    println!("cargo:rerun-if-changed=vendor/libghostty-vt/build.zig.zon");
    println!("cargo:rerun-if-changed=vendor/libghostty-vt/include");
    println!("cargo:rerun-if-changed=vendor/libghostty-vt/pkg");
    println!("cargo:rerun-if-changed=vendor/libghostty-vt/src");
    println!("cargo:rerun-if-changed=vendor/libghostty-vt/VERSION");
    println!("cargo:rerun-if-env-changed=LIBGHOSTTY_VT_OPTIMIZE");
    println!("cargo:rerun-if-env-changed=LIBGHOSTTY_VT_SIMD");
    println!("cargo:rerun-if-env-changed=LIBGHOSTTY_VT_ZIG_SYSTEM_DIR");
    println!("cargo:rerun-if-env-changed=PATH");
    println!("cargo:rerun-if-env-changed=ZIG");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let vendored_dir = manifest_dir.join("vendor/libghostty-vt");
    let optimize = env::var("LIBGHOSTTY_VT_OPTIMIZE").unwrap_or_else(|_| "ReleaseFast".into());
    let simd = env_bool("LIBGHOSTTY_VT_SIMD").unwrap_or(true);
    let target = env::var("TARGET").expect("TARGET");
    let zig_target = zig_target(&target);
    let version_string = fs::read_to_string(vendored_dir.join("VERSION"))
        .expect("failed to read vendored libghostty-vt VERSION")
        .trim()
        .to_string();

    let zig = resolve_zig(&required_zig_version(&vendored_dir));
    let mut command = Command::new(zig);
    command
        .arg("build")
        .arg("-Demit-lib-vt")
        .arg(format!("-Doptimize={optimize}"))
        .arg(format!("-Dsimd={simd}"))
        .arg(format!("-Dtarget={zig_target}"))
        .arg(format!("-Dversion-string={version_string}"));
    if let Ok(system_dir) = env::var("LIBGHOSTTY_VT_ZIG_SYSTEM_DIR") {
        command.arg("--system").arg(system_dir);
    }

    let status = command
        .current_dir(&vendored_dir)
        .status()
        .expect("failed to execute zig build for vendored libghostty-vt");
    assert!(
        status.success(),
        "zig build for vendored libghostty-vt failed: {status}"
    );

    let lib_dir = vendored_dir.join("zig-out/lib");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    if target.contains("apple-darwin") {
        let static_lib = lib_dir.join("libghostty-vt.a");
        println!("cargo:rustc-link-arg={}", static_lib.display());
    } else if target.contains("windows-msvc") {
        println!("cargo:rustc-link-lib=static=ghostty-vt-static");
    } else {
        println!("cargo:rustc-link-lib=static=ghostty-vt");
    }
}
