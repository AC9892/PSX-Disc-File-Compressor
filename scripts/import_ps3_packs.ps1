param(
    [string]$Downloads = (Join-Path $env:USERPROFILE 'Downloads'),
    [string]$Project = (Split-Path -Parent $PSScriptRoot),
    [string]$PackDate = '2026-10-28'
)
$ErrorActionPreference = 'Stop'
$regions = @('US','JA','EN','FR','DE','ES','IT','NL','PT','CH','AU','SE','DK','NO','FI','KO','ZH','RU','CZ','HU','PL')
$converter = Join-Path $Project 'src-tauri\target\debug\import_ps3_covers.exe'
$root = Join-Path $Project 'artwork'
$stage = Join-Path $Project '.test-temp\ps3-cover-import'
if (-not (Test-Path -LiteralPath $converter -PathType Leaf)) { throw "Converter missing: $converter" }
if (-not (Test-Path -LiteralPath $root -PathType Container)) { throw "Artwork folder missing: $root" }
$stageFull = [IO.Path]::GetFullPath($stage)
$projectFull = [IO.Path]::GetFullPath($Project).TrimEnd('\') + '\'
if (-not $stageFull.StartsWith($projectFull, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe staging path' }
New-Item -ItemType Directory -Force -Path $stage | Out-Null
$report = @()
foreach ($region in $regions) {
    $name = "GameTDB-ps3_coverM-$region-$PackDate.zip"
    $archive = Join-Path $Downloads $name
    if (-not (Test-Path -LiteralPath $archive -PathType Leaf)) { throw "Missing pack: $name" }
    $folder = Join-Path $stage $region
    New-Item -ItemType Directory -Force -Path $folder | Out-Null
    try {
        [IO.Compression.ZipFile]::ExtractToDirectory($archive, $folder)
        $result = & $converter $folder $root 2>&1
        if ($LASTEXITCODE -ne 0) { throw "Conversion failed for $region`: $result" }
        $line = [string]($result | Select-Object -First 1)
        $report += [pscustomobject]@{ region = $region; archive_bytes = (Get-Item -LiteralPath $archive).Length; conversion = $line }
        Write-Output "$region $line"
    } finally {
        $folderFull = [IO.Path]::GetFullPath($folder)
        if ($folderFull.StartsWith($stageFull.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $folder -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
$report | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $root 'ps3-import-report.json') -Encoding UTF8
