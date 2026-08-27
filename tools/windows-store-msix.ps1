# Copyright (c) 2026 Michael Saunders
[CmdletBinding()]
param(
    [ValidateSet("Test", "Store")]
    [string]$Mode = "Test",

    [switch]$ValidateOnly,

    [switch]$StageOnly,

    [switch]$SkipQualityChecks,

    [string]$IdentityName = $env:TAPCONDUCTOR_WINDOWS_STORE_IDENTITY_NAME,

    [string]$Publisher = $env:TAPCONDUCTOR_WINDOWS_STORE_PUBLISHER,

    [string]$PublisherDisplayName = $env:TAPCONDUCTOR_WINDOWS_STORE_PUBLISHER_DISPLAY_NAME,

    [string]$PackageVersion = $env:TAPCONDUCTOR_WINDOWS_STORE_VERSION,

    [string]$DevCertificatePassword = "TapConductor-Local-Test-Only"
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

function Convert-ToMsixVersion {
    param([string]$Version)

    if ($Version -match '^(\d+)\.(\d+)\.(\d+)$') {
        return "$($Matches[1]).$($Matches[2]).$($Matches[3]).0"
    }
    if ($Version -match '^(\d+)\.(\d+)\.(\d+)\.(\d+)$') {
        return $Version
    }
    throw "MSIX version '$Version' must contain three or four numeric components."
}

function Escape-XmlValue {
    param([string]$Value)

    return [System.Security.SecurityElement]::Escape($Value)
}

function Reset-SafeDirectory {
    param(
        [string]$Path,
        [string]$AllowedRoot
    )

    $fullPath = [System.IO.Path]::GetFullPath($Path)
    $fullAllowedRoot = [System.IO.Path]::GetFullPath($AllowedRoot).TrimEnd('\') + '\'
    Assert-Condition ($fullPath.StartsWith(
        $fullAllowedRoot,
        [System.StringComparison]::OrdinalIgnoreCase
    )) "Refusing to reset directory outside '$fullAllowedRoot': $fullPath"

    if (Test-Path -LiteralPath $fullPath) {
        Remove-Item -LiteralPath $fullPath -Recurse -Force
    }
    $null = New-Item -ItemType Directory -Path $fullPath -Force
    return $fullPath
}

function Copy-RequiredFile {
    param(
        [string]$Source,
        [string]$Destination
    )

    Assert-Condition (Test-Path -LiteralPath $Source -PathType Leaf) "Required file is missing: $Source"
    $destinationDirectory = Split-Path -Parent $Destination
    if (-not [string]::IsNullOrWhiteSpace($destinationDirectory)) {
        $null = New-Item -ItemType Directory -Path $destinationDirectory -Force
    }
    Copy-Item -LiteralPath $Source -Destination $Destination -Force
}

function Render-Manifest {
    param(
        [string]$TemplatePath,
        [string]$DestinationPath,
        [string]$ResolvedIdentityName,
        [string]$ResolvedPublisher,
        [string]$ResolvedPublisherDisplayName,
        [string]$ResolvedVersion
    )

    $content = Get-Content -LiteralPath $TemplatePath -Raw
    $values = [ordered]@{
        "__IDENTITY_NAME__" = $ResolvedIdentityName
        "__PUBLISHER__" = $ResolvedPublisher
        "__PUBLISHER_DISPLAY_NAME__" = $ResolvedPublisherDisplayName
        "__VERSION__" = $ResolvedVersion
    }
    foreach ($entry in $values.GetEnumerator()) {
        $content = $content.Replace($entry.Key, (Escape-XmlValue ([string]$entry.Value)))
    }
    Assert-Condition (-not $content.Contains("__")) "The rendered MSIX manifest contains an unresolved placeholder."
    try {
        $null = [xml]$content
    }
    catch {
        throw "The rendered MSIX manifest is invalid XML: $($_.Exception.Message)"
    }
    Set-Content -LiteralPath $DestinationPath -Value $content -Encoding UTF8
}

$repositoryRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$targetRoot = Join-Path $repositoryRoot "target"
$baseConfigPath = Join-Path $repositoryRoot "src-tauri\tauri.conf.json"
$msixConfigPath = Join-Path $repositoryRoot "src-tauri\tauri.microsoftstore.msix.conf.json"
$manifestTemplatePath = Join-Path $repositoryRoot "src-tauri\windows-msix\Package.appxmanifest.template"
$baseConfig = Read-JsonFile $baseConfigPath
$msixConfig = Read-JsonFile $msixConfigPath

$productName = [string](Get-JsonProperty $baseConfig "productName")
$baseVersion = [string](Get-JsonProperty $baseConfig "version")
$msixBundle = Get-JsonProperty $msixConfig "bundle"
Assert-Condition ($productName -eq "TapConductor") "Unexpected Tauri product name '$productName'."
Assert-Condition (-not [string]::IsNullOrWhiteSpace($baseVersion)) "The Tauri configuration has no version."
Assert-Condition (
    $null -ne $msixBundle -and -not [bool](Get-JsonProperty $msixBundle "active")
) "The MSIX overlay must disable Tauri's normal bundlers."
Assert-Condition (Test-Path -LiteralPath $manifestTemplatePath -PathType Leaf) "The MSIX manifest template is missing."

$requiredTemplateValues = @(
    "__IDENTITY_NAME__",
    "__PUBLISHER__",
    "__PUBLISHER_DISPLAY_NAME__",
    "__VERSION__",
    "Windows.FullTrustApplication",
    "runFullTrust",
    "tapconductor-app.exe",
    "windows.fileTypeAssociation"
)
$manifestTemplate = Get-Content -LiteralPath $manifestTemplatePath -Raw
foreach ($requiredValue in $requiredTemplateValues) {
    Assert-Condition ($manifestTemplate.Contains($requiredValue)) (
        "The MSIX manifest template is missing '$requiredValue'."
    )
}

$requiredResources = [ordered]@{
    "assets\SlenderSalamander44khz16bit" = "instruments\salamander"
    "assets\demo\Prelude in C Minor - Chopin 1839.musicxml" = "demo\Prelude in C Minor - Chopin 1839.musicxml"
    "assets\demo\All-Night Vigil - Rachmaninoff 1915.musicxml" = "demo\All-Night Vigil - Rachmaninoff 1915.musicxml"
    "WINDOWS-LICENSE" = "LICENSE"
    "LICENSING.md" = "LICENSING.md"
    "PRIVACY.md" = "PRIVACY.md"
    "THIRD_PARTY_NOTICES.md" = "THIRD_PARTY_NOTICES.md"
}
foreach ($sourceRelativePath in $requiredResources.Keys) {
    $sourcePath = Join-Path $repositoryRoot $sourceRelativePath
    Assert-Condition (Test-Path -LiteralPath $sourcePath) "Required MSIX resource is missing: $sourcePath"
}

$requiredIcons = @(
    "StoreLogo.png",
    "Square44x44Logo.png",
    "Square150x150Logo.png",
    "Square310x310Logo.png"
)
foreach ($iconName in $requiredIcons) {
    $iconPath = Join-Path $repositoryRoot "src-tauri\icons\$iconName"
    Assert-Condition (Test-Path -LiteralPath $iconPath -PathType Leaf) "Required MSIX icon is missing: $iconPath"
}

$sampleCount = @(
    Get-ChildItem -LiteralPath (
        Join-Path $repositoryRoot "assets\SlenderSalamander44khz16bit\samples"
    ) -File -Filter "*.wav"
).Count
Assert-Condition ($sampleCount -gt 0) "The bundled piano contains no WAV samples."

$resolvedVersion = Convert-ToMsixVersion $(if ([string]::IsNullOrWhiteSpace($PackageVersion)) {
    $baseVersion
} else {
    $PackageVersion
})

Write-Host "MSIX Store configuration is valid for $productName $resolvedVersion."
Write-Host "The existing NSIS, MSI, macOS, and iOS configurations are not inputs to this packaging lane."

if ($ValidateOnly) {
    Write-Host "Validation only: no build, staging, certificate, or package output was created."
    exit 0
}

if ($Mode -eq "Store") {
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($IdentityName)) (
        "Store mode requires -IdentityName or TAPCONDUCTOR_WINDOWS_STORE_IDENTITY_NAME. " +
        "Copy Package/Identity/Name from Partner Center."
    )
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($Publisher)) (
        "Store mode requires -Publisher or TAPCONDUCTOR_WINDOWS_STORE_PUBLISHER. " +
        "Copy Package/Identity/Publisher from Partner Center exactly."
    )
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($PublisherDisplayName)) (
        "Store mode requires -PublisherDisplayName or TAPCONDUCTOR_WINDOWS_STORE_PUBLISHER_DISPLAY_NAME."
    )
    $resolvedIdentityName = $IdentityName
    $resolvedPublisher = $Publisher
    $resolvedPublisherDisplayName = $PublisherDisplayName
}
else {
    $resolvedIdentityName = if ([string]::IsNullOrWhiteSpace($IdentityName)) {
        "TapConductorDevelopment"
    } else {
        $IdentityName
    }
    $resolvedPublisher = if ([string]::IsNullOrWhiteSpace($Publisher)) {
        "CN=TapConductor Development"
    } else {
        $Publisher
    }
    $resolvedPublisherDisplayName = if ([string]::IsNullOrWhiteSpace($PublisherDisplayName)) {
        "TapConductor Development"
    } else {
        $PublisherDisplayName
    }
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
        Invoke-NativeCommand $npmCommand @("run", "test:unit")
        Invoke-NativeCommand $cargoCommand @("fmt", "--all", "--", "--check")
        Invoke-NativeCommand $cargoCommand @("test", "--locked", "--workspace")
        Invoke-NativeCommand $cargoCommand @(
            "clippy", "--locked", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"
        )
    }

    Invoke-NativeCommand $tauriCommand @(
        "build",
        "--no-bundle",
        "--config",
        $msixConfigPath,
        "--ci"
    )

    $builtExecutable = Join-Path $repositoryRoot "target\release\tapconductor-app.exe"
    Assert-Condition (Test-Path -LiteralPath $builtExecutable -PathType Leaf) (
        "Tauri did not produce the expected executable: $builtExecutable"
    )

    $stagingRoot = Reset-SafeDirectory (
        Join-Path $targetRoot "store-msix\staging\x64"
    ) $targetRoot
    Copy-RequiredFile $builtExecutable (Join-Path $stagingRoot "tapconductor-app.exe")

    foreach ($entry in $requiredResources.GetEnumerator()) {
        $sourcePath = Join-Path $repositoryRoot $entry.Key
        $destinationPath = Join-Path $stagingRoot $entry.Value
        if (Test-Path -LiteralPath $sourcePath -PathType Container) {
            $null = New-Item -ItemType Directory -Path $destinationPath -Force
            Get-ChildItem -LiteralPath $sourcePath -Force | Copy-Item `
                -Destination $destinationPath `
                -Recurse `
                -Force
        }
        else {
            Copy-RequiredFile $sourcePath $destinationPath
        }
    }

    $assetsDirectory = Join-Path $stagingRoot "Assets"
    $null = New-Item -ItemType Directory -Path $assetsDirectory -Force
    foreach ($iconName in $requiredIcons) {
        Copy-RequiredFile (
            Join-Path $repositoryRoot "src-tauri\icons\$iconName"
        ) (Join-Path $assetsDirectory $iconName)
    }

    $manifestPath = Join-Path $stagingRoot "Package.appxmanifest"
    Render-Manifest `
        -TemplatePath $manifestTemplatePath `
        -DestinationPath $manifestPath `
        -ResolvedIdentityName $resolvedIdentityName `
        -ResolvedPublisher $resolvedPublisher `
        -ResolvedPublisherDisplayName $resolvedPublisherDisplayName `
        -ResolvedVersion $resolvedVersion

    $stagedFiles = @(Get-ChildItem -LiteralPath $stagingRoot -Recurse -File)
    Assert-Condition ($stagedFiles.Count -gt 250) "The MSIX staging layout is unexpectedly incomplete."
    Write-Host "MSIX layout staged at: $stagingRoot"
    Write-Host "Staged files: $($stagedFiles.Count)"

    if ($StageOnly) {
        Write-Host "Stage only: winapp was not invoked and no MSIX package was created."
        exit 0
    }

    $winappCommand = (Get-Command winapp.exe -ErrorAction Stop).Source
    $outputKind = if ($Mode -eq "Store") { "store-upload-unsigned" } else { "test-signed" }
    $outputDirectory = Join-Path $targetRoot "store-artifacts\msix\$outputKind"
    $null = New-Item -ItemType Directory -Path $outputDirectory -Force
    $outputName = if ($Mode -eq "Store") {
        "TapConductor_${resolvedVersion}_x64_STORE-UPLOAD-UNSIGNED.msix"
    } else {
        "TapConductor_${resolvedVersion}_x64_LOCAL-TEST.msix"
    }
    $outputArtifact = Join-Path $outputDirectory $outputName
    if (Test-Path -LiteralPath $outputArtifact) {
        Remove-Item -LiteralPath $outputArtifact -Force
    }

    $packArguments = @(
        "pack",
        $stagingRoot,
        "--manifest",
        $manifestPath,
        "--output",
        $outputArtifact,
        "--executable",
        "tapconductor-app.exe"
    )

    if ($Mode -eq "Test") {
        $developmentDirectory = Join-Path $targetRoot "store-msix\development"
        $null = New-Item -ItemType Directory -Path $developmentDirectory -Force
        $developmentCertificate = Join-Path $developmentDirectory "TapConductorDevelopment.pfx"
        if (-not (Test-Path -LiteralPath $developmentCertificate -PathType Leaf)) {
            Invoke-NativeCommand $winappCommand @(
                "cert", "generate",
                "--manifest", $manifestPath,
                "--output", $developmentCertificate,
                "--password", $DevCertificatePassword,
                "--export-cer"
            )
        }
        $packArguments += @(
            "--cert", $developmentCertificate,
            "--cert-password", $DevCertificatePassword
        )
    }

    Invoke-NativeCommand $winappCommand $packArguments
    Assert-Condition (Test-Path -LiteralPath $outputArtifact -PathType Leaf) (
        "winapp did not create the expected package: $outputArtifact"
    )

    $signature = Get-AuthenticodeSignature -LiteralPath $outputArtifact
    if ($Mode -eq "Store") {
        Assert-Condition ($null -eq $signature.SignerCertificate) (
            "The Store upload package must be unsigned so Partner Center can sign it."
        )
    }
    else {
        Assert-Condition ($null -ne $signature.SignerCertificate) (
            "The local-test MSIX package was not signed with the development certificate."
        )
    }

    $hash = Get-FileHash -LiteralPath $outputArtifact -Algorithm SHA256
    "$($hash.Hash)  $outputName" | Set-Content -LiteralPath (
        Join-Path $outputDirectory "SHA256SUMS.txt"
    ) -Encoding ASCII

    Write-Host ""
    Write-Host "MSIX artifact: $outputArtifact"
    Write-Host "SHA-256: $($hash.Hash)"
    if ($Mode -eq "Store") {
        Write-Host "Store upload package: intentionally unsigned; upload it only through Partner Center."
    }
    else {
        Write-Host "Local test only. Trust the generated CER before installing this package."
        Write-Host "Development certificate: $developmentDirectory\TapConductorDevelopment.cer"
    }
}
finally {
    Pop-Location
}
