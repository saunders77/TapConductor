param(
    [string]$ExpectedVersion = "5.11.0"
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$bundleRoot = Join-Path $workspace "sidecars\audiveris"
$manifestPath = Join-Path $bundleRoot "BUNDLE-MANIFEST.json"

$launcher = Get-ChildItem -LiteralPath (Join-Path $bundleRoot "runtime") -Recurse -File -Filter "Audiveris.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
$java = Get-ChildItem -LiteralPath (Join-Path $bundleRoot "runtime") -Recurse -File -Filter "java.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
$englishOcr = Join-Path $bundleRoot "profile\AudiverisLtd\audiveris\config\tessdata\eng.traineddata"
$sourceArchive = Get-ChildItem -LiteralPath (Join-Path $bundleRoot "source") -File -ErrorAction SilentlyContinue | Select-Object -First 1

if (-not $launcher) { throw "Audiveris.exe is not staged. Run tools/stage-audiveris.ps1 before building an installer." }
if (-not $java) { throw "The private Audiveris Java runtime is not staged." }
if (-not (Test-Path -LiteralPath $englishOcr -PathType Leaf)) { throw "eng.traineddata is not staged for Audiveris OCR." }
if (-not $sourceArchive) { throw "The exact Audiveris corresponding-source archive is not staged." }
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw "The Audiveris bundle manifest is missing." }

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.audiverisVersion -ne $ExpectedVersion) {
    throw "Expected Audiveris $ExpectedVersion, but the staged manifest identifies $($manifest.audiverisVersion)."
}

Write-Host "Verified Audiveris $ExpectedVersion runtime, private Java runtime, OCR data, and corresponding source."
