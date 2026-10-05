param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version,
    [switch]$SkipBuild
)

# Build and stage Windows releases locally. Upload only verified artifacts.
$ErrorActionPreference = 'Stop'
$releaseRoot = $PSScriptRoot
$releaseDir = Join-Path $releaseRoot ".release-v$Version"
$artifactDir = Join-Path $releaseDir 'artifacts'
$frontendDir = Join-Path $releaseRoot 'frontend'
$nativeDir = Join-Path $releaseRoot 'target/release'
$configPath = Join-Path $frontendDir 'src-tauri/tauri.conf.json'
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program exited with code $LASTEXITCODE" }
}

# Keep the displayed, package and native executable versions consistent.
$configText = [IO.File]::ReadAllText($configPath)
$configText = ([regex]'("version"\s*:\s*")[^"]+("\s*,)').Replace($configText, "`${1}$Version`${2}", 1)
[IO.File]::WriteAllText($configPath, $configText, $utf8)
$packagePath = Join-Path $frontendDir 'package.json'
$packageText = [IO.File]::ReadAllText($packagePath)
$packageText = ([regex]'("version"\s*:\s*")[^"]+("\s*,)').Replace($packageText, "`${1}$Version`${2}", 1)
[IO.File]::WriteAllText($packagePath, $packageText, $utf8)
$cargoPath = Join-Path $frontendDir 'src-tauri/Cargo.toml'
$cargoText = [IO.File]::ReadAllText($cargoPath)
$cargoText = ([regex]'(?m)^(version\s*=\s*")[^"]+("\s*)$').Replace($cargoText, "`${1}$Version`${2}", 1)
[IO.File]::WriteAllText($cargoPath, $cargoText, $utf8)
[void][IO.Directory]::CreateDirectory($artifactDir)

$credentialNames = @('TAURI_SIGNING_PRIVATE_KEY', 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD')
$originalCredentials = @{}
foreach ($name in $credentialNames) { $originalCredentials[$name] = [Environment]::GetEnvironmentVariable($name) }

Push-Location -LiteralPath $frontendDir
try {
    # Only extract named signing assignments; never execute or log local notes.
    foreach ($localFile in @((Join-Path $frontendDir '.env'), (Join-Path $releaseRoot '.env'), (Join-Path $releaseRoot 'NOTES.md'))) {
        if (-not (Test-Path -LiteralPath $localFile)) { continue }
        $localText = [IO.File]::ReadAllText($localFile)
        foreach ($name in $credentialNames) {
            if ([Environment]::GetEnvironmentVariable($name)) { continue }
            $pattern = '(?m)(?:\$env:)?' + $name + '\s*=\s*([''"''])(.*?)\1'
            $assignment = [regex]::Match($localText, $pattern)
            if ($assignment.Success) { [Environment]::SetEnvironmentVariable($name, $assignment.Groups[2].Value, 'Process') }
        }
        $localText = $null
        $assignment = $null
    }
    if (-not $env:TAURI_SIGNING_PRIVATE_KEY -and $env:TAURI_SIGNING_PRIVATE_KEY_PATH) {
        $env:TAURI_SIGNING_PRIVATE_KEY = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $env:TAURI_SIGNING_PRIVATE_KEY_PATH).Path)
    }
    if (-not $env:TAURI_SIGNING_PRIVATE_KEY) { throw 'Local updater signing credentials are unavailable' }

    if (-not $SkipBuild) { Invoke-Checked 'pnpm' @('exec', 'tauri', 'build', '--no-bundle', '--ci') }
    $executable = Join-Path $nativeDir 'meetingone.exe'
    if (-not (Test-Path -LiteralPath $executable)) { throw 'The local release executable has not been built' }
    $binaryVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($executable).ProductVersion
    if ($binaryVersion -ne $Version) { throw "Local executable version $binaryVersion does not match $Version" }

    # Include runtime libraries beside the executable. Copying only the EXE
    # would work on the build machine but fail on a clean Windows installation.
    $config = [IO.File]::ReadAllText($configPath) | ConvertFrom-Json
    $resources = [ordered]@{}
    foreach ($resource in $config.bundle.resources) {
        $destination = ($resource -replace '[^/]*\*[^/]*$', '')
        $resources[$resource] = $destination
    }
    foreach ($required in @('onnxruntime.dll', 'sherpa-onnx-c-api.dll')) {
        if (-not (Test-Path -LiteralPath (Join-Path $nativeDir $required))) { throw "Missing native runtime: $required" }
    }
    $runtimeLibraries = Get-ChildItem -LiteralPath $nativeDir -Filter '*.dll' |
        Where-Object { $_.Name -match '^(onnxruntime|sherpa-onnx-|DirectML)' }
    foreach ($library in $runtimeLibraries) { $resources["../../target/release/$($library.Name)"] = $library.Name }

    # Native C++ dependencies import MSVCP140 and MSVCP140_1. Ship the
    # redistributable release DLLs from the build toolchain, not System32.
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Visual Studio redistributable locator is unavailable' }
    $visualStudio = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($LASTEXITCODE -ne 0 -or -not $visualStudio) { throw 'Visual Studio C++ build tools are unavailable' }
    $crtDirectory = Get-ChildItem "$visualStudio/VC/Redist/MSVC/*/x64/Microsoft.VC*.CRT" -Directory |
        Sort-Object FullName | Select-Object -Last 1
    if (-not $crtDirectory) { throw 'Visual C++ x64 redistributable DLLs are unavailable' }
    foreach ($required in @('msvcp140.dll', 'msvcp140_1.dll', 'vcruntime140.dll')) {
        if (-not (Test-Path -LiteralPath (Join-Path $crtDirectory.FullName $required))) { throw "Missing Visual C++ runtime: $required" }
    }
    foreach ($library in Get-ChildItem -LiteralPath $crtDirectory.FullName -Filter '*.dll') {
        Copy-Item -LiteralPath $library.FullName -Destination (Join-Path $nativeDir $library.Name)
        $resources["../../target/release/$($library.Name)"] = $library.Name
    }
    $bundleConfig = Join-Path $releaseDir 'windows-bundle.json'
    [IO.File]::WriteAllText($bundleConfig, (@{ bundle = @{ resources = $resources } } | ConvertTo-Json -Depth 8), $utf8)
    Invoke-Checked 'pnpm' @('exec', 'tauri', 'bundle', '--bundles', 'nsis,msi', '--config', $bundleConfig, '--ci')

    $installers = @(
        (Join-Path $nativeDir "bundle/nsis/ACT MeetingOne_${Version}_x64-setup.exe"),
        (Join-Path $nativeDir "bundle/msi/ACT MeetingOne_${Version}_x64_en-US.msi")
    )
    foreach ($installer in $installers) {
        foreach ($path in @($installer, "$installer.sig")) {
            if (-not (Test-Path -LiteralPath $path)) { throw "Missing release artifact: $path" }
            Copy-Item -LiteralPath $path -Destination (Join-Path $artifactDir ((Split-Path $path -Leaf) -replace ' ', '.'))
        }
    }

    $msiName = "ACT.MeetingOne_${Version}_x64_en-US.msi"
    $exeName = "ACT.MeetingOne_${Version}_x64-setup.exe"
    $downloadBase = "https://github.com/ncuong0704/MeetingOne/releases/download/v$Version"
    $msiEntry = @{ url = "$downloadBase/$msiName"; signature = [IO.File]::ReadAllText((Join-Path $artifactDir "$msiName.sig")).Trim() }
    $exeEntry = @{ url = "$downloadBase/$exeName"; signature = [IO.File]::ReadAllText((Join-Path $artifactDir "$exeName.sig")).Trim() }
    $manifest = [ordered]@{
        version = $Version
        notes = "ACT MeetingOne v$Version"
        pub_date = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
        platforms = [ordered]@{ 'windows-x86_64' = $msiEntry; 'windows-x86_64-msi' = $msiEntry; 'windows-x86_64-nsis' = $exeEntry }
    }
    [IO.File]::WriteAllText((Join-Path $artifactDir 'latest.json'), ($manifest | ConvertTo-Json -Depth 8), $utf8)

    $verifyManifest = Join-Path $releaseRoot 'scripts/verify-update-sig/Cargo.toml'
    Invoke-Checked 'cargo' @('build', '--manifest-path', $verifyManifest, '--release', '--locked')
    $verifier = Join-Path $releaseRoot 'scripts/verify-update-sig/target/release/verify-update-sig.exe'
    Invoke-Checked $verifier @((Join-Path $artifactDir $msiName), $msiEntry.signature, $config.plugins.updater.pubkey)
    Invoke-Checked $verifier @((Join-Path $artifactDir $exeName), $exeEntry.signature, $config.plugins.updater.pubkey)
    Write-Host "Verified local release artifacts: $artifactDir"
    Write-Host 'No commits, pushes or GitHub Actions builds were triggered.'
} finally {
    foreach ($name in $credentialNames) { [Environment]::SetEnvironmentVariable($name, $originalCredentials[$name], 'Process') }
    Pop-Location
}
