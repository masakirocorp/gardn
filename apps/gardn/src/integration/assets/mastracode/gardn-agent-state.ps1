# GARDN_INTEGRATION_ID=mastracode
# GARDN_INTEGRATION_VERSION=2

param([string]$Action = "")

if ($Action -notin @("session", "working", "idle", "blocked")) { exit 0 }
if ($env:GARDN_ENV -ne "1") { exit 0 }
if ([string]::IsNullOrWhiteSpace($env:GARDN_PANE_ID)) { exit 0 }

$inputText = [Console]::In.ReadToEnd()
try {
    $payload = if ([string]::IsNullOrWhiteSpace($inputText)) { $null } else { $inputText | ConvertFrom-Json }
} catch {
    $payload = $null
}

$sessionId = if ($null -ne $payload -and $payload.session_id -is [string]) { $payload.session_id } else { $null }
$seq = [DateTime]::UtcNow.Ticks
$gardn = if ([string]::IsNullOrWhiteSpace($env:GARDN_BIN_PATH)) { "gardn" } else { $env:GARDN_BIN_PATH }
try {
    if ($Action -eq "session") {
        if ([string]::IsNullOrWhiteSpace($sessionId)) { exit 0 }
        & $gardn pane report-agent-session $env:GARDN_PANE_ID --source gardn:mastracode --agent mastracode --seq $seq --session-start-source startup --agent-session-id $sessionId 2>$null | Out-Null
    } else {
        $args = @("pane", "report-agent", $env:GARDN_PANE_ID, "--source", "gardn:mastracode", "--agent", "mastracode", "--state", $Action, "--seq", "$seq")
        if (-not [string]::IsNullOrWhiteSpace($sessionId)) {
            $args += @("--agent-session-id", $sessionId)
        }
        & $gardn @args 2>$null | Out-Null
    }
} catch {
}