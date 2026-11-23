@echo off
setlocal DisableDelayedExpansion
set "PVPBOT_BATCH_FILE=%~f0"
"%~dp0..\run-batch.bat" %*
exit /b %errorlevel%

# PowerShell
# Use the workspace-local toolchain if present, otherwise use Cargo on PATH.
# Examples: .\tools\rust\cargo.bat test --workspace; .\tools\rust\cargo.bat run --release -- duel
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $scriptDirectory '../..'))
$localCargo = Join-Path $repo '.tools/cargo/bin/cargo.exe'
if (Test-Path -LiteralPath $localCargo) {
    $env:CARGO_HOME = Join-Path $repo '.tools/cargo'
    $env:RUSTUP_HOME = Join-Path $repo '.tools/rustup'
    $env:PATH = (Join-Path $repo '.tools/cargo/bin') + [IO.Path]::PathSeparator + $env:PATH
    $cargoCommand = $localCargo
} else {
    $cargoCommand = (Get-Command cargo -ErrorAction Stop).Source
}
Push-Location $repo
try {
    & $cargoCommand @args
    $result = $LASTEXITCODE
} finally { Pop-Location }
exit $result
