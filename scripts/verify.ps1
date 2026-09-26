# Full pre-commit gate: formatting, lints, tests.
# Usage: pwsh scripts/verify.ps1
# Exits non-zero on the first failing step.

$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

$steps = @(
    @{ Name = 'fmt';    Cmd = { cargo fmt --all -- --check } },
    @{ Name = 'clippy'; Cmd = { cargo clippy --workspace --all-targets -- -D warnings } },
    @{ Name = 'test';   Cmd = { cargo test --workspace } }
)

foreach ($s in $steps) {
    Write-Host "==> $($s.Name)" -ForegroundColor Cyan
    & $s.Cmd
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: $($s.Name)" -ForegroundColor Red
        exit $LASTEXITCODE
    }
}

Write-Host 'verify: all checks passed' -ForegroundColor Green
