# log-user-input.ps1
# beforeSubmitPrompt hook: appends every submitted user prompt to prompts.log at the repo root.
# Write-only by design; the log is never read so the guard-files.ps1 policy stays intact.
# Always fails open: logging problems must never block prompt submission.

$ErrorActionPreference = 'Stop'
$allow = '{ "continue": true }'

<#
.SYNOPSIS
Reads the full hook payload from stdin as UTF-8 text.
#>
function Read-HookInput {
    $reader = New-Object IO.StreamReader([Console]::OpenStandardInput(), [Text.Encoding]::UTF8)
    try {
        return $reader.ReadToEnd()
    } finally {
        $reader.Dispose()
    }
}

<#
.SYNOPSIS
Builds one plain-text log entry: a timestamped header line, the prompt body, and a blank separator line.
#>
function Format-LogEntry {
    param(
        [Parameter(Mandatory)][string]$Text,
        [Parameter(Mandatory)][DateTimeOffset]$Timestamp
    )
    $normalized = ($Text -replace "`r`n", "`n").TrimEnd()
    $stamp = $Timestamp.ToString('yyyy-MM-dd HH:mm:ss zzz')
    return "[$stamp]`n$normalized`n`n"
}

<#
.SYNOPSIS
Appends text to the log file as UTF-8 without BOM, serialized across concurrent chats by a named mutex.
#>
function Add-LogEntry {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Entry
    )
    $mutex = New-Object Threading.Mutex($false, 'Local\FramecraftPromptLog')
    $acquired = $false
    try {
        $acquired = $mutex.WaitOne(5000)
        if (-not $acquired) { return }
        [IO.File]::AppendAllText($Path, $Entry, (New-Object Text.UTF8Encoding($false)))
    } finally {
        if ($acquired) { $mutex.ReleaseMutex() }
        $mutex.Dispose()
    }
}

try {
    $payload = Read-HookInput | ConvertFrom-Json
    $text = [string]$payload.prompt
    if (-not [string]::IsNullOrWhiteSpace($text)) {
        $repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
        $logPath = Join-Path $repoRoot 'prompts.log'
        Add-LogEntry -Path $logPath -Entry (Format-LogEntry -Text $text -Timestamp ([DateTimeOffset]::Now))
    }
} catch {
    [Console]::Error.WriteLine("log-user-input: $($_.Exception.Message)")
}

$allow
exit 0