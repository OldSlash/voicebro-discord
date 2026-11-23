# Runs the voice corpus test (change voice-corpus-tests, design D9): every WAV under
# crates\voicebro-core\testdata\voice-corpus\en\ through the app's Vosk wake word and its
# Whisper phrase spotting, played as Discord delivers a speaker, as recorded and under
# noise. Prints the report, saves it as target\voice-corpus\runs\<yyyy-MM-dd-HHmmss>.txt
# and exits with the test's code (0 when every labeled file is right).
# Run from the repository root:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\run-voice-corpus.ps1 [-VoskDir <dir>] [-WhisperDir <dir>] [-Corpus <dir>] [-NoVosk] [-NoWhisper]
# The defaults are this PC's asset folders (LOCAL-TESTING.md); nothing is downloaded. On
# Linux run the same cargo test line with VOICEBRO_VOSK_DIR and VOICEBRO_WHISPER_DIR set.
param(
    [string]$VoskDir = "target\wake-corpus\assets",
    [string]$WhisperDir = "target\whisper-test\assets",
    [string]$Corpus,
    [switch]$NoVosk,
    [switch]$NoWhisper
)
$ErrorActionPreference = "Stop"
$cmake = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin"
if ((Test-Path $cmake) -and ($env:PATH -notlike "*$cmake*")) { $env:PATH += ";$cmake" }

Remove-Item Env:VOICEBRO_VOSK_DIR, Env:VOICEBRO_WHISPER_DIR, Env:VOICEBRO_VOICE_CORPUS -ErrorAction SilentlyContinue
if (-not $NoVosk) { $env:VOICEBRO_VOSK_DIR = (Resolve-Path $VoskDir).Path }
if (-not $NoWhisper) { $env:VOICEBRO_WHISPER_DIR = (Resolve-Path $WhisperDir).Path }
if ($Corpus) { $env:VOICEBRO_VOICE_CORPUS = (Resolve-Path $Corpus).Path }

$runs = "target\voice-corpus\runs"
New-Item -ItemType Directory -Force $runs | Out-Null
$report = Join-Path $runs ((Get-Date -Format "yyyy-MM-dd-HHmmss") + ".txt")

# cargo and the test write to stderr; keep every line, in order, in the report. A stderr
# line arrives as an error record: its message is the line (an empty line has no other
# text than the record's type name).
$ErrorActionPreference = "Continue"
cargo test --release -p voicebro-core voice_corpus_when_enabled -- --nocapture 2>&1 |
    ForEach-Object {
        if ($_ -is [System.Management.Automation.ErrorRecord]) { $_.Exception.Message } else { "$_" }
    } | Tee-Object -FilePath $report
$code = $LASTEXITCODE
Write-Output "report: $report (exit $code)"
exit $code
