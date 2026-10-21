$ErrorActionPreference = 'Stop'

$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$previousFlags = $env:CARGO_ENCODED_RUSTFLAGS
$previousCflags = $env:CFLAGS
$remaps = @("--remap-path-prefix=$projectRoot=/src")
if ($env:USERPROFILE) {
    $remaps += "--remap-path-prefix=$($env:USERPROFILE)=/build-home"
}
if ($previousFlags) {
    $remaps += $previousFlags.Split([char]31)
}

try {
    $env:CARGO_ENCODED_RUSTFLAGS = [string]::Join([char]31, $remaps)
    $env:CFLAGS = if ($previousCflags) { "$previousCflags /DNDEBUG" } else { '/DNDEBUG' }
    Push-Location $projectRoot
    try {
        npm run tauri build -- --no-bundle
        if ($LASTEXITCODE -ne 0) { throw "Tauri build failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
} finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $previousFlags
    $env:CFLAGS = $previousCflags
}

$source = Join-Path $projectRoot 'src-tauri\target\release\px1-disc-compressor.exe'
$bytes = [System.IO.File]::ReadAllBytes($source)
$ascii = [System.Text.Encoding]::GetEncoding('iso-8859-1').GetString($bytes)
$utf16 = [System.Text.Encoding]::Unicode.GetString($bytes)
foreach ($privatePath in @($projectRoot, $env:USERPROFILE)) {
    if ($privatePath -and (
        $ascii.IndexOf($privatePath, [StringComparison]::OrdinalIgnoreCase) -ge 0 -or
        $utf16.IndexOf($privatePath, [StringComparison]::OrdinalIgnoreCase) -ge 0
    )) {
        throw "Build contains a local path: $privatePath"
    }
}

$portableDir = Join-Path $projectRoot 'src-tauri\target\release\bundle\portable'
New-Item -ItemType Directory -Force -Path $portableDir | Out-Null
$portable = Join-Path $portableDir 'PSX Disc File Compressor v0.1.0.exe'
Copy-Item -LiteralPath $source -Destination $portable -Force
Get-FileHash -Algorithm SHA256 -LiteralPath $portable
