@echo off
setlocal DisableDelayedExpansion
set "PVPBOT_BATCH_FILE=%~f0"
"%~dp0..\..\run-batch.bat" %*
exit /b %errorlevel%

# PowerShell
param(
    [string]$Output = 'build/recurrent-ppo/nextgen-24h',
    [switch]$Resume,
    [switch]$Quick,
    [switch]$FixedEquipment,
    [int]$Lanes = 512,
    [int]$Threads = 8,
    [int]$TorchThreads = 4,
    [switch]$Gpu,
    [int]$MinibatchTokens,
    [string]$InitCheckpoint
)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $scriptDirectory '../../..'))
$python = if ($Gpu) { Join-Path $repo 'build/rl-gpu-bench-venv/Scripts/python.exe' }
          else { Join-Path $repo 'build/rl-venv/Scripts/python.exe' }
if (-not (Test-Path -LiteralPath $python)) {
    if ($Gpu) { throw 'GPU environment missing. See docs/research/bots/next-gen-gru-run.md.' }
    throw 'Python environment missing. Run tools/research/recurrent-ppo/setup.bat first.'
}
Push-Location $repo
try {
    & (Join-Path $repo 'tools/rust/cargo.bat') build --release --locked -p pvp-rl
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $params = @((Join-Path $scriptDirectory 'nextgen.py'), $Output,
        '--lanes', "$Lanes", '--threads', "$Threads", '--torch-threads', "$TorchThreads")
    if ($Resume) { $params += '--resume' }
    if ($Quick) { $params += '--quick' }
    if ($FixedEquipment) { $params += '--fixed-equipment' }
    if ($Gpu) { $params += @('--ppo-device', 'cuda') }
    if ($PSBoundParameters.ContainsKey('MinibatchTokens')) { $params += @('--minibatch-tokens', "$MinibatchTokens") }
    if ($InitCheckpoint) { $params += @('--init-checkpoint', $InitCheckpoint) }
    & $python -B -u @params
    exit $LASTEXITCODE
} finally { Pop-Location }
