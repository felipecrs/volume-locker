param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[a-zA-Z0-9_-]+$')]
    [string]$Label,
    [string]$NamePattern = 'A50|ASTRO',
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '../target/presence-research')
)

$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null

# Compare Windows' endpoint/USB view with G HUB's wireless-device view.
# Requests are read-only. No device settings or Windows audio defaults change.
$pnp = @(Get-PnpDevice -PresentOnly | Where-Object { $_.FriendlyName -match $NamePattern } |
    Select-Object FriendlyName, Class, Status, InstanceId)
$socket = [System.Net.WebSockets.ClientWebSocket]::new()
$socket.Options.AddSubProtocol('json')
$socket.Options.SetRequestHeader('Origin', 'file://')
$timeout = [System.Threading.CancellationTokenSource]::new(8000)
$devices = @()
$ghubError = $null
try {
    $null = $socket.ConnectAsync([uri]'ws://127.0.0.1:9010', $timeout.Token).GetAwaiter().GetResult()
    $request = [System.Text.Encoding]::UTF8.GetBytes(
        '{"msgId":"volume-locker-presence","verb":"GET","path":"/devices/list"}')
    $null = $socket.SendAsync([System.ArraySegment[byte]]::new($request),
        [System.Net.WebSockets.WebSocketMessageType]::Text, $true, $timeout.Token).GetAwaiter().GetResult()
    for ($attempt = 0; $attempt -lt 16; $attempt++) {
        $buffer = New-Object byte[] 65536
        $stream = [System.IO.MemoryStream]::new()
        try {
            do {
                $received = $socket.ReceiveAsync([System.ArraySegment[byte]]::new($buffer),
                    $timeout.Token).GetAwaiter().GetResult()
                if ($received.MessageType -eq [System.Net.WebSockets.WebSocketMessageType]::Close) {
                    throw 'G HUB closed the connection.'
                }
                $stream.Write($buffer, 0, $received.Count)
                if ($stream.Length -gt 1048576) { throw 'G HUB response exceeded 1 MiB.' }
            } until ($received.EndOfMessage)
            $message = [System.Text.Encoding]::UTF8.GetString($stream.ToArray()) | ConvertFrom-Json
        } finally { $stream.Dispose() }
        # G HUB sends an OPTIONS greeting before its response.
        if ($message.path -eq '/devices/list' -and $message.payload.deviceInfos) {
            $devices = @($message.payload.deviceInfos |
                Where-Object { $_.displayName -match $NamePattern -or $_.extendedDisplayName -match $NamePattern } |
                Select-Object displayName, deviceModel, pid, state, connectionType, deviceType)
            break
        }
    }
} catch {
    $ghubError = $_.Exception.Message
} finally {
    $socket.Dispose()
    $timeout.Dispose()
}

$snapshot = [ordered]@{
    capturedAt = [DateTimeOffset]::UtcNow.ToString('o')
    label = $Label
    windowsDevices = $pnp
    ghubDevices = $devices
    ghubError = $ghubError
}
$path = Join-Path $OutputDirectory "$Label.json"
$snapshot | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $path -Encoding UTF8
$snapshot | ConvertTo-Json -Depth 6
