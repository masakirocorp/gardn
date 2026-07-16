# portable-pty local patches

This file tracks intentional local changes applied on top of the vendored
`portable-pty` source. Remove a patch only when upstream contains equivalent
behavior or exposes an option that preserves Oh My Herdr's invariant.

## 0001 force system ConPTY

status: active

patch: `apps/omh/vendor/patches/portable-pty/0001-force-system-conpty.patch`

vendored base: `portable-pty 0.9.0`

local file:

- `apps/omh/vendor/portable-pty/src/win/psuedocon.rs`

reason: `portable-pty` 0.9.0 first loads `kernel32.dll`, then probes a bare
`conpty.dll` from the DLL search path. Oh My Herdr does not ship the paired
`OpenConsole.exe`/`conpty.dll`; loading another application's DLL from `PATH`
violates the system-ConPTY invariant.

remove when: upstream no longer loads bare `conpty.dll`, upstream exposes a
way to force system ConPTY, or Oh My Herdr replaces its Windows PTY backend.

verification:

```sh
python3 -m unittest scripts.test_vendor_portable_pty
```

## 0002 preserve raw Windows command arguments

status: active

patch: `apps/omh/vendor/patches/portable-pty/0002-preserve-raw-windows-command-arguments.patch`

vendored base: `portable-pty 0.9.0`

local file:

- `apps/omh/vendor/portable-pty/src/cmdbuilder.rs`

reason: Oh My Herdr launches custom commands through `cmd.exe /d /c`. Quoting the
command as a normal argument changes shell operators such as `&`, pipes, and
redirections. The vendored builder needs the equivalent of
`std::os::windows::process::CommandExt::raw_arg`.

remove when: upstream `portable-pty` exposes an equivalent raw Windows argument
API or Oh My Herdr replaces its Windows PTY backend.

verification:

```sh
python3 -m unittest scripts.test_vendor_portable_pty
```
