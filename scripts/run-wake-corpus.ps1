# Runs the whole wake-word corpus, one file per run so that a wrong file never hides the
# others, without noise and with noise 300, and prints one line per run with what Vosk
# heard. Needs the assets (VOICEBRO_VOSK_DIR layout) in target\wake-corpus\assets and the
# WAVs made by scripts\make-wake-corpus.ps1 (-Piper for the eight Piper-voiced sets).
# Run from the repository root:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\run-wake-corpus.ps1 [-Languages de,fr]
# The phrases (activation | deactivation) are the catalog's default phrases of each model
# (change voice-recognition-rework, design D7), the same that
# catalog_models_load_when_enabled checks. The summary at the end counts, per default
# phrase, the runs that heard it, and the false hits.
# -Deactivation runs the Russian deactivation set of make-wake-corpus.ps1 -Deactivation with
# the deactivation list "Отбой братан, Заканчивай, Братан всё" and the activation "братан".
# -CorpusRoot runs the same files from another folder with the same layout (the assets stay
# in $Root\assets), for example the copies `probe opus-compare` decoded with each codec:
#   ... -File scripts\run-wake-corpus.ps1 -CorpusRoot target\opus-compare\receive\opus-rs
param(
    [string]$Root = "target\wake-corpus",
    [string]$CorpusRoot,
    [switch]$Deactivation,
    [string[]]$Languages
)
$ErrorActionPreference = "Stop"
# The old wake corpus is made with the Windows voices (change voice-corpus-tests, D10).
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    Write-Output "run-wake-corpus.ps1 runs only on Windows: the old wake corpus is made with the Windows voices."
    exit 0
}
[Console]::OutputEncoding = [Text.Encoding]::UTF8
# Through -File, "de,fr" arrives as one string.
$Languages = @($Languages | ForEach-Object { $_ -split "," } | Where-Object { $_ })
$cmake = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin"
if (-not (Get-Command cmake -ErrorAction SilentlyContinue) -and (Test-Path $cmake)) { $env:PATH += ";$cmake" }
# Windows PowerShell treats cargo's progress on stderr as an error under "Stop".
$ErrorActionPreference = "Continue"
$exe = cargo test --release -p voicebro-core --no-run --message-format=json 2>$null | ForEach-Object {
    try { $j = $_ | ConvertFrom-Json } catch { return }
    if ($j.reason -eq "compiler-artifact" -and $j.executable -and $j.target.kind -contains "lib") { $j.executable }
} | Select-Object -Last 1
$ErrorActionPreference = "Stop"
if (-not $exe) { throw "could not build the voicebro-core tests" }
$Root = (Resolve-Path $Root).Path
$CorpusRoot = if ($CorpusRoot) { (Resolve-Path $CorpusRoot).Path } else { $Root }

$sets = [ordered]@{
    "ru" = @("vosk-model-small-ru-0.22", "братан | отбой", [ordered]@{
        "ru\ru-alone.wav" = "Activate"; "ru\ru-positive.wav" = "Activate"; "ru\ru-end.wav" = "Activate"
        "ru\ru-negative.wav" = ""
        "ru\ru-goodbye-alone.wav" = "Deactivate"; "ru\ru-goodbye.wav" = "Deactivate" })
    "en" = @("vosk-model-small-en-us-0.15", "bro | peace out", [ordered]@{})
}
foreach ($voice in "david", "zira") {
    $files = $sets["en"][2]
    foreach ($name in "bro-alone", "hey-bro", "bro-start", "bro-end") { $files["en\$voice-en-$name.wav"] = "Activate" }
    foreach ($name in "brother", "lookalike", "plain") { $files["en\$voice-en-$name.wav"] = "" }
    foreach ($name in "peace-alone", "goodbye") { $files["en\$voice-en-$name.wav"] = "Deactivate" }
}
$others = [ordered]@{
    "cs" = @("vosk-model-small-cs-0.4-rhasspy", "brácha | čau")
    "nl" = @("vosk-model-small-nl-0.22", "maat | doei doei")
    "fr" = @("vosk-model-small-fr-0.22", "mon frère | à la prochaine, a la prochaine")
    "de" = @("vosk-model-small-de-0.15", "bruder | bis dann")
    "it" = @("vosk-model-small-it-0.22", "fratello | arrivederci")
    "pl" = @("vosk-model-small-pl-0.22", "ziomek | do widzenia")
    "pt" = @("vosk-model-small-pt-0.3", "irmão | até mais")
    "es" = @("vosk-model-small-es-0.42", "hermano | hasta luego")
}
foreach ($lang in $others.Keys) {
    $sets[$lang] = @($others[$lang][0], $others[$lang][1], [ordered]@{
        "$lang\$lang-alone.wav" = "Activate"; "$lang\$lang-start.wav" = "Activate"
        "$lang\$lang-end.wav" = "Activate"; "$lang\$lang-plain.wav" = ""
        "$lang\$lang-goodbye-alone.wav" = "Deactivate"; "$lang\$lang-goodbye.wav" = "Deactivate" })
}
if ($Deactivation) {
    # The user's lists of 2026-11-16: phrases of several words between commas.
    $sets = [ordered]@{
        "ru-deact" = @("vosk-model-small-ru-0.22", "братан | Отбой братан, Заканчивай, Братан всё", [ordered]@{
            "ru\ru-deact-alone.wav" = ""; "ru\ru-deact-ladno.wav" = ""; "ru\ru-deact-repeat.wav" = ""
            "ru\ru-deact-sentence.wav" = ""
            "ru\ru-deact-two.wav" = "Deactivate"; "ru\ru-deact-two-sentence.wav" = "Deactivate"
            "ru\ru-deact-finish.wav" = "Deactivate"; "ru\ru-deact-all.wav" = "Deactivate" })
    }
}

$env:VOICEBRO_VOSK_DIR = Join-Path $Root "assets"
foreach ($name in "VOICEBRO_WAKE_CATALOG", "VOICEBRO_WAKE_WAV", "VOICEBRO_WAKE_EXPECT") {
    Remove-Item "Env:$name" -ErrorAction SilentlyContinue
}
$right = 0; $total = 0
# Per language and kind: runs that should hear the phrase, those that did; false hits.
$byPhrase = [ordered]@{}
$falseHits = 0
foreach ($lang in $sets.Keys) {
    if ($Languages -and $Languages -notcontains ($lang -replace "-.*", "")) { continue }
    $model, $phrases, $files = $sets[$lang]
    $env:VOICEBRO_WAKE_MODEL = $model
    $env:VOICEBRO_WAKE_PHRASES = $phrases
    foreach ($noise in 0, 300) {
        if ($noise) { $env:VOICEBRO_WAKE_NOISE = "$noise" } else { Remove-Item Env:VOICEBRO_WAKE_NOISE -ErrorAction SilentlyContinue }
        foreach ($file in $files.Keys) {
            $path = Join-Path $CorpusRoot $file
            if (-not (Test-Path $path)) { "$lang missing $file"; continue }
            $expected = $files[$file]
            $env:VOICEBRO_WAKE_SEQUENCES = "$path=$expected"
            $ErrorActionPreference = "Continue"
            $lines = @(& $exe vosk_corpus --nocapture 2>&1 | ForEach-Object { "$_" })
            $ok = $LASTEXITCODE -eq 0
            $ErrorActionPreference = "Stop"
            $total++; if ($ok) { $right++ }
            $heard = ($lines | Where-Object { $_ -match "^sequence " }) -replace '^sequence .*?: heard ', ''
            if (-not $heard) { $heard = ($lines | Where-Object { $_ -match "panicked|unknown" }) -join " " }
            $want = if ($expected) { $expected } else { "none" }
            $mark = if ($ok) { "ok" } else { "WRONG" }
            if ($expected) {
                $key = "$lang $expected"
                if (-not $byPhrase.Contains($key)) { $byPhrase[$key] = @(0, 0) }
                $byPhrase[$key][0]++
                if ($ok) { $byPhrase[$key][1]++ }
            } elseif (-not $ok) {
                $falseHits++
            }
            "{0} noise {1,-3} {2,-30} want {3,-10} {4,-5} {5}" -f $lang, $noise, (Split-Path $file -Leaf), $want, $mark, ($heard -join " ")
        }
    }
}
"right: $right of $total"
foreach ($key in $byPhrase.Keys) {
    $lang, $kind = $key -split " "
    "{0,-9} {1,-10} heard in {2} of {3} runs ({4})" -f $lang, $kind, $byPhrase[$key][1], $byPhrase[$key][0], $sets[$lang][1]
}
"false hits: $falseHits"
