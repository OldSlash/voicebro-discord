# Builds what is handed over: the NSIS installer, the portable zip and SHA256SUMS.txt in
# target\release\dist\. Only from `main` at the tag v<version> with a clean tree, where
# <version> is the app's version in apps\desktop\tauri.conf.json and Cargo.toml.
# -Trial builds from any commit into target\release\dist-trial\ with "-trial" in every
# name, for testing only: a trial build is never handed over.
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-release.ps1 [-Trial]
param(
    [switch]$Trial
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Fail($message) {
    Write-Error "build-release: $message" -ErrorAction Continue
    exit 1
}

$version = (Get-Content -Raw "apps\desktop\tauri.conf.json" | ConvertFrom-Json).version
$cargoVersion = (Select-String -Path "apps\desktop\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
if ($version -ne $cargoVersion) {
    Fail "the version in tauri.conf.json ($version) differs from apps\desktop\Cargo.toml ($cargoVersion)"
}

if (-not $Trial) {
    $branch = (git rev-parse --abbrev-ref HEAD).Trim()
    if ($branch -ne "main") { Fail "a release is built from main, not from '$branch' (use -Trial to test)" }
    $dirty = git status --porcelain --untracked-files=no
    if ($dirty) { Fail "the working tree has uncommitted changes" }
    $tags = @(git tag --points-at HEAD)
    if ($tags -notcontains "v$version") { Fail "HEAD has no tag v$version (tags here: $($tags -join ', '))" }
}

# CMake for libopus_sys: the copy in the VS Build Tools, not on PATH by default.
$cmake = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin"
if (Test-Path $cmake) { $env:PATH += ";$cmake" }

& powershell -NoProfile -ExecutionPolicy Bypass -File "scripts\third-party-notices.ps1" -Out "target\release\dist-notices"
if ($LASTEXITCODE -ne 0) { Fail "the third-party notices could not be written" }

Push-Location "apps\desktop"
cargo tauri build
$built = $LASTEXITCODE
Pop-Location
if ($built -ne 0) { Fail "cargo tauri build failed" }

$suffix = if ($Trial) { "-trial" } else { "" }
$out = if ($Trial) { "target\release\dist-trial" } else { "target\release\dist" }
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force $out | Out-Null

$setupName = "VoiceBro_${version}_x64-setup$suffix.exe"
Copy-Item "target\release\bundle\nsis\VoiceBro_${version}_x64-setup.exe" (Join-Path $out $setupName)

$stage = "target\release\dist-zip\VoiceBro"
if (Test-Path "target\release\dist-zip") { Remove-Item -Recurse -Force "target\release\dist-zip" }
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item "target\release\voicebro.exe" $stage
Copy-Item "README.md" $stage
# The user guide and the images both documents show, at the paths their links use.
New-Item -ItemType Directory -Force "$stage\docs\images" | Out-Null
Copy-Item "docs\user-guide.md" "$stage\docs"
Copy-Item "docs\images\*.png" "$stage\docs\images"
Copy-Item "LICENSE" $stage
Copy-Item "target\release\dist-notices\THIRD-PARTY-NOTICES.txt" $stage
$zipName = "VoiceBro_${version}_x64-portable$suffix.zip"
Compress-Archive -Path "$stage\*" -DestinationPath (Join-Path $out $zipName)

$sums = foreach ($name in @($setupName, $zipName)) {
    $hash = (Get-FileHash -Algorithm SHA256 (Join-Path $out $name)).Hash.ToLower()
    "$hash  $name"
}
[System.IO.File]::WriteAllText((Join-Path (Resolve-Path $out) "SHA256SUMS.txt"), ($sums -join "`n") + "`n", (New-Object System.Text.UTF8Encoding($false)))

Write-Output "VoiceBro $version$suffix in $out"
Get-ChildItem $out | ForEach-Object { Write-Output ("  {0}  {1:N1} MB" -f $_.Name, ($_.Length / 1MB)) }
