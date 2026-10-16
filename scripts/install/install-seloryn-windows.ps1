[CmdletBinding()]
param(
    [ValidateSet('Install', 'Uninstall', 'Rollback', 'DiagnosePath')]
    [string]$Action = 'Install',
    [string]$BinaryPath,
    [string]$InstallRoot = $(if ($env:LOCALAPPDATA) { Join-Path $env:LOCALAPPDATA 'Seloryn' } else { Join-Path $env:TEMP 'Seloryn' }),
    [string]$ExpectedSha256,
    [string]$ProvenancePath,
    [string]$ExpectedLanguageVersion,
    [string]$ExpectedRuntimeVersion
)

$ErrorActionPreference = 'Stop'

function Fail([string]$Message) {
    throw "SEL-I001 $Message"
}

function FullPath([string]$Path) {
    return [IO.Path]::GetFullPath($Path)
}

function Assert-SafeRoot([string]$Root) {
    if ((FullPath $Root) -eq [IO.Path]::GetPathRoot((FullPath $Root))) {
        Fail 'install root may not be a filesystem root'
    }
}

function ManifestPath([string]$Root) {
    return Join-Path $Root 'seloryn-install.json'
}

function Read-Manifest([string]$Root) {
    $path = ManifestPath $Root
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        Fail "installation manifest is missing: $path"
    }
    return Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
}

function Safe-InstalledPath([string]$Root, [string]$Relative) {
    if ([IO.Path]::IsPathRooted($Relative) -or $Relative.Contains('..')) {
        Fail 'installation manifest contains an unsafe relative path'
    }
    $rootFull = FullPath $Root
    $pathFull = FullPath (Join-Path $Root $Relative)
    if (-not $pathFull.StartsWith($rootFull + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        Fail 'installation path escapes the install root'
    }
    return $pathFull
}

function Write-Result($Value) {
    $Value | ConvertTo-Json -Compress -Depth 8
}

Assert-SafeRoot $InstallRoot
$root = FullPath $InstallRoot
$manifest = ManifestPath $root

switch ($Action) {
    'Install' {
        if (-not $BinaryPath) { Fail 'Install requires -BinaryPath' }
        $source = FullPath $BinaryPath
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { Fail "binary is missing: $source" }
        $hash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($ExpectedSha256 -and $hash -ne $ExpectedSha256.ToLowerInvariant()) {
            Fail "SHA-256 mismatch: expected $ExpectedSha256, observed $hash"
        }
        $versionOutput = (& $source --version 2>&1 | Out-String).Trim()
        if ($LASTEXITCODE -ne 0 -or $versionOutput -notmatch '^seloryn ([^ ]+) package=([^ ]+) ') {
            Fail 'binary did not return a canonical Seloryn version record'
        }
        $languageVersion = $Matches[1]
        $runtimeVersion = $Matches[2]
        if ($ExpectedLanguageVersion -and $languageVersion -ne $ExpectedLanguageVersion) {
            Fail "language version mismatch: expected $ExpectedLanguageVersion, observed $languageVersion"
        }
        if ($ExpectedRuntimeVersion -and $runtimeVersion -ne $ExpectedRuntimeVersion) {
            Fail "runtime package mismatch: expected $ExpectedRuntimeVersion, observed $runtimeVersion"
        }
        $provenance = $null
        if ($ProvenancePath) {
            if (-not (Test-Path -LiteralPath $ProvenancePath -PathType Leaf)) { Fail "provenance is missing: $ProvenancePath" }
            $provenance = Get-Content -LiteralPath $ProvenancePath -Raw | ConvertFrom-Json
        }
        New-Item -ItemType Directory -Force -Path (Join-Path $root 'bin'), (Join-Path $root 'rollback') | Out-Null
        $destinationRelative = 'bin/seloryn.exe'
        $destination = Safe-InstalledPath $root $destinationRelative
        $backupRelative = $null
        if (Test-Path -LiteralPath $destination -PathType Leaf) {
            $stamp = [DateTime]::UtcNow.ToString('yyyyMMddHHmmssfff')
            $backupRelative = "rollback/seloryn-$stamp.exe"
            Copy-Item -LiteralPath $destination -Destination (Safe-InstalledPath $root $backupRelative)
        }
        Copy-Item -LiteralPath $source -Destination $destination -Force
        $record = [ordered]@{
            schema = 1
            artifact = 'seloryn-cli'
            platform = 'windows-x64'
            binary_relative = $destinationRelative
            sha256 = $hash
            language_version = $languageVersion
            runtime_package = $runtimeVersion
            version_output = $versionOutput
            source = $source
            build_id = $hash.Substring(0, 16)
            installed_utc = [DateTime]::UtcNow.ToString('o')
            backup_relative = $backupRelative
            provenance = $provenance
            path_entry = (Join-Path $root 'bin')
        }
        $record | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifest -Encoding UTF8
        Write-Result ([ordered]@{ action = 'install'; artifact = 'seloryn-cli'; platform = 'windows-x64'; sha256 = $hash; language_version = $languageVersion; runtime_package = $runtimeVersion; path_entry = (Join-Path $root 'bin'); rollback = [bool]$backupRelative })
    }
    'Uninstall' {
        $record = Read-Manifest $root
        $destination = Safe-InstalledPath $root ([string]$record.binary_relative)
        if (Test-Path -LiteralPath $destination -PathType Leaf) { Remove-Item -LiteralPath $destination -Force }
        Remove-Item -LiteralPath $manifest -Force
        Write-Result ([ordered]@{ action = 'uninstall'; artifact = 'seloryn-cli'; platform = 'windows-x64'; path_entry_preserved = $true })
    }
    'Rollback' {
        $record = Read-Manifest $root
        if (-not $record.backup_relative) { Fail 'no previous binary is available for rollback' }
        $backup = Safe-InstalledPath $root ([string]$record.backup_relative)
        if (-not (Test-Path -LiteralPath $backup -PathType Leaf)) { Fail "rollback binary is missing: $backup" }
        $destination = Safe-InstalledPath $root ([string]$record.binary_relative)
        Copy-Item -LiteralPath $backup -Destination $destination -Force
        $hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
        $record.sha256 = $hash
        $record | Add-Member -NotePropertyName rolled_back_utc -NotePropertyValue ([DateTime]::UtcNow.ToString('o')) -Force
        $record | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifest -Encoding UTF8
        Write-Result ([ordered]@{ action = 'rollback'; artifact = 'seloryn-cli'; platform = 'windows-x64'; sha256 = $hash })
    }
    'DiagnosePath' {
        $bin = Join-Path $root 'bin'
        $entries = @($env:PATH -split [IO.Path]::PathSeparator | Where-Object { $_ })
        $present = $entries | Where-Object { (FullPath $_) -eq (FullPath $bin) }
        Write-Result ([ordered]@{ action = 'diagnose-path'; bin = $bin; on_path = [bool]$present; fix = "Add $bin to PATH for this user or shell" })
    }
}
