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
    & python tests/n1_probe.py
    if ($LASTEXITCODE -ne 0) { throw 'N1 independent CLI verification failed' }
    & python tests/n2_probe.py
    if ($LASTEXITCODE -ne 0) { throw 'N2 independent CLI verification failed' }
    & python tests/n3_probe.py
    if ($LASTEXITCODE -ne 0) { throw 'N3a independent CLI verification failed' }
    & python tests/n3_tensor_probe.py
    if ($LASTEXITCODE -ne 0) { throw 'N3b tensor independent CLI verification failed' }
    & python tests/release_probe.py
    if ($LASTEXITCODE -ne 0) { throw 'Public project and bundle verification failed' }
    & python tests/project_manifest_probe.py
    if ($LASTEXITCODE -ne 0) { throw 'Project manifest verification failed' }
    & python tests/mlp_training_probe.py
    if ($LASTEXITCODE -ne 0) { throw 'CPU MLP independent verification failed' }
    & python tests/streaming_v2_probe.py
    if ($LASTEXITCODE -ne 0) { throw 'Hash-chained training stream verification failed' }
    & python tests/self_hosted_probe.py
    if ($LASTEXITCODE -ne 0) { throw 'Seloryn-authored training verification failed' }
    & python tests/gpu_probe.py
    if ($LASTEXITCODE -ne 0) { throw 'GPU compute verification failed' }
    & npm ci --prefix tooling/vscode --ignore-scripts --no-audit --no-fund
    if ($LASTEXITCODE -ne 0) { throw 'VS Code packaging dependencies failed to install' }
    & npm run package --prefix tooling/vscode
    if ($LASTEXITCODE -ne 0) { throw 'VS Code VSIX packaging failed' }
    & npm run test:cli --prefix tooling/vscode
    if ($LASTEXITCODE -ne 0) { throw 'VS Code to CLI diagnostic integration failed' }
    & npm test --prefix tooling/vscode
    if ($LASTEXITCODE -ne 0) { throw 'VS Code extension validation failed' }
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
