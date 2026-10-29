[CmdletBinding()]
param(
    [string]$CargoCommand = $(if ($env:SELORYN_CARGO) { $env:SELORYN_CARGO } else { 'cargo' }),
    [string]$PythonCommand = $(if ($env:SELORYN_PYTHON) { $env:SELORYN_PYTHON } else { '' }),
    [string]$NodeCommand = $(if ($env:SELORYN_NODE) { $env:SELORYN_NODE } else { 'node' }),
    [switch]$RequireNode
)

$ErrorActionPreference = 'Stop'

function Fail([string]$Message, [int]$Code = 127) {
    [Console]::Error.WriteLine("SELORYN-DOCTOR: ERROR: $Message")
    exit $Code
}

function Has-Command([string]$Name) {
    return $null -ne (Get-Command -Name $Name -ErrorAction SilentlyContinue)
}

function Get-PythonCommand {
    $candidates = @()
    if ($PythonCommand) {
        $candidates = @($PythonCommand)
    } else {
        $candidates = @('python', 'python3', 'py')
    }

    foreach ($candidate in $candidates) {
        if (-not (Has-Command $candidate)) { continue }
        $version = (& $candidate -c 'import sys; print(f"{sys.version_info[0]}.{sys.version_info[1]}")' 2>$null | Out-String).Trim()
        $exitCode = $LASTEXITCODE
        if ($exitCode -eq 0 -and $version -match '^(\d+)\.(\d+)$') {
            $major = [int]$Matches[1]
            $minor = [int]$Matches[2]
            if (($major -gt 3) -or (($major -eq 3) -and ($minor -ge 10))) {
                return [pscustomobject]@{ Command = $candidate; Version = $version }
            }
        }
    }
    return $null
}

if (-not (Has-Command $CargoCommand)) {
    Fail "Rust/Cargo is missing. Install the toolchain declared by rust-toolchain.toml with https://rustup.rs, reopen the shell, and verify with 'cargo --version'."
}
& $CargoCommand --version *> $null
if ($LASTEXITCODE -ne 0) {
    Fail "Cargo was found but could not run. Check the Rust installation and verify with 'cargo --version'."
}
if (-not (Has-Command 'rustc')) {
    Fail "rustc is missing. Install the toolchain declared by rust-toolchain.toml with https://rustup.rs, then verify with 'rustc --version'."
}
& rustc --version *> $null
if ($LASTEXITCODE -ne 0) {
    Fail "rustc was found but could not run. Check the Rust installation and verify with 'rustc --version'."
}

$python = Get-PythonCommand
if ($null -eq $python) {
    Fail "Python 3.10 or newer is required for the conformance command. Install Python from https://www.python.org/downloads/, ensure 'python' is on PATH, and verify with 'python --version'."
}

Write-Output ("SELORYN-DOCTOR: PASS: cargo={0}, rustc={1}, python={2} ({3})" -f $CargoCommand, (& rustc --version | Out-String).Trim(), $python.Command, $python.Version)

if ($RequireNode) {
    if (-not (Has-Command $NodeCommand)) {
        Fail "Node.js is required for the optional VS Code/editor path. Install Node.js from https://nodejs.org/, reopen the shell, and verify with 'node --version'."
    }
    & $NodeCommand --version *> $null
    if ($LASTEXITCODE -ne 0) {
        Fail "Node.js was found but could not run. Verify with 'node --version'."
    }
    if (-not (Has-Command 'npm')) {
        Fail "npm is required for the optional VS Code/editor path. Install npm with Node.js, reopen the shell, and verify with 'npm --version'."
    }
    & npm --version *> $null
    if ($LASTEXITCODE -ne 0) {
        Fail "npm was found but could not run. Verify with 'npm --version'."
    }
    Write-Output ("SELORYN-DOCTOR: PASS: node={0}, npm={1}" -f $NodeCommand, (& npm --version | Out-String).Trim())
} elseif ((Has-Command $NodeCommand) -and (Has-Command 'npm')) {
    Write-Output 'SELORYN-DOCTOR: INFO: Node.js/npm detected; they are not required for this CLI first-run path.'
} else {
    Write-Output 'SELORYN-DOCTOR: INFO: Node.js/npm not detected; that is expected for this CLI-only path. Install them only for the optional VS Code/editor checks.'
}
