@echo off
setlocal DisableDelayedExpansion
set "PVPBOT_BATCH_FILE=%~f0"
"%~dp0..\..\run-batch.bat" %*
exit /b %errorlevel%

# PowerShell
param(
    [string]$LoomCache = (Join-Path $env:USERPROFILE '.gradle/caches/fabric-loom')
)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $scriptDirectory '../../..'))
$output = Join-Path $repo 'build/phase1'
New-Item -ItemType Directory -Force $output | Out-Null
Add-Type -AssemblyName System.IO.Compression.FileSystem

function Read-ZipText($zip, [string]$name) {
    $entry = $zip.GetEntry($name)
    if ($null -eq $entry) { throw "Missing archive entry: $name" }
    $reader = [IO.StreamReader]::new($entry.Open())
    try { return $reader.ReadToEnd() } finally { $reader.Dispose() }
}

function Export-Entry($entry, [string]$root, [string]$relative) {
    $rootFull = [IO.Path]::GetFullPath($root) + [IO.Path]::DirectorySeparatorChar
    $target = [IO.Path]::GetFullPath((Join-Path $root $relative))
    if (!$target.StartsWith($rootFull, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Archive path escapes destination: $relative"
    }
    $null = [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target))
    [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $target, $true)
    return $target
}

$expected = @{
    client = '4e618f09a0c649dde3fdf829df443ce0b8831e65'
    server = '97ccd4c0ed3f81bbb7bfacddd1090b0c56f9bc51'
}
$artifacts = @()
foreach ($side in @('client', 'server')) {
    $path = Join-Path $LoomCache "26.1.2/minecraft-$side.jar"
    $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA1).Hash.ToLowerInvariant()
    if ($hash -ne $expected[$side]) { throw "Minecraft $side SHA-1 mismatch: $hash" }
    $artifacts += [ordered]@{
        name = "minecraft-$side.jar"; sha1 = $hash
        sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        url = "https://piston-data.mojang.com/v1/objects/$hash/$side.jar"
    }
}

$archives = @()
$sourceRoot = Join-Path $repo 'fabric/.gradle/loom-cache/minecraftMaven/net/minecraft'
foreach ($side in @('common', 'client')) {
    $namePattern = if ($side -eq 'common') { 'minecraft-common-*' } else { 'minecraft-clientOnly-*' }
    $candidates = @(Get-ChildItem -Path "$sourceRoot/$namePattern/26.1.2/*-sources.jar")
    if ($candidates.Count -ne 1) {
        throw "Expected one $side sources jar; found $($candidates.Count). Run .\fabric\gradlew.bat -p fabric genSources first."
    }
    $archive = $candidates[0]
    $archives += [ordered]@{ side = $side; name = $archive.Name; sha256 = (Get-FileHash $archive.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
    $zip = [IO.Compression.ZipFile]::OpenRead($archive.FullName)
    try {
        foreach ($entry in $zip.Entries) {
            if ($entry.FullName.StartsWith('net/minecraft/') -and $entry.FullName.EndsWith('.java')) {
                $null = Export-Entry $entry (Join-Path $output "sources-$side") $entry.FullName
            }
        }
    } finally { $zip.Dispose() }
}

# Use the libraries and inner server supplied by the verified official bundle.
# No server process is started, no world is created, and no EULA is modified.
$bundle = [IO.Compression.ZipFile]::OpenRead((Join-Path $LoomCache '26.1.2/minecraft-server.jar'))
$runtime = @()
try {
    foreach ($kind in @('versions', 'libraries')) {
        $list = Read-ZipText $bundle "META-INF/$kind.list"
        foreach ($line in ($list -split '\r?\n')) {
            if ([string]::IsNullOrWhiteSpace($line)) { continue }
            $parts = $line -split '\t'
            $entryName = "META-INF/$kind/$($parts[2])"
            $entry = $bundle.GetEntry($entryName)
            if ($null -eq $entry) { throw "Missing $entryName" }
            $path = Export-Entry $entry (Join-Path $output 'runtime') "$kind/$($parts[2])"
            $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($hash -ne $parts[0]) { throw "Bundled SHA-256 mismatch: $entryName" }
            $runtime += [ordered]@{ name = "$kind/$($parts[2])"; sha256 = $hash }
        }
    }
} finally { $bundle.Dispose() }

$serverPath = @(Get-ChildItem (Join-Path $output 'runtime/versions') -Recurse -Filter '*.jar')
if ($serverPath.Count -ne 1) { throw 'Expected one bundled server runtime' }
$serverZip = [IO.Compression.ZipFile]::OpenRead($serverPath[0].FullName)
try {
    $version = (Read-ZipText $serverZip 'version.json') | ConvertFrom-Json
    foreach ($entry in $serverZip.Entries) {
        if ($entry.FullName -match '^data/minecraft/(enchantment/protection\.json|damage_type/(player_attack|fall)\.json|tags/damage_type/[^/]+\.json)$') {
            $null = Export-Entry $entry (Join-Path $output 'resources') $entry.FullName
        }
    }
} finally { $serverZip.Dispose() }

$record = [ordered]@{
    version = $version
    evidence_kind = 'Local official jars checked against pinned Mojang SHA-1; bundled files checked against embedded SHA-256; generated Loom sources'
    artifacts = $artifacts
    source_archives = $archives
    bundled_runtime = $runtime
}
$record | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8 (Join-Path $output 'provenance.json')
Write-Output "Prepared Minecraft $($version.id) sources, data, and verified runtime in $output"
