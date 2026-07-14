param(
    [Parameter(Mandatory = $true)]
    [string]$ExePath,

    [string]$Session = "ci-windows-$([guid]::NewGuid().ToString('N'))"
)

$ErrorActionPreference = "Stop"

$exe = (Resolve-Path $ExePath).Path
$fakeDir = Join-Path ([System.IO.Path]::GetTempPath()) "hako-fake-conpty-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force $fakeDir | Out-Null

$fakeSource = Join-Path $fakeDir "fake_conpty.rs"
$fakeDll = Join-Path $fakeDir "conpty.dll"
@'
#![allow(non_snake_case)]

use std::ffi::c_void;

#[repr(C)]
pub struct COORD {
    pub X: i16,
    pub Y: i16,
}

type HANDLE = *mut c_void;
type HRESULT = i32;

#[no_mangle]
pub extern "system" fn CreatePseudoConsole(
    _size: COORD,
    _h_input: HANDLE,
    _h_output: HANDLE,
    _flags: u32,
    _hpc: *mut HANDLE,
) -> HRESULT {
    -2147467259
}

#[no_mangle]
pub extern "system" fn ResizePseudoConsole(_hpc: HANDLE, _size: COORD) -> HRESULT {
    -2147467259
}

#[no_mangle]
pub extern "system" fn ClosePseudoConsole(_hpc: HANDLE) {}
'@ | Set-Content -NoNewline -Encoding utf8 $fakeSource

& rustc --crate-type cdylib --edition 2021 $fakeSource -o $fakeDll
if ($LASTEXITCODE -ne 0) {
    throw "failed to build fake conpty.dll"
}

$oldPath = $env:PATH
$oldSession = $env:HAKO_SESSION
$env:PATH = "$fakeDir;$oldPath"
$env:HAKO_SESSION = $Session
try {
    & (Join-Path $PSScriptRoot "smoke_windows_ci.ps1") -BinaryPath $exe
    if ($LASTEXITCODE -ne 0) {
        throw "Windows smoke failed with fake conpty.dll on PATH"
    }
} finally {
    $env:PATH = $oldPath
    if ($null -eq $oldSession) {
        Remove-Item Env:HAKO_SESSION -ErrorAction SilentlyContinue
    } else {
        $env:HAKO_SESSION = $oldSession
    }
    Remove-Item -Recurse -Force $fakeDir -ErrorAction SilentlyContinue
}
