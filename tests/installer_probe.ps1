$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$installer = Join-Path $root 'scripts\install\install-seloryn-windows.ps1'
$binary = Join-Path $root 'target\release\seloryn.exe'
$installRoot = Join-Path ([IO.Path]::GetTempPath()) ("seloryn-installer-probe-{0}" -f $PID)
$pathEntry = Join-Path $installRoot 'bin'

function User-PathHas([string]$Entry) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ([string]::IsNullOrWhiteSpace($userPath)) { return $false }
    return @($userPath -split [IO.Path]::PathSeparator | Where-Object {
        $_.Trim().TrimEnd('\') -ieq $Entry.Trim().TrimEnd('\')
    }).Count -gt 0
}

function Remove-User-Path([string]$Entry) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ([string]::IsNullOrWhiteSpace($userPath)) { return }
    $remaining = @($userPath -split [IO.Path]::PathSeparator | Where-Object {
        $_.Trim().TrimEnd('\') -ine $Entry.Trim().TrimEnd('\') -and $_
    })
    $value = if ($remaining.Count -gt 0) { $remaining -join [IO.Path]::PathSeparator } else { $null }
    [Environment]::SetEnvironmentVariable('Path', $value, 'User')
}

try {
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $installer `
        -Action Install -BinaryPath $binary -InstallRoot $installRoot `
        -ExpectedLanguageVersion '0.12' -ExpectedRuntimeVersion '0.12.0' -AddToPath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'initial installer invocation failed' }
    $manifestPath = Join-Path $installRoot 'seloryn-install.json'
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.language_version -ne '0.12' -or $manifest.runtime_package -ne '0.12.0') {
        throw 'installer manifest did not persist the binary compatibility line'
    }
    if (-not $manifest.path_added -or -not (User-PathHas $pathEntry)) {
        throw 'installer did not persist the requested user PATH entry'
    }
    $diagnosis = (& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $installer `
        -Action DiagnosePath -InstallRoot $installRoot | ConvertFrom-Json)
    if (-not $diagnosis.user_path -or $diagnosis.bin -ne $pathEntry) {
        throw 'installer PATH diagnosis did not observe the user PATH entry'
    }
    $installedBinary = Join-Path $installRoot 'bin\seloryn.exe'
    $project = Join-Path $root 'examples\projects\self-hosted-language-model'
    & $installedBinary check $project --format=json | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'installed binary rejected the language-model project check' }
    $streamInput = Join-Path $installRoot 'stream.input'
    $streamStdout = Join-Path $installRoot 'stream.stdout'
    $streamStderr = Join-Path $installRoot 'stream.stderr'
    $utf8 = New-Object System.Text.UTF8Encoding($false)
    [IO.File]::WriteAllText($streamInput, '{"mode":"train"}', $utf8)
    $command = 'type "' + $streamInput + '" | ""' + $installedBinary + '"" run ""' + $project + '"" --format=json --log=json2 > "' + $streamStdout + '" 2> "' + $streamStderr + '"'
    & cmd.exe /d /c $command
    $streamExit = $LASTEXITCODE
    $streamOutput = Get-Content -LiteralPath $streamStdout -Raw
    if ($streamExit -ne 0 -or $streamOutput -notmatch 'bigram_softmax') {
        throw 'installed binary did not execute the language-model training project'
    }
    $verified = & python (Join-Path $root 'scripts\streaming\verify-event-chain.py') $streamStderr | Out-String
    if ($LASTEXITCODE -ne 0 -or $verified -notmatch 'stream valid: 23 events') {
        throw 'installed binary JSON2 receipt did not verify'
    }

    $provenancePath = Join-Path $installRoot 'provenance.json'
    $binaryHash = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
    [ordered]@{
        schema = 'seloryn.provenance.v1'
        language_version = '0.12'
        runtime_package = '0.12.0'
        runtime = [ordered]@{ binary_sha256 = $binaryHash }
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $provenancePath -Encoding UTF8
    $invalidProvenancePath = Join-Path $installRoot 'invalid-provenance.json'
    [ordered]@{
        schema = 'seloryn.provenance.v1'
        language_version = '0.12'
        runtime_package = '0.12.0'
        runtime = [ordered]@{ binary_sha256 = [string]::new('0', 64) }
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $invalidProvenancePath -Encoding UTF8
    $invalidInstall = Start-Process -FilePath 'powershell.exe' -ArgumentList @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $installer,
        '-Action', 'Install', '-BinaryPath', $binary, '-InstallRoot', $installRoot,
        '-ProvenancePath', $invalidProvenancePath
    ) -Wait -PassThru -NoNewWindow
    if ($invalidInstall.ExitCode -eq 0) { throw 'installer accepted a mismatched provenance hash' }
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $installer `
        -Action Install -BinaryPath $binary -InstallRoot $installRoot `
        -ProvenancePath $provenancePath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'installer rejected valid provenance' }

    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $installer `
        -Action Install -BinaryPath $binary -InstallRoot $installRoot | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'upgrade installer invocation failed' }
    $updated = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if (-not $updated.backup_relative) { throw 'upgrade did not create a rollback copy' }

    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $installer -Action Rollback -InstallRoot $installRoot | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'rollback invocation failed' }
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $installer -Action Uninstall -InstallRoot $installRoot -RemoveFromPath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'uninstall invocation failed' }
    if (Test-Path -LiteralPath $manifestPath) { throw 'uninstall left the installation manifest behind' }
    if (User-PathHas $pathEntry) { throw 'uninstall left the temporary user PATH entry behind' }
    Write-Output 'installer probe passed'
} finally {
    Remove-User-Path $pathEntry
    if (Test-Path -LiteralPath $installRoot) {
        Remove-Item -LiteralPath $installRoot -Recurse -Force
    }
}
