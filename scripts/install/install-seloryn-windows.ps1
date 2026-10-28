[CmdletBinding()]
param(
    [ValidateSet('Install', 'Uninstall', 'Rollback', 'DiagnosePath')]
    [string]$Action = 'Install',
    [string]$BinaryPath,
    [string]$InstallRoot = $(if ($env:LOCALAPPDATA) { Join-Path $env:LOCALAPPDATA 'Seloryn' } else { Join-Path $env:TEMP 'Seloryn' }),
    [string]$ExpectedSha256,
    [string]$ProvenancePath,
    [string]$ExpectedLanguageVersion,
    [string]$ExpectedRuntimeVersion,
    [switch]$AddToPath,
    [switch]$RemoveFromPath
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

function Provenance-Field($Document, [string]$Name) {
    if ($null -ne $Document -and $Document.PSObject.Properties.Name -contains $Name) {
        return $Document.$Name
    }
    if ($null -ne $Document -and $null -ne $Document.runtime -and
        $Document.runtime.PSObject.Properties.Name -contains $Name) {
        return $Document.runtime.$Name
    }
    return $null
}

function Verify-Provenance($Document, [string]$BinaryHash, [string]$LanguageVersion, [string]$RuntimeVersion) {
    $schema = Provenance-Field $Document 'provenance_schema'
    if (-not $schema) { $schema = Provenance-Field $Document 'schema' }
    if ($schema -and $schema -ne 'seloryn.provenance.v1') {
        Fail "unsupported provenance schema: $schema"
    }
    $provenanceHash = Provenance-Field $Document 'binary_sha256'
    if ($provenanceHash -and $provenanceHash.ToLowerInvariant() -ne $BinaryHash) {
        Fail "provenance binary SHA-256 mismatch: expected $provenanceHash, observed $BinaryHash"
    }
    $provenanceLanguage = Provenance-Field $Document 'language_version'
    if ($provenanceLanguage -and $provenanceLanguage -ne $LanguageVersion) {
        Fail "provenance language mismatch: expected $provenanceLanguage, observed $LanguageVersion"
    }
    $provenanceRuntime = Provenance-Field $Document 'runtime_package'
    if ($provenanceRuntime -and $provenanceRuntime -ne $RuntimeVersion) {
        Fail "provenance runtime mismatch: expected $provenanceRuntime, observed $RuntimeVersion"
    }
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

function User-PathEntries() {
    $value = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ([string]::IsNullOrWhiteSpace($value)) { return @() }
    return @($value -split [IO.Path]::PathSeparator | Where-Object { $_ })
}

function Set-User-PathEntries([string[]]$Entries) {
    $value = if ($Entries.Count -gt 0) { $Entries -join [IO.Path]::PathSeparator } else { $null }
    [Environment]::SetEnvironmentVariable('Path', $value, 'User')
}

function Same-PathEntry([string]$Left, [string]$Right) {
    return $Left.Trim().TrimEnd('\') -ieq $Right.Trim().TrimEnd('\')
}

function Ensure-User-PathEntry([string]$Entry) {
    $entries = @(User-PathEntries)
    if ($entries | Where-Object { Same-PathEntry $_ $Entry }) { return $false }
    Set-User-PathEntries ([string[]]($entries + $Entry))
    return $true
}

function Remove-User-PathEntry([string]$Entry) {
    $entries = @(User-PathEntries)
    $remaining = @($entries | Where-Object { -not (Same-PathEntry $_ $Entry) })
    if ($remaining.Count -eq $entries.Count) { return $false }
    Set-User-PathEntries ([string[]]$remaining)
    return $true
}

Assert-SafeRoot $InstallRoot
$root = FullPath $InstallRoot
$manifest = ManifestPath $root

switch ($Action) {
    'Install' {
        if (-not $BinaryPath) { Fail 'Install requires -BinaryPath' }
        $managedState = Join-Path $root 'state\installations.json'
        if (Test-Path -LiteralPath $managedState -PathType Leaf) {
            Fail 'this root is managed by the native versioned installer; use SelorynSetup or seloryn use so the default launcher cannot be downgraded'
        }
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
            Verify-Provenance $provenance $hash $languageVersion $runtimeVersion
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
        $pathEntry = Join-Path $root 'bin'
        $pathAdded = $false
        if ($AddToPath) {
            $pathAdded = Ensure-User-PathEntry $pathEntry
        }
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
            path_entry = $pathEntry
            path_scope = 'User'
            path_added = $pathAdded
        }
        $record | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifest -Encoding UTF8
        Write-Result ([ordered]@{ action = 'install'; artifact = 'seloryn-cli'; platform = 'windows-x64'; sha256 = $hash; language_version = $languageVersion; runtime_package = $runtimeVersion; path_entry = $pathEntry; path_scope = 'User'; path_added = $pathAdded; rollback = [bool]$backupRelative })
    }
    'Uninstall' {
        $record = Read-Manifest $root
        $destination = Safe-InstalledPath $root ([string]$record.binary_relative)
        if (Test-Path -LiteralPath $destination -PathType Leaf) { Remove-Item -LiteralPath $destination -Force }
        $pathEntry = [string]$record.path_entry
        $pathRemoved = $false
        if ($RemoveFromPath -and $pathEntry) {
            $pathRemoved = Remove-User-PathEntry $pathEntry
        }
        Remove-Item -LiteralPath $manifest -Force
        Write-Result ([ordered]@{ action = 'uninstall'; artifact = 'seloryn-cli'; platform = 'windows-x64'; path_entry = $pathEntry; path_removed = $pathRemoved; path_entry_preserved = -not $RemoveFromPath })
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
        $processEntries = @($env:PATH -split [IO.Path]::PathSeparator | Where-Object { $_ })
        $userEntries = @(User-PathEntries)
        $processPresent = @($processEntries | Where-Object { Same-PathEntry $_ $bin }).Count -gt 0
        $userPresent = @($userEntries | Where-Object { Same-PathEntry $_ $bin }).Count -gt 0
        Write-Result ([ordered]@{ action = 'diagnose-path'; bin = $bin; on_path = $processPresent; user_path = $userPresent; fix = "Run Install -AddToPath or add $bin to PATH for this user or shell" })
    }
}
