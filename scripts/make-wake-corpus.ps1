# Makes the wake-word corpus WAVs (16 kHz mono 16-bit) with the Windows voices, inside the
# project: target\wake-corpus\ru\ (Irina) and target\wake-corpus\en\ (David and Zira).
# The phrases are the catalog's default phrases of each model (change
# voice-recognition-rework, design D7): activation "братан" / "bro", deactivation "отбой" /
# "peace out". Each language has the activation alone and at a sentence's start and end, a
# plain sentence without a phrase, and the deactivation alone and at a sentence's end.
# Run from the repository root:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\make-wake-corpus.ps1
#
# -Codex also makes the sets of the eight other catalog languages, spoken by the Codex
# voice through `probe codex-say` (Windows has no voices for them): 48 kHz mono WAVs in
# target\wake-corpus\<cs|nl|fr|de|it|pl|pt|es>\, six per language, and the voice's
# transcript of every attempt in transcripts.txt there. That is 48 real calls with the
# ChatGPT sign-in (an existing file is kept, so a rerun only makes what is missing; -Force
# makes them all again); build the probe first (cargo build -p probe). -CodexOnly skips
# the Windows-voice sets; -Languages de,fr limits the Codex sets.
#
# -Deactivation makes the Russian deactivation set in target\wake-corpus\ru\ (Irina):
# "отбой" alone, after "ладно", repeated and inside a sentence; "Отбой, братан." and
# "Всё, отбой, братан, спасибо."; "Заканчивай." and "Ну, братан, всё.". The test of the
# gaps in a speaker's audio (vosk_gaps_when_assets_are_provided) and the corpus test with
# phrases of several words run on them.
param(
    [string]$Out = "target\wake-corpus",
    [switch]$Codex,
    [switch]$CodexOnly,
    [switch]$Deactivation,
    [switch]$Force,
    [string[]]$Languages,
    # The probe that runs codex-say: a copy elsewhere lets cargo rebuild meanwhile.
    [string]$Probe = "target\debug\probe.exe"
)
$ErrorActionPreference = "Stop"
# The old wake corpus is made with the Windows voices (change voice-corpus-tests, D10).
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    Write-Output "make-wake-corpus.ps1 runs only on Windows: the old wake corpus is made with the Windows voices."
    exit 0
}
Add-Type -AssemblyName System.Speech
# Through -File, "de,fr" arrives as one string.
$Languages = @($Languages | ForEach-Object { $_ -split "," } | Where-Object { $_ })

$corpus = @{
    "ru" = @{
        voices = @("Microsoft Irina Desktop")
        lines  = [ordered]@{
            "ru-alone"         = "Братан."
            "ru-positive"      = "Братан, сколько сейчас времени в Бангкоке?"
            "ru-end"           = "Хорошая была игра, братан."
            "ru-negative"      = "Пойдём к замку и подождём остальных."
            "ru-goodbye-alone" = "Отбой."
            "ru-goodbye"       = "Спасибо за помощь. Отбой."
        }
    }
    "en" = @{
        voices = @("Microsoft David Desktop", "Microsoft Zira Desktop")
        lines  = [ordered]@{
            "en-bro-alone"   = "Bro."
            "en-hey-bro"     = "Hey bro, what time is it in London?"
            "en-bro-start"   = "Bro, how do I craft a sword?"
            "en-bro-end"     = "That was a good game, bro."
            "en-brother"     = "Brother, tell me a joke."
            "en-lookalike"   = "I broke my bow, he brought the brown boat, nice bros."
            "en-plain"       = "Let's go to the castle and wait for the others."
            "en-peace-alone" = "Peace out."
            "en-goodbye"     = "Thanks for the help. Peace out."
        }
    }
}

# The Russian deactivation set (-Deactivation).
$deactivationSet = [ordered]@{
    "ru-deact-alone"         = "Отбой."
    "ru-deact-ladno"         = "Ладно, отбой."
    "ru-deact-repeat"        = "Отбой, отбой, отбой."
    "ru-deact-sentence"      = "Хорошо, отбой, увидимся завтра."
    "ru-deact-two"           = "Отбой, братан."
    "ru-deact-two-sentence"  = "Всё, отбой, братан, спасибо."
    "ru-deact-finish"        = "Заканчивай."
    "ru-deact-all"           = "Ну, братан, всё."
}

# The Codex-voiced sets: the activation phrase alone, at the start of a question and at the
# end of a sentence, a plain sentence with neither phrase, and the deactivation phrase alone
# and at the end of a sentence. The phrases (activation | deactivation) are the catalog's
# defaults, the same that catalog_models_load_when_enabled checks.
$codexCorpus = [ordered]@{
    "cs" = @{ voice = "juniper"; phrases = "brácha | čau"; lines = [ordered]@{
        "alone" = "Brácha."
        "start" = "Brácha, kolik je teď hodin v Praze?"
        "end" = "To byla dobrá hra, brácha."
        "plain" = "Pojďme k hradu a počkáme na ostatní."
        "goodbye-alone" = "Čau."
        "goodbye" = "Díky za pomoc. Čau." } }
    "nl" = @{ voice = "cove"; phrases = "maat | doei doei"; lines = [ordered]@{
        "alone" = "Maat."
        "start" = "Maat, hoe laat is het nu in Amsterdam?"
        "end" = "Dat was een goed spel, maat."
        "plain" = "Laten we naar het kasteel gaan en op de anderen wachten."
        "goodbye-alone" = "Doei doei."
        "goodbye" = "Bedankt voor de hulp. Doei doei." } }
    "fr" = @{ voice = "juniper"; phrases = "mon frère | à la prochaine, a la prochaine"; lines = [ordered]@{
        "alone" = "Mon frère."
        "start" = "Mon frère, quelle heure est-il à Paris ?"
        "end" = "C'était une bonne partie, mon frère."
        "plain" = "Allons au château et attendons les autres."
        "goodbye-alone" = "À la prochaine."
        "goodbye" = "Merci pour l'aide. À la prochaine." } }
    "de" = @{ voice = "cove"; phrases = "bruder | bis dann"; lines = [ordered]@{
        "alone" = "Bruder."
        "start" = "Bruder, wie spät ist es in Berlin?"
        "end" = "Das war ein gutes Spiel, Bruder."
        "plain" = "Lass uns zur Burg gehen und auf die anderen warten."
        "goodbye-alone" = "Bis dann."
        "goodbye" = "Danke für die Hilfe. Bis dann." } }
    "it" = @{ voice = "juniper"; phrases = "fratello | arrivederci"; lines = [ordered]@{
        "alone" = "Fratello."
        "start" = "Fratello, che ore sono a Roma?"
        "end" = "È stata una bella partita, fratello."
        "plain" = "Andiamo al castello e aspettiamo gli altri."
        "goodbye-alone" = "Arrivederci."
        "goodbye" = "Grazie per l'aiuto. Arrivederci." } }
    "pl" = @{ voice = "cove"; phrases = "ziomek | do widzenia"; lines = [ordered]@{
        "alone" = "Ziomek."
        "start" = "Ziomek, która jest godzina w Warszawie?"
        "end" = "To była dobra gra, ziomek."
        "plain" = "Chodźmy do zamku i poczekajmy na resztę."
        "goodbye-alone" = "Do widzenia."
        "goodbye" = "Dzięki za pomoc. Do widzenia." } }
    "pt" = @{ voice = "juniper"; phrases = "irmão | até mais"; lines = [ordered]@{
        "alone" = "Irmão."
        "start" = "Irmão, que horas são em São Paulo?"
        "end" = "Foi um bom jogo, irmão."
        "plain" = "Vamos ao castelo esperar os outros."
        "goodbye-alone" = "Até mais."
        "goodbye" = "Obrigado pela ajuda. Até mais." } }
    "es" = @{ voice = "cove"; phrases = "hermano | hasta luego"; lines = [ordered]@{
        "alone" = "Hermano."
        "start" = "Hermano, ¿qué hora es en Madrid?"
        "end" = "Fue una buena partida, hermano."
        "plain" = "Vamos al castillo y esperemos a los demás."
        "goodbye-alone" = "Hasta luego."
        "goodbye" = "Gracias por la ayuda. Hasta luego." } }
}

$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
$installed = $synth.GetInstalledVoices() | ForEach-Object { $_.VoiceInfo.Name }
$format = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000,
    [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,
    [System.Speech.AudioFormat.AudioChannel]::Mono)
$format48 = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(48000,
    [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,
    [System.Speech.AudioFormat.AudioChannel]::Mono)

if ($Deactivation) {
    $dir = Join-Path $Out "ru"
    New-Item -ItemType Directory -Force $dir | Out-Null
    $synth.SelectVoice("Microsoft Irina Desktop")
    foreach ($name in $deactivationSet.Keys) {
        $path = Join-Path (Resolve-Path $dir) "$name.wav"
        $synth.SetOutputToWaveFile($path, $format)
        $synth.Speak($deactivationSet[$name])
        $synth.SetOutputToNull()
        Write-Output $path
    }
}

if (-not $CodexOnly) {
    foreach ($language in $corpus.Keys) {
        $dir = Join-Path $Out $language
        New-Item -ItemType Directory -Force $dir | Out-Null
        foreach ($voice in $corpus[$language].voices) {
            if ($installed -notcontains $voice) {
                throw "The Windows voice '$voice' is not installed; it is needed for the $language corpus."
            }
            $synth.SelectVoice($voice)
            $short = ($voice -split " ")[1].ToLower()
            foreach ($name in $corpus[$language].lines.Keys) {
                $file = if ($language -eq "ru") { "$name.wav" } else { "$short-$name.wav" }
                $path = Join-Path (Resolve-Path $dir) $file
                $synth.SetOutputToWaveFile($path, $format)
                $synth.Speak($corpus[$language].lines[$name])
                $synth.SetOutputToNull()
                Write-Output $path
            }
        }
    }
}

if ($Codex -or $CodexOnly) {
    $probe = $Probe
    if (-not (Test-Path $probe)) {
        throw "Build the probe first: cargo build -p probe (with CMake on PATH)."
    }
    $codexDir = Join-Path $Out "codex"
    New-Item -ItemType Directory -Force $codexDir | Out-Null
    # Any short speech makes the voice answer; the instruction tells it what to read.
    $trigger = Join-Path (Resolve-Path $codexDir) "trigger.wav"
    $synth.SelectVoice("Microsoft David Desktop")
    $synth.SetOutputToWaveFile($trigger, $format48)
    $synth.Speak("Go ahead.")
    $synth.SetOutputToNull()
    foreach ($language in $codexCorpus.Keys) {
        if ($Languages -and $Languages -notcontains $language) { continue }
        $set = $codexCorpus[$language]
        $dir = Join-Path $Out $language
        New-Item -ItemType Directory -Force $dir | Out-Null
        $log = Join-Path (Resolve-Path $dir) "transcripts.txt"
        foreach ($name in $set.lines.Keys) {
            $path = Join-Path (Resolve-Path $dir) "$language-$name.wav"
            $sentence = $set.lines[$name]
            # A file made from the same sentence before is kept, unless -Force.
            $madeFrom = if (Test-Path $log) {
                Select-String -Path $log -SimpleMatch "$language-$name.wav," -Encoding utf8 |
                    Where-Object { $_.Line -match "PASS; wanted: (.*?);" -and $Matches[1] -eq $sentence } |
                    Select-Object -Last 1
            }
            if (-not $Force -and (Test-Path $path) -and $madeFrom) {
                Write-Output "$path (kept)"
                continue
            }
            $env:VOICEBRO_PROBE_VOICE = $set.voice
            $done = $false
            for ($attempt = 1; $attempt -le 3 -and -not $done; $attempt++) {
                $output = & $probe codex-say $trigger $path $sentence | Out-String
                $done = $LASTEXITCODE -eq 0
                $said = ($output -split "`r?`n" | Where-Object { $_ -match "transcript assistant:" }) -join " "
                $result = if ($done) { "PASS" } else { "FAIL" }
                Add-Content -Encoding utf8 $log "$language-$name.wav, $($set.voice), attempt ${attempt}: $result; wanted: $sentence; $($said.Trim())"
            }
            if (-not $done) {
                throw "The Codex voice did not read '$sentence' word for word in 3 attempts; see $log."
            }
            Write-Output $path
        }
    }
    Remove-Item Env:VOICEBRO_PROBE_VOICE -ErrorAction SilentlyContinue
}
$synth.Dispose()
