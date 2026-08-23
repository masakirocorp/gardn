{
  lib,
  stdenv,
  rustPlatform,
  callPackage,
  runCommand,
  writeShellScriptBin,
  zig_0_16,
  zstd,
  pkg-config,
  git,
  apple-sdk ? null,
  cctools ? null,
}:

let
  manifest = lib.importTOML ../apps/gardn/Cargo.toml;
  zigDeps = callPackage ../apps/gardn/vendor/libghostty-vt/build.zig.zon.nix {
    name = "gardn-libghostty-vt-zig-cache";
    inherit zstd;
    linkFarm =
      name: entries:
      runCommand name { } ''
        mkdir -p $out
        ${lib.concatMapStringsSep "\n" (entry: ''
          cp -rL ${entry.path} $out/${entry.name}
        '') entries}
      '';
  };
  zigTool = runCommand "gardn-zig-tool" { } ''
    mkdir -p $out/bin
    ln -s ${lib.getExe zig_0_16} $out/bin/zig
  '';

  darwinSdkRoot = "${apple-sdk}/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk";
  darwinDeveloperDir = "${apple-sdk}/Platforms/MacOSX.platform/Developer";
  darwinXcodeSelect = writeShellScriptBin "xcode-select" ''
    if [ "$1" = "--print-path" ]; then
      echo ${lib.escapeShellArg darwinDeveloperDir}
      exit 0
    fi
    echo "unsupported xcode-select invocation: $*" >&2
    exit 1
  '';
  darwinXcrun = writeShellScriptBin "xcrun" ''
    if [ "$1" = "--sdk" ] && [ "$3" = "--show-sdk-path" ]; then
      echo ${lib.escapeShellArg darwinSdkRoot}
      exit 0
    fi
    echo "unsupported xcrun invocation: $*" >&2
    exit 1
  '';
in
rustPlatform.buildRustPackage {
  pname = "gardn";
  version = manifest.package.version;

  src = lib.fileset.toSource {
    root = ./..;
    fileset = lib.fileset.intersection (lib.fileset.fromSource (lib.sources.cleanSource ./..)) (
      lib.fileset.unions [
        ../Cargo.lock
        ../Cargo.toml
        ../apps/gardn/assets
        ../apps/gardn/src
        ../apps/gardn/vendor/libghostty-vt
        ../apps/gardn/vendor/libghostty-vt.vendor.json
        ../apps/gardn/vendor/portable-pty
        ../apps/gardn/build.rs
        ../apps/gardn/Cargo.toml
        ../apps/gardn/CHANGELOG.md
        ../crates/gardn-local-api
        ../SKILL.md
      ]
    );
  };

  cargoLock = {
    lockFile = ../Cargo.lock;
  };

  nativeBuildInputs = [
    git
    pkg-config
    zigTool
  ]
  ++ lib.optionals stdenv.hostPlatform.isDarwin [
    cctools
    darwinXcodeSelect
    darwinXcrun
  ];

  env = {
    LIBGHOSTTY_VT_OPTIMIZE = "ReleaseFast";
    LIBGHOSTTY_VT_SIMD = "true";
    LIBGHOSTTY_VT_ZIG_SYSTEM_DIR = zigDeps;
    ZIG = lib.getExe zig_0_16;
  }
  // lib.optionalAttrs stdenv.hostPlatform.isDarwin {
    SDKROOT = darwinSdkRoot;
  };

  preBuild = ''
    export ZIG_GLOBAL_CACHE_DIR="$TMPDIR/zig-global-cache"
    export ZIG_LOCAL_CACHE_DIR="$TMPDIR/zig-local-cache"
  '';

  # Rust tests are covered by the normal CI workflow. The Nix check is
  # intentionally build-only so it validates packaging inputs without
  # duplicating the full Rust test suite.
  doCheck = false;

  meta = {
    description = "Terminal workspace manager for AI coding agents";
    homepage = "https://github.com/masakirocorp/gardn";
    license = lib.licenses.agpl3Plus;
    mainProgram = "gardn";
    platforms = lib.platforms.linux ++ lib.platforms.darwin;
  };
}
