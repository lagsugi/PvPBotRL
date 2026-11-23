@echo off
setlocal DisableDelayedExpansion
set "PVPBOT_BATCH_FILE=%~f0"
"%~dp0..\run-batch.bat" %*
exit /b %errorlevel%

# PowerShell
[CmdletBinding()]
param([switch]$AcceptEula, [switch]$Offline, [switch]$WithExample)

$ErrorActionPreference = 'Stop'
$setupArgs = @()
if ($Offline) { $setupArgs += '-Offline' }
if ($WithExample) { $setupArgs += '-WithExample' }
& (Join-Path $scriptDirectory 'setup.bat') @setupArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$run = (Resolve-Path -LiteralPath (Join-Path $scriptDirectory '..\..\paper\run-rl-standalone')).Path
$eula = Join-Path $run 'eula.txt'
if ($AcceptEula) {
    Set-Content -LiteralPath $eula -Value 'eula=true' -Encoding ASCII
}
if (-not (Test-Path -LiteralPath $eula) -or -not (Select-String -LiteralPath $eula -Pattern '^\s*eula\s*=\s*true\s*$' -Quiet)) {
    throw 'Read https://www.minecraft.net/eula and rerun with -AcceptEula if you agree.'
}

$java = if ($env:JAVA_HOME) { Join-Path $env:JAVA_HOME 'bin\java.exe' } else { 'java.exe' }
Push-Location -LiteralPath $run
try {
    & $java '-Xms1G' '-Xmx4G' '-jar' 'paper-26.1.2-74.jar' '--nogui'
    if ($LASTEXITCODE -ne 0) { throw "Paper exited with code $LASTEXITCODE" }
} finally {
    Pop-Location
}
