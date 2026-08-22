# installed by Gardn
# managed by Gardn; reinstalling or updating the integration overwrites this file.
# add custom hooks beside this file instead of editing it.
# GARDN_INTEGRATION_ID=cursor
# GARDN_INTEGRATION_VERSION=3

param([string]$Action = "")

function Exit-Hook {
    Write-Output "{}"
    exit 0
}

if ($Action -notin @("working", "idle", "release")) { Exit-Hook }
if ($env:GROK_HOOK_EVENT) { Exit-Hook }
if ($env:GARDN_ENV -ne "1") { Exit-Hook }
if ([string]::IsNullOrWhiteSpace($env:GARDN_PANE_ID)) { Exit-Hook }

$inputText = [Console]::In.ReadToEnd()
$jsonStart = $inputText.IndexOf("{")
if ($jsonStart -gt 0) {
    $inputText = $inputText.Substring($jsonStart)
}
try {
    $payload = if ([string]::IsNullOrWhiteSpace($inputText)) { $null } else { $inputText | ConvertFrom-Json }
} catch {
    Exit-Hook
}

$sessionId = $null
if ($null -ne $payload) {
    foreach ($name in @("session_id", "sessionId", "conversation_id", "conversationId", "chat_id", "chatId")) {
        $value = $payload.$name
        if ($value -is [string] -and -not [string]::IsNullOrWhiteSpace($value)) {
            $sessionId = $value
            break
        }
    }
}

$seq = [DateTime]::UtcNow.Ticks
$gardn = if ([string]::IsNullOrWhiteSpace($env:GARDN_BIN_PATH)) { "gardn" } else { $env:GARDN_BIN_PATH }
try {
    if ($Action -eq "release") {
        & $gardn pane release-agent $env:GARDN_PANE_ID --source gardn:cursor --agent cursor --seq $seq 2>$null | Out-Null
    } else {
        $args = @(
            "pane", "report-agent", $env:GARDN_PANE_ID,
            "--source", "gardn:cursor",
            "--agent", "cursor",
            "--state", $Action,
            "--seq", $seq
        )
        if (-not [string]::IsNullOrWhiteSpace($sessionId)) {
            $args += @("--agent-session-id", $sessionId)
        }
        & $gardn @args 2>$null | Out-Null
    }
} catch {
}

Exit-Hook
