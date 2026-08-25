# GARDN_INTEGRATION_ID=antigravity_cli
# GARDN_INTEGRATION_VERSION=2

param([string]$Action = "")

# Antigravity hooks must return a JSON object.
function Exit-Hook {
    Write-Output "{}"
    exit 0
}

if ($Action -ne "session") { Exit-Hook }
if ($env:GARDN_ENV -ne "1") { Exit-Hook }
if ([string]::IsNullOrWhiteSpace($env:GARDN_PANE_ID)) { Exit-Hook }

$inputText = [Console]::In.ReadToEnd()
try {
    $payload = if ([string]::IsNullOrWhiteSpace($inputText)) { $null } else { $inputText | ConvertFrom-Json }
} catch {
    Exit-Hook
}

if ($null -eq $payload) { Exit-Hook }

$conversationId = if ($payload.conversationId -is [string]) { $payload.conversationId } else { $null }
if ([string]::IsNullOrWhiteSpace($conversationId)) { Exit-Hook }

$seq = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$gardn = if ([string]::IsNullOrWhiteSpace($env:GARDN_BIN_PATH)) { "gardn" } else { $env:GARDN_BIN_PATH }
try {
    $sessionArgs = @(
        "pane",
        "report-agent-session",
        $env:GARDN_PANE_ID,
        "--source",
        "gardn:antigravity_cli",
        "--agent",
        "agy",
        "--seq",
        "$seq",
        "--agent-session-id",
        "$conversationId",
        "--session-start-source",
        "startup"
    )
    if ($payload.transcriptPath -is [string] -and -not [string]::IsNullOrWhiteSpace($payload.transcriptPath)) {
        $sessionArgs += @("--agent-session-path", "$($payload.transcriptPath)")
    }
    & $gardn @sessionArgs 2>$null | Out-Null
} catch {
}

Exit-Hook