@echo off
setlocal DisableDelayedExpansion
set "PVPBOT_BATCH_FILE=%~f0"
"%~dp0..\run-batch.bat" %*
exit /b %errorlevel%

# PowerShell
# Rust-only checks. This script never invokes Java, Gradle, or Minecraft.
$ErrorActionPreference = 'Stop'
$runner = Join-Path $scriptDirectory 'cargo.bat'
& $runner fmt --all '--' --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$metadataJson = & $runner metadata --no-deps --locked --format-version 1
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$metadata = $metadataJson | ConvertFrom-Json
$allowed = @{
    'pvp-sim' = @()
    'pvp-policy' = @('pvp-sim')
    'pvp-cli' = @('pvp-sim', 'pvp-policy')
    'pvp-play' = @('pvp-sim', 'pvp-policy')
    'pvp-rl' = @('pvp-sim', 'pvp-policy')
}
$members = @($metadata.packages | Where-Object { $_.id -in $metadata.workspace_members })
foreach ($package in $members) {
    if (-not $allowed.ContainsKey($package.name)) {
        throw "Declare the allowed dependencies of $($package.name) in the Rust checks."
    }
    foreach ($dependency in $package.dependencies) {
        if ($dependency.name -in $members.name -and $dependency.name -notin $allowed[$package.name]) {
            throw "$($package.name) must not depend on $($dependency.name). See docs/architecture.md."
        }
    }
}

& $runner clippy --workspace --all-targets --locked '--' -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& $runner test --workspace --all-targets --locked
exit $LASTEXITCODE
