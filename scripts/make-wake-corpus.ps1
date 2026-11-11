# Makes the wake-word corpus WAVs (16 kHz mono 16-bit) with the Windows voices, inside the
# project: target\wake-corpus\ru\ (Irina) and target\wake-corpus\en\ (David and Zira).
# The phrases are the test's own: activation "братан" / "bro, brother, hey bro",
# deactivation "отбой" / "goodbye". Run from the repository root:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\make-wake-corpus.ps1
#
# -Codex also makes the sets of the eight other catalog languages, spoken by the Codex
# voice through `probe codex-say` (Windows has no voices for them): 48 kHz mono WAVs in
# target\wake-corpus\<cs|nl|fr|de|it|pl|pt|es>\, four per language, and the voice's
# transcript of every attempt in transcripts.txt there. That is 32 real calls with the
# ChatGPT sign-in; build the probe first (cargo build -p probe). -CodexOnly skips the
# Windows-voice sets; -Languages de,fr limits the Codex sets.
param(
    [string]$Out = "target\wake-corpus",
    [switch]$Codex,
    [switch]$CodexOnly,
    [string[]]$Languages
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Speech

$corpus = @{
    "ru" = @{
        voices = @("Microsoft Irina Desktop")
        lines  = [ordered]@{
            "ru-positive" = "Братан, сколько сейчас времени в Москве?"
            "ru-negative" = "Пойдём к замку и подождём остальных."
            "ru-goodbye"  = "Спасибо за помощь. Отбой."
        }
    }
    "en" = @{
        voices = @("Microsoft David Desktop", "Microsoft Zira Desktop")
        lines  = [ordered]@{
            "en-hey-bro"   = "Hey bro, what time is it in London?"
            "en-bro-start" = "Bro, how do I craft a sword?"
            "en-brother"   = "Brother, tell me a joke."
            "en-bro-end"   = "That was a good game, bro."
            "en-lookalike" = "I broke my bow, he brought the brown boat, nice bros."
            "en-plain"     = "Let's go to the castle and wait for the others."
            "en-goodbye"   = "Thanks for the help. Goodbye."
        }
    }
}

# The Codex-voiced sets: the activation phrase at the start of a question, at the end of a
# sentence, a plain sentence with neither phrase, and the deactivation phrase. The phrases
# are the test's own (activation | deactivation), the same as in the live catalog test.
$codexCorpus = [ordered]@{
    "cs" = @{ voice = "juniper"; phrases = "ahoj kámo | konec"; lines = [ordered]@{
        "start" = "Ahoj kámo, kolik je teď hodin v Praze?"
        "end" = "To byla dobrá hra, ahoj kámo."
        "plain" = "Pojďme k hradu a počkáme na ostatní."
        "goodbye" = "Díky za pomoc. Konec." } }
    "nl" = @{ voice = "cove"; phrases = "hallo maat | tot ziens"; lines = [ordered]@{
        "start" = "Hallo maat, hoe laat is het nu in Amsterdam?"
        "end" = "Dat was een goed spel, hallo maat."
        "plain" = "Laten we naar het kasteel gaan en op de anderen wachten."
        "goodbye" = "Bedankt voor de hulp. Tot ziens." } }
    "fr" = @{ voice = "juniper"; phrases = "salut pote | au revoir"; lines = [ordered]@{
        "start" = "Salut pote, quelle heure est-il à Paris ?"
        "end" = "C'était une bonne partie, salut pote."
        "plain" = "Allons au château et attendons les autres."
        "goodbye" = "Merci pour l'aide. Au revoir." } }
    "de" = @{ voice = "cove"; phrases = "hallo kumpel | tschüss"; lines = [ordered]@{
        "start" = "Hallo Kumpel, wie spät ist es in Berlin?"
        "end" = "Das war ein gutes Spiel, hallo Kumpel."
        "plain" = "Lass uns zur Burg gehen und auf die anderen warten."
        "goodbye" = "Danke für die Hilfe. Tschüss." } }
    "it" = @{ voice = "juniper"; phrases = "ciao amico | arrivederci"; lines = [ordered]@{
        "start" = "Ciao amico, che ore sono a Roma?"
        "end" = "È stata una bella partita, ciao amico."
        "plain" = "Andiamo al castello e aspettiamo gli altri."
        "goodbye" = "Grazie per l'aiuto. Arrivederci." } }
    "pl" = @{ voice = "cove"; phrases = "cześć stary | do widzenia"; lines = [ordered]@{
        "start" = "Cześć stary, która jest godzina w Warszawie?"
        "end" = "To była dobra gra, cześć stary."
        "plain" = "Chodźmy do zamku i poczekajmy na resztę."
        "goodbye" = "Dzięki za pomoc. Do widzenia." } }
    "pt" = @{ voice = "juniper"; phrases = "olá amigo | tchau"; lines = [ordered]@{
        "start" = "Olá amigo, que horas são em São Paulo?"
        "end" = "Foi um bom jogo, olá amigo."
        "plain" = "Vamos ao castelo esperar os outros."
        "goodbye" = "Obrigado pela ajuda. Tchau." } }
    "es" = @{ voice = "cove"; phrases = "hola amigo | adiós"; lines = [ordered]@{
        "start" = "Hola amigo, ¿qué hora es en Madrid?"
        "end" = "Fue una buena partida, hola amigo."
        "plain" = "Vamos al castillo y esperemos a los demás."
        "goodbye" = "Gracias por la ayuda. Adiós." } }
}

$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
$installed = $synth.GetInstalledVoices() | ForEach-Object { $_.VoiceInfo.Name }
$format = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000,
    [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,
    [System.Speech.AudioFormat.AudioChannel]::Mono)
$format48 = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(48000,
    [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,
    [System.Speech.AudioFormat.AudioChannel]::Mono)

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
    $probe = "target\debug\probe.exe"
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
