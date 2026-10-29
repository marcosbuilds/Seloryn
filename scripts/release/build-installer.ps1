[CmdletBinding()]
param(
    [string]$OutputDirectory = (Join-Path (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path 'dist\installer'),
    [string]$InstallRoot = (Join-Path ([IO.Path]::GetTempPath()) ("seloryn-installer-build-{0}" -f $PID)),
    [string]$DotnetPath = 'D:\Toolchains\dotnet\dotnet.exe'
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$previousDotnetRoot = $env:DOTNET_ROOT
$previousDotnetCliHome = $env:DOTNET_CLI_HOME
Push-Location $root
try {
    function Read-Current-Version([string]$Key) {
        $inCurrent = $false
        foreach ($line in Get-Content -LiteralPath (Join-Path $root 'VERSION.toml')) {
            $trimmed = $line.Trim()
            if ($trimmed -eq '[current]') {
                $inCurrent = $true
                continue
            }
            if ($inCurrent -and $trimmed -match '^\[') { break }
            if ($inCurrent -and $trimmed -match ('^{0}\s*=\s*"([^"]+)"$' -f [regex]::Escape($Key))) {
                return $Matches[1]
            }
        }
        throw "VERSION.toml current.$Key is missing"
    }

    $languageVersion = Read-Current-Version 'language'
    $runtimeVersion = Read-Current-Version 'runtime_package'
    New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
    if (Test-Path -LiteralPath $InstallRoot) { Remove-Item -LiteralPath $InstallRoot -Recurse -Force }

    cargo build --release --locked --bin seloryn
    if ($LASTEXITCODE -ne 0) { throw 'runtime build failed' }
    cargo build --release --locked --bin seloryn-launcher
    if ($LASTEXITCODE -ne 0) { throw 'launcher build failed' }

    $env:SELORYN_RUNTIME_PAYLOAD = Join-Path $root 'target\release\seloryn.exe'
    $env:SELORYN_LAUNCHER_PAYLOAD = Join-Path $root 'target\release\seloryn-launcher.exe'
    $env:SELORYN_FONT_PAYLOAD = Join-Path $root 'assets\fonts\seloryn-font-bold.ttf'
    $env:SELORYN_BODY_FONT_PAYLOAD = Join-Path $root 'assets\fonts\seloryn-font-light.ttf'
    $env:SELORYN_PACKAGE_IMAGE_PAYLOAD = Join-Path $root 'assets\installer\seloryn-installation-package.bmp'
    cargo build --release --locked --bin seloryn-setup
    if ($LASTEXITCODE -ne 0) { throw 'setup build failed' }

    $setup = Join-Path $root 'target\release\seloryn-setup.exe'
    $backendPayload = Join-Path $root 'installer-ui\Assets\seloryn-setup-backend.exe'
    Copy-Item -LiteralPath $setup -Destination $backendPayload -Force

    if (-not (Test-Path -LiteralPath $DotnetPath -PathType Leaf)) {
        $dotnetCommand = Get-Command dotnet.exe -ErrorAction SilentlyContinue
        if ($null -eq $dotnetCommand) { throw "dotnet SDK not found at $DotnetPath or on PATH" }
        $DotnetPath = $dotnetCommand.Source
    }
    $dotnetRoot = Split-Path -Parent $DotnetPath
    $dotnetHome = 'D:\Toolchains\dotnet-home'
    if (Test-Path -LiteralPath $dotnetHome -PathType Container) {
        $env:DOTNET_CLI_HOME = $dotnetHome
    }
    $env:DOTNET_ROOT = $dotnetRoot

    $betaPackageName = 'SelorynSetup-experimental-beta-windows-x64'
    $uiPublish = Join-Path $OutputDirectory $betaPackageName
    if (Test-Path -LiteralPath $uiPublish) { Remove-Item -LiteralPath $uiPublish -Recurse -Force }
    & $DotnetPath publish (Join-Path $root 'installer-ui\SelorynInstaller.csproj') `
        -c Release -p:Platform=x64 -r win-x64 --self-contained true `
        -p:PublishTrimmed=false -p:PublishReadyToRun=false -o $uiPublish
    if ($LASTEXITCODE -ne 0) { throw 'WinUI frontend publish failed' }

    $artifact = Join-Path $OutputDirectory ($betaPackageName + '.zip')
    if (Test-Path -LiteralPath $artifact) { Remove-Item -LiteralPath $artifact -Force }
    Compress-Archive -Path (Join-Path $uiPublish '*') -DestinationPath $artifact -CompressionLevel Optimal
    $headlessArtifact = Join-Path $OutputDirectory ($betaPackageName + '-headless.exe')
    Copy-Item -LiteralPath $setup -Destination $headlessArtifact -Force
    $hash = (Get-FileHash -LiteralPath $artifact -Algorithm SHA256).Hash.ToLowerInvariant()
    [ordered]@{
        schema = 'seloryn.installer.v1'
        installer_version = 'experimental-beta'
        channel = 'experimental-beta'
        versioning = 'not-public'
        language_version = $languageVersion
        runtime_package = $runtimeVersion
        platform = 'windows-x64'
        artifact = Split-Path -Leaf $artifact
        sha256 = $hash
        modern_ui_entrypoint = 'SelorynSetup.exe'
        modern_ui_directory = Split-Path -Leaf $uiPublish
        headless_artifact = Split-Path -Leaf $headlessArtifact
        payload_runtime_sha256 = (Get-FileHash -LiteralPath $env:SELORYN_RUNTIME_PAYLOAD -Algorithm SHA256).Hash.ToLowerInvariant()
        payload_launcher_sha256 = (Get-FileHash -LiteralPath $env:SELORYN_LAUNCHER_PAYLOAD -Algorithm SHA256).Hash.ToLowerInvariant()
    } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'installer-manifest.json') -Encoding UTF8

    $setupStdout = Join-Path $OutputDirectory 'setup.stdout.txt'
    $setupStderr = Join-Path $OutputDirectory 'setup.stderr.txt'
    $setupProcess = Start-Process -FilePath $headlessArtifact -ArgumentList @('--headless', '--install-root', $InstallRoot, '--make-default') -Wait -PassThru -NoNewWindow -RedirectStandardOutput $setupStdout -RedirectStandardError $setupStderr
    Write-Output ("setup_exit={0}" -f $setupProcess.ExitCode)
    if (Test-Path -LiteralPath $setupStdout) { Get-Content -LiteralPath $setupStdout }
    if (Test-Path -LiteralPath $setupStderr) { Get-Content -LiteralPath $setupStderr }
    if ($setupProcess.ExitCode -ne 0) { throw 'embedded headless installation failed' }
    $runtime = Join-Path $InstallRoot ("versions\{0}\bin\seloryn.exe" -f $runtimeVersion)
    $launcher = Join-Path $InstallRoot ("bin\seloryn{0}.exe" -f $runtimeVersion)
    if (-not (Test-Path -LiteralPath $runtime -PathType Leaf)) { throw 'installed runtime is missing' }
    if (-not (Test-Path -LiteralPath $launcher -PathType Leaf)) { throw 'versioned launcher is missing' }
    $version = (& $runtime --version | Out-String).Trim()
    $versionPrefix = '^seloryn {0} package={1} ' -f [regex]::Escape($languageVersion), [regex]::Escape($runtimeVersion)
    if ($LASTEXITCODE -ne 0 -or $version -notmatch $versionPrefix) { throw 'installed runtime version check failed' }
    $env:SELORYN_HOME = $InstallRoot
    $listed = (& $launcher versions | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $listed -notmatch [regex]::Escape($runtimeVersion)) { throw 'installed launcher registry check failed' }
    $defaultLauncher = Join-Path $InstallRoot 'bin\seloryn.exe'
    $defaultVersion = (& $defaultLauncher --version | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $defaultVersion -notmatch $versionPrefix) { throw 'default launcher dispatch check failed' }
    Write-Output ("installer artifact={0} sha256={1} headless={2} install_root={3}" -f $artifact, $hash, $headlessArtifact, $InstallRoot)
}
finally {
    Remove-Item Env:SELORYN_RUNTIME_PAYLOAD -ErrorAction SilentlyContinue
    Remove-Item Env:SELORYN_LAUNCHER_PAYLOAD -ErrorAction SilentlyContinue
    Remove-Item Env:SELORYN_FONT_PAYLOAD -ErrorAction SilentlyContinue
    Remove-Item Env:SELORYN_BODY_FONT_PAYLOAD -ErrorAction SilentlyContinue
    Remove-Item Env:SELORYN_PACKAGE_IMAGE_PAYLOAD -ErrorAction SilentlyContinue
    Remove-Item Env:SELORYN_HOME -ErrorAction SilentlyContinue
    if ($null -eq $previousDotnetRoot) { Remove-Item Env:DOTNET_ROOT -ErrorAction SilentlyContinue } else { $env:DOTNET_ROOT = $previousDotnetRoot }
    if ($null -eq $previousDotnetCliHome) { Remove-Item Env:DOTNET_CLI_HOME -ErrorAction SilentlyContinue } else { $env:DOTNET_CLI_HOME = $previousDotnetCliHome }
    Pop-Location
}
