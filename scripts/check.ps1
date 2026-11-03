#!/usr/bin/env pwsh
# Validacoes do workspace klav: fmt, clippy e testes, numa tacada so.
# Silencioso no sucesso (uma linha por etapa); detalha apenas o que falhar.
#
# Uso:
#   pwsh scripts/check.ps1          # roda tudo
#   pwsh scripts/check.ps1 fmt      # roda so uma etapa (fmt | clippy | test)
#
# Trata warnings do clippy como erro (-D warnings). Codigo de saida != 0 se algo falhar.

$ErrorActionPreference = 'Continue'

# cargo pode nao estar no PATH desta shell (ver memory klav-build-env).
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path (Join-Path $cargoBin 'cargo.exe')) {
    $env:PATH = "$cargoBin;$env:PATH"
}

$all = @(
    @{ Name = 'fmt';    Args = @('fmt', '--check') },
    @{ Name = 'clippy'; Args = @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings') },
    @{ Name = 'test';   Args = @('test', '--workspace') }
)

# Filtro opcional por nome de etapa.
$steps = if ($args.Count -gt 0) { $all | Where-Object { $args -contains $_.Name } } else { $all }
if (-not $steps) {
    Write-Host "etapa desconhecida. Use: fmt | clippy | test" -ForegroundColor Yellow
    exit 2
}

$failed = @()
foreach ($s in $steps) {
    $out = & cargo $s.Args 2>&1
    if ($LASTEXITCODE -ne 0) {
        $failed += $s.Name
        Write-Host "X  $($s.Name)" -ForegroundColor Red
        $out | ForEach-Object { Write-Host "   $_" }
    }
    else {
        Write-Host "ok $($s.Name)" -ForegroundColor Green
    }
}

Write-Host ""
if ($failed.Count -gt 0) {
    Write-Host "FALHOU: $($failed -join ', ')" -ForegroundColor Red
    exit 1
}
Write-Host "tudo verde" -ForegroundColor Green
