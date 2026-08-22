use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

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

fn git_output(root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

fn source_state_fingerprint(manifest_dir: &Path) -> String {
    let Some(root_bytes) = git_output(manifest_dir, &["rev-parse", "--show-toplevel"]) else {
        return "source:unknown".to_string();
    };
    let root = PathBuf::from(String::from_utf8_lossy(&root_bytes).trim());
    let Some(head_bytes) = git_output(&root, &["rev-parse", "HEAD"]) else {
        return "source:unknown".to_string();
    };
    let head = String::from_utf8_lossy(&head_bytes).trim().to_string();
    let scope = ["Cargo.toml", "Cargo.lock", "apps/gardn"];
    let mut status_args = vec!["status", "--porcelain=v1", "--untracked-files=all", "--"];
    status_args.extend(scope);
    let Some(status) = git_output(&root, &status_args) else {
        return format!("git:{head}");
    };
    if status.is_empty() {
        return format!("git:{head}");
    }

    let mut hasher = Sha256::new();
    hasher.update(b"gardn-source-state-v1\0");
    hasher.update(head.as_bytes());
    let mut diff_args = vec!["diff", "--binary", "HEAD", "--"];
    diff_args.extend(scope);
    if let Some(diff) = git_output(&root, &diff_args) {
        hasher.update(&diff);
    }

    let mut untracked_args = vec!["ls-files", "--others", "--exclude-standard", "-z", "--"];
    untracked_args.extend(scope);
    if let Some(untracked) = git_output(&root, &untracked_args) {
        for path in untracked
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
        {
            hasher.update([0]);
            hasher.update(path);
            if let Ok(contents) = fs::read(root.join(String::from_utf8_lossy(path).as_ref())) {
                hasher.update([0]);
                hasher.update(contents);
            }
        }
    }

    format!(
        "dirty:{}:{}",
        &head[..head.len().min(12)],
        to_lower_hex(hasher.finalize().as_slice())
    )
}

fn to_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn emit_build_identity(manifest_dir: &Path, target: &str) {
    println!("cargo:rerun-if-env-changed=GARDN_BUILD_CHANNEL");
    println!("cargo:rerun-if-env-changed=GARDN_BUILD_COHORT");
    println!("cargo:rerun-if-env-changed=GARDN_RELEASE_TAG");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    println!("cargo:rerun-if-changed=../../Cargo.lock");

    let channel = env::var("GARDN_BUILD_CHANNEL").unwrap_or_else(|_| "development".to_string());
    assert!(
        matches!(channel.as_str(), "development" | "release"),
        "GARDN_BUILD_CHANNEL must be 'development' or 'release'"
    );
    let cohort =
        env::var("GARDN_BUILD_COHORT").unwrap_or_else(|_| source_state_fingerprint(manifest_dir));
    assert!(
        !cohort.trim().is_empty(),
        "GARDN_BUILD_COHORT must not be empty"
    );
    let release_tag = env::var("GARDN_RELEASE_TAG").unwrap_or_default();
    if channel == "release" {
        let expected_tag = format!(
            "v{}",
            env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION")
        );
        assert_eq!(
            release_tag, expected_tag,
            "official release builds require GARDN_RELEASE_TAG={expected_tag}"
        );
        assert!(
            env::var("GARDN_BUILD_COHORT").is_ok(),
            "official release builds require an explicit GARDN_BUILD_COHORT"
        );
    }

    println!("cargo:rustc-env=GARDN_BUILD_CHANNEL_EMBEDDED={channel}");
    println!("cargo:rustc-env=GARDN_BUILD_COHORT_EMBEDDED={cohort}");
    println!("cargo:rustc-env=GARDN_BUILD_TARGET_EMBEDDED={target}");
    println!("cargo:rustc-env=GARDN_RELEASE_TAG_EMBEDDED={release_tag}");
}

fn default_zig_optimize() -> &'static str {
    if env::var("OPT_LEVEL").as_deref() == Ok("0") {
        "Debug"
    } else {
        "ReleaseFast"
    }
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
    let optimize =
        env::var("LIBGHOSTTY_VT_OPTIMIZE").unwrap_or_else(|_| default_zig_optimize().to_string());
    let simd = env_bool("LIBGHOSTTY_VT_SIMD").unwrap_or(true);
    let target = env::var("TARGET").expect("TARGET");
    emit_build_identity(&manifest_dir, &target);
    let zig_target = zig_target(&target);
    let install_prefix = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("libghostty-vt");
    let version_string = fs::read_to_string(vendored_dir.join("VERSION"))
        .expect("failed to read vendored libghostty-vt VERSION")
        .trim()
        .to_string();

    let zig = resolve_zig(&required_zig_version(&vendored_dir));
    let mut command = Command::new(zig);
    command
        .arg("build")
        .arg("--prefix")
        .arg(&install_prefix)
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

    let lib_dir = install_prefix.join("lib");
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
