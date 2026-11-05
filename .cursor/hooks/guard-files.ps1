$reader = New-Object IO.StreamReader([Console]::OpenStandardInput(), [Text.Encoding]::UTF8)
$raw = $reader.ReadToEnd()

$deny = @{
    permission    = 'deny'
    user_message  = 'Blocked: prompts.md is off-limits to the agent.'
    agent_message = 'prompts.md is protected by a project hook. Do not attempt to read it by any means.'
} | ConvertTo-Json -Compress

try {
    $payload = $raw | ConvertFrom-Json
} catch {
    $deny
    exit 2
}

$candidates = @(
    $payload.file_path,
    $payload.command,
    $payload.tool_input.path,
    $payload.tool_input.file_path,
    $payload.tool_input.glob,
    $payload.tool_input.glob_pattern,
    $payload.tool_input.target_directory,
    $payload.tool_input.command
) | Where-Object { $_ }

foreach ($value in $candidates) {
    if ([string]$value -match '(?i)prompt(s|[?*\[])') {
        $deny
        exit 2
    }
}

'{ "permission": "allow" }'
exit 0