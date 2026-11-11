# Makes the wake-word corpus WAVs (16 kHz mono 16-bit) with the Windows voices, inside the
# project: target\wake-corpus\ru\ (Irina) and target\wake-corpus\en\ (David and Zira).
# The phrases are the test's own: activation "братан" / "bro, brother, hey bro",
# deactivation "отбой" / "goodbye". Run from the repository root:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\make-wake-corpus.ps1
param(
    [string]$Out = "target\wake-corpus"
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

$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
$installed = $synth.GetInstalledVoices() | ForEach-Object { $_.VoiceInfo.Name }
$format = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000,
    [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,
    [System.Speech.AudioFormat.AudioChannel]::Mono)

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
$synth.Dispose()
