@echo off
setlocal DisableDelayedExpansion
set "PVPBOT_BATCH_FILE=%~f0"
"%~dp0..\run-batch.bat" %*
exit /b %errorlevel%

# PowerShell
# Build and open the native Rust duel window from the workspace root.
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $scriptDirectory '../..'))
Push-Location $repo
try {
    & (Join-Path $repo 'tools/rust/cargo.bat') build --release --locked -p pvp-play
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    & (Join-Path $repo 'target/release/pvp-play.exe') @args
} finally { Pop-Location }
