@echo off
setlocal DisableDelayedExpansion
set "PVPBOT_BATCH_FILE=%~f0"
"%~dp0..\run-batch.bat" %*
exit /b %errorlevel%

# PowerShell
[CmdletBinding()]
param([switch]$Offline, [switch]$BuildOnly, [switch]$WithExample)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$project = (Resolve-Path -LiteralPath (Join-Path $scriptDirectory '..\..\paper')).Path
$run = Join-Path $project 'run-rl-standalone'
$plugins = Join-Path $run 'plugins'
$installedExample = Join-Path $plugins 'pvpbot-rl-example.jar'
# An installed Example must be updated together with the runtime.
$buildExample = $WithExample -or (-not $BuildOnly -and (Test-Path -LiteralPath $installedExample -PathType Leaf))
$dependencies = Join-Path $project '.gradle\pvpbot-dependencies'
$jdk = if ($env:JAVA_HOME) { Join-Path $env:JAVA_HOME 'bin\javac.exe' } else { 'javac.exe' }
$jdkVersion = & $jdk -version
if ($LASTEXITCODE -ne 0 -or "$jdkVersion" -notmatch '^javac 25(\.|$)') {
    throw "JDK 25 is required. javac -version returned: $jdkVersion"
}

function Get-VerifiedJar {
    param([string]$Name, [string]$Url, [string]$Hash, [string]$Algorithm)
    $path = Join-Path $dependencies $Name
    if (!(Test-Path -LiteralPath $path)) {
        if ($Offline) { throw "Missing pinned dependency in offline mode: $path" }
        $temporary = "$path.$PID.download"
        try {
            Write-Host "Downloading $Name"
            Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $temporary
            if ((Get-FileHash -LiteralPath $temporary -Algorithm $Algorithm).Hash -ne $Hash) {
                throw "$Algorithm mismatch from $Url"
            }
            Move-Item -LiteralPath $temporary -Destination $path
        } finally {
            if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary }
        }
    }
    if ((Get-FileHash -LiteralPath $path -Algorithm $Algorithm).Hash -ne $Hash) {
        throw "$Algorithm mismatch: $path. Move the invalid file aside and rerun setup."
    }
}

$paperJar = 'paper-26.1.2-74.jar'
$citizensJar = 'Citizens-2.0.44-b4270.jar'
New-Item -ItemType Directory -Path $dependencies -Force | Out-Null
Get-VerifiedJar $paperJar 'https://fill-data.papermc.io/v1/objects/1d70b1dab9cf4a6de615209a536f3a45a2186240253c428213ce2188ab95e5f7/paper-26.1.2-74.jar' '1d70b1dab9cf4a6de615209a536f3a45a2186240253c428213ce2188ab95e5f7' SHA256
Get-VerifiedJar $citizensJar 'https://ci.citizensnpcs.co/job/citizens2/4270/artifact/dist/target/Citizens-2.0.44-b4270.jar' '0cdcda6760eee72eebd48e889e8f38779b09619194c8ec589a30d60c8d2409e4' SHA256

$gradleArgs = @('-p', (Join-Path $project 'rlbot'), '--no-daemon', '--max-workers=1', 'build')
if ($Offline) { $gradleArgs += '--offline' }
$previousGradleHome = $env:GRADLE_USER_HOME
$workspaceGradleHome = Join-Path (Split-Path -Parent $project) '.gradle-user-home'
try {
    if (-not $previousGradleHome -and (Test-Path -LiteralPath $workspaceGradleHome -PathType Container)) {
        $env:GRADLE_USER_HOME = $workspaceGradleHome
    }
    & (Join-Path $project 'gradlew.bat') @gradleArgs
    if ($LASTEXITCODE -ne 0) { throw "Gradle build failed with exit code $LASTEXITCODE" }
    if ($buildExample) {
        $apiJar = Join-Path $project 'rlbot\build\libs\pvpbot-rl-0.1.0-api.jar'
        $exampleArgs = @('-p', (Join-Path $project 'examples\rlbot-api'), '--no-daemon', '--max-workers=1',
            "-PpvpBotApiJar=$apiJar", 'build')
        if ($Offline) { $exampleArgs += '--offline' }
        & (Join-Path $project 'gradlew.bat') @exampleArgs
        if ($LASTEXITCODE -ne 0) { throw "Example build failed with exit code $LASTEXITCODE" }
    }
} finally {
    $env:GRADLE_USER_HOME = $previousGradleHome
}
if ($BuildOnly) { return }

$activePorts = [Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners()
if ($activePorts | Where-Object { $_.Port -in 25565,25575 }) {
    throw 'Stop the local Paper server before installing or starting it.'
}

function Install-PinnedJar([string]$Name, [string]$Prefix, [string]$Destination, [string]$PreviousHash) {
    $cached = Join-Path $dependencies $Name
    $existing = @(Get-ChildItem -LiteralPath $Destination -File -Filter "$Prefix*.jar")
    if ($existing.Count -gt 1) { throw "Multiple $Prefix JARs are installed in $Destination." }
    $target = Join-Path $Destination $Name
    if ($existing.Count -eq 0) {
        Copy-Item -LiteralPath $cached -Destination $target
        return
    }
    $installedHash = (Get-FileHash -LiteralPath $existing[0].FullName).Hash
    if ($installedHash -eq (Get-FileHash -LiteralPath $cached).Hash) { return }
    if ($installedHash -ne $PreviousHash) {
        throw "The installed dependency does not match the pinned JAR: $($existing[0].FullName)"
    }

    $backupDirectory = Join-Path $run 'plugin-backups'
    New-Item -ItemType Directory -Path $backupDirectory -Force | Out-Null
    $backup = Join-Path $backupDirectory $existing[0].Name
    $temporary = "$target.$PID.install"
    try {
        Copy-Item -LiteralPath $cached -Destination $temporary
        Move-Item -LiteralPath $existing[0].FullName -Destination $backup
        try {
            Move-Item -LiteralPath $temporary -Destination $target
        } catch {
            Move-Item -LiteralPath $backup -Destination $existing[0].FullName
            throw
        }
        Write-Host "Installed $Name and backed up $($existing[0].Name) to $backupDirectory"
    } finally {
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary }
    }
}

New-Item -ItemType Directory -Path $plugins -Force | Out-Null
Install-PinnedJar $paperJar 'paper-' $run
Install-PinnedJar $citizensJar 'Citizens-' $plugins 'e6b0a0e3346bd2cb982bcd3e1b628143629042e1a50479ae3c24858e2cacde22'
Copy-Item -LiteralPath (Join-Path $project 'rlbot\build\libs\pvpbot-rl-0.1.0.jar') -Destination (Join-Path $plugins 'PvPBotRL.jar') -Force
if ($buildExample) {
    $exampleJar = Join-Path $project 'examples\rlbot-api\build\libs\pvpbot-rl-example.jar'
    Copy-Item -LiteralPath $exampleJar -Destination $installedExample -Force
}

$repo = Split-Path -Parent $project
$trainingRoot = Join-Path $repo 'build\recurrent-ppo'
if ((Test-Path -LiteralPath (Join-Path $trainingRoot 'full')) -or
    (Test-Path -LiteralPath (Join-Path $trainingRoot 'nextgen-24h'))) {
    $trainingPython = Join-Path $repo 'build\rl-venv\Scripts\python.exe'
    if (!(Test-Path -LiteralPath $trainingPython)) {
        $pythonCommand = Get-Command python.exe -ErrorAction SilentlyContinue
        if ($pythonCommand) { $trainingPython = $pythonCommand.Source }
    }
    if (Test-Path -LiteralPath $trainingPython) {
        & $trainingPython -B (Join-Path $scriptDirectory 'import_training_models.py') (Join-Path $plugins 'PvPBotRL\training-models')
        if ($LASTEXITCODE -ne 0) { Write-Warning 'Training history import failed. Check the output above.' }
    } else {
        Write-Warning 'Training history import requires Python 3. Install it or prepare the training environment.'
    }
}

$propertiesPath = Join-Path $run 'server.properties'
$lines = if (Test-Path -LiteralPath $propertiesPath) { @(Get-Content -LiteralPath $propertiesPath) } else { @() }
$password = ($lines | Where-Object { $_ -match '^rcon\.password=[a-zA-Z0-9]+$' } | Select-Object -Last 1) -replace '^rcon\.password=', ''
if (-not $password) { $password = [Guid]::NewGuid().ToString('N') + [Guid]::NewGuid().ToString('N') }
$required = [ordered]@{
    'server-ip' = '127.0.0.1'
    'server-port' = '25565'
    'online-mode' = 'true'
    'enable-rcon' = 'true'
    'rcon.port' = '25575'
    'rcon.password' = $password
    'broadcast-rcon-to-ops' = 'false'
}
$lines = @($lines | Where-Object {
    $key = ($_ -split '[=:\s]', 2)[0]
    -not $required.Contains($key)
})
foreach ($key in $required.Keys) { $lines += "$key=$($required[$key])" }
Set-Content -LiteralPath $propertiesPath -Value $lines -Encoding ASCII
Write-Host "PvPBotRL Paper server prepared in $run"
