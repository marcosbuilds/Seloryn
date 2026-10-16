$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$installer = Join-Path $root 'scripts\install\install-seloryn-windows.ps1'
$binary = Join-Path $root 'target\release\seloryn.exe'
$installRoot = Join-Path ([IO.Path]::GetTempPath()) ("seloryn-installer-probe-{0}" -f $PID)

try {
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $installer `
        -Action Install -BinaryPath $binary -InstallRoot $installRoot `
        -ExpectedLanguageVersion '0.07' -ExpectedRuntimeVersion '0.7.0' | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'initial installer invocation failed' }
    $manifestPath = Join-Path $installRoot 'seloryn-install.json'
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.language_version -ne '0.07' -or $manifest.runtime_package -ne '0.7.0') {
        throw 'installer manifest did not persist the binary compatibility line'
    }

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
