# GARDN_INTEGRATION_ID=qwen
# GARDN_INTEGRATION_VERSION=1

param([string]$Action = "")

if ($Action -ne "session") { exit 0 }
if ($env:GARDN_ENV -ne "1") { exit 0 }
if ([string]::IsNullOrWhiteSpace($env:GARDN_PANE_ID)) { exit 0 }
if ([string]::IsNullOrWhiteSpace($env:GARDN_SOCKET_PATH)) { exit 0 }

$inputText = [Console]::In.ReadToEnd()
try {
    $payload = if ([string]::IsNullOrWhiteSpace($inputText)) { $null } else { $inputText | ConvertFrom-Json }
} catch {
    $payload = $null
}

if ($null -eq $payload -or [string]::IsNullOrWhiteSpace($payload.session_id)) { exit 0 }

$seq = [DateTime]::UtcNow.Ticks
$gardn = if ([string]::IsNullOrWhiteSpace($env:GARDN_BIN_PATH)) { "gardn" } else { $env:GARDN_BIN_PATH }
$commandArgs = @(
    "pane", "report-agent-session", $env:GARDN_PANE_ID,
    "--source", "gardn:qwen", "--agent", "qwen",
    "--agent-session-id", [string]$payload.session_id,
    "--seq", [string]$seq
)
if ($payload.source -in @("startup", "resume", "clear", "compact", "branch")) {
    $commandArgs += @("--session-start-source", [string]$payload.source)
}
try {
    & $gardn @commandArgs 2>$null | Out-Null
} catch {
}
