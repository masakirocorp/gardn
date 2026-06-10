param(
    [string]$BinaryPath = ""
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent $PSScriptRoot
$bin = if ($BinaryPath) { $BinaryPath } else { Join-Path $repo "target\debug\hako.exe" }
if (!(Test-Path $bin)) {
    throw "missing binary at $bin"
}
& $bin --version
if ($LASTEXITCODE -ne 0) { throw "hako --version failed" }

$root = Join-Path ([System.IO.Path]::GetTempPath()) ("hako-windows-smoke-" + [System.Guid]::NewGuid().ToString("N"))
$config = Join-Path $root "config"
$runtime = Join-Path $root "runtime"
$socket = Join-Path $root "hako.sock"
New-Item -ItemType Directory -Force -Path $config, $runtime | Out-Null

$oldConfig = $env:XDG_CONFIG_HOME
$oldRuntime = $env:XDG_RUNTIME_DIR
$oldSocket = $env:HAKO_SOCKET_PATH
$oldClientSocket = $env:HAKO_CLIENT_SOCKET_PATH
$oldShell = $env:SHELL
$server = $null
try {
    $env:XDG_CONFIG_HOME = $config
    $env:XDG_RUNTIME_DIR = $runtime
    $env:HAKO_SOCKET_PATH = $socket
    Remove-Item Env:HAKO_CLIENT_SOCKET_PATH -ErrorAction SilentlyContinue
    Remove-Item Env:SHELL -ErrorAction SilentlyContinue

    $server = Start-Process -FilePath $bin -ArgumentList @("server") -PassThru -NoNewWindow

    $status = $null
    for ($i = 0; $i -lt 80; $i++) {
        Start-Sleep -Milliseconds 250
        try {
            $raw = & $bin status server --json
            if ($LASTEXITCODE -eq 0 -and $raw) {
                $candidate = $raw | ConvertFrom-Json
                if ($candidate.running -eq $true) {
                    $status = $candidate
                    break
                }
            }
        } catch {
            # Server may not have bound its socket yet.
        }
    }
    if ($null -eq $status) { throw "server did not become ready" }
    if ($status.capabilities.live_handoff -ne $false) {
        throw "Windows server advertised live_handoff=$($status.capabilities.live_handoff)"
    }

    $created = (& $bin workspace create --cwd $root --focus) | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw "workspace create failed" }
    $paneId = $created.result.root_pane.pane_id
    if (!$paneId) { throw "workspace create did not return root pane id" }

    & $bin pane run $paneId "echo hako-windows-smoke"
    if ($LASTEXITCODE -ne 0) { throw "pane run failed" }

    & $bin wait output $paneId --match hako-windows-smoke --source recent --timeout 10000
    if ($LASTEXITCODE -ne 0) { throw "wait output failed" }

    & $bin server stop
    if ($LASTEXITCODE -ne 0) { throw "server stop failed" }
    if ($server -and !$server.WaitForExit(5000)) {
        throw "server did not exit after stop"
    }
} finally {
    if ($server -and !$server.HasExited) {
        Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
    }
    if ($null -eq $oldConfig) { Remove-Item Env:XDG_CONFIG_HOME -ErrorAction SilentlyContinue } else { $env:XDG_CONFIG_HOME = $oldConfig }
    if ($null -eq $oldRuntime) { Remove-Item Env:XDG_RUNTIME_DIR -ErrorAction SilentlyContinue } else { $env:XDG_RUNTIME_DIR = $oldRuntime }
    if ($null -eq $oldSocket) { Remove-Item Env:HAKO_SOCKET_PATH -ErrorAction SilentlyContinue } else { $env:HAKO_SOCKET_PATH = $oldSocket }
    if ($null -eq $oldClientSocket) { Remove-Item Env:HAKO_CLIENT_SOCKET_PATH -ErrorAction SilentlyContinue } else { $env:HAKO_CLIENT_SOCKET_PATH = $oldClientSocket }
    if ($null -eq $oldShell) { Remove-Item Env:SHELL -ErrorAction SilentlyContinue } else { $env:SHELL = $oldShell }
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}
