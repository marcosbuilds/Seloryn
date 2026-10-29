$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Push-Location $root
try {
    & python -m pip install --disable-pip-version-check --quiet -r scripts/release/requirements.txt
    if ($LASTEXITCODE -ne 0) { throw 'Release script dependencies failed to install' }
    & python scripts/release/verify-version.py
    if ($LASTEXITCODE -ne 0) { throw 'Earned version comparison failed' }
    & python tests/versioning_probe.py
    if ($LASTEXITCODE -ne 0) { throw 'Versioning regression verification failed' }
    $commands = @(
        @('fmt', '--all', '--', '--check'),
        @('test', '--all-targets', '--locked', '--', '--test-threads=1'),
        @('clippy', '--all-targets', '--locked', '--', '-D', 'warnings'),
        @('test', '--release', '--all-targets', '--locked', '--', '--test-threads=1'),
        @('build', '--release', '--locked')
    )
    foreach ($argsForCargo in $commands) {
        & cargo @argsForCargo
        if ($LASTEXITCODE -ne 0) { throw "cargo $($argsForCargo -join ' ') failed" }
    }
    & python scripts/ci/conformance.py --run --suite core
    if ($LASTEXITCODE -ne 0) { throw 'Portable public conformance matrix failed' }
    & python scripts/ci/conformance.py --run --suite benchmark
    if ($LASTEXITCODE -ne 0) { throw 'Benchmark evidence suite failed' }
    & python scripts/ci/conformance.py --run --suite hardware
    if ($LASTEXITCODE -ne 0) { throw 'Hardware conformance suite failed' }
    & python scripts/ci/conformance.py --run --suite release
    if ($LASTEXITCODE -ne 0) { throw 'Release conformance suite failed' }
    & .\target\release\seloryn.exe run examples\tensors\linear-forward.sely
    if ($LASTEXITCODE -ne 0) { throw 'Linear forward example failed' }
    & .\target\release\seloryn.exe run examples\tensors\tensor-reduce.sely
    if ($LASTEXITCODE -ne 0) { throw 'Tensor reduction example failed' }
    & .\target\release\seloryn.exe run tests\result-error.sely
    if ($LASTEXITCODE -ne 65) { throw "Expected result-error fixture to exit 65, got $LASTEXITCODE" }
    Write-Output 'Seloryn public checks completed successfully'
    exit 0
} finally {
    Pop-Location
}
