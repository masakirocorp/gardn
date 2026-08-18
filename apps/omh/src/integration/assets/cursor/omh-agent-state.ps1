# installed by Oh My Herdr
# managed by Oh My Herdr; reinstalling or updating the integration overwrites this file.
# add custom hooks beside this file instead of editing it.
# OMH_INTEGRATION_ID=cursor
# OMH_INTEGRATION_VERSION=3

param([string]$Action = "")

function Exit-Hook {
    Write-Output "{}"
    exit 0
}

if ($Action -notin @("working", "idle", "release")) { Exit-Hook }
if ($env:GROK_HOOK_EVENT) { Exit-Hook }
if ($env:OMH_ENV -ne "1") { Exit-Hook }
if ([string]::IsNullOrWhiteSpace($env:OMH_PANE_ID)) { Exit-Hook }

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
$omh = if ([string]::IsNullOrWhiteSpace($env:OMH_BIN_PATH)) { "omh" } else { $env:OMH_BIN_PATH }
try {
    if ($Action -eq "release") {
        & $omh pane release-agent $env:OMH_PANE_ID --source omh:cursor --agent cursor --seq $seq 2>$null | Out-Null
    } else {
        $args = @(
            "pane", "report-agent", $env:OMH_PANE_ID,
            "--source", "omh:cursor",
            "--agent", "cursor",
            "--state", $Action,
            "--seq", $seq
        )
        if (-not [string]::IsNullOrWhiteSpace($sessionId)) {
            $args += @("--agent-session-id", $sessionId)
        }
        & $omh @args 2>$null | Out-Null
    }
} catch {
}

Exit-Hook
