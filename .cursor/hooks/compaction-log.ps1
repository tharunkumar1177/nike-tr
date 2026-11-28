# preCompact hook: appends context-compaction details (trigger, context usage,
# message counts, first-compaction flag) to prompts.log in the project root.
# Never blocks compaction; any failure exits 0 with an empty JSON response.

<#
.SYNOPSIS
Returns a payload field as a string, or 'n/a' when the field is absent.
#>
function Get-Field {
    param($Payload, [string]$Name)
    $value = $Payload.$Name
    if ($null -eq $value) { return 'n/a' }
    return [string]$value
}

<#
.SYNOPSIS
Builds the multi-line prompts.log entry describing one compaction event.
#>
function Format-CompactionEntry {
    param($Payload, [string]$Raw)
    $fields = 'trigger', 'context_usage_percent', 'context_tokens', 'context_window_size',
              'message_count', 'messages_to_compact', 'is_first_compaction'
    $known = $fields | Where-Object { $null -ne $Payload.$_ }

    $lines = @(
        "[$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')] CONTEXT COMPACTION",
        "  conversation_id      : $(Get-Field $Payload 'conversation_id')",
        "  trigger              : $(Get-Field $Payload 'trigger')",
        "  context_usage_percent: $(Get-Field $Payload 'context_usage_percent')",
        "  context_tokens       : $(Get-Field $Payload 'context_tokens')",
        "  context_window_size  : $(Get-Field $Payload 'context_window_size')",
        "  message_count        : $(Get-Field $Payload 'message_count')",
        "  messages_to_compact  : $(Get-Field $Payload 'messages_to_compact')",
        "  is_first_compaction  : $(Get-Field $Payload 'is_first_compaction')"
    )
    if (-not $known) {
        $lines += "  raw_payload          : $Raw"
    }
    return ($lines -join [Environment]::NewLine) + [Environment]::NewLine + [Environment]::NewLine
}

try {
    $reader = New-Object IO.StreamReader([Console]::OpenStandardInput(), [Text.Encoding]::UTF8)
    $raw = $reader.ReadToEnd()
    $payload = $raw | ConvertFrom-Json

    $logPath = Join-Path (Get-Location) 'prompts.log'
    $entry = Format-CompactionEntry $payload $raw
    [IO.File]::AppendAllText($logPath, $entry, (New-Object Text.UTF8Encoding($false)))
} catch {
}

'{}'
exit 0