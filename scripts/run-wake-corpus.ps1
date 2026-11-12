# Runs the whole wake-word corpus, one file per run so that a wrong file never hides the
# others, without noise and with noise 300, and prints one line per run with what Vosk
# heard. Needs the assets (VOICEBRO_VOSK_DIR layout) in target\wake-corpus\assets and the
# WAVs made by scripts\make-wake-corpus.ps1 (-Codex for the eight Codex-voiced sets).
# Run from the repository root:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\run-wake-corpus.ps1 [-Languages de,fr]
# The phrases are the test's own (activation | deactivation), the same as in
# catalog_models_load_when_enabled.
# -CorpusRoot runs the same files from another folder with the same layout (the assets stay
# in $Root\assets), for example the copies `probe opus-compare` decoded with each codec:
#   ... -File scripts\run-wake-corpus.ps1 -CorpusRoot target\opus-compare\receive\opus-rs
param(
    [string]$Root = "target\wake-corpus",
    [string]$CorpusRoot,
    [string[]]$Languages
)
$ErrorActionPreference = "Stop"
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
        "ru\ru-positive.wav" = "Activate"; "ru\ru-negative.wav" = ""; "ru\ru-goodbye.wav" = "Deactivate" })
    "en" = @("vosk-model-small-en-us-0.15", "bro, brother, hey bro | goodbye", [ordered]@{})
}
foreach ($voice in "david", "zira") {
    $files = $sets["en"][2]
    foreach ($name in "hey-bro", "bro-start", "brother", "bro-end") { $files["en\$voice-en-$name.wav"] = "Activate" }
    foreach ($name in "lookalike", "plain") { $files["en\$voice-en-$name.wav"] = "" }
    $files["en\$voice-en-goodbye.wav"] = "Deactivate"
}
$codex = [ordered]@{
    "cs" = @("vosk-model-small-cs-0.4-rhasspy", "ahoj kámo | konec")
    "nl" = @("vosk-model-small-nl-0.22", "hallo maat | tot ziens")
    "fr" = @("vosk-model-small-fr-0.22", "salut pote | au revoir")
    "de" = @("vosk-model-small-de-0.15", "hallo kumpel | tschüss")
    "it" = @("vosk-model-small-it-0.22", "ciao amico | arrivederci")
    "pl" = @("vosk-model-small-pl-0.22", "cześć stary | do widzenia")
    "pt" = @("vosk-model-small-pt-0.3", "olá amigo | tchau")
    "es" = @("vosk-model-small-es-0.42", "hola amigo | adiós")
}
foreach ($lang in $codex.Keys) {
    $sets[$lang] = @($codex[$lang][0], $codex[$lang][1], [ordered]@{
        "$lang\$lang-start.wav" = "Activate"; "$lang\$lang-end.wav" = "Activate"
        "$lang\$lang-plain.wav" = ""; "$lang\$lang-goodbye.wav" = "Deactivate" })
}

$env:VOICEBRO_VOSK_DIR = Join-Path $Root "assets"
foreach ($name in "VOICEBRO_WAKE_CATALOG", "VOICEBRO_WAKE_WAV", "VOICEBRO_WAKE_EXPECT") {
    Remove-Item "Env:$name" -ErrorAction SilentlyContinue
}
$right = 0; $total = 0
foreach ($lang in $sets.Keys) {
    if ($Languages -and $Languages -notcontains $lang) { continue }
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
            "{0} noise {1,-3} {2,-26} want {3,-10} {4,-5} {5}" -f $lang, $noise, (Split-Path $file -Leaf), $want, $mark, ($heard -join " ")
        }
    }
}
"right: $right of $total"
