@echo off
setlocal DisableDelayedExpansion
set "PSModulePath=%SystemRoot%\System32\WindowsPowerShell\v1.0\Modules"
set "PVPBOT_BATCH_RUNNER=%~f0"
"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -Command "$runner = [IO.File]::ReadAllText($env:PVPBOT_BATCH_RUNNER); & ([scriptblock]::Create(($runner -split '(?m)^# PowerShell\r?\n', 2)[1])); #" %*
exit /b %errorlevel%

# PowerShell
$ErrorActionPreference = 'Stop'
$batchPath = $env:PVPBOT_BATCH_FILE
$scriptDirectory = [IO.Path]::GetDirectoryName($batchPath)
$content = [IO.File]::ReadAllText($batchPath)
$body = [scriptblock]::Create(($content -split '(?m)^# PowerShell\r?\n', 2)[1])

# Read the arguments directly so PowerShell doesn't parse them as command text.
$arguments = [Environment]::GetCommandLineArgs()
$parameters = $body.Ast.ParamBlock.Parameters
$named = @{}
$positional = @()
$readOptions = $parameters.Count -gt 0
for ($index = 5; $index -lt $arguments.Length; $index++) {
    $argument = $arguments[$index]
    if ($readOptions -and $argument -eq '--') {
        $readOptions = $false
        continue
    }
    if (!$readOptions -or $argument -notmatch '^-[a-zA-Z][a-zA-Z0-9]*$') {
        $positional += $argument
        continue
    }

    $option = $argument.Substring(1)
    $parameter = $parameters | Where-Object { $_.Name.VariablePath.UserPath -eq $option }
    if (!$parameter) {
        $parameter = @($parameters | Where-Object { $_.Name.VariablePath.UserPath -like "$option*" })
        if ($parameter.Count -ne 1) { throw "Unknown or ambiguous option: $argument" }
        $parameter = $parameter[0]
    }
    $name = $parameter.Name.VariablePath.UserPath
    if ($named.ContainsKey($name)) { throw "Option supplied more than once: $argument" }
    if ($parameter.StaticType -eq [switch]) {
        $named[$name] = $true
        continue
    }
    $index++
    if ($index -eq $arguments.Length) { throw "Missing value for $argument" }
    $named[$name] = $arguments[$index]
}

& $body @named @positional
exit 0
