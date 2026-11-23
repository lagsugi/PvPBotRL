@echo off
setlocal DisableDelayedExpansion
set "PVPBOT_BATCH_FILE=%~f0"
"%~dp0..\..\run-batch.bat" %*
exit /b %errorlevel%

# PowerShell
param([string]$Java = 'java')
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $scriptDirectory '../../..'))
$output = Join-Path $repo 'build/phase1'
$serverJars = @(Get-ChildItem (Join-Path $output 'runtime/versions') -Recurse -Filter '*.jar')
if ($serverJars.Count -ne 1) { throw 'Run Prepare-Sources.bat first' }
$libraryJars = @(Get-ChildItem (Join-Path $output 'runtime/libraries') -Recurse -Filter '*.jar' | Sort-Object FullName)
$classPath = (@($serverJars[0].FullName) + @($libraryJars.FullName)) -join [IO.Path]::PathSeparator
$probe = Join-Path $scriptDirectory 'Phase1Probe.java'
$result = Join-Path $output 'probe-results.json'
$arguments = @('--class-path', ('"' + $classPath.Replace('\', '/') + '"'), ('"' + $probe.Replace('\', '/') + '"'), ('"' + $result.Replace('\', '/') + '"'))
$argfile = Join-Path $output 'probe.args'
[IO.File]::WriteAllLines($argfile, $arguments, [Text.UTF8Encoding]::new($false))
Push-Location $output
try {
    & $Java "@$argfile"
    if ($LASTEXITCODE -ne 0) { throw "Java probe failed with exit code $LASTEXITCODE" }
} finally { Pop-Location }
