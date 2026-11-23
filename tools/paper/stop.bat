@echo off
setlocal DisableDelayedExpansion
set "PVPBOT_BATCH_FILE=%~f0"
"%~dp0..\run-batch.bat" %*
exit /b %errorlevel%

# PowerShell
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$properties = Join-Path $scriptDirectory '..\..\paper\run-rl-standalone\server.properties'
$settings = Get-Content -LiteralPath $properties -Raw | ConvertFrom-StringData
if ($settings['server-ip'] -ne '127.0.0.1' -or -not $settings['rcon.password']) {
    throw 'The local Paper server has not been set up.'
}

function Send-RconPacket([int]$Id, [int]$Type, [string]$Message) {
    $bytes = [Text.Encoding]::UTF8.GetBytes($Message)
    $buffer = [IO.MemoryStream]::new()
    $writer = [IO.BinaryWriter]::new($buffer)
    try {
        $writer.Write([int]($bytes.Length + 10))
        $writer.Write($Id)
        $writer.Write($Type)
        $writer.Write($bytes)
        $writer.Write([int16]0)
        $packet = $buffer.ToArray()
        $stream.Write($packet, 0, $packet.Length)
    } finally {
        $writer.Dispose()
    }
}

$client = New-Object Net.Sockets.TcpClient
try {
    $client.Connect('127.0.0.1', [int]$settings['rcon.port'])
    $stream = $client.GetStream()
    $stream.ReadTimeout = 5000
    $reader = New-Object IO.BinaryReader($stream)
    Send-RconPacket -Id 1 -Type 3 -Message $settings['rcon.password']
    $length = $reader.ReadInt32()
    if ($length -lt 10 -or $length -gt 4096) { throw 'Invalid RCON response.' }
    $id = $reader.ReadInt32()
    $type = $reader.ReadInt32()
    $null = $reader.ReadBytes($length - 8)
    if ($id -ne 1 -or $type -ne 2) { throw 'RCON authentication failed.' }
    Send-RconPacket -Id 2 -Type 2 -Message 'stop'
    Write-Host 'Paper received a graceful stop request.'
} finally {
    $client.Dispose()
}
