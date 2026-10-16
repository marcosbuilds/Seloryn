$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$installer = Join-Path $root 'scripts\install\install-seloryn-windows.ps1'
$binary = Join-Path $root 'target\release\seloryn.exe'
$installRoot = Join-Path ([IO.Path]::GetTempPath()) ("seloryn-installer-probe-{0}" -f $PID)

try {
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $installer `
        -Action Install -BinaryPath $binary -InstallRoot $installRoot `
        -ExpectedLanguageVersion '0.09' -ExpectedRuntimeVersion '0.9.3' | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'initial installer invocation failed' }
    $manifestPath = Join-Path $installRoot 'seloryn-install.json'
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.language_version -ne '0.09' -or $manifest.runtime_package -ne '0.9.3') {
        throw 'installer manifest did not persist the binary compatibility line'
    }

    $provenancePath = Join-Path $installRoot 'provenance.json'
    $binaryHash = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
    [ordered]@{
        schema = 'seloryn.provenance.v1'
        language_version = '0.09'
        runtime_package = '0.9.3'
        runtime = [ordered]@{ binary_sha256 = $binaryHash }
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $provenancePath -Encoding UTF8
    $invalidProvenancePath = Join-Path $installRoot 'invalid-provenance.json'
    [ordered]@{
        schema = 'seloryn.provenance.v1'
        language_version = '0.09'
        runtime_package = '0.9.3'
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
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $installer -Action Uninstall -InstallRoot $installRoot | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'uninstall invocation failed' }
    if (Test-Path -LiteralPath $manifestPath) { throw 'uninstall left the installation manifest behind' }
    Write-Output 'installer probe passed'
} finally {
    if (Test-Path -LiteralPath $installRoot) {
        Remove-Item -LiteralPath $installRoot -Recurse -Force
    }
}
