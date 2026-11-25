@echo off
setlocal DisableDelayedExpansion
set "PVPBOT_BATCH_FILE=%~f0"
"%~dp0..\..\run-batch.bat" %*
exit /b %errorlevel%

# PowerShell
param(
    [string]$Output = 'build/g3/run',
    [string]$Python = 'build/rl-gpu-bench-venv/Scripts/python.exe',
    [ValidateRange(1,24)][int]$BudgetHours = 24,
    [switch]$Quick,
    [switch]$Resume
)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $scriptDirectory '../../..'))
Push-Location $repo
$campaignLock = $null
try {
    $Output = [IO.Path]::GetFullPath($ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Output))
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Output)) | Out-Null
    $campaignLock = [IO.File]::Open("$Output.lock", 'OpenOrCreate', 'ReadWrite', 'None')
    if (!(Test-Path -LiteralPath $Python -PathType Leaf)) { throw "GPU Python executable missing: $Python" }
    $budgetFile = Join-Path $Output 'budget.json'
    if ($Resume) {
        $budget = Get-Content -LiteralPath $budgetFile -Raw | ConvertFrom-Json
        if ($budget.quick -ne [bool]$Quick) { throw 'Repeat the same quick/full mode when resuming.' }
    } else {
        if (Test-Path -LiteralPath $Output) { throw 'Choose a fresh output directory or resume the existing campaign.' }
        $budget = @{budgetHours=$BudgetHours; quick=[bool]$Quick}
        New-Item -ItemType Directory -Path $Output | Out-Null
        $budget | ConvertTo-Json | Set-Content -LiteralPath $budgetFile -Encoding ascii
    }
    & $Python -B -u (Join-Path $scriptDirectory 'g3_budget.py') $Output $BudgetHours ([string][bool]$Quick)
    exit $LASTEXITCODE
} finally {
    if ($null -ne $campaignLock) { $campaignLock.Dispose() }
    Pop-Location
}
