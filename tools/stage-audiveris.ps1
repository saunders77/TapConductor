param(
    [Parameter(Mandatory = $true)]
    [string]$MsiPath,

    [Parameter(Mandatory = $true)]
    [string]$TessdataDirectory,

    [Parameter(Mandatory = $true)]
    [string]$SourceArchivePath,

    [string]$Version = "5.11.0"
)

$ErrorActionPreference = "Stop"

function Resolve-RequiredPath([string]$Value, [string]$Label) {
    $resolved = Resolve-Path -LiteralPath $Value -ErrorAction SilentlyContinue
    if (-not $resolved) {
        throw "$Label was not found: $Value"
    }
    return $resolved.Path
}

function Assert-ChildPath([string]$Root, [string]$Candidate) {
    $rootFull = [System.IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
    $candidateFull = [System.IO.Path]::GetFullPath($Candidate)
    if (-not $candidateFull.StartsWith($rootFull, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to modify a path outside the Audiveris staging root: $candidateFull"
    }
}

$msi = Resolve-RequiredPath $MsiPath "Audiveris MSI"
$tessdata = Resolve-RequiredPath $TessdataDirectory "Tesseract data directory"
$sourceArchive = Resolve-RequiredPath $SourceArchivePath "Audiveris source archive"

if ([System.IO.Path]::GetExtension($msi) -ne ".msi") {
    throw "The Audiveris runtime input must be an MSI."
}
if ([System.IO.Path]::GetFileName($msi) -notlike "*$Version*") {
    throw "The Audiveris MSI filename does not identify the pinned $Version release."
}
if (-not (Test-Path -LiteralPath (Join-Path $tessdata "eng.traineddata") -PathType Leaf)) {
    throw "The Tesseract data directory must include eng.traineddata."
}
if ([System.IO.Path]::GetExtension($sourceArchive) -notin @(".zip", ".gz")) {
    throw "The corresponding-source input must be a .zip or .tar.gz archive."
}
if ([System.IO.Path]::GetFileName($sourceArchive) -notlike "*$Version*") {
    throw "The corresponding-source filename does not identify the pinned $Version release."
}

$workspace = Split-Path -Parent $PSScriptRoot
$bundleRoot = Join-Path $workspace "sidecars\audiveris"
$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("tapconductor-audiveris-" + [guid]::NewGuid().ToString("N"))
$extractedRoot = Join-Path $temporaryRoot "administrative-image"
New-Item -ItemType Directory -Path $extractedRoot -Force | Out-Null

try {
    $arguments = @("/a", "`"$msi`"", "/qn", "TARGETDIR=`"$extractedRoot`"")
    $process = Start-Process -FilePath "msiexec.exe" -ArgumentList $arguments -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "Audiveris MSI administrative extraction failed with exit code $($process.ExitCode)."
    }

    $launcher = Get-ChildItem -LiteralPath $extractedRoot -Recurse -File -Filter "Audiveris.exe" |
        Sort-Object { $_.FullName.Length } |
        Select-Object -First 1
    if (-not $launcher) {
        throw "The extracted MSI did not contain Audiveris.exe."
    }
    $applicationRoot = $launcher.Directory.FullName
    if (-not (Test-Path -LiteralPath (Join-Path $applicationRoot "runtime") -PathType Container)) {
        throw "The extracted Audiveris application image did not contain its private Java runtime."
    }

    foreach ($relative in @("runtime", "profile", "source", "BUNDLE-MANIFEST.json")) {
        $target = Join-Path $bundleRoot $relative
        Assert-ChildPath $bundleRoot $target
        if (Test-Path -LiteralPath $target) {
            Remove-Item -LiteralPath $target -Recurse -Force
        }
    }

    Copy-Item -LiteralPath $applicationRoot -Destination (Join-Path $bundleRoot "runtime") -Recurse
    $profileTessdata = Join-Path $bundleRoot "profile\AudiverisLtd\audiveris\config\tessdata"
    New-Item -ItemType Directory -Path $profileTessdata -Force | Out-Null
    Copy-Item -Path (Join-Path $tessdata "*.traineddata") -Destination $profileTessdata
    $sourceTarget = Join-Path $bundleRoot "source"
    New-Item -ItemType Directory -Path $sourceTarget -Force | Out-Null
    Copy-Item -LiteralPath $sourceArchive -Destination (Join-Path $sourceTarget ([System.IO.Path]::GetFileName($sourceArchive)))

    $manifest = [ordered]@{
        audiverisVersion = $Version
        runtimeInput = [System.IO.Path]::GetFileName($msi)
        runtimeSha256 = (Get-FileHash -LiteralPath $msi -Algorithm SHA256).Hash.ToLowerInvariant()
        sourceInput = [System.IO.Path]::GetFileName($sourceArchive)
        sourceSha256 = (Get-FileHash -LiteralPath $sourceArchive -Algorithm SHA256).Hash.ToLowerInvariant()
        trainedData = @(Get-ChildItem -LiteralPath $profileTessdata -File -Filter "*.traineddata" | ForEach-Object {
            [ordered]@{
                file = $_.Name
                sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        })
    }
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $bundleRoot "BUNDLE-MANIFEST.json") -Encoding utf8
}
finally {
    $tempBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    $resolvedTemporary = [System.IO.Path]::GetFullPath($temporaryRoot)
    if ($resolvedTemporary.StartsWith($tempBase, [System.StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolvedTemporary)) {
        Remove-Item -LiteralPath $resolvedTemporary -Recurse -Force
    }
}

& (Join-Path $PSScriptRoot "verify-audiveris-bundle.ps1") -ExpectedVersion $Version
