## MODIFIED Requirements

### Requirement: Whisper falls back to the CPU without a CUDA device
When the GPU is requested and no CUDA device is available, the app SHALL load Whisper on
the CPU and SHALL say so in the log, the status and the Settings tab's Whisper card, naming
the requested device, the device used and the reason.

#### Scenario: GPU requested on a PC without an NVIDIA GPU
- **WHEN** `whisper.device` is `gpu` and the PC has no NVIDIA driver
- **THEN** Whisper runs on the CPU, the log reads "Whisper device: gpu requested, cpu used: no CUDA device", and the Settings tab's Whisper card shows the same

### Requirement: Noise does not become text
Each utterance SHALL pass Whisper's built-in voice activity detection, with the Silero model
of the catalog. An utterance without speech segments SHALL give no text. A segment whose
no-speech probability is above 0.6 SHALL be dropped. A text that repeats one word four or
more times in a row SHALL be dropped as a hallucination only when that run has more words
than five per second of the utterance's speech. Each kind of drop SHALL be counted in the
status, and a dropped hallucination SHALL be logged with the number of repeats and the
utterance's length.

#### Scenario: Game sound on an open microphone
- **WHEN** a speaker's microphone sends 3 s of game sound with no speech
- **THEN** no text comes out, no phrase counts, and the noise drop counter rises

#### Scenario: Silence with phrases in the prompt
- **WHEN** the noise-only clips of the corpus are transcribed with the activation phrases in the prompt
- **THEN** none of them produces an activation phrase

#### Scenario: A word said several times
- **WHEN** a speaker says "bro, bro, bro, bro" in 1.5 s and Whisper writes it so
- **THEN** the text is kept, and the activation phrase "bro" in it counts

#### Scenario: A decoding loop
- **WHEN** Whisper writes one word twenty times in a row for an utterance of 2 s
- **THEN** the text is dropped as a hallucination, the hallucination counter rises, and the log line names 20 repeats in 2.0 s

### Requirement: Whisper loads at a join and unloads after the last leave
In Whisper mode, the model, the VAD model and the runtime SHALL load while the bot joins
a voice channel. They SHALL unload 0.5 s after the bot has left its last voice channel,
and when the assistant mode goes back to realtime; the runtime's libraries stay loaded for
the process. A leave SHALL stop the transcription in progress of that guild, and an unload
any transcription in progress, at the next point whisper.cpp checks: the end of the
encoder pass or decoder step running. The text of a stopped transcription SHALL be
dropped. After a model on the GPU is freed, the app SHALL reset the process's CUDA context,
so the GPU memory it held goes back to the system. A change of model, device, language or
thread count while loaded SHALL load the new model in the background and SHALL keep the old
one in use until the new one is ready. The load time and the device SHALL be logged.

#### Scenario: Idle between joins
- **WHEN** the app runs in Whisper mode and the bot is in no voice channel
- **THEN** no Whisper model is in memory, and no CUDA context holds GPU memory

#### Scenario: Model changed during a session
- **WHEN** the user picks another model while the bot is in a channel
- **THEN** questions keep being transcribed with the old model until the new one has loaded, and the log names the switch

#### Scenario: Leave during a transcription on the CPU
- **WHEN** the bot leaves while Whisper on the CPU transcribes an utterance of that guild
- **THEN** the transcription stops at the end of the encoder pass or decoder step running, no text of it comes out, the log says it was stopped, and the model unloads after that

#### Scenario: GPU memory after the leave
- **WHEN** the bot leaves its last voice channel while a model is loaded on the GPU
- **THEN** after the unload the app holds no GPU memory, and the log says the CUDA context was reset
