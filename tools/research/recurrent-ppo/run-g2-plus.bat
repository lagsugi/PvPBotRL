@echo off
setlocal DisableDelayedExpansion
set "PVPBOT_BATCH_FILE=%~f0"
"%~dp0..\..\run-batch.bat" %*
exit /b %errorlevel%

# PowerShell
param(
    [string]$Output,
    [switch]$Resume,
    [switch]$Quick
)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $scriptDirectory '../../..'))
$checkpoint = 'build/recurrent-ppo/nextgen-24h/main-71001/candidate-0001677.pt'
$policy = [IO.Path]::ChangeExtension($checkpoint, '.json')
$policyHash = '13b796c29a0732d6ef10084a8ae525f147c564d0f99616b97132f1929cf582b8'
$expectedHashes = @{
    $checkpoint = '892d18b29da3198475b660f8c8ce605fd2bf7fa2d985148bb145ac79e2c4371b'
    $policy = $policyHash
    'models/bundled/recurrent_g2.json' = $policyHash
}
foreach ($path in $expectedHashes.Keys) {
    $file = Join-Path $repo $path
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "Missing recurrent-G2 input: $file" }
    if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash -ne $expectedHashes[$path]) {
        throw "SHA-256 mismatch for recurrent-G2 input: $file"
    }
}

$lanes = if ($Quick) { 32 } else { 512 }
if (-not $Output) {
    $Output = if ($Quick) { 'build/recurrent-ppo/recurrent-g2-plus-smoke' }
              else { 'build/recurrent-ppo/recurrent-g2-plus-24h' }
}
$params = @('-Output', $Output, '-InitCheckpoint', $checkpoint, '-Gpu', '-FixedEquipment',
    '-Lanes', "$lanes", '-Threads', '8', '-TorchThreads', '2', '-MinibatchTokens', '2048')
if ($Quick) { $params += '-Quick' }
if ($Resume) { $params += '-Resume' }
& (Join-Path $scriptDirectory 'run-nextgen.bat') @params
exit $LASTEXITCODE
