[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [switch]$DoctorOnly,
    [string]$CargoCommand = $(if ($env:SELORYN_CARGO) { $env:SELORYN_CARGO } else { 'cargo' }),
    [string]$PythonCommand = $(if ($env:SELORYN_PYTHON) { $env:SELORYN_PYTHON } else { '' })
)

$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))

function Fail([string]$Message, [int]$Code = 1) {
    [Console]::Error.WriteLine("SELORYN-QUICKSTART: ERROR: $Message")
    exit $Code
}

function Run-Step([string]$Label, [string]$Command, [string[]]$Arguments) {
    Write-Output ""
    Write-Output "> $Label"
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        Fail "step failed with exit code ${LASTEXITCODE}: $Label" $LASTEXITCODE
    }
}

function Get-PythonCommand {
    $candidates = if ($PythonCommand) { @($PythonCommand) } else { @('python', 'python3', 'py') }
    foreach ($candidate in $candidates) {
        if ($null -eq (Get-Command -Name $candidate -ErrorAction SilentlyContinue)) { continue }
        & $candidate -c 'import sys; assert sys.version_info >= (3, 10)' 2>$null
        if ($LASTEXITCODE -eq 0) { return $candidate }
    }
    return $null
}

Write-Output '== Seloryn first-run: prerequisites =='
$doctor = Join-Path $root 'scripts\quickstart\doctor-powershell.ps1'
if ($PythonCommand) {
    & $doctor -CargoCommand $CargoCommand -PythonCommand $PythonCommand
} else {
    & $doctor -CargoCommand $CargoCommand
}
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if ($DoctorOnly) { exit 0 }

if (-not $SkipBuild) {
    Run-Step 'cargo build --release --locked' $CargoCommand @('build', '--release', '--locked')
}

$binary = Join-Path $root 'target\release\seloryn.exe'
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    $binary = Join-Path $root 'target\release\seloryn'
}
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    Fail 'release binary is missing at target/release; rerun without -SkipBuild'
}

$python = Get-PythonCommand
if (-not $python) { Fail 'the Python command selected by the prerequisite doctor is no longer runnable' }

Run-Step 'seloryn doctor examples/projects/json-config --format=json' $binary @('doctor', (Join-Path $root 'examples\projects\json-config'), '--format=json')
Run-Step 'seloryn check examples/tensors/tensor-matmul.sely' $binary @('check', (Join-Path $root 'examples\tensors\tensor-matmul.sely'))
Run-Step 'seloryn run examples/tensors/tensor-matmul.sely' $binary @('run', (Join-Path $root 'examples\tensors\tensor-matmul.sely'))
Run-Step 'python scripts/ci/conformance.py --run --suite core' $python @((Join-Path $root 'scripts\ci\conformance.py'), '--run', '--suite', 'core')

Write-Output ''
Write-Output 'SELORYN-QUICKSTART: PASS: build, doctor, check, run, and core conformance completed.'
