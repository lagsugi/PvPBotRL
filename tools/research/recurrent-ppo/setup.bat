@echo off
setlocal DisableDelayedExpansion
set "PVPBOT_BATCH_FILE=%~f0"
"%~dp0..\..\run-batch.bat" %*
exit /b %errorlevel%

# PowerShell
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $scriptDirectory '../../..'))
Push-Location $repo
try {
    if (-not (Test-Path -LiteralPath 'build/rl-venv/Scripts/python.exe')) {
        uv --cache-dir build/rl-cache venv --python 3.12 build/rl-venv
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    uv --cache-dir build/rl-cache pip sync --python build/rl-venv/Scripts/python.exe --extra-index-url https://download.pytorch.org/whl/cpu (Join-Path $scriptDirectory 'requirements.txt')
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    & (Join-Path $repo 'tools/rust/cargo.bat') build --release --locked -p pvp-rl
    exit $LASTEXITCODE
} finally { Pop-Location }
