$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Push-Location $root
try {
    $commands = @(
        @('fmt', '--all', '--', '--check'),
        @('test', '--all-targets', '--locked'),
        @('clippy', '--all-targets', '--locked', '--', '-D', 'warnings'),
        @('test', '--release', '--all-targets', '--locked'),
        @('build', '--release', '--locked')
    )
    foreach ($argsForCargo in $commands) {
        & cargo @argsForCargo
        if ($LASTEXITCODE -ne 0) { throw "cargo $($argsForCargo -join ' ') failed" }
    }
    & python tests/n1_probe.py
    if ($LASTEXITCODE -ne 0) { throw 'N1 independent CLI verification failed' }
    & python tests/n2_probe.py
    if ($LASTEXITCODE -ne 0) { throw 'N2 independent CLI verification failed' }
    & python tests/n3_probe.py
    if ($LASTEXITCODE -ne 0) { throw 'N3a independent CLI verification failed' }
    & python tests/release_probe.py
    if ($LASTEXITCODE -ne 0) { throw 'Public project and bundle verification failed' }
    & .\target\release\seloryn.exe run examples\tensors\linear-forward.sely
    if ($LASTEXITCODE -ne 0) { throw 'Linear forward example failed' }
    & .\target\release\seloryn.exe run examples\tensors\tensor-reduce.sely
    if ($LASTEXITCODE -ne 0) { throw 'Tensor reduction example failed' }
    Write-Output 'Seloryn public checks completed successfully'
} finally {
    Pop-Location
}
