function Get-GHubResource {
    param([Parameter(Mandatory = $true)][string[]]$Paths)
    $ErrorActionPreference = 'Stop'
    $socket = [System.Net.WebSockets.ClientWebSocket]::new()
    $socket.Options.AddSubProtocol('json')
    $socket.Options.SetRequestHeader('Origin', 'file://')
    $timeout = [System.Threading.CancellationTokenSource]::new(8000)
    try {
        $null = $socket.ConnectAsync([uri]'ws://127.0.0.1:9010', $timeout.Token).GetAwaiter().GetResult()
        foreach ($path in $Paths) {
            $request = @{msgId = [Guid]::NewGuid().ToString(); verb = 'GET'; path = $path} | ConvertTo-Json -Compress
            $bytes = [System.Text.Encoding]::UTF8.GetBytes($request)
            $null = $socket.SendAsync([System.ArraySegment[byte]]::new($bytes),
                [System.Net.WebSockets.WebSocketMessageType]::Text, $true, $timeout.Token).GetAwaiter().GetResult()
            for ($attempt = 0; $attempt -lt 32; $attempt++) {
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
                if ($message.path -eq $path) { $message; break }
            }
        }
    } finally { $socket.Dispose(); $timeout.Dispose() }
}

Export-ModuleMember -Function Get-GHubResource
