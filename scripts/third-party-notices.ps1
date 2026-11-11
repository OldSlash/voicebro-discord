# Writes THIRD-PARTY-NOTICES.txt: every crate built into voicebro.exe (the normal
# dependencies of voicebro-desktop for x86_64-pc-windows-msvc, from `cargo metadata`) with
# its version, license, repository and the license and notice files it ships; the native
# libraries that crates compile or link into voicebro.exe under licenses of their own; the
# installer's components; the components the app downloads only after the user's OK (Vosk
# and its models); and what the app needs but does not ship (the Codex CLI). License texts
# that no crate ships are kept in scripts\notices\. The script stops and writes nothing when
# one of those texts is missing, a crate with a native library changed its version, or its
# list of wake models and the catalog in wake\catalog.rs differ.
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

# Native libraries that crates compile or link into voicebro.exe under licenses of their
# own. Each entry was checked for one version of its crate: another version stops the
# script, since the bundled library and the place of its license text may have changed.
# Text is a path inside the crate ("crate:...") or in this repository.
$native = @(
    @{
        Crate = "libopus_sys"; Checked = "0.3.3"; Name = "Opus 1.5"
        What = "The Opus audio codec, a C library compiled from the sources inside the crate."
        License = "BSD-3-Clause"; Source = "https://opus-codec.org"
        Text = "crate:opus\COPYING"
    },
    @{
        Crate = "webview2-com-sys"; Checked = "0.39.1"; Name = "Microsoft Edge WebView2 SDK 1.0.3800.47"
        What = "WebView2LoaderStatic.lib, the WebView2 loader, linked from the crate."
        License = "the WebView2 SDK license (BSD-style)"
        Source = "https://www.nuget.org/packages/Microsoft.Web.WebView2/1.0.3800.47"
        Text = "scripts\notices\WebView2-SDK-LICENSE.txt"
    }
)
$problems = New-Object System.Collections.Generic.List[string]
function Read-NoticeText([string]$path) {
    if (-not (Test-Path -LiteralPath $path)) {
        $problems.Add("license text not found: $path")
        return ""
    }
    return ([System.IO.File]::ReadAllText((Resolve-Path -LiteralPath $path).Path)).TrimEnd()
}
function Add-Component($name, $license, $source, $lines, $textName, $body) {
    [void]$text.AppendLine("-" * 60)
    [void]$text.AppendLine($name)
    foreach ($line in $lines) { [void]$text.AppendLine($line) }
    [void]$text.AppendLine("License: $license")
    [void]$text.AppendLine("Source: $source")
    if ($body) {
        [void]$text.AppendLine("")
        [void]$text.AppendLine("--- $textName ---")
        [void]$text.AppendLine($body)
    }
    [void]$text.AppendLine("")
}
function Add-Heading($title) {
    [void]$text.AppendLine("=" * 60)
    [void]$text.AppendLine($title)
    [void]$text.AppendLine("=" * 60)
    [void]$text.AppendLine("")
}

$text = New-Object System.Text.StringBuilder
[void]$text.AppendLine("Third-party software in VoiceBro $version")
[void]$text.AppendLine("=" * 60)
[void]$text.AppendLine("")
[void]$text.AppendLine("VoiceBro itself is under the MIT License (see LICENSE). This file lists the")
[void]$text.AppendLine("third-party software in it and around it:")
[void]$text.AppendLine("")
[void]$text.AppendLine("  1. the $($crates.Count) Rust crates voicebro.exe is built from;")
[void]$text.AppendLine("  2. the native libraries compiled or linked into voicebro.exe from inside them;")
[void]$text.AppendLine("  3. the installer's own components;")
[void]$text.AppendLine("  4. what VoiceBro downloads only after you agree;")
[void]$text.AppendLine("  5. what VoiceBro needs but does not ship.")
[void]$text.AppendLine("")
[void]$text.AppendLine("Each is listed with its version, its license and, where there is one, the full")
[void]$text.AppendLine("text of its license and notice files.")
[void]$text.AppendLine("")
Add-Heading "1. Rust crates built into voicebro.exe"
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

Add-Heading "2. Native libraries inside the crates, built into voicebro.exe"
foreach ($entry in $native) {
    $crate = $crates | Where-Object { $_["name"] -eq $entry.Crate } | Select-Object -First 1
    if (-not $crate) { continue }
    if ($crate["version"] -ne $entry.Checked) {
        $problems.Add("$($entry.Crate) is $($crate["version"]) in the build, but its entry was checked for $($entry.Checked): find out which library it bundles now and where its license text is, then update the entry")
        continue
    }
    $path = if ($entry.Text.StartsWith("crate:")) {
        Join-Path (Split-Path $crate["manifest_path"]) $entry.Text.Substring(6)
    } else {
        $entry.Text
    }
    Add-Component $entry.Name $entry.License $entry.Source @($entry.What, "Comes with: $($entry.Crate) $($crate["version"])") (Split-Path -Leaf $path) (Read-NoticeText $path)
}
$ring = $crates | Where-Object { $_["name"] -eq "ring" } | Select-Object -First 1
if ($ring) {
    [void]$text.AppendLine("-" * 60)
    [void]$text.AppendLine("ring $($ring["version"]) also contains C and assembly code derived from BoringSSL and")
    [void]$text.AppendLine("OpenSSL. Its LICENSE, LICENSE-BoringSSL and LICENSE-other-bits, listed with the crate")
    [void]$text.AppendLine("in section 1, cover that code.")
    [void]$text.AppendLine("")
}

Add-Heading "3. The installer (VoiceBro_${version}_x64-setup.exe)"
Add-Component "NSIS 3.13 (Nullsoft Scriptable Install System)" "zlib/libpng; the bzip2 and LZMA compression modules under the terms in its COPYING" "https://nsis.sourceforge.io" @("The installer's program, its plug-ins and its LZMA decompression.") "COPYING" (Read-NoticeText "scripts\notices\NSIS-COPYING.txt")
Add-Component "nsis_tauri_utils 0.5.3" "MIT OR Apache-2.0" "https://github.com/tauri-apps/nsis-tauri-utils" @("Tauri's NSIS plug-in.") "LICENSE_MIT" (Read-NoticeText "scripts\notices\nsis_tauri_utils-LICENSE_MIT.txt")
Add-Component "Microsoft Edge WebView2 Runtime" "Microsoft's license terms for the WebView2 Runtime" "https://developer.microsoft.com/microsoft-edge/webview2/" @("Not part of VoiceBro's files. When the runtime is missing, the installer downloads", "Microsoft's installer for it (MicrosoftEdgeWebview2Setup.exe) from go.microsoft.com", "and runs it.") "" ""

# The wake catalog's models with their licenses. This list must name exactly the models of
# crates\voicebro-core\src\wake\catalog.rs: a model on one side only stops the script.
$wakeModels = [ordered]@{
    "vosk-model-small-en-us-0.15" = "Apache-2.0"
    "vosk-model-small-ru-0.22" = "Apache-2.0"
    "vosk-model-small-cs-0.4-rhasspy" = "MIT"
    "vosk-model-small-nl-0.22" = "Apache-2.0"
    "vosk-model-small-fr-0.22" = "Apache-2.0"
    "vosk-model-small-de-0.15" = "Apache-2.0"
    "vosk-model-small-it-0.22" = "Apache-2.0"
    "vosk-model-small-pl-0.22" = "Apache-2.0"
    "vosk-model-small-pt-0.3" = "Apache-2.0"
    "vosk-model-small-es-0.42" = "Apache-2.0"
}
$catalogIds = @(Select-String -Path "crates\voicebro-core\src\wake\catalog.rs" -Pattern 'directory: "(vosk-model-[^"]+)"' |
    ForEach-Object { $_.Matches[0].Groups[1].Value })
foreach ($id in $catalogIds) {
    if (-not $wakeModels.Contains($id)) { $problems.Add("wake model $id is in catalog.rs but not in the notices script") }
}
foreach ($id in $wakeModels.Keys) {
    if ($catalogIds -notcontains $id) { $problems.Add("wake model $id is in the notices script but not in catalog.rs") }
}

Add-Heading "4. Downloaded only after you agree in the window"
[void]$text.AppendLine("Vosk 0.3.45 (libvosk.dll and its runtime DLLs)")
[void]$text.AppendLine("License: Apache-2.0")
[void]$text.AppendLine("Source: https://github.com/alphacep/vosk-api")
[void]$text.AppendLine("")
[void]$text.AppendLine("Vosk models, one per wake word language you choose:")
foreach ($id in $wakeModels.Keys) {
    if ($wakeModels[$id] -eq "Apache-2.0") { [void]$text.AppendLine("  $id") }
}
[void]$text.AppendLine("License: Apache-2.0")
[void]$text.AppendLine("Source: https://alphacephei.com/vosk/models")
[void]$text.AppendLine("")
Add-Component "Vosk model vosk-model-small-cs-0.4-rhasspy (Czech, from the Rhasspy project)" "MIT" "https://github.com/rhasspy/cs_kaldi-rhasspy (downloaded from https://alphacephei.com/vosk/models)" @() "LICENSE" (Read-NoticeText "scripts\notices\cs_kaldi-rhasspy-LICENSE.txt")
[void]$text.AppendLine("Models you put into the models folder yourself carry their own licenses.")
[void]$text.AppendLine("")

Add-Heading "5. Needed but not shipped"
[void]$text.AppendLine("Codex CLI (OpenAI)")
[void]$text.AppendLine("License: Apache-2.0")
[void]$text.AppendLine("Source: https://github.com/openai/codex")
[void]$text.AppendLine("You install it yourself, or from VoiceBro's Status tab through winget (package")
[void]$text.AppendLine("OpenAI.Codex). VoiceBro starts it and talks to it; it does not ship or change it.")

if ($problems.Count -gt 0) {
    foreach ($problem in $problems) { Write-Error "third-party-notices: $problem" -ErrorAction Continue }
    exit 1
}

New-Item -ItemType Directory -Force $Out | Out-Null
$path = Join-Path (Resolve-Path $Out) "THIRD-PARTY-NOTICES.txt"
[System.IO.File]::WriteAllText($path, $text.ToString(), (New-Object System.Text.UTF8Encoding($false)))
Write-Output "$path ($($crates.Count) crates)"
