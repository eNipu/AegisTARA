# Runs the shell in measure mode and prints frame, memory and size numbers.
# Usage: powershell -ExecutionPolicy Bypass -File measure.ps1 [nodes] [frames]
param([int]$Nodes = 100000, [int]$Frames = 300)
$ErrorActionPreference = 'Stop'

$root = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $PSScriptRoot 'target' }
$bin = Join-Path $root 'release\aegis-spike-gpui-shell.exe'
if (-not (Test-Path $bin)) {
    Write-Host "binary missing: $bin"
    Write-Host 'build it: cargo build --release --locked --manifest-path spikes\gpui-shell\Cargo.toml'
    exit 1
}

$out = [System.IO.Path]::GetTempFileName()
$p = Start-Process -FilePath $bin -ArgumentList @('--measure', '--nodes', $Nodes, '--frames', $Frames) `
    -PassThru -RedirectStandardOutput $out -NoNewWindow

$waited = 0
while (-not (Select-String -Path $out -Pattern 'MEASURE_DONE' -Quiet)) {
    if ($waited -ge 600) {
        Write-Host 'timed out after 120 s'
        Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
        exit 1
    }
    Start-Sleep -Milliseconds 200
    $waited++
}

$rss = [math]::Round((Get-Process -Id $p.Id).WorkingSet64 / 1MB, 1)
$p.WaitForExit()

Get-Content $out
Remove-Item $out -ErrorAction SilentlyContinue
"RSS_MB $rss"
"BINARY_MB $([math]::Round((Get-Item $bin).Length / 1MB, 1))"
"OS $((Get-CimInstance Win32_OperatingSystem).Caption)"
"GPU $((Get-CimInstance Win32_VideoController).Name -join ', ')"
"SESSION $env:SESSIONNAME"
