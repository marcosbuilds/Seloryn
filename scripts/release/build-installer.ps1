[CmdletBinding()]
param(
    [string]$OutputDirectory = (Join-Path (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path 'dist\installer'),
    [string]$InstallRoot = (Join-Path ([IO.Path]::GetTempPath()) ("seloryn-installer-build-{0}" -f $PID))
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Push-Location $root
try {
    New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
    if (Test-Path -LiteralPath $InstallRoot) { Remove-Item -LiteralPath $InstallRoot -Recurse -Force }

    cargo build --release --locked --bin seloryn
    if ($LASTEXITCODE -ne 0) { throw 'runtime build failed' }
    cargo build --release --locked --bin seloryn-launcher
    if ($LASTEXITCODE -ne 0) { throw 'launcher build failed' }

    $env:SELORYN_RUNTIME_PAYLOAD = Join-Path $root 'target\release\seloryn.exe'
    $env:SELORYN_LAUNCHER_PAYLOAD = Join-Path $root 'target\release\seloryn-launcher.exe'
    $env:SELORYN_FONT_PAYLOAD = Join-Path $root 'assets\seloryn-display.ttf'
    $env:SELORYN_PANEL_PAYLOAD = Join-Path $root 'assets\seloryn-installer-panel.bmp'
    cargo build --release --locked --bin seloryn-setup
    if ($LASTEXITCODE -ne 0) { throw 'setup build failed' }

    $setup = Join-Path $root 'target\release\seloryn-setup.exe'
    $artifact = Join-Path $OutputDirectory 'SelorynSetup-0.12.0-windows-x64.exe'
    Copy-Item -LiteralPath $setup -Destination $artifact -Force
    $hash = (Get-FileHash -LiteralPath $artifact -Algorithm SHA256).Hash.ToLowerInvariant()
    [ordered]@{
        schema = 'seloryn.installer.v1'
        installer_version = '0.12.0'
        language_version = '0.12'
        runtime_package = '0.12.0'
        platform = 'windows-x64'
        artifact = Split-Path -Leaf $artifact
        sha256 = $hash
        payload_runtime_sha256 = (Get-FileHash -LiteralPath $env:SELORYN_RUNTIME_PAYLOAD -Algorithm SHA256).Hash.ToLowerInvariant()
        payload_launcher_sha256 = (Get-FileHash -LiteralPath $env:SELORYN_LAUNCHER_PAYLOAD -Algorithm SHA256).Hash.ToLowerInvariant()
    } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'installer-manifest.json') -Encoding UTF8

    $setupStdout = Join-Path $OutputDirectory 'setup.stdout.txt'
    $setupStderr = Join-Path $OutputDirectory 'setup.stderr.txt'
    $setupProcess = Start-Process -FilePath $artifact -ArgumentList @('--headless', '--install-root', $InstallRoot, '--make-default') -Wait -PassThru -NoNewWindow -RedirectStandardOutput $setupStdout -RedirectStandardError $setupStderr
    Write-Output ("setup_exit={0}" -f $setupProcess.ExitCode)
    if (Test-Path -LiteralPath $setupStdout) { Get-Content -LiteralPath $setupStdout }
    if (Test-Path -LiteralPath $setupStderr) { Get-Content -LiteralPath $setupStderr }
    if ($setupProcess.ExitCode -ne 0) { throw 'embedded headless installation failed' }
    $runtime = Join-Path $InstallRoot 'versions\0.12.0\bin\seloryn.exe'
    $launcher = Join-Path $InstallRoot 'bin\seloryn0.12.0.exe'
    if (-not (Test-Path -LiteralPath $runtime -PathType Leaf)) { throw 'installed runtime is missing' }
    if (-not (Test-Path -LiteralPath $launcher -PathType Leaf)) { throw 'versioned launcher is missing' }
    $version = (& $runtime --version | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $version -notmatch '^seloryn 0\.12 package=0\.12\.0 ') { throw 'installed runtime version check failed' }
    $env:SELORYN_HOME = $InstallRoot
    $listed = (& $launcher versions | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $listed -notmatch '0\.12\.0') { throw 'installed launcher registry check failed' }
    $defaultLauncher = Join-Path $InstallRoot 'bin\seloryn.exe'
    $defaultVersion = (& $defaultLauncher --version | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $defaultVersion -notmatch '^seloryn 0\.12 package=0\.12\.0 ') { throw 'default launcher dispatch check failed' }
    Write-Output ("installer artifact={0} sha256={1} install_root={2}" -f $artifact, $hash, $InstallRoot)
}
finally {
    Remove-Item Env:SELORYN_RUNTIME_PAYLOAD -ErrorAction SilentlyContinue
    Remove-Item Env:SELORYN_LAUNCHER_PAYLOAD -ErrorAction SilentlyContinue
    Remove-Item Env:SELORYN_FONT_PAYLOAD -ErrorAction SilentlyContinue
    Remove-Item Env:SELORYN_PANEL_PAYLOAD -ErrorAction SilentlyContinue
    Remove-Item Env:SELORYN_HOME -ErrorAction SilentlyContinue
    Pop-Location
}
