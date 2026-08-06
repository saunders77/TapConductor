# Copyright (c) 2026 Michael Saunders
[CmdletBinding()]
param(
    [ValidateSet("TestUnsigned", "Production")]
    [string]$Mode = "TestUnsigned",

    [switch]$ValidateOnly,

    [switch]$VerifyOnly,

    [string]$ArtifactPath,

    [string]$AppExecutablePath,

    [string]$Publisher = $env:TAPCONDUCTOR_WINDOWS_STORE_PUBLISHER,

    [string]$SigningConfigPath = $env:TAPCONDUCTOR_WINDOWS_SIGN_CONFIG,

    [switch]$SkipQualityChecks
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-Condition {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Read-JsonFile {
    param([string]$Path)

    try {
        return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    }
    catch {
        throw "Invalid JSON in '$Path': $($_.Exception.Message)"
    }
}

function Get-JsonProperty {
    param(
        [object]$Object,
        [string]$Name
    )

    if ($null -eq $Object) {
        return $null
    }

    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    return $property.Value
}

function Resolve-InputPath {
    param(
        [string]$Path,
        [string]$RepositoryRoot
    )

    $candidate = $Path
    if (-not [System.IO.Path]::IsPathRooted($candidate)) {
        $candidate = Join-Path $RepositoryRoot $candidate
    }
    return (Resolve-Path -LiteralPath $candidate).Path
}

function Invoke-NativeCommand {
    param(
        [string]$FilePath,
        [string[]]$CommandArguments
    )

    Write-Host "> $FilePath $($CommandArguments -join ' ')"
    & $FilePath @CommandArguments
    if ($LASTEXITCODE -ne 0) {
        throw "'$FilePath' exited with code $LASTEXITCODE."
    }
}

function Assert-StoreOverlay {
    param(
        [object]$Configuration,
        [string]$RepositoryRoot
    )

    $bundle = Get-JsonProperty $Configuration "bundle"
    Assert-Condition ($null -ne $bundle) "The Store overlay must define bundle settings."

    $targets = @(Get-JsonProperty $bundle "targets")
    Assert-Condition (
        $targets.Count -eq 1 -and [string]$targets[0] -eq "nsis"
    ) "The Store overlay must build only the NSIS target."

    Assert-Condition (
        $null -eq (Get-JsonProperty $bundle "publisher")
    ) "Do not commit a publisher identity to the Store overlay; provide it as a release input."

    $resources = Get-JsonProperty $bundle "resources"
    Assert-Condition ($null -ne $resources) "The Store overlay must repeat all release resources."

    $requiredResources = [ordered]@{
        "../assets/SlenderSalamander44khz16bit" = "instruments/salamander"
        "../assets/demo/Prelude in C Minor - Chopin 1839.musicxml" = "demo/Prelude in C Minor - Chopin 1839.musicxml"
        "../assets/demo/All-Night Vigil - Rachmaninoff 1915.musicxml" = "demo/All-Night Vigil - Rachmaninoff 1915.musicxml"
        "../LICENSE" = "LICENSE"
        "../PRIVACY.md" = "PRIVACY.md"
        "../THIRD_PARTY_NOTICES.md" = "THIRD_PARTY_NOTICES.md"
    }
    foreach ($entry in $requiredResources.GetEnumerator()) {
        $resourceProperty = $resources.PSObject.Properties[$entry.Key]
        Assert-Condition (
            $null -ne $resourceProperty -and [string]$resourceProperty.Value -eq $entry.Value
        ) "The Store overlay is missing resource '$($entry.Key)' -> '$($entry.Value)'."
    }

    $windows = Get-JsonProperty $bundle "windows"
    $webview = Get-JsonProperty $windows "webviewInstallMode"
    Assert-Condition (
        [string](Get-JsonProperty $webview "type") -eq "offlineInstaller"
    ) "The Microsoft Store installer must use the offline WebView2 installer."
    Assert-Condition (
        [bool](Get-JsonProperty $webview "silent")
    ) "The offline WebView2 prerequisite must install silently."

    $nsis = Get-JsonProperty $windows "nsis"
    Assert-Condition (
        [string](Get-JsonProperty $nsis "installMode") -eq "currentUser"
    ) "The Store NSIS installer must retain the current-user install mode."

    $pianoDirectory = Join-Path $RepositoryRoot "assets\SlenderSalamander44khz16bit"
    Assert-Condition (Test-Path -LiteralPath $pianoDirectory -PathType Container) (
        "The bundled grand-piano directory is missing: $pianoDirectory"
    )
    $sampleCount = @(
        Get-ChildItem -LiteralPath (Join-Path $pianoDirectory "samples") -File -Filter "*.wav"
    ).Count
    Assert-Condition ($sampleCount -gt 0) "The grand-piano sample directory contains no WAV files."
    Assert-Condition (
        (Test-Path -LiteralPath (Join-Path $pianoDirectory "SlenderSalamanderGrandPiano.sfz"))
    ) "The grand-piano SFZ mapping is missing."
}

function Assert-SigningConfig {
    param([object]$Configuration)

    $topLevelNames = @($Configuration.PSObject.Properties.Name)
    Assert-Condition (
        $topLevelNames.Count -eq 1 -and $topLevelNames[0] -eq "bundle"
    ) "The external signing config may contain only the top-level 'bundle' object."

    $bundle = Get-JsonProperty $Configuration "bundle"
    $bundleNames = @($bundle.PSObject.Properties.Name)
    Assert-Condition (
        $bundleNames.Count -eq 1 -and $bundleNames[0] -eq "windows"
    ) "The external signing config may contain only 'bundle.windows'."

    $windows = Get-JsonProperty $bundle "windows"
    $allowedNames = @(
        "certificateThumbprint",
        "digestAlgorithm",
        "timestampUrl",
        "tsp",
        "signCommand"
    )
    foreach ($name in @($windows.PSObject.Properties.Name)) {
        Assert-Condition ($allowedNames -contains $name) (
            "Unsupported signing-config property 'bundle.windows.$name'."
        )
    }

    $thumbprint = [string](Get-JsonProperty $windows "certificateThumbprint")
    $signCommand = Get-JsonProperty $windows "signCommand"
    $hasThumbprint = -not [string]::IsNullOrWhiteSpace($thumbprint)
    $hasSignCommand = $null -ne $signCommand -and -not [string]::IsNullOrWhiteSpace(
        [string]$signCommand
    )
    Assert-Condition (
        $hasThumbprint -xor $hasSignCommand
    ) "Provide exactly one signing method: certificateThumbprint or signCommand."

    if ($hasThumbprint) {
        Assert-Condition (
            [string](Get-JsonProperty $windows "digestAlgorithm") -eq "sha256"
        ) "Certificate-store signing must use digestAlgorithm 'sha256'."
        Assert-Condition (
            -not [string]::IsNullOrWhiteSpace(
                [string](Get-JsonProperty $windows "timestampUrl")
            )
        ) "Certificate-store signing must configure a timestampUrl."
    }
}

function Get-SignatureSummary {
    param([string]$Path)

    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    return [pscustomobject]@{
        Path = $Path
        Status = [string]$signature.Status
        Signer = if ($null -ne $signature.SignerCertificate) {
            $signature.SignerCertificate.Subject
        }
        else {
            $null
        }
        Timestamped = $null -ne $signature.TimeStamperCertificate
    }
}

function Assert-ArtifactSignature {
    param(
        [string]$Path,
        [string]$ExpectedMode
    )

    Assert-Condition (Test-Path -LiteralPath $Path -PathType Leaf) (
        "Artifact not found: $Path"
    )
    $summary = Get-SignatureSummary $Path
    $summary | Format-List | Out-Host

    if ($ExpectedMode -eq "Production") {
        Assert-Condition ($summary.Status -eq "Valid") (
            "Production artifact does not have a valid Authenticode signature: $Path"
        )
        Assert-Condition ($summary.Timestamped) (
            "Production artifact does not have a verifiable timestamp: $Path"
        )
    }
    else {
        Assert-Condition ($summary.Status -eq "NotSigned") (
            "TestUnsigned mode expected an unsigned artifact, but status is '$($summary.Status)'."
        )
    }
    return $summary
}

if ($ValidateOnly -and $VerifyOnly) {
    throw "Choose either -ValidateOnly or -VerifyOnly, not both."
}
if ($VerifyOnly -and [string]::IsNullOrWhiteSpace($ArtifactPath)) {
    throw "-VerifyOnly requires -ArtifactPath."
}

$repositoryRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$baseConfigPath = Join-Path $repositoryRoot "src-tauri\tauri.conf.json"
$storeConfigPath = Join-Path $repositoryRoot "src-tauri\tauri.microsoftstore.conf.json"
$baseConfig = Read-JsonFile $baseConfigPath
$storeConfig = Read-JsonFile $storeConfigPath

Assert-StoreOverlay -Configuration $storeConfig -RepositoryRoot $repositoryRoot

$productName = [string](Get-JsonProperty $baseConfig "productName")
$version = [string](Get-JsonProperty $baseConfig "version")
Assert-Condition (-not [string]::IsNullOrWhiteSpace($productName)) (
    "The base Tauri config has no productName."
)
Assert-Condition (-not [string]::IsNullOrWhiteSpace($version)) (
    "The base Tauri config has no version."
)

Write-Host "Microsoft Store configuration is valid for $productName $version."

if ($ValidateOnly) {
    Write-Host "Validation only: no build, install, signing, or filesystem output was performed."
    exit 0
}

if ($VerifyOnly) {
    $resolvedArtifact = Resolve-InputPath $ArtifactPath $repositoryRoot
    $null = Assert-ArtifactSignature -Path $resolvedArtifact -ExpectedMode $Mode

    if (-not [string]::IsNullOrWhiteSpace($AppExecutablePath)) {
        $resolvedAppExecutable = Resolve-InputPath $AppExecutablePath $repositoryRoot
        $null = Assert-ArtifactSignature -Path $resolvedAppExecutable -ExpectedMode $Mode
    }
    elseif ($Mode -eq "Production") {
        throw "Production verification also requires -AppExecutablePath."
    }

    $hash = Get-FileHash -LiteralPath $resolvedArtifact -Algorithm SHA256
    Write-Host "SHA-256 $($hash.Hash)  $resolvedArtifact"
    exit 0
}

$resolvedSigningConfig = $null
if ($Mode -eq "Production") {
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($Publisher)) (
        "Production mode requires -Publisher or TAPCONDUCTOR_WINDOWS_STORE_PUBLISHER."
    )
    Assert-Condition (
        -not $Publisher.Equals($productName, [System.StringComparison]::OrdinalIgnoreCase)
    ) "The verified publisher name cannot equal the product name '$productName'."
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($SigningConfigPath)) (
        "Production mode requires -SigningConfigPath or TAPCONDUCTOR_WINDOWS_SIGN_CONFIG."
    )

    $resolvedSigningConfig = Resolve-InputPath $SigningConfigPath $repositoryRoot
    $signingConfig = Read-JsonFile $resolvedSigningConfig
    Assert-SigningConfig -Configuration $signingConfig
}

Push-Location $repositoryRoot
try {
    $tauriCommand = Join-Path $repositoryRoot "node_modules\.bin\tauri.cmd"
    Assert-Condition (Test-Path -LiteralPath $tauriCommand -PathType Leaf) (
        "Tauri CLI is missing. Run 'npm ci' before this script."
    )

    if (-not $SkipQualityChecks) {
        $npmCommand = (Get-Command npm.cmd -ErrorAction Stop).Source
        $cargoCommand = (Get-Command cargo.exe -ErrorAction Stop).Source
        Invoke-NativeCommand $npmCommand @("run", "test:auto-follow")
        Invoke-NativeCommand $npmCommand @("run", "test:beat")
        Invoke-NativeCommand $cargoCommand @("fmt", "--all", "--", "--check")
        Invoke-NativeCommand $cargoCommand @("test", "--locked", "--workspace")
        Invoke-NativeCommand $cargoCommand @(
            "clippy",
            "--locked",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings"
        )
    }

    $tauriArguments = @(
        "build",
        "--bundles",
        "nsis",
        "--config",
        $storeConfigPath,
        "--ci"
    )

    $generatedConfigPath = $null
    if ($Mode -eq "Production") {
        $generatedConfigDirectory = Join-Path $repositoryRoot "target\store-config"
        $null = New-Item -ItemType Directory -Path $generatedConfigDirectory -Force
        $generatedConfigPath = Join-Path $generatedConfigDirectory "publisher.conf.json"
        @{
            bundle = @{
                publisher = $Publisher
            }
        } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $generatedConfigPath -Encoding UTF8

        $tauriArguments += @("--config", $generatedConfigPath)
        $tauriArguments += @("--config", $resolvedSigningConfig)
    }
    else {
        $tauriArguments += "--no-sign"
        Write-Warning (
            "Building an explicitly test-only unsigned package. " +
            "It is not eligible for Microsoft Store submission or public distribution."
        )
    }

    Invoke-NativeCommand $tauriCommand $tauriArguments

    $builtArtifact = Join-Path $repositoryRoot (
        "target\release\bundle\nsis\{0}_{1}_x64-setup.exe" -f $productName, $version
    )
    $builtExecutable = Join-Path $repositoryRoot "target\release\tapconductor-app.exe"
    $null = Assert-ArtifactSignature -Path $builtArtifact -ExpectedMode $Mode
    $null = Assert-ArtifactSignature -Path $builtExecutable -ExpectedMode $Mode

    $outputKind = if ($Mode -eq "Production") { "production" } else { "test-unsigned" }
    $outputDirectory = Join-Path $repositoryRoot "target\store-artifacts\$outputKind"
    $null = New-Item -ItemType Directory -Path $outputDirectory -Force

    $outputName = if ($Mode -eq "Production") {
        "{0}_{1}_x64_store-setup.exe" -f $productName, $version
    }
    else {
        "{0}_{1}_x64_TEST-ONLY-UNSIGNED_store-setup.exe" -f $productName, $version
    }
    $outputArtifact = Join-Path $outputDirectory $outputName
    Copy-Item -LiteralPath $builtArtifact -Destination $outputArtifact -Force
    Copy-Item -LiteralPath (Join-Path $repositoryRoot "LICENSE") -Destination $outputDirectory -Force
    Copy-Item -LiteralPath (Join-Path $repositoryRoot "PRIVACY.md") -Destination $outputDirectory -Force
    Copy-Item -LiteralPath (
        Join-Path $repositoryRoot "THIRD_PARTY_NOTICES.md"
    ) -Destination $outputDirectory -Force

    if ($Mode -eq "TestUnsigned") {
        @"
TEST-ONLY UNSIGNED BUILD

This installer is not eligible for Microsoft Store submission or public distribution.
Build a Production artifact with a verified publisher identity and trusted signing configuration.
"@ | Set-Content -LiteralPath (
            Join-Path $outputDirectory "DO_NOT_SUBMIT_TO_STORE.txt"
        ) -Encoding UTF8
    }

    $hash = Get-FileHash -LiteralPath $outputArtifact -Algorithm SHA256
    "$($hash.Hash)  $outputName" | Set-Content -LiteralPath (
        Join-Path $outputDirectory "SHA256SUMS.txt"
    ) -Encoding ASCII

    Write-Host ""
    Write-Host "Store artifact staged at: $outputArtifact"
    Write-Host "SHA-256: $($hash.Hash)"
    if ($Mode -eq "TestUnsigned") {
        Write-Warning "TEST ONLY: do not upload this unsigned installer to Partner Center."
    }
}
finally {
    Pop-Location
}
