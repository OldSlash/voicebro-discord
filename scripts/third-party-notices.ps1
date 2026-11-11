# Writes THIRD-PARTY-NOTICES.txt: every crate built into voicebro.exe (the normal
# dependencies of voicebro-desktop for x86_64-pc-windows-msvc, from `cargo metadata`) with
# its version, license, repository and the license and notice files it ships, then the
# components the app downloads only after the user's OK (Vosk and its models).
# Run from the repository root; the release script calls it before `cargo tauri build`:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\third-party-notices.ps1
param(
    [string]$Out = "target\release\dist-notices"
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Web.Extensions

$version = (Get-Content -Raw "apps\desktop\tauri.conf.json" | ConvertFrom-Json).version
$json = cargo metadata --format-version 1 --filter-platform x86_64-pc-windows-msvc | Out-String
if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed" }
# ConvertFrom-Json in Windows PowerShell stops at 2 MB; the metadata is larger.
$serializer = New-Object System.Web.Script.Serialization.JavaScriptSerializer
$serializer.MaxJsonLength = [int]::MaxValue
$serializer.RecursionLimit = 1000
$metadata = $serializer.DeserializeObject($json)

# Which crates: `cargo tree` for voicebro-desktop alone, normal dependencies only (build
# scripts and dev dependencies do not ship). The metadata's resolve graph is not used for
# this: it unifies the features of the whole workspace, dev dependencies included, and
# lists crates the app does not build.
$tree = cargo tree -p voicebro-desktop -e normal --target x86_64-pc-windows-msvc --prefix none --format "{p}"
if ($LASTEXITCODE -ne 0) { throw "cargo tree failed" }
$wanted = @{}
foreach ($line in $tree) {
    $parts = $line.Trim() -split " "
    if ($parts.Count -ge 2) { $wanted["$($parts[0]) $($parts[1].TrimStart('v'))"] = $true }
}
$workspace = @{}
foreach ($id in $metadata["workspace_members"]) { $workspace[$id] = $true }

$crates = $metadata["packages"] |
    Where-Object { $wanted.ContainsKey("$($_["name"]) $($_["version"])") -and -not $workspace.ContainsKey($_["id"]) } |
    Sort-Object { $_["name"] }, { $_["version"] }

$text = New-Object System.Text.StringBuilder
[void]$text.AppendLine("Third-party software in VoiceBro $version")
[void]$text.AppendLine("=" * 60)
[void]$text.AppendLine("")
[void]$text.AppendLine("VoiceBro itself is under the MIT License (see LICENSE). voicebro.exe is built from")
[void]$text.AppendLine("the $($crates.Count) Rust crates below. Each is listed with its version, its license and,")
[void]$text.AppendLine("where the crate ships them, the texts of its license and notice files.")
[void]$text.AppendLine("")
foreach ($crate in $crates) {
    [void]$text.AppendLine("-" * 60)
    [void]$text.AppendLine("$($crate["name"]) $($crate["version"])")
    $license = if ($crate["license"]) { $crate["license"] } else { "see the license file below" }
    [void]$text.AppendLine("License: $license")
    if ($crate["repository"]) { [void]$text.AppendLine("Source: $($crate["repository"])") }
    $dir = Split-Path $crate["manifest_path"]
    $files = Get-ChildItem -File -LiteralPath $dir |
        Where-Object { $_.Name -match '^(LICEN[CS]E|COPYING|NOTICE|UNLICENSE)' } |
        Sort-Object Name
    if ($crate["license_file"] -and -not ($files | Where-Object { $_.Name -eq (Split-Path -Leaf $crate["license_file"]) })) {
        $extra = Join-Path $dir $crate["license_file"]
        if (Test-Path -LiteralPath $extra) { $files = @($files) + (Get-Item -LiteralPath $extra) }
    }
    foreach ($file in $files) {
        [void]$text.AppendLine("")
        [void]$text.AppendLine("--- $($file.Name) ---")
        [void]$text.AppendLine(([System.IO.File]::ReadAllText($file.FullName)).TrimEnd())
    }
    [void]$text.AppendLine("")
}
[void]$text.AppendLine("=" * 60)
[void]$text.AppendLine("Downloaded only after you agree in the window (Settings > Assistant > Download)")
[void]$text.AppendLine("=" * 60)
[void]$text.AppendLine("")
[void]$text.AppendLine("Vosk 0.3.45 (libvosk.dll and its runtime DLLs)")
[void]$text.AppendLine("License: Apache-2.0")
[void]$text.AppendLine("Source: https://github.com/alphacep/vosk-api")
[void]$text.AppendLine("")
[void]$text.AppendLine("Vosk models vosk-model-small-en-us-0.15 and vosk-model-small-ru-0.22")
[void]$text.AppendLine("License: Apache-2.0")
[void]$text.AppendLine("Source: https://alphacephei.com/vosk/models")
[void]$text.AppendLine("")
[void]$text.AppendLine("Models you put into the models folder yourself carry their own licenses.")

New-Item -ItemType Directory -Force $Out | Out-Null
$path = Join-Path (Resolve-Path $Out) "THIRD-PARTY-NOTICES.txt"
[System.IO.File]::WriteAllText($path, $text.ToString(), (New-Object System.Text.UTF8Encoding($false)))
Write-Output "$path ($($crates.Count) crates)"
