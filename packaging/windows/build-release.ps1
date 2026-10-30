param(
    [switch]$SkipCopy
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Push-Location $root
try {
    # Do not ship the developer's home path or the checkout path in panic
    # location strings. These are compile-time remaps; the binary still works
    # normally, but embedded source paths become stable placeholders.
    $userPrefix = $env:USERPROFILE
    $rootPrefix = $root
    $flags = @(
        "--remap-path-prefix=$userPrefix=.",
        "--remap-path-prefix=$rootPrefix=NotOffice"
    ) -join ' '
    $env:RUSTFLAGS = $flags

    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build --release failed with exit code $LASTEXITCODE"
    }

    if (-not $SkipCopy) {
        $exe = Join-Path $root 'target\release\notoffice.exe'
        $releaseExe = Join-Path $root 'NotOffice-1.0.4.exe'
        Copy-Item $exe $releaseExe -Force
        $hash = (Get-FileHash $releaseExe -Algorithm SHA256).Hash
        Set-Content (Join-Path $root 'NotOffice-1.0.4.exe.sha256') "$hash  NotOffice-1.0.4.exe" -Encoding ascii
        Write-Host "release: $releaseExe"
        Write-Host "sha256:  $hash"
    }
}
finally {
    Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
    Pop-Location
}
